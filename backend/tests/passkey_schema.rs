//! M02-MFA-PK：Passkey 迁移契约——
//! - `passkey_credentials`：WebAuthn 凭据（credential id 唯一、公钥 JSON 可解、
//!   backup 标志、created_at/last_used_at/revoked_at 状态、用户删除级联清理）；
//! - `webauthn_challenges`：服务端 challenge state（purpose 区分注册/登录断言、
//!   绑定列、一次性 consumed_at）。

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
    let dir = std::env::temp_dir().join(format!("bblbb-pk-schema-{}", uuid::Uuid::now_v7()));
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

/// passkey_credentials 契约：全列存在（公钥 JSON 不可哈希 + backup 标志 + 状态）。
#[tokio::test]
async fn passkey_schema_has_full_credential_columns() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "passkey_credentials").await;

    for required in [
        "id",
        "user_id",
        "name",            // 用户可读标签
        "credential_id",   // WebAuthn credential id 原始字节（唯一查找键）
        "credential_json", // webauthn-rs Passkey 序列化（公钥必需，不可哈希）
        "aaguid",
        "backup_eligible",
        "backed_up",
        "created_at",
        "last_used_at",
        "revoked_at", // NULL = 有效
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "passkey_credentials 缺少列 {required}，实际: {columns:?}"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// webauthn_challenges 契约：purpose 区分 + 绑定列 + 一次性消费。
#[tokio::test]
async fn webauthn_challenges_schema_has_binding_columns() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "webauthn_challenges").await;

    for required in [
        "id",
        "purpose",            // 'registration' | 'authentication'
        "user_id",            // registration 绑定会话用户；authentication 为 NULL
        "mfa_challenge_hash", // authentication 绑定 mfa_login_challenges.token_hash
        "state_json",         // webauthn-rs state 序列化（含服务端生成的 challenge）
        "created_at",
        "expires_at",
        "consumed_at", // NULL = 未消费
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "webauthn_challenges 缺少列 {required}，实际: {columns:?}"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// credential id 全局唯一（含已撤销行）：同 id 二次插入必须失败。
#[tokio::test]
async fn credential_id_is_globally_unique() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "uq").await;
    let now = now_millis();

    let insert = |id: &str, cred: Vec<u8>| {
        let pool_ref = &pool;
        let user_id = user_id.clone();
        let id = id.to_string();
        let cred = cred.clone();
        async move {
            match pool_ref {
                Either::Left(p) => {
                    sqlx::query(
                        "INSERT INTO passkey_credentials
                         (id, user_id, name, credential_id, credential_json, aaguid,
                          backup_eligible, backed_up, created_at, last_used_at, revoked_at)
                         VALUES (?, ?, 'k', ?, '{}', NULL, 0, 0, ?, NULL, NULL)",
                    )
                    .bind(&id)
                    .bind(&user_id)
                    .bind(&cred)
                    .bind(now)
                    .execute(p)
                    .await
                }
                Either::Right(_) => panic!("SQLite only"),
            }
        }
    };

    insert("pk-a", vec![1, 2, 3]).await.unwrap();
    let dup = insert("pk-b", vec![1, 2, 3]).await;
    assert!(dup.is_err(), "重复 credential_id 必须撞唯一约束");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 用户删除必须级联清理 passkey_credentials（与 totp_credentials 同契约）。
#[tokio::test]
async fn user_deletion_cascades_to_passkeys() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "cascade").await;
    let now = now_millis();

    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO passkey_credentials
                 (id, user_id, name, credential_id, credential_json, aaguid,
                  backup_eligible, backed_up, created_at, last_used_at, revoked_at)
                 VALUES ('pk-c', ?, 'k', x'0102', '{}', NULL, 0, 0, ?, NULL, NULL)",
            )
            .bind(&user_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let left: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM passkey_credentials")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(left, 0, "用户删除必须级联清理 passkey_credentials");

    close_pool(&pool).await;
    cleanup(&dir);
}
