//! 共享集成测试工具（M02-SESSION-08 引入）。
//!
//! M02-SESSION-08 起，预认证写端点（login/register/verify-email/
//! resend-verification/password-reset 及其 confirm）必须携带匿名预认证 CSRF
//! 状态：`__Host-bblbb_csrf` cookie + 匹配的 `X-CSRF-Token`。本模块提供统一的
//! 获取助手，供各测试文件复用。

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::Value;
use tower::ServiceExt;

/// 获取匿名预认证 CSRF 状态：`GET /api/v1/auth/csrf`（无会话 Cookie）。
///
/// 返回 `(Set-Cookie 完整值, CSRF token)`。`Set-Cookie` 值可直接用作
/// 后续写请求的 `cookie` 请求头。
pub async fn fetch_preauth(app: &Router) -> (String, String) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "预认证 CSRF 端点必须 200");
    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .expect("预认证 CSRF 必须签发 Set-Cookie")
        .to_str()
        .unwrap()
        .to_string();
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let token = body["token"]
        .as_str()
        .expect("预认证 CSRF 响应必须包含 token")
        .to_string();
    (set_cookie, token)
}
