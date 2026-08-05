//! M02-IDENTITY-08：重发验证邮件——统一响应（不泄漏邮箱存在性）、
//! 冷却时间、日上限、旧 token 失效与新 token 生成。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::token::{generate_token, hash_token};
use bblbb_backend::auth::{resend_verification_email, ResendError, ResendLimits, ResendOutcome};
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
    let dir = std::env::temp_dir().join(format!("bblbb-resend-{}", uuid::Uuid::now_v7()));
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

/// 插入 pending 用户，返回 (user_id, email_normalized)。
async fn insert_pending_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'pending', ?, ?)",
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

/// 插入一个未消费验证 token，返回其 hash。
async fn insert_verify_token(pool: &DatabasePool, user_id: &str) -> String {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(user_id)
            .bind(&token_hash)
            .bind(now + 24 * 60 * 60 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    token_hash
}

async fn unconsumed_token_hashes(pool: &DatabasePool, user_id: &str) -> Vec<String> {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT token_hash FROM email_verification_tokens
                 WHERE user_id = ? AND consumed_at IS NULL ORDER BY created_at ASC",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .unwrap(),
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

async fn user_status(pool: &DatabasePool, user_id: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 重发成功：旧 token 失效、新 token 生成、Outbox 事件与审计同事务写入。
#[tokio::test]
async fn resend_creates_new_token_invalidates_old_and_enqueues_mail() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let (user_id, email) = insert_pending_user(&pool, "alice").await;
    let old_hash = insert_verify_token(&pool, &user_id).await;
    assert_eq!(unconsumed_token_hashes(&pool, &user_id).await.len(), 1);

    let outcome = resend_verification_email(
        &pool,
        &limiter,
        &email,
        "req-resend-1",
        &ResendLimits::default(),
    )
    .await
    .expect("重发必须成功");
    let ResendOutcome::Sent {
        verify_token_id,
        event_id,
    } = outcome
    else {
        panic!("pending 用户重发必须返回 Sent");
    };
    assert_eq!(verify_token_id.len(), 36);
    assert_eq!(event_id.len(), 36);

    // 旧 token 失效 + 新 token 生成（未消费 token 恰好 1 个，且是新 hash）
    let remaining = unconsumed_token_hashes(&pool, &user_id).await;
    assert_eq!(remaining.len(), 1, "旧 token 必须失效，仅保留新 token");
    assert_ne!(remaining[0], old_hash, "新 token hash 必须不同于旧 token");

    // Outbox 事件 + 审计同事务
    assert_eq!(table_count(&pool, "outbox_events").await, 1);
    assert_eq!(table_count(&pool, "audit_logs").await, 1);
    let payload: serde_json::Value = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT payload FROM outbox_events")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(payload["email_verification_token_id"], verify_token_id);
    assert_eq!(payload["resend"], true);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 冷却：默认 60s 内第二次重发被拒绝（RateLimited，供 429）。
#[tokio::test]
async fn resend_cooldown_blocks_second_request_within_window() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let (user_id, email) = insert_pending_user(&pool, "bob").await;
    let _ = insert_verify_token(&pool, &user_id).await;

    resend_verification_email(&pool, &limiter, &email, "req-1", &ResendLimits::default())
        .await
        .expect("第一次重发成功");

    let err = resend_verification_email(&pool, &limiter, &email, "req-2", &ResendLimits::default())
        .await
        .unwrap_err();
    let ResendError::RateLimited {
        retry_after_secs, ..
    } = err
    else {
        panic!("冷却窗口内必须拒绝");
    };
    assert!(retry_after_secs >= 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 日上限：冷却窗口内等待后仍可重发，直到日上限耗尽（注入小窗口测试）。
#[tokio::test]
async fn resend_daily_limit_blocks_after_limit() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let (user_id, email) = insert_pending_user(&pool, "carol").await;
    let _ = insert_verify_token(&pool, &user_id).await;

    // 冷却 1ms + 每日 2 次：两次成功重发后第三次 429
    let limits = ResendLimits {
        cooldown_ms: 1,
        daily_window_ms: 24 * 60 * 60 * 1000,
        daily_limit: 2,
    };

    resend_verification_email(&pool, &limiter, &email, "req-1", &limits)
        .await
        .expect("第 1 次重发成功");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    resend_verification_email(&pool, &limiter, &email, "req-2", &limits)
        .await
        .expect("第 2 次重发成功");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let err = resend_verification_email(&pool, &limiter, &email, "req-3", &limits)
        .await
        .unwrap_err();
    let ResendError::RateLimited { .. } = err else {
        panic!("日上限耗尽必须拒绝");
    };

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 发送一次重发请求；`ip` 用于模拟客户端地址。
async fn post_resend(app: &Router, email: &str, ip: &str) -> axum::response::Response {
    let body = json!({ "email": email });
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/resend-verification")
                .header("content-type", "application/json")
                .header("x-forwarded-for", ip)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// HTTP 层统一响应：存在的 pending 邮箱与不存在的邮箱都返回 202 且响应体一致。
#[tokio::test]
async fn resend_endpoint_returns_unified_202() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, email) = insert_pending_user(&pool, "frank").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    // 存在的 pending 邮箱
    let existing = post_resend(&app, &email, "198.51.100.1").await;
    assert_eq!(existing.status(), StatusCode::ACCEPTED);
    let existing_body = existing.into_body().collect().await.unwrap().to_bytes();

    // 不存在的邮箱（不同 IP 避免 IP/账号维度限流干扰）
    let missing = post_resend(&app, "ghost@example.com", "198.51.100.2").await;
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

/// HTTP 层冷却：同一邮箱 60s 内第二次重发 → 429 + Retry-After + 契约错误码。
#[tokio::test]
async fn resend_endpoint_cooldown_returns_429() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, email) = insert_pending_user(&pool, "grace").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let first = post_resend(&app, &email, "198.51.100.3").await;
    assert_eq!(first.status(), StatusCode::ACCEPTED);

    let second = post_resend(&app, &email, "198.51.100.3").await;
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = second
        .headers()
        .get("retry-after")
        .expect("429 必须带 Retry-After");
    assert!(retry_after.to_str().unwrap().parse::<u64>().unwrap() >= 1);
    assert_eq!(second.headers().get("ratelimit-limit").unwrap(), "1");

    let body: Value =
        serde_json::from_slice(&second.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "rate_limited");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 统一响应：不存在的邮箱返回 Noop（handler 仍 202），不创建 token/Outbox。
#[tokio::test]
async fn resend_unknown_email_returns_noop_without_side_effects() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();

    let outcome = resend_verification_email(
        &pool,
        &limiter,
        "ghost@example.com",
        "req-1",
        &ResendLimits::default(),
    )
    .await
    .expect("未知邮箱必须 Ok(Noop)");
    assert!(matches!(outcome, ResendOutcome::Noop));

    assert_eq!(table_count(&pool, "users").await, 0);
    assert_eq!(table_count(&pool, "email_verification_tokens").await, 0);
    assert_eq!(table_count(&pool, "outbox_events").await, 0);
    assert_eq!(table_count(&pool, "audit_logs").await, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 统一响应：已激活用户返回 Noop，不重复生成 token。
#[tokio::test]
async fn resend_activated_user_returns_noop() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let (user_id, email) = insert_pending_user(&pool, "dave").await;
    let _ = insert_verify_token(&pool, &user_id).await;

    // 模拟已验证激活（本测试聚焦重发 Noop 语义）
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users SET email_verified = 1, email_verified_at = ?, status = 'active', updated_at = ?
                 WHERE id = ?",
            )
            .bind(now)
            .bind(now)
            .bind(&user_id)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    assert_eq!(user_status(&pool, &user_id).await, "active");

    let outcome =
        resend_verification_email(&pool, &limiter, &email, "req-1", &ResendLimits::default())
            .await
            .expect("已激活用户必须 Ok(Noop)");
    assert!(matches!(outcome, ResendOutcome::Noop));
    assert_eq!(
        table_count(&pool, "outbox_events").await,
        0,
        "已激活不发邮件"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
