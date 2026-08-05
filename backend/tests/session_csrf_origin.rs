//! M02-SESSION-09：Cookie 写请求在 CSRF token 之外校验请求来源——
//! `Origin` 必须与请求 Host 同主机或命中 `allowed_origins`；`Origin` 缺失时
//! 按策略校验 `Referer`；两者皆缺放行（非浏览器客户端）。
//!
//! 契约（SECURITY.md §4）：Rust 验证 token 与 Origin；缺少 Origin 时按策略
//! 校验 Referer。预认证写路径（login）同样校验来源（防 login CSRF）。

mod common;

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use bblbb_backend::auth::hash_password;
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
const PASSWORD: &str = "correct-password";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-origin-{}", uuid::Uuid::now_v7()));
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

/// 插入 active 用户，返回邮箱。
async fn insert_active_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let hash = hash_password(PASSWORD).unwrap();
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
    email
}

/// 登录并取 Session 绑定 CSRF，返回 (session cookie 值, csrf token)。
async fn login_with_session_csrf(app: &Router, email: &str) -> (String, String) {
    // 登录（预认证 CSRF）→ session cookie
    let (cookie, csrf) = common::fetch_preauth(app).await;
    let cookie_value = cookie.split(';').next().unwrap().to_string();
    let login_body = json!({ "identifier": email, "password": PASSWORD });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.1")
                .header("cookie", &cookie_value)
                .header("x-csrf-token", csrf)
                .body(Body::from(login_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let session_cookie = resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // 取 Session 绑定 CSRF token
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", &session_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (session_cookie, body["token"].as_str().unwrap().to_string())
}

/// 发送 DELETE /auth/sessions（Session 写请求），可带来源头。
async fn delete_sessions(
    app: &Router,
    session_cookie: &str,
    csrf: &str,
    origin: Option<&str>,
    referer: Option<&str>,
    host: &str,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("DELETE")
        .uri("/api/v1/auth/sessions")
        .header("cookie", session_cookie)
        .header("x-csrf-token", csrf)
        .header(header::HOST, host);
    if let Some(origin) = origin {
        builder = builder.header(header::ORIGIN, origin);
    }
    if let Some(referer) = referer {
        builder = builder.header(header::REFERER, referer);
    }
    app.clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// 跨 Origin 的 Session 写请求（即使 CSRF token 正确）→ 400 origin_not_allowed。
#[tokio::test]
async fn session_write_with_cross_origin_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "alice").await;
    let (session_cookie, csrf) = login_with_session_csrf(&app, &email).await;

    let resp = delete_sessions(
        &app,
        &session_cookie,
        &csrf,
        Some("https://evil.com"),
        None,
        "example.com",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "origin_not_allowed");
    assert_eq!(body["status"], 400);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 同源（Origin 与 Host 同主机）→ 放行 204。
#[tokio::test]
async fn session_write_with_same_origin_passes() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "bob").await;
    let (session_cookie, csrf) = login_with_session_csrf(&app, &email).await;

    let resp = delete_sessions(
        &app,
        &session_cookie,
        &csrf,
        Some("https://example.com"),
        None,
        "example.com",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Origin 缺失 → 按策略校验 Referer：跨站 Referer 拒绝，同站 Referer 放行。
#[tokio::test]
async fn missing_origin_falls_back_to_referer_policy() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "carol").await;
    let (session_cookie, csrf) = login_with_session_csrf(&app, &email).await;

    // 跨站 Referer → 400
    let bad = delete_sessions(
        &app,
        &session_cookie,
        &csrf,
        None,
        Some("https://evil.com/attacker.html"),
        "example.com",
    )
    .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    // 同站 Referer → 204
    let good = delete_sessions(
        &app,
        &session_cookie,
        &csrf,
        None,
        Some("https://example.com/settings"),
        "example.com",
    )
    .await;
    assert_eq!(good.status(), StatusCode::NO_CONTENT);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Origin 与 Referer 皆缺（非浏览器客户端）→ 放行（SameSite=Lax 已阻断跨站携带）。
#[tokio::test]
async fn write_without_origin_or_referer_passes() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "dave").await;
    let (session_cookie, csrf) = login_with_session_csrf(&app, &email).await;

    let resp = delete_sessions(&app, &session_cookie, &csrf, None, None, "example.com").await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 配置 allowed_origins（跨域部署）→ 命中配置的 Origin 放行。
#[tokio::test]
async fn cross_origin_allowed_by_config_passes() {
    let (pool, dir) = pool_with_migrations().await;
    let config = AppConfig {
        allowed_origins: vec!["https://forum.example.com".to_string()],
        ..AppConfig::default()
    };
    let app = build_router(config, Some(pool.clone()));
    let email = insert_active_user(&pool, "erin").await;
    let (session_cookie, csrf) = login_with_session_csrf(&app, &email).await;

    // Origin 命中 allowed_origins（即使与 Host 不同主机）→ 放行
    let resp = delete_sessions(
        &app,
        &session_cookie,
        &csrf,
        Some("https://forum.example.com"),
        None,
        "api.example.com",
    )
    .await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    // 未命中配置也未同源 → 400
    let bad = delete_sessions(
        &app,
        &session_cookie,
        &csrf,
        Some("https://evil.net"),
        None,
        "api.example.com",
    )
    .await;
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 预认证写（login）同样校验来源：跨站 Origin 即使预认证 CSRF 正确也拒绝。
#[tokio::test]
async fn preauth_login_with_cross_origin_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "frank").await;

    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let cookie_value = cookie.split(';').next().unwrap().to_string();
    let login_body = json!({ "identifier": email, "password": PASSWORD });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.1")
                .header("cookie", &cookie_value)
                .header("x-csrf-token", csrf)
                .header(header::HOST, "example.com")
                .header(header::ORIGIN, "https://evil.com")
                .body(Body::from(login_body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "跨站 login（login CSRF）必须 400 origin_not_allowed"
    );
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "origin_not_allowed");

    // 同源 → 登录成功
    let (cookie2, csrf2) = common::fetch_preauth(&app).await;
    let cookie2_value = cookie2.split(';').next().unwrap().to_string();
    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.1")
                .header("cookie", cookie2_value)
                .header("x-csrf-token", csrf2)
                .header(header::HOST, "example.com")
                .header(header::ORIGIN, "https://example.com")
                .body(Body::from(
                    json!({ "identifier": email, "password": PASSWORD }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::OK);

    close_pool(&pool).await;
    cleanup(&dir);
}
