//! 邮箱验证 token 消费服务（M02-IDENTITY-07/09）。
//!
//! 在**同一事务**内实现：
//! - **过期**：token 24 小时过期（`expires_at`），过期后一律拒绝；
//! - **一次消费**：`UPDATE ... WHERE consumed_at IS NULL` 原子消费，
//!   同一 token 只能成功消费一次；
//! - **并发消费唯一成功**：消费用 UPDATE 驱动（非「先查后改」），数据库行锁
//!   保证并发请求中恰好一个赢得 `rows_affected == 1`，其余返回
//!   [`VerifyEmailError::InvalidOrExpired`] 并回滚；
//! - **旧 token 失效**：激活成功后同用户其余未消费 token 一并标记 consumed，
//!   防止多 token 交替验证；
//! - **激活 + 审计 + 领域事件**（M02-IDENTITY-09）：激活只对 pending 用户，
//!   写 `auth.email_verified` 审计与 `user.status_changed.v1` Outbox 事件；
//!   可选新用户冷静期（`cooldown_secs` > 0 时计算 `new_user_cooldown_until`，
//!   写入审计 metadata 与事件 payload）；
//! - 失败（不存在/已消费/过期/用户非 pending）统一返回
//!   [`VerifyEmailError::InvalidOrExpired`]——不区分具体原因，防 token 枚举。

use serde_json::json;
use sqlx::Either;

use crate::{
    audit::AuditEntry,
    auth::token::hash_token,
    db::pool::DatabasePool,
    events,
    outbox::{self, OutboxTx},
};

/// 验证成功的输出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifyEmailOutcome {
    pub user_id: String,
    /// `user.status_changed.v1` 领域事件（Outbox）ID。
    pub event_id: String,
}

/// 验证错误：无效与已消费/过期共享同一变体（响应不区分，防枚举）。
#[derive(Debug)]
pub enum VerifyEmailError {
    /// 不存在 / 已消费 / 过期 / 用户状态不允许激活。
    InvalidOrExpired,
    /// 其他数据库错误。
    Database(sqlx::Error),
}

impl std::fmt::Display for VerifyEmailError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VerifyEmailError::InvalidOrExpired => {
                write!(f, "invalid or expired verification token")
            }
            VerifyEmailError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for VerifyEmailError {}

/// 验证邮箱：单事务原子消费 token 并激活 pending 用户。
///
/// `cooldown_secs`：可选新用户冷静期时长（秒）；> 0 时计算
/// `new_user_cooldown_until = 激活时间 + cooldown_secs`，写入审计 metadata
/// 与领域事件 payload（默认 0 = 关闭，见 M02-IDENTITY-09）。
pub async fn verify_email_token(
    pool: &DatabasePool,
    token: &str,
    cooldown_secs: i64,
    request_id: &str,
) -> Result<VerifyEmailOutcome, VerifyEmailError> {
    let mut tx = begin_tx(pool).await.map_err(VerifyEmailError::Database)?;

    let token_hash = hash_token(token);
    let now = outbox::now_millis();

    // 事务内读 token（随 UPDATE 原子消费，避免 TOCTOU）
    let row: Option<(String, i64)> = match &mut tx {
        Either::Left(t) => sqlx::query_as(
            "SELECT user_id, expires_at FROM email_verification_tokens WHERE token_hash = ?",
        )
        .bind(&token_hash)
        .fetch_optional(&mut **t)
        .await
        .map_err(VerifyEmailError::Database)?,
        Either::Right(t) => sqlx::query_as(
            "SELECT user_id, expires_at FROM email_verification_tokens WHERE token_hash = ?",
        )
        .bind(&token_hash)
        .fetch_optional(&mut **t)
        .await
        .map_err(VerifyEmailError::Database)?,
    };
    let Some((user_id, expires_at)) = row else {
        return Err(VerifyEmailError::InvalidOrExpired); // 回滚（无写入）
    };
    if expires_at <= now {
        return Err(VerifyEmailError::InvalidOrExpired); // 过期
    }

    // 原子消费：并发下只有一个请求赢得 rows_affected == 1
    let consumed_rows = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE email_verification_tokens
                 SET consumed_at = ? WHERE token_hash = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(&token_hash)
        .execute(&mut **t)
        .await
        .map_err(VerifyEmailError::Database)?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE email_verification_tokens
                 SET consumed_at = ? WHERE token_hash = ? AND consumed_at IS NULL",
        )
        .bind(now)
        .bind(&token_hash)
        .execute(&mut **t)
        .await
        .map_err(VerifyEmailError::Database)?
        .rows_affected(),
    };
    if consumed_rows != 1 {
        return Err(VerifyEmailError::InvalidOrExpired); // 已被并发消费/重复请求
    }

    // 激活 pending 用户（幂等防线：非 pending 不激活）
    let activated_rows = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE users
                 SET email_verified = 1, email_verified_at = ?, status = 'active', updated_at = ?
                 WHERE id = ? AND status = 'pending'",
        )
        .bind(now)
        .bind(now)
        .bind(&user_id)
        .execute(&mut **t)
        .await
        .map_err(VerifyEmailError::Database)?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE users
                 SET email_verified = 1, email_verified_at = ?, status = 'active', updated_at = ?
                 WHERE id = ? AND status = 'pending'",
        )
        .bind(now)
        .bind(now)
        .bind(&user_id)
        .execute(&mut **t)
        .await
        .map_err(VerifyEmailError::Database)?
        .rows_affected(),
    };
    if activated_rows != 1 {
        // 用户缺失或状态不允许 → 回滚消费，返回统一错误
        return Err(VerifyEmailError::InvalidOrExpired);
    }

    // 可选新用户冷静期（M02-IDENTITY-09）：cooldown_secs > 0 时计算到期时间
    let cooldown_until = if cooldown_secs > 0 {
        Some(now.saturating_add(cooldown_secs * 1000))
    } else {
        None
    };
    let mut metadata = json!({
        "from_status": "pending",
        "to_status": "active",
        "email_verified_at": now,
    });
    let mut payload = json!({
        "user_id": &user_id,
        "from_status": "pending",
        "to_status": "active",
        "email_verified_at": now,
    });
    if let Some(until) = cooldown_until {
        metadata["new_user_cooldown_until"] = json!(until);
        payload["new_user_cooldown_until"] = json!(until);
    }

    // 审计 + 领域事件（与激活同事务提交）
    AuditEntry::user_action(&user_id, "auth.email_verified")
        .with_target("user", &user_id)
        .with_request_id(request_id)
        .with_metadata(metadata)
        .record_in_tx(&mut tx)
        .await
        .map_err(VerifyEmailError::Database)?;

    let event_id = outbox::enqueue_in_tx(&mut tx, events::types::USER_STATUS_CHANGED, payload)
        .await
        .map_err(VerifyEmailError::Database)?;

    // 旧 token 失效：同用户其余未消费 token 一并标记 consumed
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "UPDATE email_verification_tokens
                 SET consumed_at = ? WHERE user_id = ? AND consumed_at IS NULL",
            )
            .bind(now)
            .bind(&user_id)
            .execute(&mut **t)
            .await
            .map_err(VerifyEmailError::Database)?
            .rows_affected();
        }
        Either::Right(t) => {
            sqlx::query(
                "UPDATE email_verification_tokens
                 SET consumed_at = ? WHERE user_id = ? AND consumed_at IS NULL",
            )
            .bind(now)
            .bind(&user_id)
            .execute(&mut **t)
            .await
            .map_err(VerifyEmailError::Database)?
            .rows_affected();
        }
    };

    commit_tx(tx).await.map_err(VerifyEmailError::Database)?;

    Ok(VerifyEmailOutcome { user_id, event_id })
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
