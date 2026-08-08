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

use bblbb_backend::{
    auth::{base32_decode, begin_enrollment, confirm_enrollment, totp_at, TOTP_PERIOD_SECS},
    db::DatabasePool,
    outbox::now_millis,
};

/// 测试 TOTP 加密密钥（与各 mfa 测试文件保持一致）。
#[allow(dead_code)] // 共享测试工具：并非每个引用 common 的测试二进制都使用
pub const TEST_TOTP_ENC_KEY: &[u8] = b"test-encryption-key-material";

/// 为指定用户启用并确认 TOTP（M02-MFA-05/06）。
///
/// 强制启用规则生效后，elevated 角色/权限账号（administrator / moderator /
/// 高风险账务）未完成 enrollment 时聚合会被降级为 member 基线——需要高权限的
/// 测试必须先调用本助手完成 enrollment。普通 member 测试无需调用（TOTP 可选）。
/// 返回 base32 secret（如需经 HTTP 走两步登录计算 TOTP code）。
#[allow(dead_code)] // 共享测试工具：并非每个引用 common 的测试二进制都使用
pub async fn enroll_totp(pool: &DatabasePool, user_id: &str) -> String {
    let challenge = begin_enrollment(
        pool,
        user_id,
        "BBLBB",
        "test@example.com",
        TEST_TOTP_ENC_KEY,
    )
    .await
    .expect("begin TOTP enrollment");
    let secret = base32_decode(&challenge.secret_base32).expect("decode TOTP secret");
    let now_secs = (now_millis() / 1000) as u64;
    let code = format!("{:06}", totp_at(&secret, now_secs / TOTP_PERIOD_SECS));
    confirm_enrollment(pool, user_id, &code, TEST_TOTP_ENC_KEY, now_secs)
        .await
        .expect("confirm TOTP enrollment");
    challenge.secret_base32
}

/// 直接为已存在的用户签发会话（绕过 HTTP 登录，返回完整 Cookie 值）。
///
/// 两步登录流程由 mfa_login.rs/mfa_forced.rs 覆盖；管理端点测试需要的是已
/// 认证会话 + SessionUser.roles 实时聚合（含 MFA-05 降级），直接签发等价且
/// 无需在测试 AppConfig 配置 MFA 加密密钥。
#[allow(dead_code)] // 共享测试工具：并非每个引用 common 的测试二进制都使用
pub async fn direct_session_cookie(pool: &DatabasePool, user_id: &str) -> String {
    let token = bblbb_backend::auth::session::create_session(pool, user_id, None, false)
        .await
        .expect("create_session");
    format!(
        "{}={token}",
        bblbb_backend::auth::session::SESSION_COOKIE_NAME
    )
}

/// 获取匿名预认证 CSRF 状态：`GET /api/v1/auth/csrf`（无会话 Cookie）。
///
/// 返回 `(Set-Cookie 完整值, CSRF token)`。`Set-Cookie` 值可直接用作
/// 后续写请求的 `cookie` 请求头。
#[allow(dead_code)] // 共享测试工具：并非每个引用 common 的测试二进制都使用
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
