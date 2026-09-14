//! M06-QUOTA 管理端存储配置/测试连接集成测试（SQLite + 路由层）。
//!
//! 覆盖用户报告的「S3 配置后无法保存和测试」修复面：
//! - `POST /api/v1/admin/storage/test` 必须**实际探测提交的候选后端**
//!   （S3 候选 → mock S3 服务器 happy path；此前实现无条件探测 local）；
//! - step-up 门（M02-MFA-07）：会话超过近期认证窗口 → 403 `step_up_required`，
//!   `mark_step_up`（re-auth 等价）后放行——前端据此渲染重认证表单；
//! - endpoint 协议门：开发环境允许 `http://` 内网 MinIO（与
//!   `BBLBB__S3_ENDPOINT` 环境变量行为一致），生产仅 `https://`。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;

mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-admsto-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
    (pool, dir)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

/// 极简 mock S3：PUT/DELETE → 200 + ETag；HEAD → 200 + 探针长度 19。
/// （探针写入 b"bblbb-storage-probe" 共 19 字节后 head 比对大小。）
async fn spawn_mock_s3() -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(s) => s,
                Err(_) => return,
            };
            tokio::spawn(async move {
                let mut buf = vec![0u8; 16384];
                let n = socket.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                let resp = if head.starts_with("HEAD") {
                    "HTTP/1.1 200 OK\r\nContent-Length: 19\r\nETag: \"probe-etag\"\r\nContent-Type: text/plain\r\n\r\n"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Length: 0\r\nETag: \"probe-etag\"\r\n\r\n"
                };
                let _ = socket.write_all(resp.as_bytes()).await;
                let _ = socket.shutdown().await;
            });
        }
    });
    format!("http://{addr}")
}

async fn authed_post(
    app: &Router,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn authed_patch(
    app: &Router,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
    if_match: &str,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .header("if-match", if_match)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

/// 管理员会话（未 mark_step_up → 立即命中 step-up 门）；返回 (token, session, csrf)。
async fn admin_session_without_step_up(
    app: &Router,
    pool: &DatabasePool,
) -> (String, String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("adm_{}", uuid::Uuid::now_v7().simple());
    let now = bblbb_backend::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(format!("{username}@example.com"))
            .bind(now - 25 * 3600 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = 'administrator'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(&user_id)
            .bind(&role_id)
            .bind(now - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    common::enroll_totp(pool, &user_id).await;
    let session = common::direct_session_cookie(pool, &user_id).await;
    let token = session.split('=').nth(1).unwrap().to_string();

    // CSRF（会话绑定）
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let csrf: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (token, session, csrf["token"].as_str().unwrap().to_string())
}

#[tokio::test]
async fn storage_test_probes_submitted_s3_candidate() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (token, session, csrf) = admin_session_without_step_up(&app, &pool).await;

    // 会话签发即 auth_verified_at=now；把窗口拨回 1 小时前，模拟
    // 「登录超过 step_up 窗口（默认 300s）」的真实用户状态。
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET auth_verified_at = ?")
                .bind((bblbb_backend::outbox::now_millis() as i64) - 3600 * 1000)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // step-up 门：会话超过近期认证窗口 → 403 step_up_required（前端渲染重认证表单）
    let (status, body) = authed_post(
        &app,
        "/api/v1/admin/storage/test",
        &session,
        &csrf,
        json!({"backend": "local", "reason": "t"}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "必须先命中 step-up 门: {body}"
    );
    assert_eq!(body["code"], "step_up_required", "{body}");

    // re-auth 等价：mark_step_up 刷新窗口后放行
    bblbb_backend::auth::session::mark_step_up(&pool, &token)
        .await
        .unwrap();

    // S3 候选：必须实际探测 S3（mock），而不是旧的“永远探测 local”
    let endpoint = spawn_mock_s3().await;
    let (status, body) = authed_post(
        &app,
        "/api/v1/admin/storage/test",
        &session,
        &csrf,
        json!({
            "backend": "s3",
            "s3_endpoint": endpoint,
            "s3_region": "auto",
            "s3_bucket": "bblbb-test",
            "s3_path_style": true,
            "s3_access_key_id": "AKIATEST",
            "s3_secret_access_key": "test-secret",
            "reason": "测试 S3 候选"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["backend"], "s3", "必须探测候选 S3 后端: {body}");
    assert_eq!(body["ok"], true, "mock S3 探针必须成功: {body}");
    assert_eq!(body["error_class"], "ok", "{body}");
    assert!(
        body["elapsed_ms"].as_u64().is_some(),
        "响应必须包含 elapsed_ms: {body}"
    );
    assert!(
        body["message"].as_str().is_some_and(|m| !m.is_empty()),
        "响应必须包含脱敏诊断 message: {body}"
    );

    // local 候选：探测本地存储根（默认配置可写）
    let (status, body) = authed_post(
        &app,
        "/api/v1/admin/storage/test",
        &session,
        &csrf,
        json!({"backend": "local", "reason": "t"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["backend"], "local", "{body}");
    assert_eq!(body["ok"], true, "{body}");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn storage_test_invalid_backend_and_missing_bucket_are_sanitized() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (token, session, csrf) = admin_session_without_step_up(&app, &pool).await;
    bblbb_backend::auth::session::mark_step_up(&pool, &token)
        .await
        .unwrap();

    // 未知 backend → invalid 诊断（HTTP 200 + ok:false，不落 5xx）
    let (status, body) = authed_post(
        &app,
        "/api/v1/admin/storage/test",
        &session,
        &csrf,
        json!({"backend": "ftp"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], false, "{body}");
    assert_eq!(body["error_class"], "invalid", "{body}");

    // s3 候选缺 bucket（环境也未配置）→ invalid 诊断
    let (status, body) = authed_post(
        &app,
        "/api/v1/admin/storage/test",
        &session,
        &csrf,
        json!({"backend": "s3"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["ok"], false, "{body}");
    assert_eq!(body["error_class"], "invalid", "{body}");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn storage_config_patch_http_endpoint_dev_vs_production() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (token, session, csrf) = admin_session_without_step_up(&app, &pool).await;
    bblbb_backend::auth::session::mark_step_up(&pool, &token)
        .await
        .unwrap();

    // 开发环境（默认 env=development）：内网 http:// MinIO endpoint 允许
    let (status, body) = authed_patch(
        &app,
        "/api/v1/admin/storage/config",
        &session,
        &csrf,
        json!({
            "backend": "s3",
            "s3_endpoint": "http://10.10.10.10:9000",
            "s3_region": "auto",
            "s3_bucket": "bblbb",
            "signed_url_ttl_seconds": 300,
            "expected_version": 1,
            "reason": "配置内部 MinIO"
        }),
        "1",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "开发环境必须允许 http endpoint: {body}"
    );
    // 258d180 起 PATCH 持久化到 site_settings（managed_by=database，热重载生效）
    assert_eq!(body["managed_by"], "database", "{body}");
    assert_eq!(body["source"], "database", "{body}");
    assert_eq!(body["configured"], true, "{body}");

    // 生产环境：仅 https
    let prod_app = build_router(
        AppConfig {
            env: "production".to_string(),
            ..AppConfig::default()
        },
        Some(pool.clone()),
    );
    let (status, body) = authed_patch(
        &prod_app,
        "/api/v1/admin/storage/config",
        &session,
        &csrf,
        json!({
            "backend": "s3",
            "s3_endpoint": "http://10.10.10.10:9000",
            "s3_region": "auto",
            "s3_bucket": "bblbb",
            "expected_version": 1,
            "reason": "配置内部 MinIO"
        }),
        "1",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "生产环境必须拒绝 http endpoint: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
