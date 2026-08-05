use axum::{
    extract::{FromRequestParts, Request},
    http::{request::Parts, HeaderMap},
    response::Response,
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::Serialize;
use sqlx::Either;

use crate::{app::AppState, auth::token::hash_token, db::pool::DatabasePool};

/// Session cookie 名称
///
/// `__Host-` 前缀要求 cookie 必须带 Secure、Path=/ 且不含 Domain 属性，
/// 可防止子域名伪造会话 cookie（M02-SESSION-02）。
pub const SESSION_COOKIE_NAME: &str = "__Host-bblbb_session";

/// 默认 idle 超时：30 分钟（秒）
pub const IDLE_TIMEOUT_SECS: i64 = 30 * 60;
/// 默认 absolute 超时：7 天（秒）
pub const ABSOLUTE_TIMEOUT_SECS: i64 = 7 * 24 * 60 * 60;

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
    let token_hash = hash_token(token);
    let now = Utc::now().timestamp();

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
                // 更新 last_seen_at 和 idle_expires_at
                let new_idle = now + IDLE_TIMEOUT_SECS;
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
                let new_idle = now + IDLE_TIMEOUT_SECS;
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
    let now = Utc::now().timestamp();
    let idle_expires = now + IDLE_TIMEOUT_SECS;
    let absolute_expires = now + ABSOLUTE_TIMEOUT_SECS;

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&session_id)
            .bind(user_id)
            .bind(&token_hash)
            .bind(&token_hash)
            .bind(now)
            .bind(now)
            .bind(idle_expires)
            .bind(absolute_expires)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&session_id)
            .bind(user_id)
            .bind(&token_hash)
            .bind(&token_hash)
            .bind(now)
            .bind(now)
            .bind(idle_expires)
            .bind(absolute_expires)
            .execute(p)
            .await?;
        }
    }

    Ok(token)
}

/// 撤销会话
pub async fn revoke_session(pool: &DatabasePool, token: &str) -> Result<(), sqlx::Error> {
    let token_hash = hash_token(token);
    let now = Utc::now().timestamp();

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

/// 构建 session cookie
pub fn build_session_cookie(token: &str) -> axum_extra::extract::cookie::Cookie<'static> {
    use axum_extra::extract::cookie::{Cookie, SameSite};

    Cookie::build((SESSION_COOKIE_NAME, token.to_string()))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::seconds(ABSOLUTE_TIMEOUT_SECS))
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
