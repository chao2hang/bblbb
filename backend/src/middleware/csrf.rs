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
    http::{header, HeaderMap, Method, StatusCode},
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
    config::AppConfig,
    db::pool::DatabasePool,
    error::Problem,
    middleware::{host_origin::origin_allowed, request_id::RequestId},
};

/// 需要 CSRF 校验的 HTTP 方法
fn is_state_changing(method: &Method) -> bool {
    matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    )
}

/// 请求是否携带 `Authorization: Bearer` 令牌（M02-SESSION-10）。
/// Bearer 身份不依赖 Cookie，跨站无法伪造，因此不适用 Cookie CSRF 校验。
fn has_bearer_token(headers: &HeaderMap) -> bool {
    headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            let value = value.trim_start();
            value.len() > 7
                && value
                    .get(..7)
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case("bearer "))
        })
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

    // Bearer-only：请求携带 `Authorization: Bearer` 且无会话 Cookie → 不适用
    // CSRF（M02-SESSION-10，SECURITY.md §4：Bearer Token API 不依赖 Cookie 时
    // 不要求 CSRF，但必须防 Token 泄漏）。Bearer 身份不经 Cookie 携带，跨站
    // 无法伪造该头（跨源 fetch 触发 CORS 预检且默认 CORS 关闭），故放行。
    if jar.get(SESSION_COOKIE_NAME).is_none() && has_bearer_token(request.headers()) {
        return next.run(request).await;
    }

    // 无数据库时不存在任何会话/预认证状态：等同无 Cookie 场景放行
    // （与 M02-SESSION-07 会话分支行为一致；真实部署恒有数据库）。
    let Some(pool) = &state.db else {
        return next.run(request).await;
    };

    // 携带会话 Cookie → 会话绑定 synchronizer token 校验（已认证写请求）
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        return validate_session_csrf(
            pool,
            &state.config,
            request,
            session_cookie.value(),
            &request_id,
            next,
        )
        .await;
    }

    // 无会话 Cookie 的预认证写路径（login/register/verify-email/resend/
    // password-reset）→ 必须携带服务端可回溯的匿名预认证 CSRF 状态
    // （`__Host-bblbb_csrf` cookie + 匹配的 X-CSRF-Token），否则 403——
    // 防 login CSRF（M02-SESSION-08，SECURITY.md §4）。
    if is_preauth_write_path(path) {
        return validate_preauth_csrf(pool, &state.config, request, &jar, &request_id, next).await;
    }

    // 其余无会话写请求（如 Bearer-only 端点）：宽松策略放行，由路由层处理
    next.run(request).await
}

/// 会话绑定 synchronizer token 校验（原逻辑，M02-SESSION-07）。
async fn validate_session_csrf(
    pool: &DatabasePool,
    config: &AppConfig,
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

    // M02-SESSION-09：Cookie 写请求同时校验请求来源（Origin，缺则 Referer）
    if let Err(response) = validate_request_source(request.headers(), config, request_id) {
        return response;
    }

    next.run(request).await
}

/// 匿名预认证 CSRF 校验（M02-SESSION-08）：预认证写请求必须携带
/// `__Host-bblbb_csrf` cookie 与匹配的 `X-CSRF-Token`。任一缺失/不匹配/
/// 过期 → 403 `csrf_failed`（fail closed，防 login CSRF）。
async fn validate_preauth_csrf(
    pool: &DatabasePool,
    config: &AppConfig,
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

    // M02-SESSION-09：预认证写请求同样校验请求来源（防 login CSRF 的
    // 跨站表单提交：浏览器必带 Origin 或 Referer）
    if let Err(response) = validate_request_source(request.headers(), config, request_id) {
        return response;
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

/// 请求来源校验（M02-SESSION-09，SECURITY.md §4：验证 token 与 Origin，
/// 缺少 Origin 时按策略校验 Referer）。
///
/// 策略：
/// - `Origin` 存在 → 必须与请求 `Host` 同主机，或命中配置的
///   `allowed_origins`（严格部署模式）；
/// - `Origin` 缺失但 `Referer` 存在 → 将 Referer 归一化为 origin 后同样校验；
/// - 两者皆缺 → 放行（非浏览器客户端；SameSite=Lax 已阻断跨站携带 cookie，
///   Bearer-only 场景由 M02-SESSION-10 处理）。
///
/// 校验失败返回 `Err(400 origin_not_allowed)` 响应。
#[allow(clippy::result_large_err)] // Response 为中间件统一拒绝载体，体积固定可接受
fn validate_request_source(
    headers: &HeaderMap,
    config: &AppConfig,
    request_id: &str,
) -> Result<(), Response> {
    let origin = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok());
    let referer = headers
        .get(header::REFERER)
        .and_then(|value| value.to_str().ok());

    // 归一化为 "scheme://host[:port]" 形式的候选来源
    let candidate = match (origin, referer) {
        (Some(o), _) => Some(o.to_string()),
        (None, Some(r)) => referer_origin(r),
        (None, None) => return Ok(()),
    };
    let Some(candidate) = candidate else {
        // Referer 无法解析出合法来源 → 拒绝
        tracing::warn!(request_id = %request_id, "csrf: referer unparseable");
        return Err(source_rejected(request_id));
    };

    // 命中配置的 allowed_origins（严格部署模式）
    if !config.allowed_origins.is_empty()
        && config
            .allowed_origins
            .iter()
            .any(|allowed| origin_allowed(allowed, &candidate))
    {
        return Ok(());
    }

    // 与请求 Host 同主机（忽略 scheme 与端口，与 SameSite 语义一致）
    if let Some(host) = headers.get(header::HOST).and_then(|v| v.to_str().ok()) {
        if hostname_of(host) == origin_hostname(&candidate) {
            return Ok(());
        }
    }

    tracing::warn!(request_id = %request_id, candidate, "csrf: request source not allowed");
    Err(source_rejected(request_id))
}

/// 从 Referer URL 提取 origin（"scheme://host[:port]"）；无法解析返回 None。
fn referer_origin(referer: &str) -> Option<String> {
    let (scheme, rest) = referer.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.is_empty() {
        return None;
    }
    Some(format!("{scheme}://{authority}"))
}

/// 提取 origin 字符串（"scheme://host[:port]" 或裸 host）的主机名。
fn origin_hostname(origin: &str) -> &str {
    let (_, rest) = origin.split_once("://").unwrap_or(("", origin));
    let authority = rest.split(['/', '?']).next().unwrap_or(rest);
    hostname_of(authority)
}

/// 提取 authority 的 hostname（忽略端口；支持 IPv6 字面量 `[...]`）。
fn hostname_of(authority: &str) -> &str {
    if let Some(rest) = authority.strip_prefix('[') {
        return rest.split(']').next().unwrap_or(rest);
    }
    authority.split(':').next().unwrap_or(authority)
}

/// 400 Problem JSON 响应（来源不匹配）。
fn source_rejected(request_id: &str) -> Response {
    let problem = Problem {
        type_uri: "about:blank",
        title: "Bad Request",
        status: StatusCode::BAD_REQUEST.as_u16(),
        code: "origin_not_allowed",
        detail: "Request origin is not allowed".to_string(),
        instance: None,
        request_id: request_id.to_string(),
        errors: None,
    };
    (
        StatusCode::BAD_REQUEST,
        [(header::CONTENT_TYPE, "application/problem+json")],
        Json(problem),
    )
        .into_response()
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
    fn bearer_token_detection() {
        use axum::http::HeaderValue;
        let mut headers = HeaderMap::new();
        assert!(
            !has_bearer_token(&headers),
            "无 Authorization 头不是 Bearer-only"
        );

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Bearer abc.def"),
        );
        assert!(has_bearer_token(&headers));

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("bearer abc.def"),
        );
        assert!(has_bearer_token(&headers), "scheme 大小写不敏感");

        headers.insert(
            header::AUTHORIZATION,
            HeaderValue::from_static("Basic dXNlcjpwYXNz"),
        );
        assert!(!has_bearer_token(&headers), "Basic 不是 Bearer-only");

        headers.insert(header::AUTHORIZATION, HeaderValue::from_static("Bearer"));
        assert!(!has_bearer_token(&headers), "缺少令牌不视为 Bearer-only");
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

    #[test]
    fn hostname_of_ignores_port_and_ipv6() {
        assert_eq!(hostname_of("example.com"), "example.com");
        assert_eq!(hostname_of("example.com:8080"), "example.com");
        assert_eq!(hostname_of("example.com:443"), "example.com");
        assert_eq!(hostname_of("[::1]:8080"), "::1");
        assert_eq!(hostname_of("[2001:db8::1]"), "2001:db8::1");
    }

    #[test]
    fn origin_hostname_strips_scheme_and_path() {
        assert_eq!(origin_hostname("https://example.com"), "example.com");
        assert_eq!(origin_hostname("https://example.com:8080"), "example.com");
        assert_eq!(
            origin_hostname("https://example.com/path?q=1"),
            "example.com"
        );
        assert_eq!(origin_hostname("example.com"), "example.com");
    }

    #[test]
    fn referer_origin_extracts_scheme_and_authority() {
        assert_eq!(
            referer_origin("https://example.com/login"),
            Some("https://example.com".to_string())
        );
        assert_eq!(
            referer_origin("https://example.com:8080/a?b=c#d"),
            Some("https://example.com:8080".to_string())
        );
        assert_eq!(
            referer_origin("https://example.com"),
            Some("https://example.com".to_string())
        );
        assert_eq!(referer_origin("not-a-url"), None);
        assert_eq!(referer_origin("https:///missing-host"), None);
    }
}
