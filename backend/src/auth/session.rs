use axum::{
    extract::{FromRequestParts, Request},
    http::{request::Parts, HeaderMap},
    response::Response,
};
use axum_extra::extract::CookieJar;
use serde::Serialize;
use sqlx::Either;

use crate::{app::AppState, auth::token::hash_token, db::pool::DatabasePool};

/// Session cookie 名称
///
/// `__Host-` 前缀要求 cookie 必须带 Secure、Path=/ 且不含 Domain 属性，
/// 可防止子域名伪造会话 cookie（M02-SESSION-02）。
pub const SESSION_COOKIE_NAME: &str = "__Host-bblbb_session";

/// 默认 idle 超时：30 分钟（Unix 毫秒，M01-DB-08）
pub const IDLE_TIMEOUT_MS: i64 = 30 * 60 * 1000;
/// 默认 absolute 超时：7 天（Unix 毫秒，M01-DB-08）
pub const ABSOLUTE_TIMEOUT_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// 默认 step-up 窗口：5 分钟（M02-MFA-07；配置 BBLBB__STEP_UP_WINDOW_SECS）
pub const DEFAULT_STEP_UP_WINDOW_SECS: u64 = 5 * 60;

/// 已认证的会话用户信息
#[derive(Clone, Debug, Serialize)]
pub struct SessionUser {
    pub id: String,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub status: String,
    pub display_name: Option<String>,
    pub level: i64,
    pub roles: Vec<String>,
}

/// 认证会话 — 从请求中提取的当前用户
#[derive(Clone, Debug, Default)]
pub struct AuthSession {
    pub user: Option<SessionUser>,
    pub session_id: Option<String>,
    pub csrf_token: Option<String>,
}

impl AuthSession {
    pub fn is_authenticated(&self) -> bool {
        self.user.is_some()
    }

    #[allow(clippy::result_large_err)] // AppError 为全 handler 统一错误类型，体积固定可接受
    pub fn require_auth(&self, request_id: &str) -> Result<&SessionUser, crate::error::AppError> {
        self.user.as_ref().ok_or_else(|| {
            crate::error::AppError::unauthorized("authentication required", request_id)
        })
    }
}

/// 从请求部分提取认证会话
impl FromRequestParts<AppState> for AuthSession {
    type Rejection = ();

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let jar = CookieJar::from_headers(&parts.headers);
        let session_token = jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string());

        if let Some(token) = session_token {
            if let Some(pool) = &state.db {
                match resolve_session(pool, &token).await {
                    Ok(Some((user, session_id, csrf_token))) => {
                        return Ok(AuthSession {
                            user: Some(user),
                            session_id: Some(session_id),
                            csrf_token: Some(csrf_token),
                        });
                    }
                    Ok(None) => {}
                    Err(e) => {
                        tracing::warn!(error = %e, "session resolution failed");
                    }
                }
            }
        }

        // 提取 CSRF token 从 header
        let csrf_token = parts
            .headers
            .get("x-csrf-token")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string());

        Ok(AuthSession {
            user: None,
            session_id: None,
            csrf_token,
        })
    }
}

/// 会话中间件 — 将 AuthSession 注入请求扩展
pub async fn session_middleware(mut request: Request, next: axum::middleware::Next) -> Response {
    // AuthSession 由 FromRequestParts 在需要时提取，
    // 此中间件确保每个请求都有一个默认值可用
    request.extensions_mut().insert(AuthSession::default());
    next.run(request).await
}

/// 解析 session token，返回用户和会话信息
async fn resolve_session(
    pool: &DatabasePool,
    token: &str,
) -> Result<Option<(SessionUser, String, String)>, sqlx::Error> {
    use crate::outbox::now_millis;

    let token_hash = hash_token(token);
    let now = now_millis();

    match pool {
        Either::Left(p) => {
            // 检查 session 是否有效
            let valid: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM user_sessions WHERE token_hash = ? AND revoked_at IS NULL AND idle_expires_at > ? AND absolute_expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .bind(now)
            .fetch_optional(p)
            .await?;

            if valid.is_none() {
                return Ok(None);
            }

            // 获取用户和会话信息
            let row = sqlx::query_as::<_, UserSessionRow>(
                "SELECT u.id, u.username_normalized, u.email_normalized, u.email_verified, u.status, u.display_name,
                        s.id as session_id
                 FROM users u
                 JOIN user_sessions s ON s.user_id = u.id
                 WHERE s.token_hash = ?",
            )
            .bind(&token_hash)
            .fetch_optional(p)
            .await?;

            if let Some(row) = row {
                // 实时状态检查（M02-SESSION-06）：banned/deleted 即使 Session
                // 有效也不认证——封禁/删除实时生效，不依赖后台任务
                if row.status == "banned" || row.status == "deleted" {
                    return Ok(None);
                }
                // 更新 last_seen_at 和 idle_expires_at（滑动超时）
                let new_idle = now + IDLE_TIMEOUT_MS;
                sqlx::query("UPDATE user_sessions SET last_seen_at = ?, idle_expires_at = ? WHERE token_hash = ?")
                    .bind(now)
                    .bind(new_idle)
                    .bind(&token_hash)
                    .execute(p)
                    .await?;

                let csrf = generate_csrf_token(&row.session_id, &token_hash);

                Ok(Some((
                    SessionUser {
                        id: row.id,
                        username: row.username_normalized,
                        email: row.email_normalized,
                        email_verified: row.email_verified != 0,
                        status: row.status,
                        display_name: row.display_name,
                        level: 1,
                        roles: vec![],
                    },
                    row.session_id,
                    csrf,
                )))
            } else {
                Ok(None)
            }
        }
        Either::Right(p) => {
            let valid: Option<i64> = sqlx::query_scalar(
                "SELECT 1 FROM user_sessions WHERE token_hash = ? AND revoked_at IS NULL AND idle_expires_at > ? AND absolute_expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .bind(now)
            .fetch_optional(p)
            .await?;

            if valid.is_none() {
                return Ok(None);
            }

            let row = sqlx::query_as::<_, UserSessionRow>(
                "SELECT u.id, u.username_normalized, u.email_normalized, u.email_verified, u.status, u.display_name,
                        s.id as session_id
                 FROM users u
                 JOIN user_sessions s ON s.user_id = u.id
                 WHERE s.token_hash = ?",
            )
            .bind(&token_hash)
            .fetch_optional(p)
            .await?;

            if let Some(row) = row {
                let new_idle = now + IDLE_TIMEOUT_MS;
                sqlx::query("UPDATE user_sessions SET last_seen_at = ?, idle_expires_at = ? WHERE token_hash = ?")
                    .bind(now)
                    .bind(new_idle)
                    .bind(&token_hash)
                    .execute(p)
                    .await?;

                let csrf = generate_csrf_token(&row.session_id, &token_hash);

                Ok(Some((
                    SessionUser {
                        id: row.id,
                        username: row.username_normalized,
                        email: row.email_normalized,
                        email_verified: row.email_verified != 0,
                        status: row.status,
                        display_name: row.display_name,
                        level: 1,
                        roles: vec![],
                    },
                    row.session_id,
                    csrf,
                )))
            } else {
                Ok(None)
            }
        }
    }
}

/// 创建新会话并返回 session token
pub async fn create_session(pool: &DatabasePool, user_id: &str) -> Result<String, sqlx::Error> {
    let token = crate::auth::token::generate_token();
    let token_hash = hash_token(&token);
    let session_id = uuid::Uuid::now_v7().to_string();
    let now = crate::outbox::now_millis();
    let idle_expires = now + IDLE_TIMEOUT_MS;
    let absolute_expires = now + ABSOLUTE_TIMEOUT_MS;

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at, auth_verified_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&session_id)
            .bind(user_id)
            .bind(&token_hash)
            .bind(&token_hash)
            .bind(now)
            .bind(now)
            .bind(idle_expires)
            .bind(absolute_expires)
            // 完整认证时间：登录签发会话即视为已完整认证（M02-MFA-07 step-up）
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at, auth_verified_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&session_id)
            .bind(user_id)
            .bind(&token_hash)
            .bind(&token_hash)
            .bind(now)
            .bind(now)
            .bind(idle_expires)
            .bind(absolute_expires)
            .bind(now)
            .execute(p)
            .await?;
        }
    }

    Ok(token)
}

/// 撤销会话
pub async fn revoke_session(pool: &DatabasePool, token: &str) -> Result<(), sqlx::Error> {
    let token_hash = hash_token(token);
    let now = crate::outbox::now_millis();

    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL")
                .bind(now)
                .bind(&token_hash)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE user_sessions SET revoked_at = ? WHERE token_hash = ? AND revoked_at IS NULL")
                .bind(now)
                .bind(&token_hash)
                .execute(p)
                .await?;
        }
    }

    Ok(())
}

/// 旋转会话（M02-SESSION-04）：撤销当前 token 的会话并签发新 Session token，
/// 防止 Session fixation。
///
/// 在登录、权限提升、改密和高风险重新认证后调用：旧 token 立即失效
/// （`revoked_at` + `revoke_reason`，`version` +1），新 token 由
/// [`create_session`] 签发（全新 session 行，version 从 0 开始）。
///
/// 当前 token 无有效会话时返回 `Err(sqlx::Error::RowNotFound)`。
pub async fn rotate_session(
    pool: &DatabasePool,
    current_token: &str,
    reason: &str,
) -> Result<String, sqlx::Error> {
    use crate::outbox::now_millis;

    let token_hash = hash_token(current_token);
    let now = now_millis();

    let user_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT user_id FROM user_sessions
                 WHERE token_hash = ? AND revoked_at IS NULL",
            )
            .bind(&token_hash)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT user_id FROM user_sessions
                 WHERE token_hash = ? AND revoked_at IS NULL",
            )
            .bind(&token_hash)
            .fetch_optional(p)
            .await?
        }
    };
    let Some(user_id) = user_id else {
        return Err(sqlx::Error::RowNotFound);
    };

    // 撤销旧 session（version +1，记录旋转原因）
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE user_sessions
                 SET revoked_at = ?, revoke_reason = ?, version = version + 1
                 WHERE token_hash = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(reason)
            .bind(&token_hash)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE user_sessions
                 SET revoked_at = ?, revoke_reason = ?, version = version + 1
                 WHERE token_hash = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(reason)
            .bind(&token_hash)
            .execute(p)
            .await?;
        }
    }

    // 签发新 Session（全新 token，旧 token 已失效）
    create_session(pool, &user_id).await
}

/// 设备会话列表项（M02-SESSION-05）。
#[derive(Debug, Clone, Serialize)]
pub struct DeviceSession {
    /// session 记录 ID（设备列表唯一标识，用于逐设备撤销）。
    pub id: String,
    /// 设备/UA（截断后）。
    pub user_agent: Option<String>,
    /// 创建时间（Unix 毫秒）。
    pub created_at: i64,
    /// 最近活跃（Unix 毫秒）。
    pub last_seen_at: i64,
    /// 最长有效期截止（Unix 毫秒）。
    pub absolute_expires_at: i64,
    /// Session 旋转计数。
    pub version: i64,
}

/// 列出用户的全部有效会话（设备列表，M02-SESSION-05）。
pub async fn list_sessions(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Vec<DeviceSession>, sqlx::Error> {
    match pool {
        Either::Left(p) => sqlx::query_as::<_, DeviceSessionRow>(
            "SELECT id, user_agent, created_at, last_seen_at, absolute_expires_at, version
                 FROM user_sessions
                 WHERE user_id = ? AND revoked_at IS NULL
                 ORDER BY last_seen_at DESC",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map(to_device_sessions),
        Either::Right(p) => sqlx::query_as::<_, DeviceSessionRow>(
            "SELECT id, user_agent, created_at, last_seen_at, absolute_expires_at, version
                 FROM user_sessions
                 WHERE user_id = ? AND revoked_at IS NULL
                 ORDER BY last_seen_at DESC",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map(to_device_sessions),
    }
}

/// 全部登出：撤销用户全部有效会话（M02-SESSION-05）。返回撤销条数。
pub async fn revoke_all_sessions(
    pool: &DatabasePool,
    user_id: &str,
    reason: &str,
) -> Result<u64, sqlx::Error> {
    use crate::outbox::now_millis;
    let now = now_millis();
    match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE user_sessions
                 SET revoked_at = ?, revoke_reason = ?
                 WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(user_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
        Either::Right(p) => sqlx::query(
            "UPDATE user_sessions
                 SET revoked_at = ?, revoke_reason = ?
                 WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(user_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected()),
    }
}

/// 逐设备撤销：撤销指定 session（必须属于该用户）。返回是否撤销成功。
pub async fn revoke_session_by_id(
    pool: &DatabasePool,
    user_id: &str,
    session_id: &str,
    reason: &str,
) -> Result<bool, sqlx::Error> {
    use crate::outbox::now_millis;
    let now = now_millis();
    match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE user_sessions
                 SET revoked_at = ?, revoke_reason = ?
                 WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(session_id)
        .bind(user_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected() == 1),
        Either::Right(p) => sqlx::query(
            "UPDATE user_sessions
                 SET revoked_at = ?, revoke_reason = ?
                 WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(session_id)
        .bind(user_id)
        .execute(p)
        .await
        .map(|r| r.rows_affected() == 1),
    }
}

fn to_device_sessions(rows: Vec<DeviceSessionRow>) -> Vec<DeviceSession> {
    rows.into_iter()
        .map(|r| DeviceSession {
            id: r.id,
            user_agent: r.user_agent,
            created_at: r.created_at,
            last_seen_at: r.last_seen_at,
            absolute_expires_at: r.absolute_expires_at,
            version: r.version,
        })
        .collect()
}

/// 构建 session cookie
pub fn build_session_cookie(token: &str) -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::{Cookie, SameSite};

    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::milliseconds(ABSOLUTE_TIMEOUT_MS))
        .build()
}

/// 构建清除 session cookie
///
/// 与 `build_session_cookie` 保持相同的属性（Secure/HttpOnly/SameSite/Path=/），
/// 否则 `__Host-` 前缀的 cookie 不会被清除。
pub fn build_clear_session_cookie() -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::{Cookie, SameSite};

    Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(0))
        .build()
}

/// 生成确定性 CSRF token（基于 session_id 和 token_hash）
///
/// `pub(crate)` 供 CSRF 防护中间件派生期望 token 使用。
pub(crate) fn generate_csrf_token(session_id: &str, token_hash: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(session_id.as_bytes());
    hasher.update(b":");
    hasher.update(token_hash.as_bytes());
    hasher.update(b":csrf");
    hex::encode(hasher.finalize())
}

// ─────────────────────────── 近期认证 / step-up ───────────────────────────

/// step-up 判定（M02-MFA-07）：`auth_verified_at`（Unix 毫秒）距今是否超过
/// 窗口（秒）。`None`（从未完整认证）→ 需要 step-up（fail closed）。
pub fn step_up_required(auth_verified_at_ms: Option<i64>, now_secs: u64, window_secs: u64) -> bool {
    let Some(verified_ms) = auth_verified_at_ms else {
        return true;
    };
    let verified_secs = (verified_ms / 1000) as u64;
    now_secs.saturating_sub(verified_secs) > window_secs
}

/// 标记近期认证：把当前会话的 `auth_verified_at` 刷新为 now。
///
/// 在高风险操作（改密、停用 MFA、角色提升、退款、密钥/Secret 操作）完成
/// 重认证后调用；会话无效/已撤销返回 `Err(sqlx::Error::RowNotFound)`。
pub async fn mark_step_up(pool: &DatabasePool, session_token: &str) -> Result<(), sqlx::Error> {
    let token_hash = hash_token(session_token);
    let now = crate::outbox::now_millis();
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE user_sessions SET auth_verified_at = ?
             WHERE token_hash = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(&token_hash)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE user_sessions SET auth_verified_at = ?
             WHERE token_hash = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(&token_hash)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if affected != 1 {
        return Err(sqlx::Error::RowNotFound);
    }
    Ok(())
}

/// 会话是否要求 step-up（M02-MFA-07）。
///
/// 会话无效/已过期/已撤销一律视为需要 step-up（fail closed）。
pub async fn is_step_up_required_for_session(
    pool: &DatabasePool,
    session_token: &str,
    window_secs: u64,
) -> Result<bool, sqlx::Error> {
    let token_hash = hash_token(session_token);
    let now = crate::outbox::now_millis();
    let verified: Option<Option<i64>> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT auth_verified_at FROM user_sessions
             WHERE token_hash = ? AND revoked_at IS NULL
               AND idle_expires_at > ? AND absolute_expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .bind(now)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT auth_verified_at FROM user_sessions
             WHERE token_hash = ? AND revoked_at IS NULL
               AND idle_expires_at > ? AND absolute_expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .bind(now)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(step_up_required(
        verified.flatten(),
        (now / 1000) as u64,
        window_secs,
    ))
}

/// 从 HeaderMap 获取 request_id
///
/// 与 `middleware::request_id` 中间件的判定保持一致：仅接受合法请求 ID
/// （非空、≤128 字节、纯 ASCII、无控制字符），非法/缺失时回退为 "unknown"。
/// 中间件已将合法 ID 注入请求扩展，此处仅作无扩展场景下的兜底。
pub fn get_request_id(headers: &HeaderMap) -> String {
    headers
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|value| crate::middleware::request_id::is_valid_request_id(value))
        .unwrap_or("unknown")
        .to_string()
}

/// 数据库行结构
#[derive(sqlx::FromRow)]
struct UserSessionRow {
    id: String,
    username_normalized: String,
    email_normalized: String,
    email_verified: i64,
    status: String,
    display_name: Option<String>,
    session_id: String,
}

/// 设备列表行结构。
#[derive(sqlx::FromRow)]
struct DeviceSessionRow {
    id: String,
    user_agent: Option<String>,
    created_at: i64,
    last_seen_at: i64,
    absolute_expires_at: i64,
    version: i64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum_extra::extract::cookie::SameSite;

    /// M02-SESSION-02：Session token 至少 256 bit 熵（32 字节），
    /// 数据库只存 SHA-256 hash（64 hex）。
    #[test]
    fn session_token_has_256_bit_entropy_and_only_hash_is_stored() {
        let token = crate::auth::token::generate_token();
        // base64 URL-safe 无填充：32 字节 → 43 字符（≥40）
        assert!(token.len() >= 40, "token 长度 {} < 40，熵不足", token.len());

        let hash = crate::auth::token::hash_token(&token);
        assert_eq!(hash.len(), 64, "数据库必须只存 64 位 hex SHA-256");
        assert_ne!(token, hash);
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash 必须为 hex，而非可逆明文"
        );
    }

    /// M02-SESSION-02：`__Host-` 前缀 Cookie 必须带 Secure、Path=/ 且无 Domain。
    #[test]
    fn session_cookie_uses_host_prefix_with_secure_attributes() {
        let cookie = build_session_cookie("tok");
        assert_eq!(cookie.name(), SESSION_COOKIE_NAME);
        assert!(
            cookie.name().starts_with("__Host-"),
            "必须使用 __Host- 前缀"
        );
        assert_eq!(cookie.path().unwrap_or(""), "/");
        assert!(cookie.secure().unwrap_or(false), "__Host- 要求 Secure");
        assert!(
            cookie.http_only().unwrap_or(false),
            "Session token 必须 HttpOnly"
        );
        match cookie.same_site() {
            Some(same) => assert_eq!(same, SameSite::Lax, "SameSite=Lax 防跨站携带"),
            None => panic!("必须显式设置 SameSite"),
        }
        assert!(
            cookie.domain().is_none(),
            "__Host- 前缀禁止 Domain 属性（防子域伪造）"
        );
        assert!(
            cookie.max_age().is_some(),
            "Cookie 必须有 max-age（absolute timeout）"
        );
    }

    /// 清除 cookie 与设置 cookie 属性一致（否则 __Host- cookie 无法清除）。
    #[test]
    fn clear_cookie_matches_session_cookie_attributes() {
        let clear = build_clear_session_cookie();
        assert_eq!(clear.name(), SESSION_COOKIE_NAME);
        assert_eq!(clear.path().unwrap_or(""), "/");
        assert!(clear.secure().unwrap_or(false));
        assert!(clear.http_only().unwrap_or(false));
        match clear.same_site() {
            Some(same) => assert_eq!(same, SameSite::Lax),
            None => panic!("必须显式设置 SameSite"),
        }
        assert!(clear.domain().is_none());
        assert_eq!(clear.max_age().unwrap_or_default(), time::Duration::ZERO);
    }

    /// CSRF token 由 session_id + token_hash 确定性派生，同一会话稳定。
    #[test]
    fn csrf_token_is_deterministic_per_session() {
        let a = generate_csrf_token("s1", "h1");
        let b = generate_csrf_token("s1", "h1");
        let c = generate_csrf_token("s1", "h2");
        assert_eq!(a, b);
        assert_ne!(a, c, "token_hash 变化必须改变派生 CSRF token");
        assert_eq!(a.len(), 64, "CSRF token 为 SHA-256 hex");
    }
}
