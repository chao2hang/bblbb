//! M02-IDENTITY-12：注册/验证/重发/重置的事务故障注入。
//!
//! 每个 service 在**同一事务**写入业务行 + token + 审计 + Outbox；用 SQLite
//! `BEFORE INSERT/UPDATE ... RAISE(ABORT)` 触发器在特定步骤注入失败，
//! 断言整事务回滚（无半完成状态）：失败步骤之前的写入全部消失。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::password_reset::{
    confirm_password_reset, request_password_reset, PasswordResetLimits,
};
use bblbb_backend::auth::resend::{resend_verification_email, ResendLimits};
use bblbb_backend::auth::token::{generate_token, hash_token};
use bblbb_backend::auth::{register_user, verify_email_token, RegisterUserError, VerifyEmailError};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::domain::registration::{validate_register, RegisterRequest};
use bblbb_backend::outbox::now_millis;
use bblbb_backend::ratelimit::RateLimiter;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-fault-{}", uuid::Uuid::now_v7()));
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

async fn table_count(pool: &DatabasePool, table: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn unconsumed_verify_tokens(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM email_verification_tokens
             WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(user_id)
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

/// 建一个在指定表指定时机 RAISE(ABORT) 的故障注入触发器。
async fn inject_failure(pool: &DatabasePool, timing: &str, table: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query(&format!(
                "CREATE TRIGGER inject_failure {timing} ON {table}
                 BEGIN
                     SELECT RAISE(ABORT, 'injected failure');
                 END"
            ))
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn drop_injector(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => {
            sqlx::query("DROP TRIGGER IF EXISTS inject_failure")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn valid_reg(
    username: &str,
    email: &str,
) -> bblbb_backend::domain::registration::NormalizedRegistration {
    validate_register(&RegisterRequest {
        username: username.to_string(),
        email: email.to_string(),
        password: "passw0rd9".to_string(),
    })
    .unwrap()
}

/// 注册：token INSERT 失败 → 用户/审计/Outbox 全部回滚。
#[tokio::test]
async fn register_rolls_back_all_rows_when_token_insert_fails() {
    let (pool, dir) = pool_with_migrations().await;
    inject_failure(&pool, "BEFORE INSERT", "email_verification_tokens").await;

    let err = register_user(&pool, &valid_reg("alice", "alice@example.com"), "req-1")
        .await
        .unwrap_err();
    assert!(
        matches!(err, RegisterUserError::Database(_)),
        "token INSERT 注入失败必须报 Database 错误：{err}"
    );

    // 无半完成状态：四表全空
    assert_eq!(table_count(&pool, "users").await, 0, "用户行必须回滚");
    assert_eq!(table_count(&pool, "email_verification_tokens").await, 0);
    assert_eq!(table_count(&pool, "audit_logs").await, 0);
    assert_eq!(table_count(&pool, "outbox_events").await, 0);

    drop_injector(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 验证：激活 UPDATE 失败 → token 消费回滚（无半完成：token 未消费、无审计/事件）。
#[tokio::test]
async fn verify_rolls_back_consumption_when_activation_fails() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = uuid::Uuid::now_v7().to_string();
    let token = generate_token();
    let token_hash = hash_token(&token);
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'pending', ?, ?)",
            )
            .bind(&user_id)
            .bind("bob_user")
            .bind("bob@example.com")
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(&token_hash)
            .bind(now + 24 * 60 * 60 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    inject_failure(&pool, "BEFORE UPDATE", "users").await;

    let err = verify_email_token(&pool, &token, 0, "req-1")
        .await
        .unwrap_err();
    assert!(
        matches!(err, VerifyEmailError::Database(_)),
        "激活 UPDATE 注入失败必须报 Database 错误：{err}"
    );

    // 无半完成：token 未消费、用户仍 pending、无审计/事件
    assert_eq!(user_status(&pool, &user_id).await, "pending");
    assert_eq!(
        unconsumed_verify_tokens(&pool, &user_id).await,
        1,
        "token 消费必须回滚"
    );
    assert_eq!(table_count(&pool, "audit_logs").await, 0);
    assert_eq!(table_count(&pool, "outbox_events").await, 0);

    drop_injector(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 验证（并发激活守卫）：用户已 active 时激活 rows=0 → 消费回滚。
#[tokio::test]
async fn verify_rolls_back_when_user_already_active() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = uuid::Uuid::now_v7().to_string();
    let token = generate_token();
    let token_hash = hash_token(&token);
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 1, ?, ?)",
            )
            .bind(&user_id)
            .bind("carol_user")
            .bind("carol@example.com")
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(&token_hash)
            .bind(now + 24 * 60 * 60 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 用户已 active（非 pending）→ 激活 rows=0 → InvalidOrExpired + 消费回滚
    let err = verify_email_token(&pool, &token, 0, "req-1")
        .await
        .unwrap_err();
    assert!(matches!(err, VerifyEmailError::InvalidOrExpired));
    assert_eq!(
        unconsumed_verify_tokens(&pool, &user_id).await,
        1,
        "消费必须回滚"
    );
    assert_eq!(table_count(&pool, "audit_logs").await, 0);
    assert_eq!(table_count(&pool, "outbox_events").await, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 重发：新 token INSERT 失败 → 旧 token 失效回滚、无新 token/Outbox/审计。
#[tokio::test]
async fn resend_rolls_back_old_token_invalidation_when_new_insert_fails() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = "dave@example.com".to_string();
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'pending', ?, ?)",
            )
            .bind(&user_id)
            .bind("dave_user")
            .bind(&email)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(hash_token(&generate_token()))
            .bind(now + 24 * 60 * 60 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    inject_failure(&pool, "BEFORE INSERT", "email_verification_tokens").await;

    let err = resend_verification_email(&pool, &limiter, &email, "req-1", &ResendLimits::default())
        .await
        .unwrap_err();
    assert!(
        matches!(err, bblbb_backend::auth::ResendError::Database(_)),
        "新 token INSERT 注入失败必须报 Database 错误"
    );

    // 无半完成：旧 token 未失效、无新 token、无 Outbox/审计
    assert_eq!(
        unconsumed_verify_tokens(&pool, &user_id).await,
        1,
        "旧 token 失效必须回滚"
    );
    assert_eq!(table_count(&pool, "email_verification_tokens").await, 1);
    assert_eq!(table_count(&pool, "outbox_events").await, 0);
    assert_eq!(table_count(&pool, "audit_logs").await, 0);

    drop_injector(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 重置请求：新 reset token INSERT 失败 → 旧 token 失效回滚、无 Outbox/审计。
#[tokio::test]
async fn reset_request_rolls_back_when_token_insert_fails() {
    let (pool, dir) = pool_with_migrations().await;
    let limiter = RateLimiter::new();
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = "erin@example.com".to_string();
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind("erin_user")
            .bind(&email)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(hash_token(&generate_token()))
            .bind(now + 30 * 60 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    inject_failure(&pool, "BEFORE INSERT", "password_reset_tokens").await;

    let err = request_password_reset(
        &pool,
        &limiter,
        &email,
        "req-1",
        &PasswordResetLimits::default(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, bblbb_backend::auth::RequestResetError::Database(_)),
        "新 reset token INSERT 注入失败必须报 Database 错误"
    );

    // 无半完成：旧 reset token 未失效、无新 token、无 Outbox/审计
    assert_eq!(
        unconsumed_reset_tokens(&pool, &user_id).await,
        1,
        "旧 token 失效必须回滚"
    );
    assert_eq!(table_count(&pool, "password_reset_tokens").await, 1);
    assert_eq!(table_count(&pool, "outbox_events").await, 0);
    assert_eq!(table_count(&pool, "audit_logs").await, 0);

    drop_injector(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 重置确认：改密 UPDATE 失败 → token 消费回滚、密码未变、Session 未撤销。
#[tokio::test]
async fn reset_confirm_rolls_back_consumption_when_password_update_fails() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = uuid::Uuid::now_v7().to_string();
    let reset_token = generate_token();
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'old-hash', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind("frank_user")
            .bind("frank@example.com")
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(hash_token(&reset_token))
            .bind(now + 30 * 60 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
                 VALUES (?, ?, 'sess', 'csrf', ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
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
    inject_failure(&pool, "BEFORE UPDATE", "users").await;

    let err = confirm_password_reset(&pool, &reset_token, "new-hash", "req-1")
        .await
        .unwrap_err();
    assert!(
        matches!(err, bblbb_backend::auth::ConfirmResetError::Database(_)),
        "改密 UPDATE 注入失败必须报 Database 错误"
    );

    // 无半完成：token 未消费、密码未变、Session 未撤销
    assert_eq!(
        unconsumed_reset_tokens(&pool, &user_id).await,
        1,
        "token 消费必须回滚"
    );
    let password_hash: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(password_hash, "old-hash", "密码必须保持不变");
    let active_sessions: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_sessions WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(&user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(active_sessions, 1, "Session 不得被撤销");
    assert_eq!(table_count(&pool, "audit_logs").await, 0);

    drop_injector(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}
