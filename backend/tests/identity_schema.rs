//! M02-IDENTITY-01：身份迁移契约——email_verified_at 列、邮箱验证/密码重置
//! token 表（只存 hash、token_hash 唯一、外键级联）。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::token::hash_token;
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
    let dir = std::env::temp_dir().join(format!("bblbb-ident-{}", uuid::Uuid::now_v7()));
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

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 插入一条邮箱验证 token；返回 Result 便于断言唯一约束。
async fn insert_verification(
    pool: &DatabasePool,
    user_id: &str,
    id: &str,
    token_hash: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(user_id)
            .bind(token_hash)
            .bind(now + 3_600_000)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 插入一个用户，返回其 id。
async fn insert_user(pool: &DatabasePool) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_ms();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, 'user_a', 'a@example.com', 'phc-test', 'pending', ?, ?)",
            )
            .bind(&id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    id
}

/// 身份迁移 schema 契约：users.email_verified_at + 两个 token 表结构与唯一约束。
#[tokio::test]
async fn identity_migration_schema_contract() {
    let (pool, dir) = pool_with_migrations().await;

    match &pool {
        Either::Left(p) => {
            // users 增加 email_verified_at（可空）
            let user_columns: Vec<String> =
                sqlx::query_scalar("SELECT name FROM pragma_table_info('users') ORDER BY cid")
                    .fetch_all(p)
                    .await
                    .unwrap();
            assert!(
                user_columns.contains(&"email_verified_at".to_string()),
                "users 缺少 email_verified_at"
            );

            // 两个 token 表结构一致
            for table in ["email_verification_tokens", "password_reset_tokens"] {
                let columns: Vec<String> = sqlx::query_scalar(&format!(
                    "SELECT name FROM pragma_table_info('{table}') ORDER BY cid"
                ))
                .fetch_all(p)
                .await
                .unwrap();
                for expected in [
                    "id",
                    "user_id",
                    "token_hash",
                    "expires_at",
                    "consumed_at",
                    "created_at",
                ] {
                    assert!(
                        columns.contains(&expected.to_string()),
                        "{table} 缺少列 {expected}"
                    );
                }
            }

            // token_hash 唯一索引存在（0002 命名）
            let indexes: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type='index' AND sql LIKE '%UNIQUE%'",
            )
            .fetch_all(p)
            .await
            .unwrap();
            for expected in [
                "email_verification_tokens_hash_uq",
                "password_reset_tokens_hash_uq",
            ] {
                assert!(
                    indexes.iter().any(|i| i.contains(expected)),
                    "缺少唯一索引 {expected}: {indexes:?}"
                );
            }
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// token 只以 hash 入库：token_hash 唯一约束拒绝重复 hash；明文 token 不出现在库中。
#[tokio::test]
async fn token_tables_store_only_hashes_and_dedupe() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool).await;
    let now = now_ms();

    // 生成明文 token，只存 hash
    let raw = bblbb_backend::auth::token::generate_token();
    let hash = hash_token(&raw);
    insert_verification(&pool, &user_id, "v1", &hash, now)
        .await
        .unwrap();
    assert!(
        insert_verification(&pool, &user_id, "v2", &hash, now)
            .await
            .is_err(),
        "相同 token_hash 必须被唯一约束拒绝"
    );

    // 明文 token 不得出现在任何 token 表中
    let verification_hashes: Vec<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT token_hash FROM email_verification_tokens")
            .fetch_all(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        !verification_hashes.contains(&raw),
        "明文 token 不得入库（只存 hash）"
    );

    // 密码重置表同样只存 hash
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES ('r1', ?, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&hash)
            .bind(now + 1_800_000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let reset_hashes: Vec<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT token_hash FROM password_reset_tokens")
            .fetch_all(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(!reset_hashes.contains(&raw), "明文 token 不得进入重置表");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 外键：无效 user_id 被拒绝；删除用户级联删除其 token。
#[tokio::test]
async fn token_tables_enforce_foreign_keys_and_cascade() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool).await;
    let now = now_ms();
    let hash = hash_token("dummy-hash-value");

    // 无效 user_id → 外键拒绝
    match &pool {
        Either::Left(p) => {
            let err = sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES ('v1', 'missing-user', ?, ?, ?)",
            )
            .bind(&hash)
            .bind(now + 3_600_000)
            .bind(now)
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                err.to_string().to_lowercase().contains("foreign key"),
                "无效 user_id 必须被外键拒绝: {err}"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 合法插入，删除用户级联删除 token
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES ('v1', ?, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&hash)
            .bind(now + 3_600_000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM email_verification_tokens")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1);

    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM email_verification_tokens")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 0, "删除用户必须级联删除其 token");

    close_pool(&pool).await;
    cleanup(&dir);
}
