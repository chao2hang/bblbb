//! M02-SESSION-06：每次请求实时执行账号状态（pending/active/restricted/
//! banned/deleted）、封禁与 Session revoked 检查——不依赖后台任务延迟。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::session::{revoke_session as revoke_service, SESSION_COOKIE_NAME};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use serde_json::json;
use sqlx::Either;
use tower::ServiceExt;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-status-{}", uuid::Uuid::now_v7()));
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

/// 插入指定 status 用户并登录，返回 session cookie。
async fn login_as(pool: &DatabasePool, app: &Router, tag: &str, status: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let hash = bblbb_backend::auth::hash_password(PASSWORD).unwrap();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_user"))
            .bind(&email)
            .bind(&hash)
            .bind(status)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let body = json!({ "identifier": email, "password": PASSWORD });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.1")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "status={status} 用户应能登录"
    );
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

async fn get_sessions(app: &Router, cookie: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/sessions")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

/// banned：封禁后下一个请求立即失效（实时生效，不等后台）。
#[tokio::test]
async fn banned_user_session_immediately_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie = login_as(&pool, &app, "alice", "active").await;

    // 登录时有效 → 访问成功
    let resp = get_sessions(&app, &cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);

    // 封禁（实时 UPDATE，模拟 moderation 动作）
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users SET status = 'banned' WHERE email_normalized = 'alice@example.com'",
            )
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let resp = get_sessions(&app, &cookie).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "封禁必须立即拒绝");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// deleted：删除账号后 session 立即失效。
#[tokio::test]
async fn deleted_user_session_immediately_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie = login_as(&pool, &app, "bob", "active").await;

    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users SET status = 'deleted' WHERE email_normalized = 'bob@example.com'",
            )
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let resp = get_sessions(&app, &cookie).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "删除必须立即拒绝");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// active/restricted/pending 用户都可认证（pending 可登录浏览，REQUIREMENTS）。
#[tokio::test]
async fn active_restricted_and_pending_users_can_authenticate() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    for (tag, status) in [
        ("carol", "active"),
        ("dave", "restricted"),
        ("erin", "pending"),
    ] {
        let cookie = login_as(&pool, &app, tag, status).await;
        let resp = get_sessions(&app, &cookie).await;
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "{status} 用户必须能认证（可登录浏览）"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Session revoked：撤销后立即失效（revoked_at 实时检查）。
#[tokio::test]
async fn revoked_session_immediately_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie = login_as(&pool, &app, "frank", "active").await;

    // 从 cookie 提取 token 撤销
    let token = cookie
        .trim_start_matches(&format!("{SESSION_COOKIE_NAME}="))
        .to_string();
    revoke_service(&pool, &token).await.unwrap();

    let resp = get_sessions(&app, &cookie).await;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "撤销后必须立即失效"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
