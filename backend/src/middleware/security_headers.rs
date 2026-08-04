use axum::{
    http::{header, HeaderValue, Request},
    middleware::Next,
    response::Response,
};

/// 安全响应头中间件
///
/// 为所有响应添加标准安全头，防止常见的 Web 攻击：
/// - X-Content-Type-Options: nosniff
/// - X-Frame-Options: DENY
/// - Referrer-Policy: strict-origin-when-cross-origin
/// - X-XSS-Protection: 0 (现代浏览器使用 CSP)
/// - Permissions-Policy: 限制危险 API
pub async fn security_headers(request: Request<axum::body::Body>, next: Next) -> Response {
    let mut response = next.run(request).await;

    let headers = response.headers_mut();
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("strict-origin-when-cross-origin"),
    );
    headers.insert(header::X_XSS_PROTECTION, HeaderValue::from_static("0"));
    // 限制摄像头、麦克风、地理位置等敏感 API
    headers.insert(
        "permissions-policy",
        HeaderValue::from_static("camera=(), microphone=(), geolocation=(), payment=()"),
    );

    response
}
