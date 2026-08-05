//! M00-BACKEND-10 边界行为集成测试
//!
//! 覆盖：非法/超长 request_id、超限 body（413）、超限 header（431）、
//! 错误 Content-Type（415）、Problem 字段（instance/request_id）、
//! 慢请求超时（408）、停机期间在途请求完成、Host/Origin 严格模式边界、
//! openapi.json 与提交 YAML 语义一致（M00-BACKEND-11）。

use std::time::Duration;

use axum::{
    body::Body,
    http::{header, HeaderValue, Request, StatusCode},
    routing::get,
    Router,
};
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

// ─── request_id 边界 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn overlong_request_id_is_replaced_with_uuid() {
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", "x".repeat(129))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let value = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    assert_eq!(value.len(), 36, "expected a generated UUID v7, got {value}");
    uuid::Uuid::parse_str(value).expect("must be a valid UUID");
}

#[tokio::test]
async fn request_id_with_non_ascii_is_replaced_with_uuid() {
    // http 层允许非 ASCII 字节（≥0x80），但中间件校验拒绝非 ASCII
    let bad = HeaderValue::from_bytes(b"bad\xc3\xa9id").unwrap();
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", bad)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let value = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    assert_ne!(value, "bad\u{e9}id");
    assert!(
        uuid::Uuid::parse_str(value).is_ok(),
        "expected a UUID, got {value}"
    );
}

#[tokio::test]
async fn empty_request_id_is_replaced_with_uuid() {
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", "")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let value = response
        .headers()
        .get("x-request-id")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        uuid::Uuid::parse_str(value).is_ok(),
        "expected a UUID, got {value}"
    );
}

#[tokio::test]
async fn valid_long_request_id_is_preserved() {
    let good = "a".repeat(128);
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header("x-request-id", good.clone())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        good.as_str()
    );
}

// ─── 请求体 / 头 / Content-Type 边界 ─────────────────────────────────────────

#[tokio::test]
async fn oversized_body_returns_413() {
    let big = vec![b'a'; 11 * 1024 * 1024];
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(big))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn wrong_content_type_returns_415() {
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "text/plain")
                .body(Body::from("hello"))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
}

#[tokio::test]
async fn oversized_headers_rejected_with_431() {
    let app = build_router(AppConfig::default(), None);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let (mut rd, wr) = stream.into_split();

    // 后台写入超限请求；服务器检测到超限后可能重置连接，写失败可接受
    tokio::spawn(async move {
        let mut wr = wr;
        let mut request = String::from("GET /healthz HTTP/1.1\r\nHost: localhost\r\n");
        // 超过 hyper 默认读缓冲上限（约 408KB），触发 431
        request.push_str(&format!("X-Huge: {}\r\n", "h".repeat(600 * 1024)));
        request.push_str("\r\n");
        let _ = wr.write_all(request.as_bytes()).await;
        let _ = wr.shutdown().await;
    });

    // 服务器应对超限请求返回 431；即使随后重置连接，
    // 已到达接收缓冲的 431 响应仍可被读到
    let mut buf = Vec::new();
    let outcome = tokio::time::timeout(Duration::from_secs(5), rd.read_to_end(&mut buf)).await;
    match outcome {
        Ok(_) => {}
        Err(_) => panic!("server did not respond to oversized header request"),
    }
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.contains("431"),
        "expected 431 for oversized headers, got: {text}"
    );
}

// ─── Problem 字段（instance / request_id） ───────────────────────────────────

#[tokio::test]
async fn problem_response_carries_request_id_and_instance() {
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header(header::CONTENT_TYPE, "application/json")
                .header("x-request-id", "edge-test-001")
                .body(Body::from(
                    br#"{"username":"alice_test","email":"a@b.com","password":"password123"}"#
                        .to_vec(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(
        response.headers().get("x-request-id").unwrap(),
        "edge-test-001"
    );
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["type"], "about:blank");
    assert_eq!(json["status"], 500);
    assert_eq!(json["code"], "internal_error");
    assert_eq!(json["request_id"], "edge-test-001");
    assert_eq!(json["instance"], "/api/v1/auth/register");
}

// ─── 慢请求超时 ──────────────────────────────────────────────────────────────

#[tokio::test(start_paused = true)]
async fn slow_request_times_out_with_408() {
    // 使用应用实际的超时常量，验证超时机制：慢于 REQUEST_TIMEOUT 的请求返回 408
    let app = Router::new()
        .route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                "too late"
            }),
        )
        .layer(tower_http::timeout::TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            bblbb_backend::app::REQUEST_TIMEOUT,
        ));

    let request = Request::builder().uri("/slow").body(Body::empty()).unwrap();
    let mut oneshot = Box::pin(app.oneshot(request));

    // 推进虚拟时钟超过超时阈值，触发 408
    let advance = tokio::spawn(async {
        tokio::time::advance(Duration::from_secs(31)).await;
    });

    let response = (&mut oneshot).await.unwrap();
    advance.await.unwrap();
    assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
}

// ─── 停机期间的请求行为 ──────────────────────────────────────────────────────

#[tokio::test]
async fn graceful_shutdown_waits_for_inflight_request() {
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();
    let app = Router::new().route(
        "/slow",
        get(|| async {
            tokio::time::sleep(Duration::from_secs(3)).await;
            "slow response"
        }),
    );

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("server should exit cleanly");
    });

    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();

    // 让请求进入处理，然后触发停机
    tokio::time::sleep(Duration::from_millis(300)).await;
    shutdown_tx.send(()).unwrap();

    // 在途请求应完整返回 200，而不是被停机中断
    let mut buf = Vec::new();
    stream.read_to_end(&mut buf).await.unwrap();
    let text = String::from_utf8_lossy(&buf);
    assert!(
        text.starts_with("HTTP/1.1 200"),
        "inflight request should complete, got: {text}"
    );
    assert!(
        text.contains("slow response"),
        "inflight request should return handler output, got: {text}"
    );

    // 服务器应在在途请求完成后退出
    tokio::time::timeout(Duration::from_secs(5), server)
        .await
        .expect("server should exit after inflight requests finish")
        .expect("server task should not fail");

    // 监听器已关闭：新连接被拒绝
    let reconnect = tokio::net::TcpStream::connect(addr).await;
    assert!(
        reconnect.is_err(),
        "server should no longer accept connections after shutdown"
    );
}

// ─── Host / Origin 严格模式边界（M00-BACKEND-06） ───────────────────────────

#[tokio::test]
async fn strict_host_rejects_disallowed_host() {
    let config = AppConfig {
        allowed_hosts: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let response = build_router(config, None)
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .header(header::HOST, "evil.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "host_not_allowed");
    assert_eq!(json["instance"], "/api/v1/auth/csrf");
    assert_eq!(json["status"], 400);
}

#[tokio::test]
async fn strict_host_requires_host_header() {
    let config = AppConfig {
        allowed_hosts: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let response = build_router(config, None)
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn strict_host_accepts_allowed_hostname_with_any_port() {
    let config = AppConfig {
        allowed_hosts: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let response = build_router(config, None)
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .header(header::HOST, "example.com:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn strict_host_exempts_probe_endpoints() {
    let config = AppConfig {
        allowed_hosts: vec!["example.com".to_string()],
        ..AppConfig::default()
    };
    let response = build_router(config, None)
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .header(header::HOST, "10.0.0.5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn strict_origin_rejects_disallowed_origin_on_write() {
    let config = AppConfig {
        allowed_origins: vec!["http://localhost:8080".to_string()],
        ..AppConfig::default()
    };
    let response = build_router(config, None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/storage/test")
                .header(header::ORIGIN, "http://evil.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["code"], "origin_not_allowed");
}

#[tokio::test]
async fn strict_origin_accepts_allowed_origin_on_write() {
    let config = AppConfig {
        allowed_origins: vec!["http://localhost:8080".to_string()],
        ..AppConfig::default()
    };
    let response = build_router(config, None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/storage/test")
                .header(header::ORIGIN, "http://localhost:8080")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

#[tokio::test]
async fn lenient_mode_does_not_reject_host_or_origin() {
    // 默认配置（无 allowed_hosts / allowed_origins）：宽松模式仅记录日志
    let response = build_router(AppConfig::default(), None)
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/storage/test")
                .header(header::HOST, "attacker.example.com")
                .header(header::ORIGIN, "http://attacker.example.com")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
}

// ─── openapi.json 与提交 YAML 语义一致（M00-BACKEND-11） ─────────────────────

#[tokio::test]
async fn openapi_json_matches_committed_yaml_operations() {
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
    let served: serde_json::Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();

    let yaml_text = std::fs::read_to_string("../openapi/openapi.yaml").unwrap();
    let yaml: serde_json::Value = serde_yaml::from_str(&yaml_text).unwrap();

    fn collect_operation_ids(doc: &serde_json::Value) -> Vec<(String, String)> {
        let mut ops = Vec::new();
        if let Some(paths) = doc["paths"].as_object() {
            for (path, item) in paths {
                if let Some(methods) = item.as_object() {
                    for (method, operation) in methods {
                        if let Some(id) = operation
                            .get("operationId")
                            .and_then(|value| value.as_str())
                        {
                            ops.push((path.clone(), format!("{method} {id}")));
                        }
                    }
                }
            }
        }
        ops.sort();
        ops
    }

    let yaml_ops = collect_operation_ids(&yaml);
    let served_ops = collect_operation_ids(&served);
    assert!(
        !yaml_ops.is_empty(),
        "committed openapi.yaml must define operations"
    );
    assert_eq!(
        served_ops, yaml_ops,
        "served openapi.json must be semantically consistent with committed openapi.yaml"
    );
}
