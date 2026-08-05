//! M03-SCHEMA-03：RBAC 迁移契约——roles / permissions / role_permissions /
//! user_roles：
//! - roles：name 唯一、is_system 标记内置角色；
//! - permissions：name 唯一（对应 OpenAPI x-permission / 权限矩阵）；
//! - role_permissions：复合主键 (role_id, permission_id)，删角色/权限级联；
//! - user_roles：复合主键 (user_id, role_id)，expires_at 可空、granted_by
//!   记录授予者、删用户级联。

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
    let dir = std::env::temp_dir().join(format!("bblbb-rbac-{}", uuid::Uuid::now_v7()));
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

async fn insert_role(pool: &DatabasePool, name: &str) -> String {
    let role_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO roles (id, name, display_name, is_system, created_at, updated_at)
                 VALUES (?, ?, ?, 0, ?, ?)",
            )
            .bind(&role_id)
            .bind(name)
            .bind(name)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    role_id
}

async fn insert_permission(pool: &DatabasePool, name: &str) -> String {
    let perm_id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO permissions (id, name, created_at) VALUES (?, ?, ?)")
                .bind(&perm_id)
                .bind(name)
                .bind(now_millis())
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    perm_id
}

/// roles / permissions / role_permissions / user_roles 全列契约。
#[tokio::test]
async fn rbac_tables_have_full_columns() {
    let (pool, dir) = pool_with_migrations().await;
    for (table, required) in [
        (
            "roles",
            vec![
                "id",
                "name",
                "display_name",
                "description",
                "is_system",
                "created_at",
                "updated_at",
            ],
        ),
        (
            "permissions",
            vec![
                "id",
                "name",
                "description",
                "risk_level",
                "is_system",
                "created_at",
            ],
        ),
        ("role_permissions", vec!["role_id", "permission_id"]),
        (
            "user_roles",
            vec![
                "user_id",
                "role_id",
                "granted_by",
                "granted_at",
                "expires_at",
            ],
        ),
    ] {
        let columns = table_columns(&pool, table).await;
        for col in required {
            assert!(
                columns.iter().any(|c| c == col),
                "{table} 缺少列 {col}，实际: {columns:?}"
            );
        }
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// roles.name 唯一：重复角色名第二次插入必须失败。
#[tokio::test]
async fn role_name_is_unique() {
    let (pool, dir) = pool_with_migrations().await;
    match &pool {
        Either::Left(p) => {
            let now = now_millis();
            sqlx::query(
                "INSERT INTO roles (id, name, display_name, is_system, created_at, updated_at)
                 VALUES (?, 'moderator', '版主', 1, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            let dup = sqlx::query(
                "INSERT INTO roles (id, name, display_name, is_system, created_at, updated_at)
                 VALUES (?, 'moderator', '版主二', 0, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "roles.name 唯一约束必须生效: {dup}"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// role_permissions 复合主键 + 删除角色级联清理映射。
#[tokio::test]
async fn role_permissions_composite_pk_and_cascade() {
    let (pool, dir) = pool_with_migrations().await;
    let role_id = insert_role(&pool, "moderator").await;
    let p1 = insert_permission(&pool, "post.hide").await;
    let p2 = insert_permission(&pool, "post.restore").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)")
                .bind(&role_id)
                .bind(&p1)
                .execute(p)
                .await
                .unwrap();
            sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)")
                .bind(&role_id)
                .bind(&p2)
                .execute(p)
                .await
                .unwrap();
            // 复合主键：同 (role, permission) 二次插入必须失败
            let dup =
                sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)")
                    .bind(&role_id)
                    .bind(&p1)
                    .execute(p)
                    .await
                    .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "role_permissions 复合主键必须生效: {dup}"
            );
            // 删除角色 → 映射级联清理
            sqlx::query("DELETE FROM roles WHERE id = ?")
                .bind(&role_id)
                .execute(p)
                .await
                .unwrap();
            let left: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM role_permissions WHERE role_id = ?")
                    .bind(&role_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(left, 0, "删除角色必须级联清理 role_permissions");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// user_roles：复合主键 + expires_at 可空 + 删除用户级联。
#[tokio::test]
async fn user_roles_composite_pk_expires_and_cascade() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "rbac").await;
    let role_id = insert_role(&pool, "member").await;
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            // 永久 assignment（expires_at NULL）
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(&user_id)
            .bind(&role_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            // 复合主键：同 (user, role) 二次插入必须失败
            let dup = sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, ?)",
            )
            .bind(&user_id)
            .bind(&role_id)
            .bind(now)
            .bind(Some(now + 1000))
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "user_roles 复合主键必须生效: {dup}"
            );

            // 删除用户 → assignment 级联清理
            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
            let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM user_roles WHERE user_id = ?")
                .bind(&user_id)
                .fetch_one(p)
                .await
                .unwrap();
            assert_eq!(left, 0, "删除用户必须级联清理 user_roles");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// permissions.risk_level：默认 normal + CHECK 拒绝非法风险等级。
#[tokio::test]
async fn permission_risk_level_default_and_check() {
    let (pool, dir) = pool_with_migrations().await;
    match &pool {
        Either::Left(p) => {
            let now = now_millis();
            sqlx::query(
                "INSERT INTO permissions (id, name, created_at) VALUES (?, 'post.hide', ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            let risk: String =
                sqlx::query_scalar("SELECT risk_level FROM permissions WHERE name = 'post.hide'")
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(risk, "normal", "权限风险等级默认必须为 normal");

            let invalid = sqlx::query(
                "UPDATE permissions SET risk_level = 'nuclear' WHERE name = 'post.hide'",
            )
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                matches!(invalid, sqlx::Error::Database(ref e) if e.is_check_violation()),
                "permissions.risk_level CHECK 必须生效: {invalid}"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}
