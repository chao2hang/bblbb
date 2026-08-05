//! M02-SESSION-03：登录——常量时间失败 + 统一 invalid credentials（不区分
//! 账号不存在/密码错误/账号状态）、每 IP 限流、每账号连续失败 5 次锁定 10 分钟。

use std::path::{Path, PathBuf};
use std::time::Instant;

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
use serde_json::json;
use sqlx::Either;
use tower::ServiceExt;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-login-{}", uuid::Uuid::now_v7()));
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

const CORRECT_PASSWORD: &str = "correct-password";

/// 插入 active 用户（真实 Argon2id hash），返回 (user_id, username, email)。
async fn insert_active_user(pool: &DatabasePool, tag: &str) -> (String, String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_user");
    let email = format!("{tag}@example.com");
    let hash = hash_password(CORRECT_PASSWORD).unwrap();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
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
    (user_id, username, email)
}

async fn failed_login_count(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT failed_login_count FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn locked_until(pool: &DatabasePool, user_id: &str) -> Option<i64> {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT locked_until FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn post_login(
    app: &Router,
    identifier: &str,
    password: &str,
    ip: &str,
) -> axum::response::Response {
    let body = json!({ "identifier": identifier, "password": password });
    app.clone()
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
        .unwrap()
}

/// 登录成功：Set-Cookie __Host- + me 响应；连续失败计数重置。
#[tokio::test]
async fn login_success_sets_cookie_and_resets_failure_count() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, username, email) = insert_active_user(&pool, "alice").await;
    // 预置失败计数，验证成功登录重置
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET failed_login_count = 3 WHERE id = ?")
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let resp = post_login(&app, &email, CORRECT_PASSWORD, "198.51.100.1").await;
    assert_eq!(resp.status(), StatusCode::OK);

    let set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(
        set_cookie.contains("__Host-bblbb_session="),
        "必须签发 __Host- session cookie: {set_cookie}"
    );
    assert!(set_cookie.contains("Secure"));
    assert!(set_cookie.contains("HttpOnly"));

    let body: serde_json::Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["username"], username);
    assert_eq!(body["email"], email);
    assert_eq!(body["status"], "active");

    // 连续失败计数重置
    assert_eq!(failed_login_count(&pool, &user_id).await, 0);
    assert_eq!(locked_until(&pool, &user_id).await, None);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误密码：统一 401 invalid credentials，连续失败计数递增。
#[tokio::test]
async fn login_wrong_password_increments_failure_count() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _, email) = insert_active_user(&pool, "bob").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let resp = post_login(&app, &email, "wrong-password", "198.51.100.1").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "unauthorized");
    assert!(body["detail"]
        .as_str()
        .unwrap()
        .contains("invalid credentials"));

    assert_eq!(failed_login_count(&pool, &user_id).await, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 账号不存在：统一 401（与错误密码相同响应体），不创建 Session。
#[tokio::test]
async fn login_unknown_account_returns_unified_401() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let resp = post_login(&app, "ghost@example.com", CORRECT_PASSWORD, "198.51.100.1").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    assert!(
        resp.headers().get("set-cookie").is_none(),
        "登录失败不得签发 Session cookie"
    );
    let body: serde_json::Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "unauthorized");

    let sessions: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM user_sessions")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(sessions, 0, "登录失败不得创建 Session");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 连续 5 次失败 → 账号锁定（locked_until 设置）；第 6 次即使密码正确也 429。
#[tokio::test]
async fn login_account_locks_after_five_failures() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _, email) = insert_active_user(&pool, "carol").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    for _ in 0..5 {
        let resp = post_login(&app, &email, "wrong-password", "198.51.100.1").await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
    assert_eq!(failed_login_count(&pool, &user_id).await, 5);
    let locked = locked_until(&pool, &user_id).await;
    assert!(
        locked.unwrap_or(0) > now_millis(),
        "连续 5 次失败必须锁定账号"
    );

    // 第 6 次：即使密码正确 → 429 锁定（账号维度限流）
    let resp = post_login(&app, &email, CORRECT_PASSWORD, "198.51.100.1").await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = resp.headers().get("retry-after").unwrap().to_str().unwrap();
    assert!(retry_after.parse::<u64>().unwrap() >= 1);
    let body: serde_json::Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "rate_limited");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// banned 账号登录：统一 invalid credentials（不泄漏账号状态）。
#[tokio::test]
async fn login_banned_account_returns_unified_401() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, _, email) = insert_active_user(&pool, "dave").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET status = 'banned' WHERE email_normalized = ?")
                .bind(&email)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let resp = post_login(&app, &email, CORRECT_PASSWORD, "198.51.100.1").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let body: serde_json::Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "unauthorized");
    assert!(
        !body["detail"].as_str().unwrap().contains("banned"),
        "不得泄漏封禁状态"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 每 IP 限流：10 次/分钟后第 11 次登录尝试 → 429。
/// 用不同（不存在的）账号避免触发账号锁定，单独测 IP 维度。
#[tokio::test]
async fn login_ip_rate_limited_after_ten_attempts() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, _, email) = insert_active_user(&pool, "erin").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    for i in 0..10 {
        // 每次不同不存在账号（不触发账号锁定），IP 计数始终在查询前消耗
        let resp = post_login(
            &app,
            &format!("user{i}@example.com"),
            "wrong-password",
            "203.0.113.1",
        )
        .await;
        assert_eq!(
            resp.status(),
            StatusCode::UNAUTHORIZED,
            "第 {} 次应放行",
            i + 1
        );
    }
    let resp = post_login(&app, "user10@example.com", "wrong-password", "203.0.113.1").await;
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "第 11 次必须 429"
    );
    assert_eq!(resp.headers().get("ratelimit-limit").unwrap(), "10");

    // 其他 IP 不受影响
    let resp = post_login(&app, &email, CORRECT_PASSWORD, "203.0.113.2").await;
    assert_eq!(resp.status(), StatusCode::OK);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 常量时间：账号不存在与密码错误两条路径都执行完整 Argon2id 验证（不短路）。
#[tokio::test]
async fn login_constant_time_both_paths_are_expensive() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, _, email) = insert_active_user(&pool, "frank").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    // 已存在账号 + 错误密码：完整验证
    let start = Instant::now();
    let resp = post_login(&app, &email, "wrong-password", "198.51.100.9").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let existing_ms = start.elapsed().as_millis() as i64;

    // 不存在账号：dummy hash 完整验证（不短路）
    let start = Instant::now();
    let resp = post_login(&app, "ghost@example.com", "wrong-password", "198.51.100.8").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let missing_ms = start.elapsed().as_millis() as i64;

    assert!(
        existing_ms >= 5,
        "已存在账号错误密码路径必须执行 Argon2id，实际 {existing_ms}ms"
    );
    assert!(
        missing_ms >= 5,
        "不存在账号路径必须执行 dummy Argon2id（防枚举时序），实际 {missing_ms}ms"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
