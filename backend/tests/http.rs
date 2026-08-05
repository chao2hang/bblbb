use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bblbb_backend::config::flags::{FeatureFlags, FeatureName};
use bblbb_backend::{build_router, build_router_with_flags, AppConfig};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_ok_and_request_id() {
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let request_id = response.headers().get("x-request-id").unwrap();
    assert!(!request_id.is_empty());
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert_eq!(&body[..], br#"{"status":"ok","version":"0.1.0"}"#);
}

#[tokio::test]
async fn supplied_request_id_is_preserved() {
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", "test-request-123")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "test-request-123"
    );
}

#[tokio::test]
async fn openapi_endpoint_reads_configured_document() {
    let config = AppConfig {
        openapi_path: std::path::PathBuf::from("../openapi/openapi.yaml"),
        ..AppConfig::default()
    };
    let response = build_router(config, None)
        .oneshot(
            Request::builder()
                .uri("/api/v1/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    assert!(body.starts_with(b"{"));
    assert!(body
        .windows(b"openapi".len())
        .any(|window| window == b"openapi"));
}

#[tokio::test]
async fn readyz_returns_status() {
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // 无数据库/存储目录缺失 → 明确失败（503），响应体只含状态枚举
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "degraded");
    assert!(json["checks"]["database"].is_string());
}

#[tokio::test]
async fn security_headers_are_present() {
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get("x-content-type-options").unwrap(),
        "nosniff"
    );
    assert_eq!(response.headers().get("x-frame-options").unwrap(), "DENY");
    assert_eq!(
        response.headers().get("referrer-policy").unwrap(),
        "strict-origin-when-cross-origin"
    );
}

#[tokio::test]
async fn write_without_session_cookie_passes_csrf_check() {
    // 预认证写请求（无会话 Cookie）走宽松策略，不应被 CSRF 中间件拦截。
    // 该路由为未实现桩，返回 501 说明请求已到达处理器。
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/storage/test")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn write_with_session_cookie_without_db_passes_csrf_check() {
    // 无数据库时不存在会话状态，携带 Cookie 的写请求等同无 Cookie 场景。
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/storage/test")
                .header(
                    "cookie",
                    "bblbb_session=invalid-cookie-value-without-db; __Host-bblbb_session=also-invalid",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

/// M01-CONFIG-06：可选能力默认关闭 → 命中其路由前缀返回 409 feature_disabled。
#[tokio::test]
async fn disabled_feature_route_returns_409_feature_disabled() {
    for path in [
        "/api/v1/ai/capabilities",
        "/api/v1/video-embeds/resolve",
        "/api/v1/marketplace/offers",
        "/oauth/token",
        "/.well-known/openid-configuration",
        "/api/v1/attachments/abc/download",
    ] {
        let response = build_router(AppConfig::default(), None)
            .oneshot(
                Request::builder()
                    .uri(path)
                    .method("GET")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT, "path {path}");
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["code"], "feature_disabled", "path {path}: {json}");
        assert_eq!(json["status"], 409, "path {path}");
    }
}

/// Flag 启用后，同一路由放行到真实 handler（此时为 stub → 501）。
#[tokio::test]
async fn enabled_feature_route_passes_the_gate() {
    let mut flags = FeatureFlags::all_default();
    flags
        .set(
            FeatureName::Ai,
            true,
            1,
            0,
            "test",
            "enable for test",
            1_700_000_000_000,
        )
        .unwrap();
    let response = build_router_with_flags(AppConfig::default(), None, flags)
        .oneshot(
            Request::builder()
                .uri("/api/v1/ai/capabilities")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    // Gate 放行 → 走到 stub handler（501 Not Implemented）
    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

/// 上传与核心路径不受 Download Billing / 其他 Flag 门控。
#[tokio::test]
async fn upload_and_core_routes_are_not_gated() {
    for path in ["/api/v1/attachments", "/healthz", "/api/v1/openapi.json"] {
        let response = build_router(AppConfig::default(), None)
            .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_ne!(response.status(), StatusCode::CONFLICT, "path {path}");
    }
}
