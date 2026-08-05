//! 密码重置服务（M02-IDENTITY-10）。
//!
//! **请求重置**（[`request_password_reset`]）：
//! - 统一响应：邮箱不存在/已删除与正常请求都返回 `Ok`（handler 统一 202），
//!   不泄漏邮箱是否已注册；冷却与日上限对全部请求计数（防账号枚举）；
//! - 冷却（60s）与日上限（3 次）命中 → 429（docs/SECURITY.md §16：
//!   「找回密码每账号和每 IP 分别限流，但始终返回相同响应」）；
//! - 存在用户时单事务：旧 reset token 全部失效 → 新 30 分钟一次性 token →
//!   审计 `auth.password_reset_requested` → 邮件 Outbox（payload 只含
//!   `password_reset_token_id` 引用，无明文，M01-JOBS-12）。
//!
//! **确认重置**（[`confirm_password_reset`]）：
//! - UPDATE 驱动原子消费（`consumed_at IS NULL AND expires_at > now`），
//!   `rows_affected == 1` 保证并发唯一成功，其余返回
//!   [`ConfirmResetError::InvalidOrExpired`] 并回滚；
//! - 同事务更新密码哈希 + 撤销该用户全部 Session + 审计
//!   `auth.password_reset_completed`；
//! - 无效/已消费/过期统一错误（防 token 枚举）。

use serde_json::json;
use sqlx::Either;

use crate::{
    audit::AuditEntry,
    auth::token::hash_token,
    db::pool::DatabasePool,
    events,
    outbox::{self, OutboxTx},
    ratelimit::{RateLimitStatus, RateLimiter},
};

/// 密码重置 token 有效期：30 分钟（毫秒）。
const RESET_TOKEN_TTL_MS: i64 = 30 * 60 * 1000;

/// 请求重置限流参数（生产默认：冷却 60s、每天 3 次；测试可注入小值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PasswordResetLimits {
    pub cooldown_ms: i64,
    pub daily_window_ms: i64,
    pub daily_limit: u32,
}

impl Default for PasswordResetLimits {
    fn default() -> Self {
        Self {
            cooldown_ms: 60 * 1000,
            daily_window_ms: 24 * 60 * 60 * 1000,
            daily_limit: 3,
        }
    }
}

/// 请求重置的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestResetOutcome {
    /// 已生成新 reset token 并写入邮件 Outbox。
    Sent {
        reset_token_id: String,
        event_id: String,
    },
    /// 邮箱不存在或已删除：不创建 token、不发邮件（统一 202 响应）。
    Noop,
}

/// 请求重置错误。
#[derive(Debug)]
pub enum RequestResetError {
    /// 冷却或日上限命中（handler 返回 429）。
    RateLimited {
        retry_after_secs: u64,
        limit: u32,
        remaining: u32,
        reset_at_unix_secs: i64,
    },
    Database(sqlx::Error),
}

/// 确认重置的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfirmResetOutcome {
    pub user_id: String,
}

/// 确认重置错误。
#[derive(Debug)]
pub enum ConfirmResetError {
    /// 不存在 / 已消费 / 过期。
    InvalidOrExpired,
    Database(sqlx::Error),
}

impl std::fmt::Display for RequestResetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestResetError::RateLimited { .. } => {
                write!(f, "too many password reset requests, try again later")
            }
            RequestResetError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for RequestResetError {}

impl std::fmt::Display for ConfirmResetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfirmResetError::InvalidOrExpired => {
                write!(f, "invalid or expired password reset token")
            }
            ConfirmResetError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for ConfirmResetError {}

/// 请求找回密码：冷却 → 日上限 → 事务生成新 reset token（旧失效 + Outbox）。
pub async fn request_password_reset(
    pool: &DatabasePool,
    limiter: &RateLimiter,
    email_normalized: &str,
    request_id: &str,
    limits: &PasswordResetLimits,
) -> Result<RequestResetOutcome, RequestResetError> {
    let now = outbox::now_millis();

    // 冷却：60s 内仅 1 次（对所有请求计数，防枚举）
    let cooldown_key = format!("reset:cooldown:{email_normalized}");
    let cooldown = limiter.check(&cooldown_key, 1, limits.cooldown_ms, now);
    if !cooldown.allowed {
        return Err(rate_limited_from(cooldown));
    }
    // 日上限：每天 N 次（对所有请求计数，防枚举）
    let daily_key = format!("reset:daily:{email_normalized}");
    let daily = limiter.check(&daily_key, limits.daily_limit, limits.daily_window_ms, now);
    if !daily.allowed {
        return Err(rate_limited_from(daily));
    }

    // 仅对存在的非删除用户生成 token；否则统一 Noop
    let user_id: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT id FROM users WHERE email_normalized = ? AND status != 'deleted'",
        )
        .bind(email_normalized)
        .fetch_optional(p)
        .await
        .map_err(RequestResetError::Database)?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT id FROM users WHERE email_normalized = ? AND status != 'deleted'",
        )
        .bind(email_normalized)
        .fetch_optional(p)
        .await
        .map_err(RequestResetError::Database)?,
    };
    let Some(user_id) = user_id else {
        return Ok(RequestResetOutcome::Noop);
    };

    // 事务：旧 token 失效 → 新 token(30min) → 审计 → 邮件 Outbox
    let mut tx = begin_tx(pool).await.map_err(RequestResetError::Database)?;

    invalidate_old_tokens(&mut tx, &user_id, now)
        .await
        .map_err(RequestResetError::Database)?;

    let reset_token_id = uuid::Uuid::now_v7().to_string();
    insert_reset_token(
        &mut tx,
        &reset_token_id,
        &user_id,
        now,
        now + RESET_TOKEN_TTL_MS,
    )
    .await
    .map_err(RequestResetError::Database)?;

    AuditEntry::user_action(&user_id, "auth.password_reset_requested")
        .with_target("user", &user_id)
        .with_request_id(request_id)
        .record_in_tx(&mut tx)
        .await
        .map_err(RequestResetError::Database)?;

    let event_id = outbox::enqueue_in_tx(
        &mut tx,
        events::types::USER_REGISTERED,
        json!({
            "user_id": &user_id,
            "email": email_normalized,
            "password_reset_token_id": &reset_token_id,
            "reset_token_expires_at": now + RESET_TOKEN_TTL_MS,
            "kind": "password_reset",
        }),
    )
    .await
    .map_err(RequestResetError::Database)?;

    commit_tx(tx).await.map_err(RequestResetError::Database)?;

    Ok(RequestResetOutcome::Sent {
        reset_token_id,
        event_id,
    })
}

/// 确认重置：单事务原子消费 token + 更新密码哈希 + 撤销全部 Session + 审计。
///
/// `new_password_hash` 由调用方先哈希（Argon2id），本函数只负责落库。
pub async fn confirm_password_reset(
    pool: &DatabasePool,
    token: &str,
    new_password_hash: &str,
    request_id: &str,
) -> Result<ConfirmResetOutcome, ConfirmResetError> {
    let mut tx = begin_tx(pool).await.map_err(ConfirmResetError::Database)?;

    let token_hash = hash_token(token);
    let now = outbox::now_millis();

    // 事务内读 token（随 UPDATE 原子消费，避免 TOCTOU）
    let row: Option<(String,)> = match &mut tx {
        Either::Left(t) => {
            sqlx::query_as("SELECT user_id FROM password_reset_tokens WHERE token_hash = ?")
                .bind(&token_hash)
                .fetch_optional(&mut **t)
                .await
                .map_err(ConfirmResetError::Database)?
        }
        Either::Right(t) => {
            sqlx::query_as("SELECT user_id FROM password_reset_tokens WHERE token_hash = ?")
                .bind(&token_hash)
                .fetch_optional(&mut **t)
                .await
                .map_err(ConfirmResetError::Database)?
        }
    };
    let Some((user_id,)) = row else {
        return Err(ConfirmResetError::InvalidOrExpired);
    };

    // 原子消费：并发下只有一个请求赢得 rows_affected == 1
    let consumed_rows = match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "UPDATE password_reset_tokens
                 SET consumed_at = ? WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(now)
            .bind(&token_hash)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(ConfirmResetError::Database)?
            .rows_affected()
        }
        Either::Right(t) => {
            sqlx::query(
                "UPDATE password_reset_tokens
                 SET consumed_at = ? WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(now)
            .bind(&token_hash)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(ConfirmResetError::Database)?
            .rows_affected()
        }
    };
    if consumed_rows != 1 {
        // 已消费/过期/不存在 → 回滚，统一错误
        return Err(ConfirmResetError::InvalidOrExpired);
    }

    // 更新密码哈希 + 撤销该用户全部 Session（M02-IDENTITY-10）
    let updated = match &mut tx {
        Either::Left(t) => {
            sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                .bind(new_password_hash)
                .bind(now)
                .bind(&user_id)
                .execute(&mut **t)
                .await
                .map_err(ConfirmResetError::Database)?
                .rows_affected()
        }
        Either::Right(t) => {
            sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                .bind(new_password_hash)
                .bind(now)
                .bind(&user_id)
                .execute(&mut **t)
                .await
                .map_err(ConfirmResetError::Database)?
                .rows_affected()
        }
    };
    if updated != 1 {
        return Err(ConfirmResetError::InvalidOrExpired); // 用户缺失 → 回滚
    }

    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(&user_id)
            .execute(&mut **t)
            .await
            .map_err(ConfirmResetError::Database)?;
        }
        Either::Right(t) => {
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(&user_id)
            .execute(&mut **t)
            .await
            .map_err(ConfirmResetError::Database)?;
        }
    }

    AuditEntry::user_action(&user_id, "auth.password_reset_completed")
        .with_target("user", &user_id)
        .with_request_id(request_id)
        .record_in_tx(&mut tx)
        .await
        .map_err(ConfirmResetError::Database)?;

    commit_tx(tx).await.map_err(ConfirmResetError::Database)?;

    Ok(ConfirmResetOutcome { user_id })
}

fn rate_limited_from(status: RateLimitStatus) -> RequestResetError {
    RequestResetError::RateLimited {
        retry_after_secs: status.retry_after_secs,
        limit: status.limit,
        remaining: status.remaining,
        reset_at_unix_secs: status.reset_at_ms / 1000,
    }
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

async fn invalidate_old_tokens<'e>(
    tx: &mut OutboxTx<'e>,
    user_id: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    match tx {
        Either::Left(t) => sqlx::query(
            "UPDATE password_reset_tokens
                 SET consumed_at = ? WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map(|_| ()),
        Either::Right(t) => sqlx::query(
            "UPDATE password_reset_tokens
                 SET consumed_at = ? WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map(|_| ()),
    }
}

/// 插入新 reset token（token 为随机生成，DB 只存 hash）。
async fn insert_reset_token<'e>(
    tx: &mut OutboxTx<'e>,
    token_id: &str,
    user_id: &str,
    now: i64,
    expires_at: i64,
) -> Result<(), sqlx::Error> {
    let token = crate::auth::token::generate_token();
    let token_hash = hash_token(&token);
    match tx {
        Either::Left(t) => sqlx::query(
            "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
        )
        .bind(token_id)
        .bind(user_id)
        .bind(&token_hash)
        .bind(expires_at)
        .bind(now)
        .execute(&mut **t)
        .await
        .map(|_| ()),
        Either::Right(t) => sqlx::query(
            "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
        )
        .bind(token_id)
        .bind(user_id)
        .bind(&token_hash)
        .bind(expires_at)
        .bind(now)
        .execute(&mut **t)
        .await
        .map(|_| ()),
    }
}
