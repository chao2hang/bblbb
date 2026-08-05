//! 注册事务服务（M02-IDENTITY-05）。
//!
//! 在**同一事务**内原子创建：
//! 1. `users` 行（`status='pending'`，规范化 username/email，Argon2id PHC hash）；
//! 2. `email_verification_tokens` 行（一次性验证 token 的 SHA-256 hash，24h 过期，
//!    数据库只存 hash，原始 token 仅存在于响应/邮件构建路径）；
//! 3. `audit_logs` 行（注册审计，与业务变更同事务——无审计不得提交）；
//! 4. `outbox_events` 行（`user.registered.v1` 领域事件，payload 只含
//!    `email_verification_token_id` 引用，不含明文 token，见 M01-JOBS-12）。
//!
//! 任一写入失败 → 整个事务回滚（无半完成状态）。用户名/邮箱唯一约束冲突
//! 返回 [`RegisterUserError::AlreadyExists`]，由调用方给出统一响应
//! （不泄漏哪个标识已存在，M02-IDENTITY-06 在此基础上加限流）。

use serde_json::json;
use sqlx::Either;

use crate::{
    audit::AuditEntry,
    auth::{
        password::hash_password,
        token::{generate_token, hash_token},
    },
    db::pool::DatabasePool,
    domain::registration::NormalizedRegistration,
    events,
    outbox::{self, OutboxTx},
};

/// 验证邮件 token 有效期：24 小时（Unix 毫秒，跨库时间戳约定 M01-DB-08）。
const VERIFY_TOKEN_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// 注册事务成功输出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegistrationOutcome {
    pub user_id: String,
    /// 验证 token 记录的 ID（Outbox payload 以此为引用）。
    pub verify_token_id: String,
    /// 验证邮件领域事件（`user.registered.v1`）的 Outbox 事件 ID。
    pub event_id: String,
}

/// 注册事务错误。
#[derive(Debug)]
pub enum RegisterUserError {
    /// 用户名或邮箱已存在（唯一约束冲突；不泄漏具体是哪一个）。
    AlreadyExists,
    /// Argon2id 哈希失败。
    PasswordHashFailed(String),
    /// 其他数据库错误。
    Database(sqlx::Error),
}

impl std::fmt::Display for RegisterUserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisterUserError::AlreadyExists => write!(f, "username or email already exists"),
            RegisterUserError::PasswordHashFailed(e) => write!(f, "password hashing failed: {e}"),
            RegisterUserError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for RegisterUserError {}

/// 在单事务中创建 pending 用户、验证 token、审计与验证邮件 Outbox。
///
/// 提交前四类写入全部就绪；任何一步失败或唯一约束冲突都会让整个事务回滚，
/// 数据库不残留半完成状态（用户/token/审计/Outbox 要么全有，要么全无）。
pub async fn register_user(
    pool: &DatabasePool,
    registration: &NormalizedRegistration,
    request_id: &str,
) -> Result<RegistrationOutcome, RegisterUserError> {
    let mut tx = begin_tx(pool).await.map_err(RegisterUserError::Database)?;

    let user_id = uuid::Uuid::now_v7().to_string();
    let now = outbox::now_millis();
    let password_hash = hash_password(&registration.password)
        .map_err(|e| RegisterUserError::PasswordHashFailed(e.to_string()))?;

    // 1) pending 用户（规范化列入库；email_verified/timezone 等走默认值）
    let inserted = insert_user(&mut tx, &user_id, registration, &password_hash, now).await;
    if let Err(sqlx::Error::Database(ref e)) = inserted {
        if e.is_unique_violation() {
            return Err(RegisterUserError::AlreadyExists);
        }
    }
    inserted.map_err(RegisterUserError::Database)?;

    // 2) 一次性验证 token（只存 SHA-256 hash；原始 token 不入库、不进 payload）
    let verify_token = generate_token();
    let token_hash = hash_token(&verify_token);
    let token_id = uuid::Uuid::now_v7().to_string();
    let expires_at = now + VERIFY_TOKEN_TTL_MS;
    insert_verify_token(&mut tx, &token_id, &user_id, &token_hash, expires_at, now)
        .await
        .map_err(RegisterUserError::Database)?;

    // 3) 审计（与业务变更同事务提交）
    AuditEntry::user_action(&user_id, "auth.register")
        .with_target("user", &user_id)
        .with_request_id(request_id)
        .record_in_tx(&mut tx)
        .await
        .map_err(RegisterUserError::Database)?;

    // 4) 验证邮件 Outbox（payload 只含 token 引用，无明文；验证任务自校验）
    let event_id = outbox::enqueue_in_tx(
        &mut tx,
        events::types::USER_REGISTERED,
        json!({
            "user_id": &user_id,
            "username": registration.username,
            "email": registration.email_normalized,
            "email_verification_token_id": &token_id,
            "verify_token_expires_at": expires_at,
        }),
    )
    .await
    .map_err(RegisterUserError::Database)?;

    commit_tx(tx).await.map_err(RegisterUserError::Database)?;

    Ok(RegistrationOutcome {
        user_id,
        verify_token_id: token_id,
        event_id,
    })
}

async fn begin_tx(pool: &DatabasePool) -> Result<OutboxTx<'_>, sqlx::Error> {
    match pool {
        Either::Left(p) => Ok(Either::Left(p.begin().await?)),
        Either::Right(p) => Ok(Either::Right(p.begin().await?)),
    }
}

async fn commit_tx(tx: OutboxTx<'_>) -> Result<(), sqlx::Error> {
    match tx {
        Either::Left(t) => t.commit().await,
        Either::Right(t) => t.commit().await,
    }
}

async fn insert_user<'e>(
    tx: &mut OutboxTx<'e>,
    user_id: &str,
    r: &NormalizedRegistration,
    password_hash: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    match tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(user_id)
            .bind(&r.username_normalized)
            .bind(&r.email_normalized)
            .bind(password_hash)
            .bind(now)
            .bind(now)
            .execute(&mut **t)
            .await
            .map(|_| ())
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(user_id)
            .bind(&r.username_normalized)
            .bind(&r.email_normalized)
            .bind(password_hash)
            .bind(now)
            .bind(now)
            .execute(&mut **t)
            .await
            .map(|_| ())
        }
    }
}

async fn insert_verify_token<'e>(
    tx: &mut OutboxTx<'e>,
    token_id: &str,
    user_id: &str,
    token_hash: &str,
    expires_at: i64,
    now: i64,
) -> Result<(), sqlx::Error> {
    match tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(token_id)
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .bind(now)
            .execute(&mut **t)
            .await
            .map(|_| ())
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(token_id)
            .bind(user_id)
            .bind(token_hash)
            .bind(expires_at)
            .bind(now)
            .execute(&mut **t)
            .await
            .map(|_| ())
        }
    }
}
