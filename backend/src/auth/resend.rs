//! 重发验证邮件服务（M02-IDENTITY-08）。
//!
//! 行为契约：
//! - **统一响应**：邮箱不存在、已激活与正常重发都返回 `Ok`（handler 统一 202），
//!   不泄漏邮箱是否已注册/已验证；
//! - **冷却时间**：同账号重发间隔（默认 60s），期间再请求 → 429；
//! - **日上限**：同账号每天重发次数上限（默认 3），超限 → 429；
//! - **冷却/日上限对所有请求计数**（包括不存在的邮箱），保证"重发 N 次仍
//!   不 429"这一可观测行为对存在/不存在一致，防账号枚举；
//! - **旧 token 失效**：重发成功时同用户旧验证 token 全部标记 consumed，
//!   并生成新一次性 token + 验证邮件 Outbox（payload 只含 token 引用）。
//!
//! 限流为进程内（docs/SECURITY.md §16），窗口/上限可注入（测试用小值）。

use serde_json::json;
use sqlx::Either;

use crate::{
    audit::AuditEntry,
    auth::token::{generate_token, hash_token},
    db::pool::DatabasePool,
    events,
    outbox::{self, OutboxTx},
    ratelimit::{RateLimitStatus, RateLimiter},
};

/// 验证 token 有效期：24 小时（毫秒）。
const VERIFY_TOKEN_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// 重发限流参数（生产默认：冷却 60s、每天 3 次；测试可注入小值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResendLimits {
    pub cooldown_ms: i64,
    pub daily_window_ms: i64,
    pub daily_limit: u32,
}

impl Default for ResendLimits {
    fn default() -> Self {
        Self {
            cooldown_ms: 60 * 1000,
            daily_window_ms: 24 * 60 * 60 * 1000,
            daily_limit: 3,
        }
    }
}

/// 重发结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResendOutcome {
    /// 已生成新 token 并写入验证邮件 Outbox。
    Sent {
        verify_token_id: String,
        event_id: String,
    },
    /// 邮箱不存在或账号已激活：不创建 token、不发邮件（统一 202 响应）。
    Noop,
}

/// 重发错误。
#[derive(Debug)]
pub enum ResendError {
    /// 冷却或日上限命中（限流拒绝，handler 返回 429）。
    RateLimited {
        retry_after_secs: u64,
        limit: u32,
        remaining: u32,
        reset_at_unix_secs: i64,
    },
    /// 其他数据库错误。
    Database(sqlx::Error),
}

impl std::fmt::Display for ResendError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResendError::RateLimited { .. } => {
                write!(f, "too many resend requests, try again later")
            }
            ResendError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for ResendError {}

/// 重发验证邮件：冷却 → 日上限 → 事务生成新 token（旧 token 失效 + Outbox）。
pub async fn resend_verification_email(
    pool: &DatabasePool,
    limiter: &RateLimiter,
    email_normalized: &str,
    request_id: &str,
    limits: &ResendLimits,
) -> Result<ResendOutcome, ResendError> {
    let now = outbox::now_millis();

    // 冷却：60s 内仅 1 次（对所有请求计数，防枚举）
    let cooldown_key = format!("resend:cooldown:{email_normalized}");
    let cooldown = limiter.check(&cooldown_key, 1, limits.cooldown_ms, now);
    if !cooldown.allowed {
        return Err(rate_limited_from(cooldown));
    }
    // 日上限：每天 N 次（对所有请求计数，防枚举）
    let daily_key = format!("resend:daily:{email_normalized}");
    let daily = limiter.check(&daily_key, limits.daily_limit, limits.daily_window_ms, now);
    if !daily.allowed {
        return Err(rate_limited_from(daily));
    }

    // 仅对存在的 pending 用户重发；否则统一 Noop（响应一致）
    let user_id: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT id FROM users WHERE email_normalized = ? AND status = 'pending'",
        )
        .bind(email_normalized)
        .fetch_optional(p)
        .await
        .map_err(ResendError::Database)?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT id FROM users WHERE email_normalized = ? AND status = 'pending'",
        )
        .bind(email_normalized)
        .fetch_optional(p)
        .await
        .map_err(ResendError::Database)?,
    };
    let Some(user_id) = user_id else {
        return Ok(ResendOutcome::Noop);
    };

    // 事务：旧 token 失效 → 新 token → 审计 → 验证邮件 Outbox
    let mut tx = begin_tx(pool).await.map_err(ResendError::Database)?;

    invalidate_old_tokens(&mut tx, &user_id, now)
        .await
        .map_err(ResendError::Database)?;

    let token = generate_token();
    let token_hash = hash_token(&token);
    let token_id = uuid::Uuid::now_v7().to_string();
    let expires_at = now + VERIFY_TOKEN_TTL_MS;
    insert_verify_token(&mut tx, &token_id, &user_id, &token_hash, expires_at, now)
        .await
        .map_err(ResendError::Database)?;

    AuditEntry::user_action(&user_id, "auth.resend_verification")
        .with_target("user", &user_id)
        .with_request_id(request_id)
        .record_in_tx(&mut tx)
        .await
        .map_err(ResendError::Database)?;

    let event_id = outbox::enqueue_in_tx(
        &mut tx,
        events::types::USER_REGISTERED,
        json!({
            "user_id": &user_id,
            "email": email_normalized,
            "email_verification_token_id": &token_id,
            "verify_token_expires_at": expires_at,
            "resend": true,
        }),
    )
    .await
    .map_err(ResendError::Database)?;

    commit_tx(tx).await.map_err(ResendError::Database)?;

    Ok(ResendOutcome::Sent {
        verify_token_id: token_id,
        event_id,
    })
}

fn rate_limited_from(status: RateLimitStatus) -> ResendError {
    ResendError::RateLimited {
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
            "UPDATE email_verification_tokens
                 SET consumed_at = ? WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map(|_| ()),
        Either::Right(t) => sqlx::query(
            "UPDATE email_verification_tokens
                 SET consumed_at = ? WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map(|_| ()),
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
