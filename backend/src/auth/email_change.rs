//! 换绑邮箱（GA 用户请求：settings 显示验证状态 + 支持修改绑定邮箱）。
//!
//! 流程（Flarum 同款语义）：
//! 1. `POST /api/v1/me/email-change`（登录 + 当前密码确认）→ 创建
//!    `email_change_tokens` 行（明文不落库：token_hash 校验 + token_encrypted
//!    密文，GA P0-2 同款）→ Outbox `user.email_change_requested.v1` →
//!    消费者入队确认邮件（收件人经 `params.to_email` 定向**新邮箱**——新邮箱
//!    确认前不写 users.email_normalized）。
//! 2. 新邮箱收确认链接 → `POST /api/v1/auth/email-change/confirm`（匿名，
//!    一次性 token）→ 事务：消费 token + `UPDATE users SET email_normalized
//!    = new_email, email_verified = 1`（换绑成功即视为已验证——证明了对新
//!    邮箱的控制权）。
//!
//! 冷却：同新邮箱 60s 内 1 次、每日 3 次（RateLimiter，与 resend 同量级；
//! 密码确认兜底防枚举）。

use serde_json::json;
use sqlx::Either;

use crate::{
    audit::AuditEntry,
    auth::password::{verify_password, VerifyResult},
    auth::token::{generate_token, hash_token},
    db::pool::DatabasePool,
    events,
    outbox::{self, OutboxTx},
    ratelimit::{RateLimitStatus, RateLimiter},
};

/// 换绑 token 有效期：30 分钟（毫秒）。
const EMAIL_CHANGE_TOKEN_TTL_MS: i64 = 30 * 60 * 1000;

/// 换绑限流参数（生产默认：冷却 60s、每天 3 次；测试可注入小值）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmailChangeLimits {
    pub cooldown_ms: i64,
    pub daily_window_ms: i64,
    pub daily_limit: u32,
}

impl Default for EmailChangeLimits {
    fn default() -> Self {
        Self {
            cooldown_ms: 60 * 1000,
            daily_window_ms: 24 * 60 * 60 * 1000,
            daily_limit: 3,
        }
    }
}

/// 换绑申请结果（token_id/expires_at 供响应与测试）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestEmailChangeOutcome {
    pub token_id: String,
    pub new_email: String,
    pub expires_at: i64,
}

/// 换绑申请错误。
#[derive(Debug)]
pub enum EmailChangeError {
    /// 新邮箱格式非法（规范化后）。
    InvalidEmail,
    /// 新邮箱与当前邮箱相同。
    SameEmail,
    /// 新邮箱已被其他账号占用（响应层统一 422，不区分）。
    EmailTaken,
    /// 当前密码错误。
    WrongPassword,
    /// token 无效/已消费/已过期（响应统一 400，不区分原因）。
    InvalidToken,
    /// 冷却或日上限命中（handler 返回 429）。
    RateLimited {
        retry_after_secs: u64,
        limit: u32,
        remaining: u32,
        reset_at_unix_secs: i64,
    },
    Database(sqlx::Error),
}

impl std::fmt::Display for EmailChangeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EmailChangeError::InvalidEmail => write!(f, "invalid email shape"),
            EmailChangeError::SameEmail => write!(f, "new email equals current email"),
            EmailChangeError::EmailTaken => write!(f, "email already in use"),
            EmailChangeError::WrongPassword => write!(f, "wrong password"),
            EmailChangeError::InvalidToken => write!(f, "invalid or expired token"),
            EmailChangeError::RateLimited { .. } => {
                write!(f, "too many email change requests, try again later")
            }
            EmailChangeError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for EmailChangeError {}

/// 基础邮箱格式检查（与 routes/auth.rs `valid_email_shape` 同规则）。
fn valid_email_shape(email: &str) -> bool {
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if parts.next().is_some() || local.is_empty() || domain.is_empty() {
        return false;
    }
    if local.contains(char::is_whitespace) || domain.contains(char::is_whitespace) {
        return false;
    }
    domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

/// 申请换绑邮箱：校验 → 冷却/日上限（按新邮箱计数，防撞库刷信箱）→
/// 事务（token + 审计 + Outbox 事件）。
#[allow(clippy::too_many_arguments)]
pub async fn request_email_change(
    pool: &DatabasePool,
    limiter: &RateLimiter,
    user_id: &str,
    username: &str,
    current_email: &str,
    current_password: &str,
    new_email_raw: &str,
    request_id: &str,
    limits: &EmailChangeLimits,
    settings_key: &str,
) -> Result<RequestEmailChangeOutcome, EmailChangeError> {
    let now = outbox::now_millis();
    let new_email = crate::auth::identity::normalize_email(new_email_raw.trim());

    if !valid_email_shape(&new_email) {
        return Err(EmailChangeError::InvalidEmail);
    }
    if new_email == current_email {
        return Err(EmailChangeError::SameEmail);
    }

    // 冷却/日上限按用户计数（用户自身操作节奏；IP 维度防刷由 handler 层
    // 限流兜底——向任意新邮箱发信的滥用面在请求源收敛）
    let cooldown_key = format!("email_change:cooldown:{user_id}");
    let cooldown = limiter.check(&cooldown_key, 1, limits.cooldown_ms, now);
    if !cooldown.allowed {
        return Err(rate_limited_from(cooldown));
    }
    let daily_key = format!("email_change:daily:{user_id}");
    let daily = limiter.check(&daily_key, limits.daily_limit, limits.daily_window_ms, now);
    if !daily.allowed {
        return Err(rate_limited_from(daily));
    }

    // 新邮箱占用检查（其他账号已绑定 → 拒绝）
    let taken: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM users WHERE email_normalized = ?")
            .bind(&new_email)
            .fetch_optional(p)
            .await
            .map_err(EmailChangeError::Database)?,
        Either::Right(p) => sqlx::query_scalar("SELECT id FROM users WHERE email_normalized = ?")
            .bind(&new_email)
            .fetch_optional(p)
            .await
            .map_err(EmailChangeError::Database)?,
    };
    if taken.map(|id| id != user_id).unwrap_or(false) {
        return Err(EmailChangeError::EmailTaken);
    }

    // 当前密码确认（需要 password_hash；由调用方传入本次查询值）
    let password_hash: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(EmailChangeError::Database)?,
        Either::Right(p) => sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(EmailChangeError::Database)?,
    };
    let Some(hash) = password_hash else {
        return Err(EmailChangeError::Database(sqlx::Error::RowNotFound));
    };
    if verify_password(current_password, &hash) != VerifyResult::Ok {
        return Err(EmailChangeError::WrongPassword);
    }

    // 事务：token（明文不落库）+ 审计 + Outbox
    let mut tx = begin_tx(pool).await.map_err(EmailChangeError::Database)?;
    let token = generate_token();
    let token_hash = hash_token(&token);
    let token_sealed = crate::auth::token::seal_token(settings_key, &token);
    let token_id = uuid::Uuid::now_v7().to_string();
    let expires_at = now + EMAIL_CHANGE_TOKEN_TTL_MS;

    insert_change_token(
        &mut tx,
        &token_id,
        user_id,
        &new_email,
        &token_hash,
        &token_sealed,
        expires_at,
        now,
    )
    .await
    .map_err(EmailChangeError::Database)?;

    AuditEntry::user_action(user_id, "auth.email_change_request")
        .with_target("user", user_id)
        .with_request_id(request_id)
        .record_in_tx(&mut tx)
        .await
        .map_err(EmailChangeError::Database)?;

    outbox::enqueue_in_tx(
        &mut tx,
        events::types::USER_EMAIL_CHANGE_REQUESTED,
        json!({
            "user_id": user_id,
            "username": username,
            "new_email": new_email,
            "email_change_token_id": token_id,
            "email_change_expires_at": expires_at,
        }),
    )
    .await
    .map_err(EmailChangeError::Database)?;

    commit_tx(tx).await.map_err(EmailChangeError::Database)?;

    Ok(RequestEmailChangeOutcome {
        token_id,
        new_email,
        expires_at,
    })
}

/// 确认换绑（匿名，一次性 token）。成功返回新邮箱；token 无效/过期/已消费
/// 统一 `InvalidToken`（不泄漏原因）。
pub async fn confirm_email_change(
    pool: &DatabasePool,
    token: &str,
    request_id: &str,
) -> Result<String, EmailChangeError> {
    let now = outbox::now_millis();
    let token_hash = hash_token(token);

    let mut tx = begin_tx(pool).await.map_err(EmailChangeError::Database)?;

    // 查找未消费且未过期的 token（按 hash；幂等防并发双击——UPDATE 条件消费）
    let row: Option<(String, String, i64)> = match &mut tx {
        Either::Left(t) => {
            sqlx::query_as(
                "SELECT id, new_email, expires_at FROM email_change_tokens
                 WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .fetch_optional(&mut **t)
            .await
        }
        Either::Right(t) => {
            sqlx::query_as(
                "SELECT id, new_email, expires_at FROM email_change_tokens
                 WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .fetch_optional(&mut **t)
            .await
        }
    }
    .map_err(EmailChangeError::Database)?;
    let Some((token_id, new_email, _expires_at)) = row else {
        return Err(EmailChangeError::InvalidToken);
    };

    // 条件消费（只消费这一行；再查用户仍存在且未被删除）
    let user_id: Option<String> = match &mut tx {
        Either::Left(t) => sqlx::query_scalar(
            "UPDATE email_change_tokens SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL
                 RETURNING user_id",
        )
        .bind(now)
        .bind(&token_id)
        .fetch_optional(&mut **t)
        .await,
        Either::Right(t) => {
            // MySQL 无 UPDATE..RETURNING：先锁定行再消费（事务内 SELECT + UPDATE）
            let uid: Option<String> = sqlx::query_scalar(
                "SELECT user_id FROM email_change_tokens WHERE id = ? AND consumed_at IS NULL FOR UPDATE",
            )
            .bind(&token_id)
            .fetch_optional(&mut **t)
            .await
            .map_err(EmailChangeError::Database)?;
            if uid.is_some() {
                sqlx::query("UPDATE email_change_tokens SET consumed_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(&token_id)
                    .execute(&mut **t)
                    .await
                    .map_err(EmailChangeError::Database)?;
            }
            Ok(uid)
        }
    }
    .map_err(EmailChangeError::Database)?;
    let Some(user_id) = user_id else {
        return Err(EmailChangeError::InvalidToken);
    };

    // 新邮箱此刻仍不得被占用（申请与确认之间可能被他人绑定）
    let taken: Option<String> = match &mut tx {
        Either::Left(t) => {
            sqlx::query_scalar("SELECT id FROM users WHERE email_normalized = ?")
                .bind(&new_email)
                .fetch_optional(&mut **t)
                .await
        }
        Either::Right(t) => {
            sqlx::query_scalar("SELECT id FROM users WHERE email_normalized = ?")
                .bind(&new_email)
                .fetch_optional(&mut **t)
                .await
        }
    }
    .map_err(EmailChangeError::Database)?;
    if taken.map(|id| id != user_id).unwrap_or(false) {
        return Err(EmailChangeError::EmailTaken);
    }

    // 换绑生效：更新邮箱 + 视为已验证（证明了对新邮箱的控制）
    match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE users SET email_normalized = ?, email_verified = 1, email_verified_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&new_email)
        .bind(now)
        .bind(now)
        .bind(&user_id)
        .execute(&mut **t)
        .await
        .map(|_| ()),
        Either::Right(t) => sqlx::query(
            "UPDATE users SET email_normalized = ?, email_verified = 1, email_verified_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&new_email)
        .bind(now)
        .bind(now)
        .bind(&user_id)
        .execute(&mut **t)
        .await
        .map(|_| ()),
    }
    .map_err(EmailChangeError::Database)?;

    AuditEntry::user_action(&user_id, "auth.email_change_confirm")
        .with_target("user", &user_id)
        .with_request_id(request_id)
        .record_in_tx(&mut tx)
        .await
        .map_err(EmailChangeError::Database)?;

    commit_tx(tx).await.map_err(EmailChangeError::Database)?;

    Ok(new_email)
}

/// 确认换绑错误别名（token 类失败统一展示，不泄漏具体原因）。
impl EmailChangeError {
    pub fn is_token_invalid(&self) -> bool {
        matches!(self, EmailChangeError::InvalidToken)
    }
}

fn rate_limited_from(status: RateLimitStatus) -> EmailChangeError {
    EmailChangeError::RateLimited {
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

#[allow(clippy::too_many_arguments)]
async fn insert_change_token<'e>(
    tx: &mut OutboxTx<'e>,
    token_id: &str,
    user_id: &str,
    new_email: &str,
    token_hash: &str,
    token_sealed: &str,
    expires_at: i64,
    now: i64,
) -> Result<(), sqlx::Error> {
    match tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO email_change_tokens (id, user_id, new_email, token_hash, token_encrypted, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(token_id)
            .bind(user_id)
            .bind(new_email)
            .bind(token_hash)
            .bind(token_sealed)
            .bind(expires_at)
            .bind(now)
            .execute(&mut **t)
            .await
            .map(|_| ())
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO email_change_tokens (id, user_id, new_email, token_hash, token_encrypted, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(token_id)
            .bind(user_id)
            .bind(new_email)
            .bind(token_hash)
            .bind(token_sealed)
            .bind(expires_at)
            .bind(now)
            .execute(&mut **t)
            .await
            .map(|_| ())
        }
    }
}
