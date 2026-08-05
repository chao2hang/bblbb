//! M02-IDENTITY-07：验证 token 过期、一次消费、旧 token 失效、
//! 并发消费唯一成功；失败统一响应（不区分原因，防 token 枚举）。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::token::{generate_token, hash_token};
use bblbb_backend::auth::{verify_email_token, VerifyEmailError};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-verify-{}", uuid::Uuid::now_v7()));
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

/// 插入一个 pending 用户，返回其 id。
async fn insert_pending_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'pending', ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_user"))
            .bind(format!("{tag}@example.com"))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

/// 插入一个验证 token；返回原始 token（测试需要用它来验证）。
async fn insert_verify_token(pool: &DatabasePool, user_id: &str, expires_in_ms: i64) -> String {
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

async fn user_status(pool: &DatabasePool, user_id: &str) -> (String, i64, Option<i64>) {
    match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT status, email_verified, email_verified_at FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn token_consumed_at(pool: &DatabasePool, token: &str) -> Option<i64> {
    let token_hash = hash_token(token);
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT consumed_at FROM email_verification_tokens WHERE token_hash = ?",
        )
        .bind(&token_hash)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn unconsumed_tokens_for_user(pool: &DatabasePool, user_id: &str) -> i64 {
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

/// 正常验证：pending → active，email_verified_at 写入，token 消费；
/// 同事务写 `auth.email_verified` 审计与 `user.status_changed.v1` 领域事件。
#[tokio::test]
async fn verify_activates_pending_user_and_consumes_token() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_pending_user(&pool, "alice").await;
    let token = insert_verify_token(&pool, &user_id, 24 * 60 * 60 * 1000).await;

    let outcome = verify_email_token(&pool, &token, 0, "req-verify")
        .await
        .expect("验证必须成功");
    assert_eq!(outcome.user_id, user_id);
    assert_eq!(outcome.event_id.len(), 36, "领域事件 ID");

    let (status, verified, verified_at) = user_status(&pool, &user_id).await;
    assert_eq!(status, "active");
    assert_eq!(verified, 1);
    assert!(verified_at.is_some(), "email_verified_at 必须写入");
    assert!(
        token_consumed_at(&pool, &token).await.is_some(),
        "token 必须标记已消费"
    );

    // 审计（M02-IDENTITY-09）：auth.email_verified + request_id 贯通
    let (audit_action, audit_target, audit_req): (String, String, String) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT action, target_id, request_id FROM audit_logs")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit_action, "auth.email_verified");
    assert_eq!(audit_target, user_id);
    assert_eq!(audit_req, "req-verify");

    // 领域事件：user.status_changed.v1，from=pending to=active
    let (event_type, payload): (String, String) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT event_type, payload FROM outbox_events")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(event_type, "user.status_changed.v1");
    let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(payload["from_status"], "pending");
    assert_eq!(payload["to_status"], "active");
    assert_eq!(payload["user_id"], user_id);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 可选新用户冷静期：cooldown_secs > 0 时审计 metadata 与事件 payload
/// 都记录 new_user_cooldown_until = 激活时间 + 时长。
#[tokio::test]
async fn verify_with_cooldown_records_cooldown_until() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_pending_user(&pool, "zoe").await;
    let token = insert_verify_token(&pool, &user_id, 24 * 60 * 60 * 1000).await;
    let before = now_millis();

    verify_email_token(&pool, &token, 3600, "req-cooldown")
        .await
        .expect("验证必须成功");
    let after = now_millis();

    let audit_metadata: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT metadata FROM audit_logs")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let event_payload: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT payload FROM outbox_events")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let audit_meta: serde_json::Value = serde_json::from_str(&audit_metadata).unwrap();
    let payload: serde_json::Value = serde_json::from_str(&event_payload).unwrap();

    for doc in [&audit_meta, &payload] {
        let until = doc["new_user_cooldown_until"]
            .as_i64()
            .expect("必须记录冷静期到期时间");
        assert!(
            until >= before + 3600 * 1000 && until <= after + 3600 * 1000,
            "cooldown_until 应约为激活时间 + 3600s，实际 {until}"
        );
    }

    // 冷静期关闭（0）时不得记录 cooldown_until
    let user2 = insert_pending_user(&pool, "zoe2").await;
    let token2 = insert_verify_token(&pool, &user2, 24 * 60 * 60 * 1000).await;
    verify_email_token(&pool, &token2, 0, "req-no-cooldown")
        .await
        .expect("验证必须成功");
    let payloads: Vec<String> = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT payload FROM outbox_events ORDER BY created_at ASC")
                .fetch_all(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(payloads.len(), 2, "两个验证各写一个领域事件");
    let second: serde_json::Value = serde_json::from_str(&payloads[1]).unwrap();
    assert_eq!(second["user_id"], user2);
    assert!(
        second.get("new_user_cooldown_until").is_none(),
        "冷静期关闭时不得记录 cooldown_until"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 一次消费：同一 token 第二次验证必须失败（统一 InvalidOrExpired）。
#[tokio::test]
async fn verify_second_use_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_pending_user(&pool, "bob").await;
    let token = insert_verify_token(&pool, &user_id, 24 * 60 * 60 * 1000).await;

    verify_email_token(&pool, &token, 0, "req-verify")
        .await
        .expect("首次验证成功");

    let err = verify_email_token(&pool, &token, 0, "req-verify")
        .await
        .unwrap_err();
    assert!(
        matches!(err, VerifyEmailError::InvalidOrExpired),
        "重复使用必须拒绝：{err}"
    );

    // 用户保持 active，不重复激活
    let (status, verified, _) = user_status(&pool, &user_id).await;
    assert_eq!(status, "active");
    assert_eq!(verified, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 过期 token 拒绝，用户保持 pending。
#[tokio::test]
async fn verify_expired_token_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_pending_user(&pool, "carol").await;
    // 已过期（负过期时间）
    let token = insert_verify_token(&pool, &user_id, -1000).await;

    let err = verify_email_token(&pool, &token, 0, "req-verify")
        .await
        .unwrap_err();
    assert!(matches!(err, VerifyEmailError::InvalidOrExpired));

    let (status, verified, _) = user_status(&pool, &user_id).await;
    assert_eq!(status, "pending", "过期 token 不得激活用户");
    assert_eq!(verified, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未知 token 统一拒绝（不区分原因）。
#[tokio::test]
async fn verify_unknown_token_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let token = generate_token(); // 从未入库

    let err = verify_email_token(&pool, &token, 0, "req-verify")
        .await
        .unwrap_err();
    assert!(matches!(err, VerifyEmailError::InvalidOrExpired));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 并发消费唯一成功：两个并发请求同一 token，恰好一个成功。
#[tokio::test]
async fn concurrent_verification_has_single_winner() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_pending_user(&pool, "dave").await;
    let token = insert_verify_token(&pool, &user_id, 24 * 60 * 60 * 1000).await;

    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let token_a = token.clone();
    let token_b = token.clone();
    let (r1, r2) = tokio::join!(
        async move { verify_email_token(&pool_a, &token_a, 0, "req-verify").await },
        async move { verify_email_token(&pool_b, &token_b, 0, "req-verify").await },
    );

    let wins = [r1, r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(wins, 1, "并发消费必须恰好一个成功，实际 {wins}");

    // 终态正确：active + token 消费一次
    let (status, verified, _) = user_status(&pool, &user_id).await;
    assert_eq!(status, "active");
    assert_eq!(verified, 1);
    assert!(
        token_consumed_at(&pool, &token).await.is_some(),
        "token 必须恰好消费一次"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 旧 token 失效：同用户存在多个未消费 token 时，验证其中一个后
/// 其余全部失效；再验证旧 token 必须失败。
#[tokio::test]
async fn verify_invalidates_sibling_tokens() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_pending_user(&pool, "erin").await;
    let token_a = insert_verify_token(&pool, &user_id, 24 * 60 * 60 * 1000).await;
    let token_b = insert_verify_token(&pool, &user_id, 24 * 60 * 60 * 1000).await;
    assert_eq!(unconsumed_tokens_for_user(&pool, &user_id).await, 2);

    verify_email_token(&pool, &token_a, 0, "req-verify")
        .await
        .expect("验证 token_a 成功");

    // token_b（旧 token）已失效
    assert_eq!(
        unconsumed_tokens_for_user(&pool, &user_id).await,
        0,
        "激活后同用户其余未消费 token 必须全部失效"
    );
    let err = verify_email_token(&pool, &token_b, 0, "req-verify")
        .await
        .unwrap_err();
    assert!(matches!(err, VerifyEmailError::InvalidOrExpired));

    close_pool(&pool).await;
    cleanup(&dir);
}
