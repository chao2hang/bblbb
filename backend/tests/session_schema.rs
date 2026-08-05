//! M02-SESSION-01：session 迁移契约——user_sessions 含设备/版本字段：
//! token_hash（唯一）、csrf_secret_hash、user_agent、ip_prefix_hash、
//! created_at/last_seen_at、idle/absolute expires、revoked_at、revoke_reason、
//! version（默认 0）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-sess-{}", uuid::Uuid::now_v7()));
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

/// 读取 user_sessions 表的全部列名。
async fn session_columns(pool: &DatabasePool) -> Vec<String> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT name FROM pragma_table_info('user_sessions')")
                .fetch_all(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// M02-SESSION-01 契约：全部生命周期与设备/版本列存在。
#[tokio::test]
async fn session_schema_has_full_lifecycle_and_version_columns() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = session_columns(&pool).await;

    for required in [
        "id",
        "user_id",
        "token_hash",       // Session token 只存 hash
        "csrf_secret_hash", // CSRF secret 的哈希
        "user_agent",       // 设备/UA（截断）
        "ip_prefix_hash",   // 可选安全提醒
        "created_at",
        "last_seen_at",
        "idle_expires_at",     // 滑动过期
        "absolute_expires_at", // 最长有效期
        "revoked_at",
        "revoke_reason",
        "version", // Session 旋转计数（防 fixation）
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "user_sessions 缺少列 {required}，实际: {columns:?}"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 插入一条 session（同 token_hash 用于触发唯一约束）。
async fn insert_session_with_token(
    p: &sqlx::SqlitePool,
    id: &str,
    user_id: &str,
    token_hash: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
         VALUES (?, ?, ?, 'csrf', ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(user_id)
    .bind(token_hash)
    .bind(now)
    .bind(now)
    .bind(now + 3600 * 1000)
    .bind(now + 24 * 3600 * 1000)
    .execute(p)
    .await
    .map(|_| ())
}

/// token_hash 唯一索引存在（同一 token 不可能出现两次）。
#[tokio::test]
async fn session_token_hash_is_unique() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind("sess_user")
            .bind("sess@example.com")
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            insert_session_with_token(p, "s1", &user_id, "same-token-hash", now)
                .await
                .unwrap();
            let dup = insert_session_with_token(p, "s2", &user_id, "same-token-hash", now)
                .await
                .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "token_hash 唯一约束必须生效"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// version 默认 0：新 session 从版本 0 开始，旋转时递增。
#[tokio::test]
async fn session_version_defaults_to_zero() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind("sess2_user")
            .bind("sess2@example.com")
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO user_sessions (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at, idle_expires_at, absolute_expires_at)
                 VALUES (?, ?, 'tok-hash-1', 'csrf', ?, ?, ?, ?)",
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

            let version: i64 = sqlx::query_scalar("SELECT version FROM user_sessions LIMIT 1")
                .fetch_one(p)
                .await
                .unwrap();
            assert_eq!(version, 0, "新 session version 必须默认 0");
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}
