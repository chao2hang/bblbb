#![allow(dead_code)]
//! M13 共享测试造数助手（SQLite 真库 + 全量迁移）。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

pub async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-m13-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
    (pool, dir)
}

pub fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

pub async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

pub async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let sql = "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
               VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&user_id)
                .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
                .bind(format!(
                    "{tag}_{}@example.com",
                    uuid::Uuid::now_v7().simple()
                ))
                .bind(now - 30 * 86_400 * 1000)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&user_id)
                .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
                .bind(format!(
                    "{tag}_{}@example.com",
                    uuid::Uuid::now_v7().simple()
                ))
                .bind(now - 30 * 86_400 * 1000)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
    }
    user_id
}

/// 给用户授予全局角色（写入 user_roles）。
pub async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role_name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, 'test', ?, NULL)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}
