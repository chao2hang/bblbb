//! M02-SESSION-10：Bearer-only 请求（`Authorization: Bearer` + 无 Cookie）
//! 不被错误要求 CSRF；GET/HEAD/OPTIONS 为幂等读方法，不被 CSRF 拦截且无
//! 业务副作用。
//!
//! 契约（SECURITY.md §4）：
//! - Bearer Token API 不依赖 Cookie 时不要求 CSRF，但必须防 Token 泄漏；
//! - GET/HEAD/OPTIONS 必须无业务副作用。

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
    let dir = std::env::temp_dir().join(format!("bblbb-bearer-{}", uuid::Uuid::now_v7()));
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

/// 登录并返回 session cookie 值。
async fn login_cookie(app: &Router, email: &str) -> String {
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
                .header("x-forwarded-for", "198.51.100.1")
                .header("cookie", &cookie_value)
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
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

/// Bearer-only 写请求（无 Cookie）不被 CSRF 拦截：到达真实处理器（鉴权层）。
#[tokio::test]
async fn bearer_only_write_passes_csrf_without_token() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/storage/test")
                .header(header::AUTHORIZATION, "Bearer eyJhbGciOiJIUzI1NiJ9")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(r#"{"reason":"test"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    // 401 = 请求穿透 CSRF 到达真实处理器（Bearer 无效被鉴权拒绝）；
    // 403 = 被 CSRF 错误拦截。
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "Bearer-only 写请求不得被 CSRF 拦截"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Bearer-only POST /login 不被错误要求预认证 CSRF（到达登录处理器）。
#[tokio::test]
async fn bearer_only_login_not_wrongly_csrf_required() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "alice").await;

    let body = json!({ "identifier": email, "password": PASSWORD });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.1")
                .header(header::AUTHORIZATION, "Bearer eyJhbGciOiJIUzI1NiJ9")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "Bearer-only 登录不得被预认证 CSRF 拦截"
    );
    assert_eq!(resp.status(), StatusCode::OK);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Bearer + 会话 Cookie 同时存在：Cookie 维度仍强制 CSRF（不因 Bearer 豁免）。
#[tokio::test]
async fn bearer_with_session_cookie_still_requires_csrf() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "bob").await;
    let session_cookie = login_cookie(&app, &email).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &session_cookie)
                .header(header::AUTHORIZATION, "Bearer eyJhbGciOiJIUzI1NiJ9")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "携带会话 Cookie 的写请求必须校验 CSRF（Bearer 不能豁免 Cookie 维度）"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// GET/HEAD/OPTIONS 不被 CSRF 拦截（幂等读方法，即使携带会话 Cookie 也无 token）。
#[tokio::test]
async fn get_head_options_are_not_csrf_protected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "carol").await;
    let session_cookie = login_cookie(&app, &email).await;

    // GET：带会话 Cookie、无 X-CSRF-Token → 不被 CSRF 拦截，返回会话列表
    let get = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &session_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(get.status(), StatusCode::OK, "GET 不得被 CSRF 拦截");

    // HEAD：同路由，无 token → 不被 CSRF 拦截
    let head = app
        .clone()
        .oneshot(
            Request::builder()
                .method("HEAD")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &session_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        head.status(),
        StatusCode::FORBIDDEN,
        "HEAD 不得被 CSRF 拦截"
    );

    // OPTIONS：不被 CSRF 拦截
    let options = app
        .clone()
        .oneshot(
            Request::builder()
                .method("OPTIONS")
                .uri("/api/v1/auth/sessions")
                .header("cookie", &session_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_ne!(
        options.status(),
        StatusCode::FORBIDDEN,
        "OPTIONS 不得被 CSRF 拦截"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// GET 无业务副作用：GET 会话列表不创建/修改任何会话。
#[tokio::test]
async fn get_has_no_business_side_effect() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "dave").await;
    let session_cookie = login_cookie(&app, &email).await;

    let session_count = async |pool: &DatabasePool| -> i64 {
        match pool {
            Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM user_sessions")
                .fetch_one(p)
                .await
                .unwrap(),
            Either::Right(_) => panic!("SQLite only"),
        }
    };

    // 注意：login 已创建 1 个会话；GET 之后计数必须保持不变
    let before = session_count(&pool).await;
    for _ in 0..3 {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/api/v1/auth/sessions")
                    .header("cookie", &session_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
    let after = session_count(&pool).await;
    assert_eq!(after, before, "GET 不得创建/撤销任何会话（无业务副作用）");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 携带 Cookie 的 PUT 写请求仍被 CSRF 保护（读豁免不扩展到写）。
#[tokio::test]
async fn cookie_put_write_is_still_csrf_protected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "erin").await;
    let session_cookie = login_cookie(&app, &email).await;

    // 无 token 的 PUT（带会话 Cookie）→ 403 csrf_failed
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/me/preferences/theme")
                .header("content-type", "application/json")
                .header("cookie", &session_cookie)
                .body(Body::from(r#"{"theme":"dark"}"#))
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
