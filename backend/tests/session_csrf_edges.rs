//! M02-SESSION-11：Session/CSRF 边界测试——
//! 反向代理 Set-Cookie 传播、错误 token、其他 Session 的 token、
//! 跨 Origin 与无 Referer 请求。
//!
//! 契约（SECURITY.md §4）：
//! - Session cookie 为 `__Host-` 前缀（Secure/HttpOnly/Path=/ 无 Domain），
//!   反向代理可原样传播且客户端可回带；
//! - CSRF token 与 Session 绑定：其他 Session 的 token 或错误 token 一律
//!   403 `csrf_failed`；
//! - Cookie 写请求校验 Origin（缺则 Referer）：跨 Origin 400
//!   `origin_not_allowed`；两者皆缺放行（非浏览器客户端）。

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
    let dir = std::env::temp_dir().join(format!("bblbb-csrfedge-{}", uuid::Uuid::now_v7()));
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

/// 反向代理风格登录（携带 x-forwarded-for/proto 与 Host），返回完整
/// Set-Cookie 值。
async fn login_proxy_style(app: &Router, email: &str) -> String {
    let (cookie, csrf) = common::fetch_preauth(app).await;
    let cookie_value = cookie.split(';').next().unwrap().to_string();
    let body = json!({ "identifier": email, "password": PASSWORD });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "203.0.113.9")
                .header("x-forwarded-proto", "https")
                .header(header::HOST, "forum.example.com")
                .header("cookie", cookie_value)
                .header("x-csrf-token", csrf)
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
        .to_string()
}

/// 反向代理传播：登录 Set-Cookie 为完整 `__Host-` 属性（可被代理原样转发），
/// 客户端回带后会话可用（round-trip）。
#[tokio::test]
async fn set_cookie_propagates_through_proxy_style_request() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "alice").await;

    let set_cookie = login_proxy_style(&app, &email).await;
    // __Host- 前缀要求：Secure + Path=/ + 无 Domain（代理可安全透传）
    assert!(
        set_cookie.starts_with("__Host-bblbb_session="),
        "必须签发 __Host- session cookie: {set_cookie}"
    );
    assert!(set_cookie.contains("Secure"), "必须有 Secure");
    assert!(set_cookie.contains("HttpOnly"), "必须有 HttpOnly");
    assert!(set_cookie.contains("Path=/"), "必须有 Path=/");
    assert!(set_cookie.contains("SameSite=Lax"), "必须有 SameSite=Lax");
    assert!(
        !set_cookie.contains("Domain="),
        "__Host- 禁止 Domain 属性（防子域伪造）"
    );
    assert!(set_cookie.contains("Max-Age="), "必须有 Max-Age");

    // 客户端回带 cookie（代理往返后原样）→ 会话可用
    let session_cookie = set_cookie.split(';').next().unwrap().to_string();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/sessions")
                .header(header::HOST, "forum.example.com")
                .header("cookie", &session_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "代理往返后的 session cookie 必须仍有效"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误 token（任意伪造）→ 403 csrf_failed。
#[tokio::test]
async fn session_write_with_wrong_token_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "bob").await;
    let set_cookie = login_proxy_style(&app, &email).await;
    let session_cookie = set_cookie.split(';').next().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &session_cookie)
                .header("x-csrf-token", "f".repeat(64))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "csrf_failed");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 其他 Session 的 token 不能用于本 Session（token 与 Session 绑定）。
#[tokio::test]
async fn other_sessions_token_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "carol").await;

    // 两个独立登录 → 两个 Session
    let (c1, t1_pre) = common::fetch_preauth(&app).await;
    let c1 = c1.split(';').next().unwrap().to_string();
    let login = |cookie: &str, csrf: &str, ip: &str| {
        let body = json!({ "identifier": email, "password": PASSWORD });
        app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", ip)
                .header("cookie", cookie)
                .header("x-csrf-token", csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
    };
    let s1 = login(&c1, &t1_pre, "198.51.100.1").await.unwrap();
    assert_eq!(s1.status(), StatusCode::OK);
    let s1_cookie = s1
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    let (c2, t2_pre) = common::fetch_preauth(&app).await;
    let c2 = c2.split(';').next().unwrap().to_string();
    let s2 = login(&c2, &t2_pre, "198.51.100.2").await.unwrap();
    assert_eq!(s2.status(), StatusCode::OK);
    let s2_cookie = s2
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // 取各自 Session 绑定的 CSRF token
    let session_csrf = |cookie: &str| {
        app.clone().oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
    };
    let t1: Value = serde_json::from_slice(
        &session_csrf(&s1_cookie)
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let t1 = t1["token"].as_str().unwrap().to_string();
    let t2: Value = serde_json::from_slice(
        &session_csrf(&s2_cookie)
            .await
            .unwrap()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes(),
    )
    .unwrap();
    let t2 = t2["token"].as_str().unwrap().to_string();
    assert_ne!(t1, t2, "不同 Session 必须派生不同 CSRF token");

    // Session 2 的 token 用于 Session 1 → 403
    let wrong = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &s1_cookie)
                .header("x-csrf-token", &t2)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        wrong.status(),
        StatusCode::FORBIDDEN,
        "其他 Session 的 token 必须拒绝"
    );

    // 本 Session 自己的 token → 204
    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &s1_cookie)
                .header("x-csrf-token", &t1)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::NO_CONTENT);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 跨 Origin 预认证写（verify-email）：即使预认证 CSRF 正确也 400。
#[tokio::test]
async fn preauth_verify_email_cross_origin_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let cookie_value = cookie.split(';').next().unwrap().to_string();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/verify-email")
                .header("content-type", "application/json")
                .header("cookie", &cookie_value)
                .header("x-csrf-token", &csrf)
                .header(header::HOST, "example.com")
                .header(header::ORIGIN, "https://evil.com")
                .body(Body::from(r#"{"token":"whatever"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "origin_not_allowed");

    // 同源 → 放行到 token 校验（统一 400 invalid or expired）
    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/verify-email")
                .header("content-type", "application/json")
                .header("cookie", cookie_value)
                .header("x-csrf-token", csrf)
                .header(header::HOST, "example.com")
                .header(header::ORIGIN, "https://example.com")
                .body(Body::from(r#"{"token":"whatever"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::BAD_REQUEST);
    let body: Value =
        serde_json::from_slice(&ok.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("invalid or expired"),
        "同源放行后由 token 校验统一处理: {}",
        body["detail"]
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 无 Origin 且无 Referer（非浏览器客户端）→ Session 写请求放行。
#[tokio::test]
async fn session_write_without_origin_or_referer_passes() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "dave").await;
    let set_cookie = login_proxy_style(&app, &email).await;
    let session_cookie = set_cookie.split(';').next().unwrap().to_string();

    // 取 Session 绑定 CSRF
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
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let csrf = body["token"].as_str().unwrap().to_string();

    // 无 Origin/无 Referer → 放行（策略：非浏览器客户端，SameSite=Lax 已阻断跨站携带）
    let ok = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &session_cookie)
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok.status(), StatusCode::NO_CONTENT);

    close_pool(&pool).await;
    cleanup(&dir);
}
