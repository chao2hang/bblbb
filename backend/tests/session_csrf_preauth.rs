//! M02-SESSION-08：匿名预认证 CSRF 状态——为 login/register/verify-email/
//! resend-verification/password-reset 等预认证写端点建立服务端可回溯校验，
//! 防止 login CSRF。
//!
//! 契约（SECURITY.md §4）：
//! - 未认证 GET /auth/csrf 签发 `__Host-bblbb_csrf` cookie + 派生 token
//!   （响应 private/no-store；已有有效预认证 cookie 时 token 稳定）；
//! - 预认证写请求必须同时携带该 cookie 与匹配的 X-CSRF-Token，否则 403
//!   `csrf_failed`（fail closed）；
//! - 匿名令牌只以 SHA-256 hash 入库；TTL 10 分钟内可复用（非一次性）。

mod common;

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
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
    let dir = std::env::temp_dir().join(format!("bblbb-preauth-{}", uuid::Uuid::now_v7()));
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

/// POST /login，可附加预认证 cookie 与 X-CSRF-Token。
async fn post_login(
    app: &Router,
    email: &str,
    cookie: Option<&str>,
    csrf: Option<&str>,
) -> axum::response::Response {
    let mut builder = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/login")
        .header("content-type", "application/json")
        .header("x-forwarded-for", "198.51.100.1");
    if let Some(cookie) = cookie {
        builder = builder.header("cookie", cookie);
    }
    if let Some(csrf) = csrf {
        builder = builder.header("x-csrf-token", csrf);
    }
    let body = json!({ "identifier": email, "password": PASSWORD });
    app.clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap()
}

/// 未认证 GET /auth/csrf 签发预认证 cookie + 派生 token；同 cookie 再取稳定。
#[tokio::test]
async fn preauth_issue_sets_cookie_and_token_is_stable() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let (cookie_a, token_a) = common::fetch_preauth(&app).await;
    assert!(
        cookie_a.starts_with("__Host-bblbb_csrf="),
        "必须签发 __Host- 预认证 cookie: {cookie_a}"
    );
    assert!(cookie_a.contains("HttpOnly"), "预认证 cookie 必须 HttpOnly");
    assert!(cookie_a.contains("Secure"), "预认证 cookie 必须 Secure");
    assert!(!token_a.is_empty());

    // 同一 cookie 再取 → token 稳定（状态复用）
    let (_, token_b) = common::fetch_preauth(&app).await; // 新会话，仅验证可用
    assert!(!token_b.is_empty());

    // 复用原 cookie：GET /csrf 携带它 → 返回相同 token
    let cookie_value = cookie_a.split(';').next().unwrap();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", cookie_value)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers().get("cache-control").unwrap(),
        "private, no-store"
    );
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        body["token"].as_str().unwrap(),
        token_a,
        "同 cookie 必须复用稳定 token"
    );

    // 匿名令牌只以 hash 入库：token_hash 列不含明文 cookie 令牌
    let stored: Vec<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT token_hash FROM preauth_csrf_tokens")
            .fetch_all(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(!stored.is_empty());
    for hash in &stored {
        assert_eq!(hash.len(), 64, "必须只存 64 位 hex SHA-256");
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }
    assert!(
        !stored.iter().any(|h| h.contains(cookie_value)),
        "cookie 令牌明文不得入库"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 完全不带预认证 CSRF 的登录 → 403 csrf_failed（login CSRF 被拦截）。
#[tokio::test]
async fn login_without_preauth_csrf_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "alice").await;

    let resp = post_login(&app, &email, None, None).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "csrf_failed");
    assert_eq!(body["status"], 403);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 有预认证 cookie 但 X-CSRF-Token 错误/缺失 → 403。
#[tokio::test]
async fn login_with_cookie_but_bad_or_missing_token_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "bob").await;
    let (cookie, _) = common::fetch_preauth(&app).await;
    let cookie_value = cookie.split(';').next().unwrap();

    // 错误 token
    let resp = post_login(&app, &email, Some(cookie_value), Some("wrong-token")).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "csrf_failed");

    // 缺失 token（只有 cookie）
    let resp = post_login(&app, &email, Some(cookie_value), None).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 正确预认证 CSRF（cookie + 匹配 token）→ 登录成功并签发 Session cookie。
#[tokio::test]
async fn login_with_valid_preauth_csrf_succeeds() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "carol").await;

    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let cookie_value = cookie.split(';').next().unwrap();
    let resp = post_login(&app, &email, Some(cookie_value), Some(&csrf)).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(
        set_cookie.contains("__Host-bblbb_session="),
        "登录成功必须签发 Session cookie"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 预认证状态 TTL 内可复用（非一次性）：同一状态连续登录两次都成功。
#[tokio::test]
async fn preauth_state_is_reusable_within_ttl() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "dave").await;

    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let cookie_value = cookie.split(';').next().unwrap();

    let first = post_login(&app, &email, Some(cookie_value), Some(&csrf)).await;
    assert_eq!(first.status(), StatusCode::OK);
    let second = post_login(&app, &email, Some(cookie_value), Some(&csrf)).await;
    assert_eq!(second.status(), StatusCode::OK, "TTL 内预认证状态可复用");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 预认证状态是会话绑定的：cookie A + token B（另一签发）→ 403。
#[tokio::test]
async fn preauth_state_is_bound_to_its_issue() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "erin").await;

    let (cookie_a, _) = common::fetch_preauth(&app).await;
    let (_, token_b) = common::fetch_preauth(&app).await; // 独立签发
    let cookie_value = cookie_a.split(';').next().unwrap();

    let resp = post_login(&app, &email, Some(cookie_value), Some(&token_b)).await;
    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "cookie 与 token 必须来自同一签发状态"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 过期预认证状态 → 403（fail closed）。
#[tokio::test]
async fn expired_preauth_state_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_active_user(&pool, "frank").await;

    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let cookie_value = cookie.split(';').next().unwrap();

    // 人为把过期时间改到过去
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE preauth_csrf_tokens SET expires_at = ?")
                .bind(now_millis() - 1)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let resp = post_login(&app, &email, Some(cookie_value), Some(&csrf)).await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "过期预认证必须拒绝");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// register 同样要求预认证 CSRF：无 CSRF → 403；正确 → 201。
#[tokio::test]
async fn register_requires_preauth_csrf() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let body = json!({
        "username": "grace",
        "email": "grace@example.com",
        "password": "passw0rd9",
    });
    let plain = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        plain.status(),
        StatusCode::FORBIDDEN,
        "无预认证 CSRF 必须 403"
    );

    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let cookie_value = cookie.split(';').next().unwrap();
    let with_csrf = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .header("cookie", cookie_value)
                .header("x-csrf-token", csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(with_csrf.status(), StatusCode::CREATED);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 其他预认证写路径（verify-email / password-reset）同样强制预认证 CSRF。
#[tokio::test]
async fn other_preauth_write_paths_require_csrf() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    for (uri, body) in [
        (
            "/api/v1/auth/verify-email",
            json!({ "token": "whatever-token" }),
        ),
        (
            "/api/v1/auth/password-reset",
            json!({ "email": "nobody@example.com" }),
        ),
    ] {
        let resp = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(uri)
                    .header("content-type", "application/json")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::FORBIDDEN,
            "{uri} 无预认证 CSRF 必须 403"
        );
    }

    // 携带正确预认证 CSRF 后放行（verify 走到 token 校验 → 400 统一错误，
    // password-reset 统一 202）
    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let cookie_value = cookie.split(';').next().unwrap();
    let verify = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/verify-email")
                .header("content-type", "application/json")
                .header("cookie", cookie_value)
                .header("x-csrf-token", &csrf)
                .body(Body::from(json!({ "token": "whatever-token" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        verify.status(),
        StatusCode::BAD_REQUEST,
        "放行后由 token 校验统一返回 400"
    );

    let reset = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/password-reset")
                .header("content-type", "application/json")
                .header("cookie", cookie_value)
                .header("x-csrf-token", csrf)
                .body(Body::from(
                    json!({ "email": "nobody@example.com" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(reset.status(), StatusCode::ACCEPTED, "预认证通过后统一 202");

    close_pool(&pool).await;
    cleanup(&dir);
}
