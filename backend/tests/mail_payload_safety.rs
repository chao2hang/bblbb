//! M01-JOBS-12：邮件任务 payload 只存 token 引用/密文最小信息，
//! 任何日志（含 last_error 持久化）不得输出验证或重置 token。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::token::generate_token;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::payload::{redact_token, validate_mail_payload, PayloadTokenError};
use bblbb_backend::jobs::retry::{fail_job, RetryClass, RetryPolicy};
use bblbb_backend::jobs::worker;
use serde_json::json;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-mail-{}", uuid::Uuid::now_v7()));
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

/// 含明文验证 token 的 payload 必须被拒绝（邮件任务只存引用/密文最小信息）。
#[tokio::test]
async fn mail_payload_with_plaintext_token_is_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let now = chrono::Utc::now().timestamp_millis();

    // 用真实 token 构建邮件任务 payload
    let payload = json!({
        "email": "user@example.com",
        "verification_token": generate_token()
    });
    let err = validate_mail_payload(&payload).unwrap_err();
    assert!(matches!(
        err,
        PayloadTokenError::PlaintextToken { ref key } if key == "verification_token"
    ));

    // 引用形式（token_id）合法
    let reference = json!({
        "email": "user@example.com",
        "verification_token_id": "tok_abc123"
    });
    assert!(validate_mail_payload(&reference).is_ok());

    // 写库前的最后防线：即使构造了 job，payload 也不会携带明文 token
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, created_at, updated_at)
                 VALUES ('mail-1', 'mail', 'mail', ?, 1, 'queued', 0, 5, ?, ?, ?)",
            )
            .bind(serde_json::to_string(&reference).unwrap())
            .bind(now - 10_000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 任何日志不得输出 token：last_error（持久化日志）必须经 redact_token 脱敏，
/// 原始 token 不得出现在数据库错误字段中。
#[tokio::test]
async fn last_error_never_leaks_token() {
    let (pool, dir) = pool_with_migrations().await;
    let now = chrono::Utc::now().timestamp_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, created_at, updated_at)
                 VALUES ('mail-1', 'mail', 'mail', '{}', 1, 'queued', 0, 5, ?, ?, ?)",
            )
            .bind(now - 10_000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let claimed = worker::claim_batch(&pool, "mail-worker", "mail", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);

    // 错误文本包含 token（例如 SMTP 响应回显），写入前必须脱敏
    let token = generate_token();
    let raw_error = format!("smtp 550 recipient rejected, token={token}");
    let safe_error = redact_token(&raw_error);
    assert!(
        !safe_error.contains(&token),
        "redact_token 必须去除明文 token"
    );
    assert!(safe_error.contains("[REDACTED]"));

    let policy = RetryPolicy {
        base_delay_ms: 1_000,
        max_delay_ms: 8_000,
        jitter_ms: 0,
    };
    fail_job(
        &pool,
        "mail-worker",
        "mail-1",
        &safe_error,
        RetryClass::Permanent,
        &policy,
    )
    .await
    .unwrap();

    // 持久化的 last_error 不含原始 token
    let last_error: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT last_error FROM jobs WHERE id = 'mail-1'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let last_error = last_error.expect("last_error 必须已写入");
    assert!(
        !last_error.contains(&token),
        "last_error 不得泄漏 token，实际: {last_error}"
    );
    assert!(last_error.contains("[REDACTED]"));

    close_pool(&pool).await;
    cleanup(&dir);
}
