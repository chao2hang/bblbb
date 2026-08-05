//! M01-DB-12：/readyz 失败语义与 DSN 不泄漏测试。
//!
//! 场景：
//! - 连接失败/未配置 → 503，status=degraded；
//! - 迁移落后（behind）、超前（ahead）、checksum 不匹配 → 503 + 对应状态；
//! - 全部就绪 → 200；
//! - 响应体绝不包含 DSN/凭据。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
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
    std::env::temp_dir().join(format!("bblbb-readyz-{}", uuid::Uuid::now_v7()))
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

async fn sqlite_pool(dir: &Path) -> bblbb_backend::db::DatabasePool {
    let url = format!("sqlite://{}", dir.display());
    create_pool(&url).await.unwrap()
}

async fn get_readyz(
    config: AppConfig,
    db: Option<bblbb_backend::db::DatabasePool>,
) -> (StatusCode, String) {
    let response = build_router(config, db)
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

/// 全部就绪 → 200 + status=ok。
#[tokio::test]
async fn readyz_ok_when_everything_ready() {
    let db_dir = temp_dir();
    let storage_dir = temp_dir();
    std::fs::create_dir_all(&storage_dir).unwrap();
    let pool = sqlite_pool(&db_dir).await;

    let files = read_migration_files(&real_migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();

    let config = AppConfig {
        migrations_dir: real_migrations_dir(),
        storage_dir: storage_dir.clone(),
        ..AppConfig::default()
    };
    let (status, body) = get_readyz(config, Some(pool)).await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(body.contains("\"status\":\"ok\""), "body: {body}");
    assert!(body.contains("\"migrations\":\"ok\""), "body: {body}");

    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

/// 数据库未配置 → 503 + not_configured。
#[tokio::test]
async fn readyz_fails_without_database() {
    let config = AppConfig::default();
    let (status, body) = get_readyz(config, None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(
        body.contains("\"database\":\"not_configured\""),
        "body: {body}"
    );
}

/// 迁移落后（全新库尚未迁移）→ 503 + behind。
#[tokio::test]
async fn readyz_fails_when_migrations_behind() {
    let db_dir = temp_dir();
    let storage_dir = temp_dir();
    std::fs::create_dir_all(&storage_dir).unwrap();
    let pool = sqlite_pool(&db_dir).await;

    let config = AppConfig {
        migrations_dir: real_migrations_dir(),
        storage_dir: storage_dir.clone(),
        ..AppConfig::default()
    };
    let (status, body) = get_readyz(config, Some(pool)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body: {body}");
    assert!(body.contains("\"migrations\":\"behind\""), "body: {body}");

    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

/// 迁移超前（代码落后于数据库）→ 503 + ahead。
#[tokio::test]
async fn readyz_fails_when_migrations_ahead() {
    let db_dir = temp_dir();
    let storage_dir = temp_dir();
    std::fs::create_dir_all(&storage_dir).unwrap();
    let pool = sqlite_pool(&db_dir).await;

    let all_files = read_migration_files(&real_migrations_dir()).unwrap();
    run_migrations(&pool, &all_files).await.unwrap();

    // 代码只剩 1..5，数据库已到 6 → 超前
    let old_dir = temp_dir();
    std::fs::create_dir_all(&old_dir).unwrap();
    for f in all_files.iter().take(5) {
        std::fs::write(
            old_dir.join(format!("{:04}_{}.sql", f.version, f.name)),
            &f.sql,
        )
        .unwrap();
    }

    let config = AppConfig {
        migrations_dir: old_dir.clone(),
        storage_dir: storage_dir.clone(),
        ..AppConfig::default()
    };
    let (status, body) = get_readyz(config, Some(pool)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body: {body}");
    assert!(body.contains("\"migrations\":\"ahead\""), "body: {body}");

    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
    let _ = std::fs::remove_dir_all(&old_dir);
}

/// checksum 不匹配（已执行迁移内容被修改）→ 503 + checksum_mismatch。
#[tokio::test]
async fn readyz_fails_on_checksum_mismatch() {
    let db_dir = temp_dir();
    let storage_dir = temp_dir();
    std::fs::create_dir_all(&storage_dir).unwrap();
    let pool = sqlite_pool(&db_dir).await;

    // 用真实目录应用 1..6
    let real = real_migrations_dir();
    let all_files = read_migration_files(&real).unwrap();
    run_migrations(&pool, &all_files).await.unwrap();

    // 构造篡改目录：0006 内容被修改（checksum 变化），其余与真实一致
    let tampered_dir = temp_dir();
    std::fs::create_dir_all(&tampered_dir).unwrap();
    for f in &all_files {
        let sql = if f.version == 6 {
            format!("-- tampered\n{}", f.sql)
        } else {
            f.sql.clone()
        };
        std::fs::write(
            tampered_dir.join(format!("{:04}_{}.sql", f.version, f.name)),
            &sql,
        )
        .unwrap();
    }

    let config = AppConfig {
        migrations_dir: tampered_dir.clone(),
        storage_dir: storage_dir.clone(),
        ..AppConfig::default()
    };
    let (status, body) = get_readyz(config, Some(pool)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "body: {body}");
    assert!(
        body.contains("\"migrations\":\"checksum_mismatch\""),
        "body: {body}"
    );

    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
    let _ = std::fs::remove_dir_all(&tampered_dir);
}

/// DSN 不泄漏：配置含带密码的数据库 URL，响应体不得出现 URL 或凭据。
#[tokio::test]
async fn readyz_does_not_leak_dsn() {
    let secret = "supersecret-password-please-hide";
    let dsn = format!("mysql://dbuser:{secret}@db.internal.example:3306/bblbb");
    let config = AppConfig {
        database_url: dsn.clone(),
        ..AppConfig::default()
    };
    let (status, body) = get_readyz(config, None).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert!(!body.contains(secret), "响应体泄漏了数据库凭据: {body}");
    assert!(
        !body.contains("db.internal.example") && !body.contains("dbuser"),
        "响应体泄漏了 DSN 主机或用户: {body}"
    );
    assert!(!body.contains("3306"), "响应体不应包含端口: {body}");
}
