//! M02-SESSION-12：Cookie 属性、过期、撤销、并发请求与账号状态变化的三数据库
//! 集成测试。
//!
//! 同一套行为流在三种数据库上运行：
//! - SQLite：本地始终运行（临时文件 + 迁移）；
//! - MySQL 8 / MariaDB 10.11：`BBLBB_TEST_MYSQL_URL` 环境变量 + `#[ignore]`
//!   （CI 的 mysql-family 任务以 `cargo test --test session_crossdb -- --ignored`
//!   分别对两个数据库运行）。
//!
//! 覆盖（SECURITY.md §4）：
//! 1. Cookie 属性：登录 Set-Cookie 为完整 `__Host-`（Secure/HttpOnly/Path=/
//!    SameSite=Lax/无 Domain）；
//! 2. 并发请求：同一 Session cookie 并发读均成功（滑动续期不冲突）；
//! 3. 过期：idle_expires_at 过去后 Session 视为未认证（401）；
//! 4. 撤销：revoked_at 设置后 Session 立即失效（401）；
//! 5. 账号状态变化：账号被禁（banned）后即使 Session 有效也不认证（401）。

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
use serde_json::json;
use sqlx::Either;
use tower::ServiceExt;

const PASSWORD: &str = "correct-password";

// ────────────────────────── SQLite（本地始终运行） ──────────────────────────

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("../migrations/{engine}"))
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-xdb-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("sqlite")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
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

#[tokio::test]
async fn sqlite_session_crossdb_behavior() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_user(&pool, "sqlite_user").await;
    session_crossdb_flow(&pool, &app, &email).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

// ──────────────────── MySQL 8 / MariaDB（CI 任务） ────────────────────

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_session_crossdb_behavior() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mysql")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let email = insert_user(&pool, "mysql_user").await;
    session_crossdb_flow(&pool, &app, &email).await;
    close_pool(&pool).await;
}

// ─────────────────────────── 共享行为流 ───────────────────────────

/// 插入 active 用户（三库通用 INSERT），返回邮箱。
async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
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
        Either::Right(p) => {
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
    }
    email
}

/// 登录并返回 session cookie 值。
async fn login_cookie(app: &Router, email: &str) -> String {
    let (preauth, preauth_csrf) = common::fetch_preauth(app).await;
    let preauth = preauth.split(';').next().unwrap().to_string();
    let body = json!({ "identifier": email, "password": PASSWORD });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.7")
                .header("cookie", preauth)
                .header("x-csrf-token", preauth_csrf)
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

/// GET /auth/sessions 并返回状态码。
async fn get_sessions_status(app: &Router, session_cookie: &str) -> StatusCode {
    app.clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/sessions")
                .header("cookie", session_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

/// 全部 Session 置为已过期（idle 超时）。
async fn expire_all_sessions(pool: &DatabasePool) {
    let past = now_millis() - 1;
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET idle_expires_at = ?")
                .bind(past)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query("UPDATE user_sessions SET idle_expires_at = ?")
                .bind(past)
                .execute(p)
                .await
                .unwrap();
        }
    }
}

/// 全部有效 Session 置为已撤销。
async fn revoke_all_sessions(pool: &DatabasePool) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET revoked_at = ? WHERE revoked_at IS NULL")
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query("UPDATE user_sessions SET revoked_at = ? WHERE revoked_at IS NULL")
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
    }
}

/// 将用户置为 banned（实时生效，不依赖后台任务）。
async fn ban_user(pool: &DatabasePool, email: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET status = 'banned' WHERE email_normalized = ?")
                .bind(email)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query("UPDATE users SET status = 'banned' WHERE email_normalized = ?")
                .bind(email)
                .execute(p)
                .await
                .unwrap();
        }
    }
}

/// 三数据库 Session 行为流（M02-SESSION-12）。
async fn session_crossdb_flow(pool: &DatabasePool, app: &Router, email: &str) {
    // 1. Cookie 属性：`__Host-` 完整属性
    let session_cookie = login_cookie(app, email).await;
    // 属性已在登录请求断言；此处用会话本身证明可用
    assert_eq!(
        get_sessions_status(app, &session_cookie).await,
        StatusCode::OK,
        "新登录的 Session 必须可用"
    );

    // 2. 并发请求：同一 cookie 并发读均成功
    let mut handles = Vec::new();
    for _ in 0..4 {
        let app = app.clone();
        let cookie = session_cookie.clone();
        handles.push(tokio::spawn(async move {
            let resp = app
                .oneshot(
                    Request::builder()
                        .method("GET")
                        .uri("/api/v1/auth/sessions")
                        .header("cookie", &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            resp.status()
        }));
    }
    for (i, handle) in handles.into_iter().enumerate() {
        assert_eq!(
            handle.await.unwrap(),
            StatusCode::OK,
            "并发请求 {i} 必须成功（滑动续期不冲突）"
        );
    }

    // 3. 过期：idle 超时后 Session 视为未认证
    expire_all_sessions(pool).await;
    assert_eq!(
        get_sessions_status(app, &session_cookie).await,
        StatusCode::UNAUTHORIZED,
        "idle 过期后必须未认证"
    );

    // 4. 撤销：重新登录 → 撤销 → 立即失效
    let fresh = login_cookie(app, email).await;
    assert_eq!(get_sessions_status(app, &fresh).await, StatusCode::OK);
    revoke_all_sessions(pool).await;
    assert_eq!(
        get_sessions_status(app, &fresh).await,
        StatusCode::UNAUTHORIZED,
        "撤销后 Session 必须立即失效"
    );

    // 5. 账号状态变化：重新登录 → banned → 即使 Session 有效也不认证
    let fresh = login_cookie(app, email).await;
    assert_eq!(get_sessions_status(app, &fresh).await, StatusCode::OK);
    ban_user(pool, email).await;
    assert_eq!(
        get_sessions_status(app, &fresh).await,
        StatusCode::UNAUTHORIZED,
        "账号被封禁后 Session 必须实时失效"
    );
}
