//! M02-MFA-01：MFA 迁移契约——
//! - `totp_credentials`：TOTP enrollment（加密 secret、last accepted step 防
//!   重放、confirmed_at/revoked_at 状态）；
//! - `mfa_recovery_codes`：恢复码只存 hash（不存明文），消费原子标记。

use std::path::{Path, PathBuf};

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
    let dir = std::env::temp_dir().join(format!("bblbb-mfa-{}", uuid::Uuid::now_v7()));
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

/// 读取指定表的全部列名。
async fn table_columns(pool: &DatabasePool, table: &str) -> Vec<String> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .fetch_all(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 插入一个用户，返回 user_id。
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

/// totp_credentials 契约：enrollment 全列存在（加密 secret + 防重放 step）。
#[tokio::test]
async fn totp_schema_has_full_enrollment_columns() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "totp_credentials").await;

    for required in [
        "id",
        "user_id",
        "encrypted_secret",   // 加密后的 TOTP secret（AEAD 密文，可解密验证）
        "last_accepted_step", // 最近接受的 time-step（防重放）
        "created_at",
        "confirmed_at", // NULL = 未完成 enrollment
        "revoked_at",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "totp_credentials 缺少列 {required}，实际: {columns:?}"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// last_accepted_step 默认 0：新 enrollment 从未接受任何 time-step。
#[tokio::test]
async fn totp_last_accepted_step_defaults_to_zero() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "step").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO totp_credentials (id, user_id, encrypted_secret, created_at)
                 VALUES (?, ?, 'ciphertext', ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap();
            let step: i64 = sqlx::query_scalar(
                "SELECT last_accepted_step FROM totp_credentials WHERE user_id = ?",
            )
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(step, 0, "新 enrollment 的 last_accepted_step 必须默认 0");
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// mfa_recovery_codes 契约：全列存在 + code_hash 唯一（同一恢复码不可能两次）。
#[tokio::test]
async fn recovery_codes_schema_columns_and_hash_unique() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "mfa_recovery_codes").await;
    for required in [
        "id",
        "user_id",
        "code_hash", // 只存 SHA-256 hash，不存明文
        "created_at",
        "consumed_at", // 消费原子标记（NULL = 未消费）
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "mfa_recovery_codes 缺少列 {required}，实际: {columns:?}"
        );
    }

    // 唯一约束：同 code_hash 第二次插入必须失败
    let user_id = insert_user(&pool, "codes").await;
    match &pool {
        Either::Left(p) => {
            let hash = "same-hash".to_string();
            sqlx::query(
                "INSERT INTO mfa_recovery_codes (id, user_id, code_hash, created_at)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(&hash)
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap();
            let dup = sqlx::query(
                "INSERT INTO mfa_recovery_codes (id, user_id, code_hash, created_at)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(&hash)
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "code_hash 唯一约束必须生效: {dup}"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 用户删除级联清理 MFA 数据（FK ON DELETE CASCADE）。
#[tokio::test]
async fn mfa_rows_cascade_on_user_delete() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "cascade").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO totp_credentials (id, user_id, encrypted_secret, created_at)
                 VALUES (?, ?, 'ciphertext', ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO mfa_recovery_codes (id, user_id, code_hash, created_at)
                 VALUES (?, ?, 'hash-1', ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap();

            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();

            let totp_left: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM totp_credentials WHERE user_id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(totp_left, 0, "用户删除必须级联清理 totp_credentials");
            let codes_left: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM mfa_recovery_codes WHERE user_id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(codes_left, 0, "用户删除必须级联清理 mfa_recovery_codes");
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}
