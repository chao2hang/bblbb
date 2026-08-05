//! M02-MFA-04：恢复码——一次生成只展示一次、数据库只存 hash、
//! 消费原子标记（并发唯一）、审计可追踪。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::{
    consume_recovery_code, generate_recovery_codes, MfaError, RECOVERY_CODE_COUNT,
};
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
    let dir = std::env::temp_dir().join(format!("bblbb-mfarec-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
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

/// 该用户未消费恢复码的 hash 列表。
async fn stored_hashes(pool: &DatabasePool, user_id: &str) -> Vec<String> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT code_hash FROM mfa_recovery_codes WHERE user_id = ? AND consumed_at IS NULL ORDER BY created_at",
            )
            .bind(user_id)
            .fetch_all(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn audit_actions(pool: &DatabasePool, user_id: &str) -> Vec<String> {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT action FROM audit_logs WHERE actor_id = ? ORDER BY created_at",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 生成恢复码：返回明文一次；数据库只存 64 位 hex hash；审计记录生成。
#[tokio::test]
async fn generate_returns_codes_once_and_stores_only_hashes() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "alice").await;

    let codes = generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-1")
        .await
        .unwrap();
    assert_eq!(codes.len(), RECOVERY_CODE_COUNT, "每组恢复码数量");
    // 所有 code 各不相同
    let unique: std::collections::HashSet<_> = codes.iter().collect();
    assert_eq!(unique.len(), codes.len(), "恢复码必须互不相同");

    let hashes = stored_hashes(&pool, &user_id).await;
    assert_eq!(hashes.len(), RECOVERY_CODE_COUNT);
    for (hash, code) in hashes.iter().zip(&codes) {
        assert_eq!(hash.len(), 64, "必须只存 64 位 hex SHA-256");
        assert!(
            hash.chars().all(|c| c.is_ascii_hexdigit()),
            "hash 必须为 hex"
        );
        assert!(!hash.contains(code), "数据库不得存明文恢复码（只展示一次）");
    }
    // 明文 code 不出现在任何 hash 列
    for code in &codes {
        assert!(
            !hashes.iter().any(|h| h.contains(code)),
            "明文恢复码不得出现在数据库"
        );
    }

    // 审计记录生成（与生成同事务）
    let actions = audit_actions(&pool, &user_id).await;
    assert!(
        actions.contains(&"auth.mfa_recovery_codes_generated".to_string()),
        "必须写生成审计: {actions:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 消费恢复码：原子标记 consumed_at，并发同码恰好一个成功。
#[tokio::test]
async fn consume_marks_atomically_and_concurrent_unique() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "bob").await;
    let codes = generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-1")
        .await
        .unwrap();
    let code = codes[0].clone();

    consume_recovery_code(&pool, &user_id, &code, "req-2")
        .await
        .unwrap();

    // 已消费：同一 code 再消费 → InvalidCode
    let err = consume_recovery_code(&pool, &user_id, &code, "req-3")
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode), "{err:?}");

    // 消费审计已写
    let actions = audit_actions(&pool, &user_id).await;
    assert!(
        actions.contains(&"auth.mfa_recovery_code_used".to_string()),
        "必须写消费审计: {actions:?}"
    );

    // 并发同码：恰好一个成功
    let p1 = pool.clone();
    let p2 = pool.clone();
    let u1 = user_id.clone();
    let u2 = user_id.clone();
    let c1 = codes[1].clone();
    let c2 = codes[1].clone();
    let (r1, r2) = tokio::join!(
        async move { consume_recovery_code(&p1, &u1, &c1, "req-4").await },
        async move { consume_recovery_code(&p2, &u2, &c2, "req-5").await },
    );
    let ok_count = [r1, r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 1, "同一恢复码并发消费必须恰好一个成功");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未知恢复码 → 统一 InvalidCode（防枚举）。
#[tokio::test]
async fn consume_unknown_code_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "carol").await;
    generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-1")
        .await
        .unwrap();

    let err = consume_recovery_code(&pool, &user_id, "ZZZZZZZZZZZZZZZZ", "req-2")
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 消费大小写不敏感（恢复码为 base32 大写，用户可能输入小写）。
#[tokio::test]
async fn consume_is_case_insensitive() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "dave").await;
    let codes = generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-1")
        .await
        .unwrap();
    let lowercase = codes[0].to_ascii_lowercase();

    consume_recovery_code(&pool, &user_id, &lowercase, "req-2")
        .await
        .unwrap();

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 新一组生成使旧未用恢复码全部失效（旧码消费 → InvalidCode）。
#[tokio::test]
async fn new_generation_invalidates_old_unused_codes() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "erin").await;
    let set_a = generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-1")
        .await
        .unwrap();
    // 消费 A 中一个码（该码已被消费，本就不可再用）
    consume_recovery_code(&pool, &user_id, &set_a[0], "req-2")
        .await
        .unwrap();

    // 生成新一组
    let set_b = generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-3")
        .await
        .unwrap();
    assert_ne!(set_a[0], set_b[0], "新一组必须与旧一组不同");

    // 旧未用码（set_a[1]）现已失效
    let err = consume_recovery_code(&pool, &user_id, &set_a[1], "req-4")
        .await
        .unwrap_err();
    assert!(
        matches!(err, MfaError::InvalidCode),
        "旧未用恢复码必须失效: {err:?}"
    );

    // 新一组可正常消费
    consume_recovery_code(&pool, &user_id, &set_b[0], "req-5")
        .await
        .unwrap();

    close_pool(&pool).await;
    cleanup(&dir);
}
