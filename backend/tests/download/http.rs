//! M06-DOWNLOAD HTTP 测试：下载路由、未 ready 拒绝、未授权拒绝（SQLite）。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::Request;
use axum::http::StatusCode;
use bblbb_backend::app::build_router_full;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::economy::ledger::service::CURRENCY_COIN;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::storage::StorageService;
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::Either;
use tower::ServiceExt;

#[path = "../common/mod.rs"]
mod common;

fn flags_for_download() -> bblbb_backend::config::flags::FeatureFlags {
    let mut flags = bblbb_backend::config::flags::FeatureFlags::all_default();
    flags
        .set(
            bblbb_backend::config::flags::FeatureName::DownloadBilling,
            true,
            1,
            0,
            "test",
            "enable download billing for test",
            0,
        )
        .unwrap();
    flags
}

async fn setup() -> (DatabasePool, PathBuf, StorageService) {
    let dir = std::env::temp_dir().join(format!("bblbb-dlhttp-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let url = format!("sqlite://{}", dir.join("db.sqlite").display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
    let storage = StorageService::local_only(dir.join("uploads")).unwrap();
    (pool, dir, storage)
}

async fn fetch_csrf(app: &axum::Router, cookie: Option<&str>) -> String {
    let mut builder = Request::builder().method("GET").uri("/api/v1/auth/csrf");
    if let Some(c) = cookie {
        builder = builder.header("cookie", c);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    v["token"].as_str().unwrap().to_string()
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    user_id
}

async fn insert_attachment(
    pool: &DatabasePool,
    storage: &StorageService,
    owner_id: &str,
    status: &str,
) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let key = format!("u/{owner_id}/{}/f.bin", uuid::Uuid::now_v7());
    let data = b"http-download";
    let adapter = storage
        .adapter(bblbb_backend::storage::model::StorageBackend::Local)
        .unwrap();
    adapter.write_object(&key, data, None).await.unwrap();
    let sha = hex::encode({
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(data);
        h.finalize().to_vec()
    });
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO attachments (id, owner_id, storage_backend, storage_key, media_type, size_bytes, sha256, status, quota_bytes_charged, is_public, ref_count, processing_version, created_at)
                 VALUES (?, ?, 'local', ?, 'application/octet-stream', ?, ?, ?, 14, 0, 0, 0, ?)",
            )
            .bind(&id)
            .bind(owner_id)
            .bind(&key)
            .bind(data.len() as i64)
            .bind(&sha)
            .bind(status)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO attachments (id, owner_id, storage_backend, storage_key, media_type, size_bytes, sha256, status, quota_bytes_charged, is_public, ref_count, processing_version, created_at)
                 VALUES (?, ?, 'local', ?, 'application/octet-stream', ?, ?, ?, 14, 0, 0, 0, ?)",
            )
            .bind(&id)
            .bind(owner_id)
            .bind(&key)
            .bind(data.len() as i64)
            .bind(&sha)
            .bind(status)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    id
}

async fn set_free_policy(pool: &DatabasePool, attachment_id: &str) {
    let now = now_millis();
    let id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO download_billing_policies (id, scope_type, scope_id, mode, currency_id, amount, authorization_ttl_seconds, grace_on_disable, version, is_enabled, created_at, updated_at)
                 VALUES (?, 'attachment', ?, 'free', ?, 0, 3600, 1, 1, 1, ?, ?)",
            )
            .bind(&id)
            .bind(attachment_id)
            .bind(CURRENCY_COIN)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO download_billing_policies (id, scope_type, scope_id, mode, currency_id, amount, authorization_ttl_seconds, grace_on_disable, version, is_enabled, created_at, updated_at)
                 VALUES (?, 'attachment', ?, 'free', ?, 0, 3600, 1, 1, 1, ?, ?)",
            )
            .bind(&id)
            .bind(attachment_id)
            .bind(CURRENCY_COIN)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
}

#[tokio::test]
async fn authenticated_download_returns_authorization() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let cookie = common::direct_session_cookie(&pool, &user).await;
    let att = insert_attachment(&pool, &storage, &owner, "ready").await;
    set_free_policy(&pool, &att).await;

    let app = build_router_full(
        bblbb_backend::AppConfig::default(),
        Some(pool.clone()),
        flags_for_download(),
        Some(storage),
    );
    let csrf = fetch_csrf(&app, Some(&cookie)).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/attachments/{att}/download"))
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .header("x-request-id", "http-test")
                .body(Body::from(
                    json!({ "idempotency_key": "http-1" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn unauthenticated_download_is_unauthorized() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let att = insert_attachment(&pool, &storage, &owner, "ready").await;
    set_free_policy(&pool, &att).await;
    let app = build_router_full(
        bblbb_backend::AppConfig::default(),
        Some(pool.clone()),
        flags_for_download(),
        Some(storage),
    );
    let csrf = fetch_csrf(&app, None).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/attachments/{att}/download"))
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("x-request-id", "http-test2")
                .body(Body::from(
                    json!({ "idempotency_key": "http-2" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn pending_attachment_download_is_not_found() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let cookie = common::direct_session_cookie(&pool, &user).await;
    let att = insert_attachment(&pool, &storage, &owner, "pending").await;
    set_free_policy(&pool, &att).await;
    let app = build_router_full(
        bblbb_backend::AppConfig::default(),
        Some(pool.clone()),
        flags_for_download(),
        Some(storage),
    );
    let csrf = fetch_csrf(&app, Some(&cookie)).await;
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/attachments/{att}/download"))
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .header("x-request-id", "http-test3")
                .body(Body::from(
                    json!({ "idempotency_key": "http-3" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND, "未 ready 不泄漏");
    close_pool(&pool).await;
    cleanup(&dir);
}
