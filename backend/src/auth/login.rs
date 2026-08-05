//! 登录服务（M02-SESSION-03）。
//!
//! 行为契约：
//! - **常量时间失败**：无论账号是否存在都执行一次 Argon2id 验证
//!   （不存在时对固定 dummy hash 验证），错误响应统一
//!   [`LoginError::InvalidCredentials`]——不区分账号不存在、密码错误或
//!   账号状态（banned/deleted 一律 invalid credentials，防账号枚举）；
//! - **账号/IP 双维度限流**（docs/SECURITY.md §16）：每 IP 10 次/分钟；
//!   每账号连续失败 5 次短时锁定 10 分钟（`users.failed_login_count` +
//!   `locked_until` 持久化，跨进程生效）；锁定命中返回 429 `RateLimited`；
//! - 登录成功重置连续失败计数并创建 Session（`create_session`，token 只存
//!   hash，`__Host-` Cookie 由 handler 签发）。

use sqlx::Either;

use crate::{
    auth::{
        password::{verify_password, VerifyResult},
        security_notify::{has_device_seen, notify_new_device},
        session::create_session,
    },
    db::pool::DatabasePool,
    outbox::now_millis,
    ratelimit::{RateLimitStatus, RateLimiter},
};

/// 登录限流参数（生产默认：IP 10/分钟、连续失败 5 次锁定 10 分钟）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoginLimits {
    pub ip_limit: u32,
    pub ip_window_ms: i64,
    pub fail_threshold: u32,
    pub lockout_ms: i64,
}

impl Default for LoginLimits {
    fn default() -> Self {
        Self {
            ip_limit: 10,
            ip_window_ms: 60 * 1000,
            fail_threshold: 5,
            lockout_ms: 10 * 60 * 1000,
        }
    }
}

/// 登录成功输出。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginOutcome {
    pub user_id: String,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub status: String,
    pub display_name: Option<String>,
    /// 新 Session token（仅此一次明文，handler 写入 `__Host-` Cookie）。
    pub session_token: String,
    /// 用户启用 TOTP：密码验证成功但未签发会话，需第二步
    /// `POST /api/v1/auth/login/mfa` 完成登录（M02-UX-03）。
    pub mfa_required: bool,
}

/// 登录错误。
#[derive(Debug)]
pub enum LoginError {
    /// 账号不存在 / 密码错误 / 账号被禁：统一响应（防枚举）。
    InvalidCredentials,
    /// IP 限流或账号锁定（handler 返回 429 + Retry-After）。
    RateLimited {
        retry_after_secs: u64,
        limit: u32,
        remaining: u32,
        reset_at_unix_secs: i64,
    },
    Database(sqlx::Error),
}

impl std::fmt::Display for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoginError::InvalidCredentials => write!(f, "invalid credentials"),
            LoginError::RateLimited { .. } => write!(f, "too many login attempts, try again later"),
            LoginError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for LoginError {}

/// MfaError::Database(String) → LoginError::Database（保持错误类型一致）。
fn db_err_from_mfa(e: crate::auth::mfa::MfaError) -> LoginError {
    let msg = match e {
        crate::auth::mfa::MfaError::Database(s) => s,
        other => other.to_string(),
    };
    LoginError::Database(sqlx::Error::protocol(msg))
}

/// 常量时间兜底 hash：账号不存在时对它验证，保证两条路径耗时一致。
const DUMMY_HASH: &str = "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA";

/// 登录：IP 限流 → 常量时间验证 → 失败计数/锁定 or 成功重置 + 创建 Session。
///
/// `ua`：设备 User-Agent（可选，用于新设备登录安全通知 M02-MFA-08）。
/// `request_id`：审计/通知关联 ID。
/// `#[allow(clippy::too_many_arguments)]`：登录契约参数均为领域必需
/// （身份、凭据、限流、设备指纹、关联 ID），保持平铺便于调用方逐项传入。
#[allow(clippy::too_many_arguments)]
pub async fn login_user(
    pool: &DatabasePool,
    limiter: &RateLimiter,
    identifier_normalized: &str,
    password: &str,
    ip: &str,
    ua: Option<&str>,
    request_id: &str,
    limits: &LoginLimits,
) -> Result<LoginOutcome, LoginError> {
    let now = now_millis();

    // IP 维度限流（所有登录请求计数）
    let ip_key = format!("login:ip:{ip}");
    let ip_status = limiter.check(&ip_key, limits.ip_limit, limits.ip_window_ms, now);
    if !ip_status.allowed {
        return Err(rate_limited_from(ip_status));
    }

    // 查找用户（用户名或邮箱均可）
    let row: Option<LoginUserRow> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT id, username_normalized, email_normalized, email_verified, status,
                    display_name, password_hash, failed_login_count, locked_until
             FROM users WHERE email_normalized = ? OR username_normalized = ?",
        )
        .bind(identifier_normalized)
        .bind(identifier_normalized)
        .fetch_optional(p)
        .await
        .map_err(LoginError::Database)?,
        Either::Right(p) => sqlx::query_as(
            "SELECT id, username_normalized, email_normalized, email_verified, status,
                    display_name, password_hash, failed_login_count, locked_until
             FROM users WHERE email_normalized = ? OR username_normalized = ?",
        )
        .bind(identifier_normalized)
        .bind(identifier_normalized)
        .fetch_optional(p)
        .await
        .map_err(LoginError::Database)?,
    };

    // 账号锁定检查（持久化，跨进程生效）
    if let Some(user) = &row {
        if let Some(locked_until) = user.locked_until {
            if locked_until > now {
                let delta = (locked_until - now).max(0);
                let retry_after_secs = (delta.div_euclid(1000)
                    + if delta.rem_euclid(1000) != 0 { 1 } else { 0 })
                .max(1) as u64;
                return Err(LoginError::RateLimited {
                    retry_after_secs,
                    limit: limits.fail_threshold,
                    remaining: 0,
                    reset_at_unix_secs: locked_until / 1000,
                });
            }
        }
    }

    // 常量时间验证：总是对真实 hash 或 dummy hash 执行 Argon2id
    let hash = row
        .as_ref()
        .map(|u| u.password_hash.as_str())
        .unwrap_or(DUMMY_HASH);
    let verify_ok = matches!(verify_password(password, hash), VerifyResult::Ok);

    let Some(user) = row else {
        // 账号不存在：与密码错误统一响应（已执行 dummy 验证，耗时一致）
        return Err(LoginError::InvalidCredentials);
    };

    if verify_ok {
        // 账号被禁（banned/deleted）→ 统一 invalid credentials，不泄漏状态
        if user.status == "banned" || user.status == "deleted" {
            return Err(LoginError::InvalidCredentials);
        }
        // 新设备判定（M02-MFA-08）：在 create_session 之前查询，避免新会话
        // 自身计入“已见设备”。UA 为空视为无法判定（不通知）。
        let ua_clean = ua.map(str::trim).filter(|u| !u.is_empty());
        let is_new_device = match ua_clean {
            Some(ua) => !has_device_seen(pool, &user.id, ua)
                .await
                .map_err(LoginError::Database)?,
            None => false,
        };
        // 登录成功：重置连续失败计数
        reset_failure_count(pool, &user.id, now)
            .await
            .map_err(LoginError::Database)?;

        // 启用 TOTP：密码已验，但暂不签发会话——由 handler 签发一次性
        // challenge，第二步 /auth/login/mfa 完成后才签发（M02-UX-03）。
        if crate::auth::mfa::has_confirmed_totp(pool, &user.id)
            .await
            .map_err(db_err_from_mfa)?
        {
            return Ok(LoginOutcome {
                user_id: user.id,
                username: user.username_normalized,
                email: user.email_normalized,
                email_verified: user.email_verified != 0,
                status: user.status,
                display_name: user.display_name,
                session_token: String::new(),
                mfa_required: true,
            });
        }

        let session_token = create_session(pool, &user.id, ua_clean)
            .await
            .map_err(LoginError::Database)?;
        if is_new_device {
            // 安全通知尽力而为：失败不阻断登录（记 warn，后续审计可追踪）
            if let Some(ua) = ua_clean {
                if let Err(e) = notify_new_device(pool, &user.id, ua, request_id).await {
                    tracing::warn!(user_id = %user.id, error = %e, "new device security notification failed");
                }
            }
        }
        return Ok(LoginOutcome {
            user_id: user.id,
            username: user.username_normalized,
            email: user.email_normalized,
            email_verified: user.email_verified != 0,
            status: user.status,
            display_name: user.display_name,
            session_token,
            mfa_required: false,
        });
    }

    // 密码错误：递增连续失败计数，达到阈值触发短时锁定
    let new_count = user.failed_login_count + 1;
    let new_locked = if new_count >= limits.fail_threshold as i64 {
        Some(now + limits.lockout_ms)
    } else {
        None
    };
    update_failure_count(pool, &user.id, new_count, new_locked, now)
        .await
        .map_err(LoginError::Database)?;

    Err(LoginError::InvalidCredentials)
}

fn rate_limited_from(status: RateLimitStatus) -> LoginError {
    LoginError::RateLimited {
        retry_after_secs: status.retry_after_secs,
        limit: status.limit,
        remaining: status.remaining,
        reset_at_unix_secs: status.reset_at_ms / 1000,
    }
}

async fn reset_failure_count(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users SET failed_login_count = 0, locked_until = NULL, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE users SET failed_login_count = 0, locked_until = NULL, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ())
        }
    }
}

async fn update_failure_count(
    pool: &DatabasePool,
    user_id: &str,
    new_count: i64,
    new_locked: Option<i64>,
    now: i64,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users SET failed_login_count = ?, locked_until = ?, updated_at = ? WHERE id = ?",
            )
            .bind(new_count)
            .bind(new_locked)
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE users SET failed_login_count = ?, locked_until = ?, updated_at = ? WHERE id = ?",
            )
            .bind(new_count)
            .bind(new_locked)
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map(|_| ())
        }
    }
}

/// 登录查询行。
#[derive(sqlx::FromRow)]
struct LoginUserRow {
    id: String,
    username_normalized: String,
    email_normalized: String,
    email_verified: i64,
    status: String,
    display_name: Option<String>,
    password_hash: String,
    failed_login_count: i64,
    locked_until: Option<i64>,
}
