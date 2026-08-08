//! M15-OBSERVE-04/05 + M15-PACKAGE-07：`/metrics` 端点测试。
//!
//! - loopback 来源 → 200 + Prometheus 文本（HTTP 计数器/耗时摘要/DB pool/队列/Outbox）；
//! - 非 loopback 来源 → 404（隐藏端点存在，不暴露内部诊断）；
//! - 响应内容不含配置、DSN 或 Secret。

use std::net::SocketAddr;
use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, StatusCode},
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use tower::ServiceExt;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn real_migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("bblbb-metrics-{}", uuid::Uuid::now_v7()))
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

fn metrics_request(peer: SocketAddr) -> Request<Body> {
    Request::builder()
        .uri("/metrics")
        .extension(ConnectInfo(peer))
        .body(Body::empty())
        .unwrap()
}

async fn ready_pool() -> (bblbb_backend::db::DatabasePool, PathBuf, PathBuf) {
    let db_dir = temp_dir();
    let storage_dir = temp_dir();
    std::fs::create_dir_all(&storage_dir).unwrap();
    let pool = create_pool(&format!("sqlite://{}", db_dir.display()))
        .await
        .unwrap();
    let files = read_migration_files(&real_migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, db_dir, storage_dir)
}

/// loopback 抓取 → 200，包含必需指标与耗时摘要。
#[tokio::test]
async fn metrics_ok_from_loopback_with_expected_series() {
    let (pool, db_dir, storage_dir) = ready_pool().await;
    let config = AppConfig {
        migrations_dir: real_migrations_dir(),
        storage_dir: storage_dir.clone(),
        ..AppConfig::default()
    };

    let response = build_router(config, Some(pool.clone()))
        .oneshot(metrics_request("127.0.0.1:1".parse().unwrap()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();

    // M15-OBSERVE-04 基础设施指标
    assert!(text.contains("bblbb_http_requests_total"), "{text}");
    assert!(
        text.contains("bblbb_http_request_duration_seconds{quantile=\"0.5\"}"),
        "{text}"
    );
    assert!(text.contains("bblbb_db_pool_max 8"), "{text}");
    assert!(text.contains("bblbb_sqlite_busy_total"), "{text}");
    // M15-OBSERVE-05 领域指标（名称为准；精确值由 CSRF 回路测试断言）
    assert!(text.contains("bblbb_csrf_rejections_total"), "{text}");
    assert!(text.contains("bblbb_jobs_queued"), "{text}");
    assert!(text.contains("bblbb_outbox_pending"), "{text}");
    // M15-PACKAGE-07：不泄漏内部配置/DSN
    assert!(!text.contains("database_url"), "{text}");
    assert!(!text.contains("sqlite://"), "{text}");

    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
    let _ = pool;
}

/// 非 loopback 来源 → 404（隐藏端点）。
#[tokio::test]
async fn metrics_hidden_from_non_loopback() {
    let (pool, db_dir, storage_dir) = ready_pool().await;
    let config = AppConfig {
        migrations_dir: real_migrations_dir(),
        storage_dir: storage_dir.clone(),
        ..AppConfig::default()
    };

    let response = build_router(config, Some(pool.clone()))
        .oneshot(metrics_request("203.0.113.7:4444".parse().unwrap()))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
    let _ = pool;
}

/// 触发一次 CSRF 拒绝后，指标计数反映拒绝次数（M15-OBSERVE-05 回路验证）。
#[tokio::test]
async fn csrf_rejection_increments_counter() {
    let (pool, db_dir, storage_dir) = ready_pool().await;
    let config = AppConfig {
        migrations_dir: real_migrations_dir(),
        storage_dir: storage_dir.clone(),
        ..AppConfig::default()
    };

    // 无会话 Cookie + 无 CSRF token 的写请求 → 403 csrf_failed（消费一次拒绝）
    let router = build_router(config, Some(pool.clone()));
    let rejected = router
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(rejected.status(), StatusCode::FORBIDDEN);

    let response = router
        .oneshot(metrics_request("127.0.0.1:1".parse().unwrap()))
        .await
        .unwrap();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8(body.to_vec()).unwrap();
    assert!(
        text.contains("bblbb_csrf_rejections_total 1"),
        "CSRF 拒绝后指标应计数 1: {text}"
    );

    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
    let _ = pool;
}
