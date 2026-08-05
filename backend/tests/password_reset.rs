//! M02-IDENTITY-10：找回密码——统一响应（不泄漏邮箱存在性）、30 分钟
//! 一次性 token、成功改密后撤销全部 Session；请求/确认均单事务 + 限流。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::token::{generate_token, hash_token};
use bblbb_backend::auth::{
    confirm_password_reset, request_password_reset, ConfirmResetError, PasswordResetLimits,
    RequestResetError, RequestResetOutcome,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::ratelimit::RateLimiter;
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
    let dir = std::env::temp_dir().join(format!("bblbb-reset-{}", uuid::Uuid::now_v7()));
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

/// 插入一个用户（status='active'），返回 (user_id, email_normalized)。
async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy-hash', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_user"))
            .bind(&email)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (user_id, email)
}

/// 插入一个未消费 reset token，返回原始 token。
async fn insert_reset_token(pool: &DatabasePool, user_id: &str, expires_in_ms: i64) -> String {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(user_id)
            .bind(&token_hash)
            .bind(now + expires_in_ms)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    token
}

/// 插入一条活跃 session（用于断言改密后撤销）。
async fn insert_session(pool: &DatabasePool, user_id: &str) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
                 VALUES (?, ?, 'sess-hash', 'csrf-hash', ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(user_id)
            .bind(now)
            .bind(now)
            .bind(now + 3600 * 1000)
            .bind(now + 24 * 3600 * 1000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn table_count(pool: &DatabasePool, table: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn unconsumed_reset_tokens(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM password_reset_tokens
                 WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn active_sessions(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_sessions
                 WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn user_password_hash(pool: &DatabasePool, user_id: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 请求重置：新 token + 旧失效 + Outbox（token 引用）+ 审计同事务。
#[tokio::test]
async fn request_reset_creates_token_invalidates_old_and_enqueues_mail() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let (user_id, email) = insert_user(&pool, "alice").await;
    let _old = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;
    assert_eq!(unconsumed_reset_tokens(&pool, &user_id).await, 1);

    let outcome = request_password_reset(
        &pool,
        &limiter,
        &email,
        "req-reset-1",
        &PasswordResetLimits::default(),
    )
    .await
    .expect("请求重置必须成功");
    let RequestResetOutcome::Sent {
        reset_token_id,
        event_id,
    } = outcome
    else {
        panic!("存在的用户必须返回 Sent");
    };
    assert_eq!(reset_token_id.len(), 36);
    assert_eq!(event_id.len(), 36);

    // 旧 token 失效 + 仅保留新 token
    assert_eq!(unconsumed_reset_tokens(&pool, &user_id).await, 1);
    assert_eq!(table_count(&pool, "password_reset_tokens").await, 2);

    // Outbox payload 只含 token 引用，无明文
    assert_eq!(table_count(&pool, "outbox_events").await, 1);
    let payload: serde_json::Value = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT payload FROM outbox_events")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(payload["password_reset_token_id"], reset_token_id);
    assert_eq!(payload["kind"], "password_reset");

    // 审计
    assert_eq!(table_count(&pool, "audit_logs").await, 1);
    let action: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT action FROM audit_logs")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(action, "auth.password_reset_requested");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 统一响应：不存在的邮箱返回 Noop，无 token/Outbox/审计副作用。
#[tokio::test]
async fn request_reset_unknown_email_returns_noop() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();

    let outcome = request_password_reset(
        &pool,
        &limiter,
        "ghost@example.com",
        "req-1",
        &PasswordResetLimits::default(),
    )
    .await
    .expect("未知邮箱必须 Ok(Noop)");
    assert!(matches!(outcome, RequestResetOutcome::Noop));

    assert_eq!(table_count(&pool, "password_reset_tokens").await, 0);
    assert_eq!(table_count(&pool, "outbox_events").await, 0);
    assert_eq!(table_count(&pool, "audit_logs").await, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 冷却：默认 60s 内第二次请求被拒。
#[tokio::test]
async fn request_reset_cooldown_blocks_second() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let (user_id, email) = insert_user(&pool, "bob").await;
    let _ = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;

    request_password_reset(
        &pool,
        &limiter,
        &email,
        "req-1",
        &PasswordResetLimits::default(),
    )
    .await
    .expect("第一次请求成功");

    let err = request_password_reset(
        &pool,
        &limiter,
        &email,
        "req-2",
        &PasswordResetLimits::default(),
    )
    .await
    .unwrap_err();
    let RequestResetError::RateLimited {
        retry_after_secs, ..
    } = err
    else {
        panic!("冷却窗口内必须拒绝");
    };
    assert!(retry_after_secs >= 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 日上限：冷却窗口内等待后仍可请求，直到日上限耗尽（注入小窗口）。
#[tokio::test]
async fn request_reset_daily_limit_blocks_after_limit() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let (user_id, email) = insert_user(&pool, "carol").await;
    let _ = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;

    let limits = PasswordResetLimits {
        cooldown_ms: 1,
        daily_window_ms: 24 * 60 * 60 * 1000,
        daily_limit: 2,
    };
    request_password_reset(&pool, &limiter, &email, "req-1", &limits)
        .await
        .expect("第 1 次成功");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    request_password_reset(&pool, &limiter, &email, "req-2", &limits)
        .await
        .expect("第 2 次成功");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let err = request_password_reset(&pool, &limiter, &email, "req-3", &limits)
        .await
        .unwrap_err();
    assert!(
        matches!(err, RequestResetError::RateLimited { .. }),
        "日上限耗尽必须拒绝"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 确认重置：密码更新 + token 消费 + 全部 Session 撤销 + 审计，单事务。
#[tokio::test]
async fn confirm_reset_updates_password_and_revokes_sessions() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_user(&pool, "dave").await;
    insert_session(&pool, &user_id).await;
    let token = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;
    let new_hash = bblbb_backend::auth::hash_password("new-passw0rd9").unwrap();
    assert_ne!(user_password_hash(&pool, &user_id).await, new_hash);

    let outcome = confirm_password_reset(&pool, &token, &new_hash, "req-confirm")
        .await
        .expect("确认重置必须成功");
    assert_eq!(outcome.user_id, user_id);

    // 密码已更新、token 消费、会话撤销
    assert_eq!(user_password_hash(&pool, &user_id).await, new_hash);
    assert_eq!(unconsumed_reset_tokens(&pool, &user_id).await, 0);
    assert_eq!(
        active_sessions(&pool, &user_id).await,
        0,
        "改密后全部 Session 撤销"
    );
    assert_eq!(table_count(&pool, "audit_logs").await, 1);
    let action: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT action FROM audit_logs")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(action, "auth.password_reset_completed");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 一次性：同一 reset token 第二次确认必须失败（统一 InvalidOrExpired）。
#[tokio::test]
async fn confirm_reset_second_use_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_user(&pool, "erin").await;
    let token = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;
    let new_hash = bblbb_backend::auth::hash_password("new-passw0rd9").unwrap();

    confirm_password_reset(&pool, &token, &new_hash, "req-1")
        .await
        .expect("首次确认成功");

    let err = confirm_password_reset(&pool, &token, &new_hash, "req-2")
        .await
        .unwrap_err();
    assert!(matches!(err, ConfirmResetError::InvalidOrExpired));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 过期 token 拒绝（30 分钟一次性语义）。
#[tokio::test]
async fn confirm_reset_expired_token_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_user(&pool, "frank").await;
    let token = insert_reset_token(&pool, &user_id, -1000).await;
    let new_hash = bblbb_backend::auth::hash_password("new-passw0rd9").unwrap();

    let err = confirm_password_reset(&pool, &token, &new_hash, "req-1")
        .await
        .unwrap_err();
    assert!(matches!(err, ConfirmResetError::InvalidOrExpired));
    // 密码未变
    assert_eq!(user_password_hash(&pool, &user_id).await, "dummy-hash");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 并发确认唯一成功：两个并发请求同一 token，恰好一个成功。
#[tokio::test]
async fn confirm_reset_concurrent_single_winner() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_user(&pool, "grace").await;
    let token = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;
    let new_hash = bblbb_backend::auth::hash_password("new-passw0rd9").unwrap();

    let p1 = pool.clone();
    let p2 = pool.clone();
    let t1 = token.clone();
    let t2 = token.clone();
    let h1 = new_hash.clone();
    let h2 = new_hash.clone();
    let (r1, r2) = tokio::join!(
        async move { confirm_password_reset(&p1, &t1, &h1, "req-1").await },
        async move { confirm_password_reset(&p2, &t2, &h2, "req-2").await },
    );
    assert_eq!(
        [r1, r2].iter().filter(|r| r.is_ok()).count(),
        1,
        "并发确认必须恰好一个成功"
    );
    assert_eq!(user_password_hash(&pool, &user_id).await, new_hash);
    assert_eq!(unconsumed_reset_tokens(&pool, &user_id).await, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 发送一次请求重置请求；`ip` 用于模拟客户端地址。
async fn post_reset_request(app: &Router, email: &str, ip: &str) -> axum::response::Response {
    let body = json!({ "email": email });
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/password-reset")
                .header("content-type", "application/json")
                .header("x-forwarded-for", ip)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// HTTP 层统一响应：存在的邮箱与不存在的邮箱都返回 202 且响应体一致。
#[tokio::test]
async fn reset_endpoint_returns_unified_202() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, email) = insert_user(&pool, "heidi").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let existing = post_reset_request(&app, &email, "198.51.100.1").await;
    assert_eq!(existing.status(), StatusCode::ACCEPTED);
    let existing_body = existing.into_body().collect().await.unwrap().to_bytes();

    let missing = post_reset_request(&app, "ghost@example.com", "198.51.100.2").await;
    assert_eq!(missing.status(), StatusCode::ACCEPTED);
    let missing_body = missing.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(
        existing_body, missing_body,
        "存在与不存在邮箱必须返回相同响应（防枚举）"
    );
    assert_eq!(&existing_body[..], br#"{"ok":true}"#);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// HTTP 层冷却：同一邮箱 60s 内第二次请求 → 429 + Retry-After + 契约错误码。
#[tokio::test]
async fn reset_endpoint_cooldown_returns_429() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, email) = insert_user(&pool, "ivan").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let first = post_reset_request(&app, &email, "198.51.100.3").await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let second = post_reset_request(&app, &email, "198.51.100.3").await;
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = second
        .headers()
        .get("retry-after")
        .expect("429 必须带 Retry-After");
    assert!(retry_after.to_str().unwrap().parse::<u64>().unwrap() >= 1);

    let body: Value =
        serde_json::from_slice(&second.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "rate_limited");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// HTTP 层 confirm：无效/已消费/过期 token 统一 400。
#[tokio::test]
async fn reset_confirm_endpoint_returns_unified_400() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_user(&pool, "judy").await;
    let token = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let confirm = |t: &str| {
        let body = json!({ "token": t, "password": "new-passw0rd9" });
        app.clone().oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/password-reset/confirm")
                .header("content-type", "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
    };

    // 有效 token → 200
    let ok = confirm(&token).await.unwrap();
    assert_eq!(ok.status(), StatusCode::OK);

    // 已消费 token → 400 统一错误
    let again = confirm(&token).await.unwrap();
    assert_eq!(again.status(), StatusCode::BAD_REQUEST);
    let body: Value =
        serde_json::from_slice(&again.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "bad_request");

    // 未知 token → 同一 400 响应
    let bogus = confirm(&generate_token()).await.unwrap();
    assert_eq!(bogus.status(), StatusCode::BAD_REQUEST);

    close_pool(&pool).await;
    cleanup(&dir);
}
