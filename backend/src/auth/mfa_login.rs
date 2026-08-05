//! 两步登录的 MFA challenge（M02-UX-03）。
//!
//! 启用 TOTP 的用户登录分两步：
//! 1. `POST /api/v1/auth/login`（密码）：`login_user` 校验密码成功后
//!    **不**签发会话，而是由 handler 调用 [`start_mfa_login`] 签发一次性
//!    challenge token（只存 SHA-256 hash，5 分钟过期）并返回；
//! 2. `POST /api/v1/auth/login/mfa`：[`complete_mfa_login`] 用 TOTP code
//!    或恢复码完成登录——原子消费 challenge + 校验第二因素（TOTP 防重放 /
//!    恢复码原子消费，均已有服务）→ 签发会话（auth_verified_at=now）。
//!
//! 安全约定：
//! - challenge token 高熵随机，数据库只存 hash（防库泄露直接复用）；
//! - challenge 一次性：`UPDATE WHERE consumed_at IS NULL AND expires_at > now`
//!   原子消费，并发同 challenge 恰好一个成功；
//! - 失败统一返回 `InvalidChallenge`（防枚举：不区分 challenge 不存在/
//!   已消费/过期），第二因素错误统一 `InvalidCode`（不泄漏 TOTP 或恢复码
//!   是否有效之外的信息）。

use sqlx::Either;

use crate::{
    auth::{
        mfa::{consume_recovery_code, verify_totp_login, MfaError},
        session::create_session,
        token::{generate_token, hash_token},
    },
    db::pool::DatabasePool,
    outbox::{now_millis, OutboxTx},
};

/// challenge 有效期：5 分钟（毫秒）。
pub const MFA_CHALLENGE_TTL_MS: i64 = 5 * 60 * 1000;

/// 两步登录错误。
#[derive(Debug)]
pub enum MfaLoginError {
    /// challenge 不存在 / 已消费 / 过期（统一，防枚举）。
    InvalidChallenge,
    /// 第二因素（TOTP code / 恢复码）无效或已使用。
    InvalidCode,
    Database(String),
}

impl std::fmt::Display for MfaLoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MfaLoginError::InvalidChallenge => write!(f, "invalid or expired MFA challenge"),
            MfaLoginError::InvalidCode => write!(f, "invalid MFA code"),
            MfaLoginError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for MfaLoginError {}

/// 签发一次性 MFA login challenge，返回明文 token（只此一次）。
pub async fn start_mfa_login(pool: &DatabasePool, user_id: &str) -> Result<String, sqlx::Error> {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO mfa_login_challenges (id, user_id, token_hash, created_at, expires_at, consumed_at)
                 VALUES (?, ?, ?, ?, ?, NULL)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&token_hash)
            .bind(now)
            .bind(now + MFA_CHALLENGE_TTL_MS)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO mfa_login_challenges (id, user_id, token_hash, created_at, expires_at, consumed_at)
                 VALUES (?, ?, ?, ?, ?, NULL)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(&token_hash)
            .bind(now)
            .bind(now + MFA_CHALLENGE_TTL_MS)
            .execute(p)
            .await?;
        }
    }

    Ok(token)
}

/// 完成 MFA 登录的结果：会话 token + 用户档案（供 handler 直接构建 Me）。
#[derive(Debug, Clone)]
pub struct MfaLoginCompleted {
    pub session_token: String,
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub status: String,
    pub display_name: Option<String>,
}

/// 完成 MFA 登录：`totp_code` 与 `recovery_code` 二选一，校验第二因素后
/// 原子消费 challenge 并签发会话。返回会话 token 与用户档案。
pub async fn complete_mfa_login(
    pool: &DatabasePool,
    challenge_token: &str,
    totp_code: Option<&str>,
    recovery_code: Option<&str>,
    encryption_key: &[u8],
    request_id: &str,
) -> Result<MfaLoginCompleted, MfaLoginError> {
    let token_hash = hash_token(challenge_token);

    // 1) 读 challenge（不消费）：不存在/已消费/过期 → 统一 InvalidChallenge
    let row: Option<(String,)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT user_id FROM mfa_login_challenges
             WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(&token_hash)
        .bind(now_millis())
        .fetch_optional(p)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT user_id FROM mfa_login_challenges
             WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(&token_hash)
        .bind(now_millis())
        .fetch_optional(p)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?,
    };
    let Some((user_id,)) = row else {
        return Err(MfaLoginError::InvalidChallenge);
    };

    // 2) 校验第二因素（原子；失败统一 InvalidCode，不泄漏细节）
    match (totp_code, recovery_code) {
        (Some(code), None) => {
            verify_totp_login(pool, &user_id, code, encryption_key, now_secs(), 1)
                .await
                .map_err(|e| match e {
                    MfaError::Database(msg) => MfaLoginError::Database(msg),
                    _ => MfaLoginError::InvalidCode,
                })?;
        }
        (None, Some(code)) => {
            consume_recovery_code(pool, &user_id, code, request_id)
                .await
                .map_err(|e| match e {
                    MfaError::Database(msg) => MfaLoginError::Database(msg),
                    _ => MfaLoginError::InvalidCode,
                })?;
        }
        _ => return Err(MfaLoginError::InvalidCode),
    }

    // 3) 原子消费 challenge（并发同 challenge 恰好一个成功）→ 签发会话
    let mut tx = begin_tx(pool)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?;
    let consumed = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE mfa_login_challenges SET consumed_at = ?
             WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(now_millis())
        .bind(&token_hash)
        .bind(now_millis())
        .execute(&mut **t)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE mfa_login_challenges SET consumed_at = ?
             WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
        )
        .bind(now_millis())
        .bind(&token_hash)
        .bind(now_millis())
        .execute(&mut **t)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?
        .rows_affected(),
    };
    if consumed != 1 {
        // 已消费（并发输家）：tx 丢弃即回滚
        return Err(MfaLoginError::InvalidChallenge);
    }
    commit_tx(tx)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?;

    // 会话签发（auth_verified_at=now，M02-MFA-07 step-up 即刻满足）
    let session_token = create_session(pool, &user_id, None)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?;

    // 用户档案（handler 构建 Me 投影用；banned/deleted 由密码步拦截过，
    // 此处仍查询最新状态，保持与 /me 一致）
    let profile: Option<(String, String, i64, String, Option<String>)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT username_normalized, email_normalized, email_verified, status, display_name
             FROM users WHERE id = ?",
        )
        .bind(&user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT username_normalized, email_normalized, email_verified, status, display_name
             FROM users WHERE id = ?",
        )
        .bind(&user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| MfaLoginError::Database(e.to_string()))?,
    };
    let Some((username, email, email_verified, status, display_name)) = profile else {
        return Err(MfaLoginError::InvalidChallenge); // 用户缺失（已删除）→ 统一失败
    };

    Ok(MfaLoginCompleted {
        session_token,
        user_id,
        username,
        email,
        email_verified: email_verified != 0,
        status,
        display_name,
    })
}

fn now_secs() -> u64 {
    (now_millis() / 1000) as u64
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
