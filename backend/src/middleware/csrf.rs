//! CSRF 防护中间件
//!
//! 对状态变更请求（POST/PUT/PATCH/DELETE）分两档校验 `X-CSRF-Token`：
//!
//! - 携带会话 Cookie 的写请求：必须提供与当前会话派生一致的
//!   synchronizer token，否则返回 403 Problem JSON（M02-SESSION-07）。
//! - 无会话 Cookie 的预认证写路径（login/register/verify-email/
//!   resend-verification/password-reset 及其 confirm）：必须携带服务端
//!   可回溯的匿名预认证 CSRF 状态——`__Host-bblbb_csrf` cookie 与匹配的
//!   `X-CSRF-Token`，否则 403（M02-SESSION-08，防 login CSRF）。
//! - 其余无会话写请求（如 Bearer-only 端点）：宽松策略放行。
//!
//! 已认证请求的期望 token 由会话记录中的 `session_id` 与 `csrf_secret_hash`
//! 确定性派生（与 `auth::session::generate_csrf_token` 一致），
//! 预认证请求同理由 `preauth_csrf_tokens` 的 `(id, csrf_secret_hash)` 派生，
//! 因此攻击者即使能伪造请求也无法在不知晓记录秘密的情况下构造合法 token。

use axum::{
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use sqlx::Either;

use crate::{
    app::AppState,
    auth::{
        preauth::{resolve_preauth, PREAUTH_COOKIE_NAME},
        session::{generate_csrf_token, get_request_id, SESSION_COOKIE_NAME},
        token::hash_token,
    },
    db::pool::DatabasePool,
    error::Problem,
    middleware::request_id::RequestId,
};

/// 需要 CSRF 校验的 HTTP 方法
fn is_state_changing(method: &Method) -> bool {
    matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    )
}

/// CSRF 校验中间件
pub async fn csrf_protection(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    // 只保护状态变更方法，幂等读请求直接放行
    if !is_state_changing(request.method()) {
        return next.run(request).await;
    }

    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|rid| rid.0.clone())
        .unwrap_or_else(|| get_request_id(request.headers()));

    let jar = CookieJar::from_headers(request.headers());
    let path = request.uri().path();

    // 无数据库时不存在任何会话/预认证状态：等同无 Cookie 场景放行
    // （与 M02-SESSION-07 会话分支行为一致；真实部署恒有数据库）。
    let Some(pool) = &state.db else {
        return next.run(request).await;
    };

    // 携带会话 Cookie → 会话绑定 synchronizer token 校验（已认证写请求）
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        return validate_session_csrf(pool, request, session_cookie.value(), &request_id, next)
            .await;
    }

    // 无会话 Cookie 的预认证写路径（login/register/verify-email/resend/
    // password-reset）→ 必须携带服务端可回溯的匿名预认证 CSRF 状态
    // （`__Host-bblbb_csrf` cookie + 匹配的 X-CSRF-Token），否则 403——
    // 防 login CSRF（M02-SESSION-08，SECURITY.md §4）。
    if is_preauth_write_path(path) {
        return validate_preauth_csrf(pool, request, &jar, &request_id, next).await;
    }

    // 其余无会话写请求（如 Bearer-only 端点）：宽松策略放行，由路由层处理
    next.run(request).await
}

/// 会话绑定 synchronizer token 校验（原逻辑，M02-SESSION-07）。
async fn validate_session_csrf(
    pool: &DatabasePool,
    request: Request,
    session_cookie: &str,
    request_id: &str,
    next: Next,
) -> Response {
    // 解析会话，派生期望的 CSRF token
    let expected = match resolve_csrf_secret(pool, session_cookie).await {
        Ok(Some((session_id, csrf_secret_hash))) => {
            generate_csrf_token(&session_id, &csrf_secret_hash)
        }
        Ok(None) => {
            // 会话无效/已过期：等同无 Cookie 场景，交由路由层按未认证处理
            tracing::debug!(
                request_id = %request_id,
                "csrf: session cookie present but session invalid, skipping check"
            );
            return next.run(request).await;
        }
        Err(error) => {
            tracing::warn!(
                request_id = %request_id,
                error = %error,
                "csrf: session resolution failed, skipping check"
            );
            return next.run(request).await;
        }
    };

    let provided = request
        .headers()
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok());

    let valid = match provided {
        Some(value) => constant_time_eq(value, &expected),
        None => false,
    };

    if !valid {
        tracing::warn!(request_id = %request_id, "csrf validation failed");
        return csrf_rejected(request_id);
    }

    next.run(request).await
}

/// 匿名预认证 CSRF 校验（M02-SESSION-08）：预认证写请求必须携带
/// `__Host-bblbb_csrf` cookie 与匹配的 `X-CSRF-Token`。任一缺失/不匹配/
/// 过期 → 403 `csrf_failed`（fail closed，防 login CSRF）。
async fn validate_preauth_csrf(
    pool: &DatabasePool,
    request: Request,
    jar: &CookieJar,
    request_id: &str,
    next: Next,
) -> Response {
    let Some(preauth_cookie) = jar.get(PREAUTH_COOKIE_NAME) else {
        tracing::warn!(
            request_id = %request_id,
            "csrf: preauth write without preauth cookie"
        );
        return csrf_rejected(request_id);
    };

    let expected = match resolve_preauth(pool, preauth_cookie.value()).await {
        Ok(Some((id, csrf_secret_hash))) => generate_csrf_token(&id, &csrf_secret_hash),
        Ok(None) => {
            tracing::warn!(
                request_id = %request_id,
                "csrf: preauth state missing or expired"
            );
            return csrf_rejected(request_id);
        }
        Err(error) => {
            tracing::warn!(
                request_id = %request_id,
                error = %error,
                "csrf: preauth resolution failed"
            );
            return csrf_rejected(request_id);
        }
    };

    let provided = request
        .headers()
        .get("x-csrf-token")
        .and_then(|value| value.to_str().ok());

    let valid = match provided {
        Some(value) => constant_time_eq(value, &expected),
        None => false,
    };

    if !valid {
        tracing::warn!(
            request_id = %request_id,
            "csrf: preauth token missing or mismatch"
        );
        return csrf_rejected(request_id);
    }

    next.run(request).await
}

/// 预认证写路径：与 OpenAPI `x-csrf-context: preauth` 标记一一对应。
/// 这些端点无会话 Cookie 时也必须携带匿名预认证 CSRF 状态。
fn is_preauth_write_path(path: &str) -> bool {
    matches!(
        path,
        "/api/v1/auth/login"
            | "/api/v1/auth/register"
            | "/api/v1/auth/verify-email"
            | "/api/v1/auth/resend-verification"
            | "/api/v1/auth/password-reset"
            | "/api/v1/auth/password-reset/confirm"
    )
}

/// 根据会话 token 解析出会话 ID 与 CSRF 秘密哈希
async fn resolve_csrf_secret(
    pool: &DatabasePool,
    token: &str,
) -> Result<Option<(String, String)>, sqlx::Error> {
    let token_hash = hash_token(token);
    let now = Utc::now().timestamp();

    let row: Option<SessionCsrfRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, SessionCsrfRow>(
                "SELECT id, csrf_secret_hash FROM user_sessions
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
            sqlx::query_as::<_, SessionCsrfRow>(
                "SELECT id, csrf_secret_hash FROM user_sessions
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

    Ok(row.map(|r| (r.id, r.csrf_secret_hash)))
}

/// 常量时间字符串比较，避免时序侧信道泄漏 token 信息
fn constant_time_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b) {
        diff |= x ^ y;
    }
    diff == 0
}

/// 403 Problem JSON 响应
fn csrf_rejected(request_id: &str) -> Response {
    let problem = Problem {
        type_uri: "about:blank",
        title: "Forbidden",
        status: StatusCode::FORBIDDEN.as_u16(),
        code: "csrf_failed",
        detail: "CSRF token missing or invalid".to_string(),
        instance: None,
        request_id: request_id.to_string(),
        errors: None,
    };
    (
        StatusCode::FORBIDDEN,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(problem),
    )
        .into_response()
}

/// 数据库行结构
#[derive(sqlx::FromRow)]
struct SessionCsrfRow {
    id: String,
    csrf_secret_hash: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_changing_methods_are_covered() {
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(is_state_changing(&method), "{method} should be protected");
        }
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(!is_state_changing(&method), "{method} should pass through");
        }
    }

    #[test]
    fn constant_time_eq_matches_and_mismatches() {
        assert!(constant_time_eq("abc123", "abc123"));
        assert!(constant_time_eq("", ""));
        assert!(!constant_time_eq("abc123", "abc124"));
        assert!(!constant_time_eq("abc", "abcd"));
        assert!(!constant_time_eq("abc123", "ABC123"));
    }

    /// 预认证写路径必须完整覆盖六条预认证端点（与 OpenAPI
    /// `x-csrf-context: preauth` 标记一一对应，M02-SESSION-08）。
    #[test]
    fn preauth_write_paths_are_covered() {
        for path in [
            "/api/v1/auth/login",
            "/api/v1/auth/register",
            "/api/v1/auth/verify-email",
            "/api/v1/auth/resend-verification",
            "/api/v1/auth/password-reset",
            "/api/v1/auth/password-reset/confirm",
        ] {
            assert!(is_preauth_write_path(path), "{path} 必须强制预认证 CSRF");
        }
        // Session 端点到 /api/v1/auth/session(s) 不属预认证（带会话 Cookie 走会话校验）
        for path in [
            "/api/v1/auth/session",
            "/api/v1/auth/sessions",
            "/api/v1/auth/sessions/abc",
            "/api/v1/auth/csrf",
            "/api/v1/users/me",
            "/api/v1/posts",
        ] {
            assert!(!is_preauth_write_path(path), "{path} 不应是预认证写路径");
        }
    }
}
