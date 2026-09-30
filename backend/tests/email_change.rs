//! GA 邮箱换绑：申请（密码确认 + 冷却 + 占用检查 + Outbox）与确认
//! （一次性 token 消费 + 新邮箱生效 + email_verified 置位 + 复用拒绝）。

mod common;

use std::path::{Path, PathBuf};

use bblbb_backend::auth::email_change::{
    confirm_email_change, request_email_change, EmailChangeError, EmailChangeLimits,
};
use bblbb_backend::auth::hash_password;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::ratelimit::RateLimiter;
use serde_json::Value;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-emailchange-{}", uuid::Uuid::now_v7()));
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

const PASSWORD: &str = "Password1";

/// 插入 active 用户（密码 Password1），返回 (user_id, email)。
async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let now = now_millis();
    let hash = hash_password(PASSWORD).unwrap();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(tag)
            .bind(&email)
            .bind(&hash)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(tag)
            .bind(&email)
            .bind(&hash)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    (user_id, email)
}

#[tokio::test]
async fn request_rejects_wrong_password() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "alice").await;
    let limiter = RateLimiter::new();
    let err = request_email_change(
        &pool,
        &limiter,
        &user_id,
        "alice",
        &email,
        "WrongPass9",
        "bob2@example.com",
        "req-1",
        &EmailChangeLimits::default(),
        "",
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, EmailChangeError::WrongPassword),
        "错误密码必须拒绝"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn request_rejects_taken_email() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "alice").await;
    let (_bob_id, _bob_email) = insert_user(&pool, "bob").await;
    let limiter = RateLimiter::new();
    let err = request_email_change(
        &pool,
        &limiter,
        &user_id,
        "alice",
        &email,
        PASSWORD,
        "bob@example.com",
        "req-1",
        &EmailChangeLimits::default(),
        "",
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, EmailChangeError::EmailTaken),
        "占用邮箱必须拒绝"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn request_creates_token_and_outbox_then_confirm_rotates_email() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "alice").await;
    let limiter = RateLimiter::new();

    // 1) 申请：token 落库（hash + 密文）+ outbox 事件（new_email/to_email）
    let outcome = request_email_change(
        &pool,
        &limiter,
        &user_id,
        "alice",
        &email,
        PASSWORD,
        "alice.new@example.com",
        "req-1",
        &EmailChangeLimits::default(),
        "",
    )
    .await
    .expect("换绑申请必须成功");
    assert_eq!(outcome.new_email, "alice.new@example.com");

    let email_change_jobs: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE event_type = 'user.email_change_requested.v1'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE event_type = 'user.email_change_requested.v1'")
            .fetch_one(p)
            .await
            .unwrap(),
    };
    assert_eq!(email_change_jobs, 1, "必须写入换绑 outbox 事件");

    // 事件 payload：token 引用 + new_email（无明文 token）
    let payload_raw: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT payload FROM outbox_events WHERE event_type = 'user.email_change_requested.v1' LIMIT 1")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar("SELECT payload FROM outbox_events WHERE event_type = 'user.email_change_requested.v1' LIMIT 1")
            .fetch_one(p)
            .await
            .unwrap(),
    };
    let payload: Value = serde_json::from_str(&payload_raw).unwrap();
    assert_eq!(
        payload["email_change_token_id"].as_str().unwrap(),
        outcome.token_id
    );
    assert!(payload.get("token").is_none(), "payload 不得携带明文 token");

    // 2) 旧邮箱在确认前不变
    let current: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT email_normalized FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar("SELECT email_normalized FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap(),
    };
    assert_eq!(current, email, "确认前旧邮箱保持不变");

    // 3) 确认（测试从 outbox 事件重建明文不可能——直接用密文列 + open_token
    //    得到明文 token（生产路径：worker 解密渲染链接）
    let sealed: String = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT token_encrypted FROM email_change_tokens WHERE id = ?")
                .bind(&outcome.token_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT token_encrypted FROM email_change_tokens WHERE id = ?")
                .bind(&outcome.token_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    let plaintext = bblbb_backend::auth::token::open_token("", &sealed).expect("密文可解密");

    let new_email = confirm_email_change(&pool, &plaintext, "req-2")
        .await
        .expect("确认必须成功");
    assert_eq!(new_email, "alice.new@example.com");

    // 4) 生效：新邮箱 + email_verified=1
    let (new_current, verified): (String, i64) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT email_normalized, email_verified FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT email_normalized, email_verified FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(new_current, "alice.new@example.com");
    assert_eq!(verified, 1, "换绑后新邮箱自动视为已验证");

    // 5) token 复用拒绝
    let err = confirm_email_change(&pool, &plaintext, "req-3")
        .await
        .unwrap_err();
    assert!(err.is_token_invalid(), "已消费 token 必须拒绝");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn request_is_rate_limited_per_new_email() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "carol").await;
    let limiter = RateLimiter::new();
    let tiny = EmailChangeLimits {
        cooldown_ms: 60_000,
        daily_window_ms: 60_000,
        daily_limit: 1,
    };

    request_email_change(
        &pool,
        &limiter,
        &user_id,
        "carol",
        &email,
        PASSWORD,
        "c1@example.com",
        "req-1",
        &tiny,
        "",
    )
    .await
    .expect("第 1 次成功");
    let err = request_email_change(
        &pool,
        &limiter,
        &user_id,
        "carol",
        &email,
        PASSWORD,
        "c2@example.com",
        "req-2",
        &tiny,
        "",
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, EmailChangeError::RateLimited { .. }),
        "同新邮箱 60s 冷却内必须限流"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn confirm_rejects_when_email_taken_between_request_and_confirm() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "dave").await;
    let limiter = RateLimiter::new();
    let outcome = request_email_change(
        &pool,
        &limiter,
        &user_id,
        "dave",
        &email,
        PASSWORD,
        "target@example.com",
        "req-1",
        &EmailChangeLimits::default(),
        "",
    )
    .await
    .unwrap();
    // 申请后他人抢注 target@example.com
    let (_other_id, _) = insert_user(&pool, "target").await;
    let sealed: String = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT token_encrypted FROM email_change_tokens WHERE id = ?")
                .bind(&outcome.token_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT token_encrypted FROM email_change_tokens WHERE id = ?")
                .bind(&outcome.token_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    let plaintext = bblbb_backend::auth::token::open_token("", &sealed).unwrap();
    let err = confirm_email_change(&pool, &plaintext, "req-2")
        .await
        .unwrap_err();
    assert!(
        matches!(err, EmailChangeError::EmailTaken),
        "确认时邮箱已被他人占用必须拒绝"
    );
    // 原 token 已消费（不可重试同 token）
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn expand_injects_confirm_url_for_email_change_template() {
    // GA 回归：expand_verification_params 必须覆盖 email.change（此前漏分支
    // 导致确认邮件正文无链接，text 只剩尾部两行）。
    use bblbb_backend::email::service::expand_verification_params;
    use bblbb_backend::notifications::templates::{render, TemplateKey};

    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "erin").await;
    let limiter = RateLimiter::new();
    let outcome = request_email_change(
        &pool,
        &limiter,
        &user_id,
        "erin",
        &email,
        PASSWORD,
        "erin.new@example.com",
        "req-1",
        &EmailChangeLimits::default(),
        "",
    )
    .await
    .unwrap();

    let mut params = serde_json::Map::new();
    params.insert(
        "username".to_string(),
        serde_json::Value::String("erin".to_string()),
    );
    params.insert(
        "expires_minutes".to_string(),
        serde_json::Value::String("30".to_string()),
    );
    params.insert(
        "token_id".to_string(),
        serde_json::Value::String(outcome.token_id.clone()),
    );

    let expanded = expand_verification_params(&pool, "", TemplateKey::EmailChange, &params)
        .await
        .expect("expand 必须成功")
        .expect("email.change 必须命中展开分支");
    let confirm_url = expanded
        .get("confirm_url")
        .and_then(|v| v.as_str())
        .expect("必须注入 confirm_url");
    assert!(
        confirm_url.contains("/email-change/confirm?token="),
        "confirm_url 必须指向换绑确认页: {confirm_url}"
    );

    // 渲染正文必须包含链接（投递邮件含一次性链接的最终保证）
    let rendered = render(TemplateKey::EmailChange, &expanded);
    assert!(
        rendered
            .body
            .as_deref()
            .unwrap_or_default()
            .contains("email-change/confirm?token="),
        "渲染正文必须包含确认链接"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
