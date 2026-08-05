//! Host / Origin 校验中间件（M00-BACKEND-06）
//!
//! 防御 DNS rebinding 与跨站请求伪造的请求边界校验：
//!
//! - **严格模式**（配置了 `BBLBB__ALLOWED_HOSTS` / `BBLBB__ALLOWED_ORIGINS` 时生效）：
//!   - `Host` 头不在 `allowed_hosts` 中 → 400 Problem（`host_not_allowed`）
//!   - 状态变更请求（POST/PUT/PATCH/DELETE）携带的 `Origin` 不在
//!     `allowed_origins` 中 → 400 Problem（`origin_not_allowed`）
//! - **宽松模式**（默认，均未配置）：仅记录 debug 日志，不拒绝任何请求，
//!   便于开发环境与首次部署。
//!
//! `/healthz` 与 `/readyz` 是进程级探针端点，Kubernetes 等探针的 `Host`
//! 头通常是 Pod IP 或服务名，因此豁免 Host 校验（它们均为 GET，不涉及
//! Origin 校验）。
//!
//! 受信代理：当前不消费任何 `X-Forwarded-*` 转发头，客户端地址一律取自
//! socket 对端；`trusted_proxies` 是 M1 接入反向代理/负载均衡时的扩展点，
//! 届时才决定是否信任转发头。

use axum::{
    extract::{Request, State},
    http::{header, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
};

use crate::{app::AppState, error::Problem, middleware::request_id::RequestId};

/// 需要 Host/Origin 校验的方法（与 CSRF 中间件保持一致）
fn is_state_changing(method: &Method) -> bool {
    matches!(
        method,
        &Method::POST | &Method::PUT | &Method::PATCH | &Method::DELETE
    )
}

/// 豁免 Host 校验的探针路径
fn is_probe_path(path: &str) -> bool {
    path == "/healthz" || path == "/readyz"
}

/// Host/Origin 校验中间件
pub async fn host_origin_guard(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Response {
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|rid| rid.0.clone())
        .unwrap_or_else(|| "unknown".to_string());

    // 1. Host 校验（严格模式仅在配置了 allowed_hosts 时生效）
    if !state.config.allowed_hosts.is_empty() && !is_probe_path(request.uri().path()) {
        let host = request
            .headers()
            .get(header::HOST)
            .and_then(|value| value.to_str().ok());
        match host {
            Some(host) => {
                if !state
                    .config
                    .allowed_hosts
                    .iter()
                    .any(|allowed| host_allowed(allowed, host))
                {
                    tracing::warn!(request_id = %request_id, host, "host not allowed");
                    return rejected(
                        &request_id,
                        "host_not_allowed",
                        "Host header is not allowed",
                    );
                }
            }
            None => {
                tracing::warn!(request_id = %request_id, "host header missing in strict mode");
                return rejected(&request_id, "host_not_allowed", "Host header is required");
            }
        }
    } else if let Some(host) = request
        .headers()
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
    {
        // 宽松模式：仅记录，不拒绝
        tracing::debug!(request_id = %request_id, host, "host present (lenient mode)");
    }

    // 2. Origin 校验（仅状态变更请求；严格模式仅在配置了 allowed_origins 时生效）
    if is_state_changing(request.method()) {
        if let Some(origin) = request
            .headers()
            .get(header::ORIGIN)
            .and_then(|value| value.to_str().ok())
        {
            if !state.config.allowed_origins.is_empty() {
                if !state
                    .config
                    .allowed_origins
                    .iter()
                    .any(|allowed| origin_allowed(allowed, origin))
                {
                    tracing::warn!(request_id = %request_id, origin, "origin not allowed");
                    return rejected(&request_id, "origin_not_allowed", "Origin is not allowed");
                }
            } else {
                // 宽松模式：仅记录，不拒绝
                tracing::debug!(request_id = %request_id, origin, "origin present (lenient mode)");
            }
        }
    }

    next.run(request).await
}

/// Host 是否匹配允许项：允许项含端口时要求精确一致；
/// 允许项为裸主机名（无端口）时匹配任意端口。
fn host_allowed(allowed: &str, host: &str) -> bool {
    if allowed == host {
        return true;
    }
    if !allowed.contains(':') {
        return host.split(':').next() == Some(allowed);
    }
    false
}

/// Origin 是否匹配允许项：scheme 必须一致；允许项无端口时匹配同主机任意端口。
/// `pub(crate)` 供 CSRF 中间件（M02-SESSION-09）复用配置校验。
pub(crate) fn origin_allowed(allowed: &str, origin: &str) -> bool {
    if allowed == origin {
        return true;
    }
    let (allowed_scheme, allowed_rest) = allowed.split_once("://").unwrap_or(("", allowed));
    let (origin_scheme, origin_rest) = origin.split_once("://").unwrap_or(("", origin));
    if allowed_scheme != origin_scheme {
        return false;
    }
    if allowed_rest.contains(':') {
        return false;
    }
    let allowed_host = allowed_rest.split(':').next().unwrap_or(allowed_rest);
    let origin_host = origin_rest.split(':').next().unwrap_or(origin_rest);
    allowed_host == origin_host
}

/// 400 Problem JSON 响应
fn rejected(request_id: &str, code: &'static str, detail: &'static str) -> Response {
    let problem = Problem {
        type_uri: "about:blank",
        title: "Bad Request",
        status: StatusCode::BAD_REQUEST.as_u16(),
        code,
        detail: detail.to_string(),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_changing_methods_are_covered() {
        for method in [Method::POST, Method::PUT, Method::PATCH, Method::DELETE] {
            assert!(is_state_changing(&method), "{method} should be covered");
        }
        for method in [Method::GET, Method::HEAD, Method::OPTIONS] {
            assert!(!is_state_changing(&method), "{method} should pass through");
        }
    }

    #[test]
    fn probe_paths_are_exempt() {
        assert!(is_probe_path("/healthz"));
        assert!(is_probe_path("/readyz"));
        assert!(!is_probe_path("/api/v1/auth/csrf"));
    }

    #[test]
    fn host_allowed_matches_exact_and_wildcard_port() {
        assert!(host_allowed("example.com", "example.com"));
        assert!(host_allowed("example.com", "example.com:8080"));
        assert!(host_allowed("example.com:8080", "example.com:8080"));
        assert!(!host_allowed("example.com:8080", "example.com:9090"));
        assert!(!host_allowed("example.com", "evil.example.com"));
        assert!(!host_allowed("example.com", "example.org"));
    }

    #[test]
    fn origin_allowed_matches_scheme_and_host() {
        assert!(origin_allowed(
            "http://localhost:8080",
            "http://localhost:8080"
        ));
        assert!(origin_allowed("http://localhost", "http://localhost:8080"));
        assert!(origin_allowed("https://example.com", "https://example.com"));
        assert!(!origin_allowed(
            "http://localhost:8080",
            "http://localhost:9090"
        ));
        assert!(!origin_allowed(
            "http://localhost",
            "https://localhost:8080"
        ));
        assert!(!origin_allowed("https://example.com", "https://evil.com"));
        assert!(!origin_allowed("http://example.com", "http://example.org"));
    }
}
