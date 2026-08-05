//! M02-SESSION-07：Session 绑定 synchronizer CSRF token——
//! 同一会话 token 稳定、跨会话不同、端点 private/no-store、错误 token 拒绝。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-csrf-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

const PASSWORD: &str = "correct-password";

async fn login_cookie(app: &Router, email: &str, ip: &str) -> String {
    let body = json!({ "identifier": email, "password": PASSWORD });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", ip)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    resp.headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let hash = bblbb_backend::auth::hash_password(PASSWORD).unwrap();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_user"))
            .bind(&email)
            .bind(&hash)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

/// GET /api/v1/auth/csrf，返回 (cache-control, token)。
async fn get_csrf(app: &Router, cookie: Option<&str>) -> (String, String) {
    let mut builder = Request::builder().method("GET").uri("/api/v1/auth/csrf");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cache_control = resp
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (cache_control, body["token"].as_str().unwrap().to_string())
}

/// 端点 private/no-store + 同一会话 token 稳定（synchronizer 派生）。
#[tokio::test]
async fn csrf_endpoint_returns_no_store_and_stable_session_token() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = "alice@example.com".to_string();
    insert_user(&pool, "alice").await;
    let cookie = login_cookie(&app, &email, "198.51.100.1").await;

    let (cache_a, token_a) = get_csrf(&app, Some(&cookie)).await;
    assert_eq!(cache_a, "private, no-store");
    let (cache_b, token_b) = get_csrf(&app, Some(&cookie)).await;
    assert_eq!(cache_b, "private, no-store");

    assert_eq!(token_a, token_b, "同一会话 CSRF token 必须稳定");
    assert!(!token_a.is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 不同 Session → 不同 CSRF token（Session 绑定 synchronizer）。
#[tokio::test]
async fn csrf_token_differs_per_session() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    insert_user(&pool, "bob").await;
    let cookie_a = login_cookie(&app, "bob@example.com", "198.51.100.1").await;
    let cookie_b = login_cookie(&app, "bob@example.com", "198.51.100.2").await;

    let (_, token_a) = get_csrf(&app, Some(&cookie_a)).await;
    let (_, token_b) = get_csrf(&app, Some(&cookie_b)).await;
    assert_ne!(token_a, token_b, "不同 Session 必须派生不同 token");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未认证：也返回 token（预认证流程）且 no-store。
#[tokio::test]
async fn csrf_endpoint_unauthenticated_returns_token_with_no_store() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let (cache, token) = get_csrf(&app, None).await;
    assert_eq!(cache, "private, no-store");
    assert!(!token.is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 状态变更请求：错误 X-CSRF-Token → 403 csrf_failed；正确 token → 放行。
#[tokio::test]
async fn state_changing_request_validates_csrf_token() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    insert_user(&pool, "carol").await;
    let cookie = login_cookie(&app, "carol@example.com", "198.51.100.1").await;
    let (_, csrf) = get_csrf(&app, Some(&cookie)).await;

    // 错误 token → 403
    let bad = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &cookie)
                .header("x-csrf-token", "wrong-token")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::FORBIDDEN);
    let body: Value =
        serde_json::from_slice(&bad.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "csrf_failed");

    // 正确 token → 放行
    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::NO_CONTENT);

    close_pool(&pool).await;
    cleanup(&dir);
}
