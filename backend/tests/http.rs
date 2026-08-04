use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use tower::ServiceExt;

#[tokio::test]
async fn healthz_returns_ok_and_request_id() {
    let response = build_router(AppConfig::default())
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
    let response = build_router(AppConfig::default())
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
    let response = build_router(config)
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
