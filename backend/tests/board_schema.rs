//! M03-SCHEMA-04：板块与板块级角色迁移契约——
//! - boards：parent_id（软自引用层级）、visibility（默认 public）与
//!   posting_mode（默认 normal）新增列 + CHECK 拒绝非法值；
//! - board_roles：板块启用角色（复合主键，删板块/角色级联）；
//! - board_role_assignments：带有效期的板块 assignment（UNIQUE(board_id,
//!   user_id, role_id)、expires_at 可空=永久、删用户级联）。

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
    let dir = std::env::temp_dir().join(format!("bblbb-board-{}", uuid::Uuid::now_v7()));
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

async fn insert_board(pool: &DatabasePool, slug: &str) -> String {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&board_id)
            .bind(slug)
            .bind(slug)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    board_id
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

/// boards 新列 + 默认值契约。
#[tokio::test]
async fn boards_gains_hierarchy_and_moderation_columns() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "boards").await;
    for required in ["parent_id", "visibility", "posting_mode"] {
        assert!(
            columns.iter().any(|c| c == required),
            "boards 缺少列 {required}，实际: {columns:?}"
        );
    }

    let board_id = insert_board(&pool, "announcements").await;
    match &pool {
        Either::Left(p) => {
            let (visibility, posting_mode): (String, String) =
                sqlx::query_as("SELECT visibility, posting_mode FROM boards WHERE id = ?")
                    .bind(&board_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(visibility, "public", "板块可见性默认必须 public");
            assert_eq!(posting_mode, "normal", "板块发帖模式默认必须 normal");

            // CHECK 约束拒绝非法取值
            let bad_vis = sqlx::query("UPDATE boards SET visibility = 'top-secret' WHERE id = ?")
                .bind(&board_id)
                .execute(p)
                .await
                .unwrap_err();
            assert!(
                matches!(bad_vis, sqlx::Error::Database(ref e) if e.is_check_violation()),
                "boards.visibility CHECK 必须生效: {bad_vis}"
            );
            let bad_mode = sqlx::query("UPDATE boards SET posting_mode = 'chaos' WHERE id = ?")
                .bind(&board_id)
                .execute(p)
                .await
                .unwrap_err();
            assert!(
                matches!(bad_mode, sqlx::Error::Database(ref e) if e.is_check_violation()),
                "boards.posting_mode CHECK 必须生效: {bad_mode}"
            );

            // parent_id 软自引用：子板块可指向父板块
            let child = uuid::Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO boards (id, slug, name, parent_id, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&child)
            .bind("child-board")
            .bind("子板块")
            .bind(&board_id)
            .bind(now_millis())
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap();
            let parent: Option<String> =
                sqlx::query_scalar("SELECT parent_id FROM boards WHERE id = ?")
                    .bind(&child)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(parent.as_deref(), Some(board_id.as_str()));
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// board_roles：复合主键 + 删除板块级联。
#[tokio::test]
async fn board_roles_composite_pk_and_cascade() {
    let (pool, dir) = pool_with_migrations().await;
    let board_id = insert_board(&pool, "gaming").await;
    let role_id = insert_role(&pool, "board_mod").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO board_roles (board_id, role_id, granted_at) VALUES (?, ?, ?)")
                .bind(&board_id)
                .bind(&role_id)
                .bind(now_millis())
                .execute(p)
                .await
                .unwrap();
            let dup = sqlx::query(
                "INSERT INTO board_roles (board_id, role_id, granted_at) VALUES (?, ?, ?)",
            )
            .bind(&board_id)
            .bind(&role_id)
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "board_roles 复合主键必须生效: {dup}"
            );

            sqlx::query("DELETE FROM boards WHERE id = ?")
                .bind(&board_id)
                .execute(p)
                .await
                .unwrap();
            let left: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM board_roles WHERE board_id = ?")
                    .bind(&board_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(left, 0, "删除板块必须级联清理 board_roles");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// board_role_assignments：UNIQUE(board,user,role) + expires_at 可空 + 删用户级联。
#[tokio::test]
async fn board_assignments_unique_expiry_and_cascade() {
    let (pool, dir) = pool_with_migrations().await;
    let board_id = insert_board(&pool, "devlog").await;
    let user_id = insert_user(&pool, "board").await;
    let role_id = insert_role(&pool, "board_mod").await;
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            // 永久 assignment（expires_at NULL）
            sqlx::query(
                "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_at, expires_at)
                 VALUES (?, ?, ?, ?, ?, NULL)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&board_id)
            .bind(&user_id)
            .bind(&role_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            // UNIQUE(board,user,role)：同组合二次插入必须失败
            let dup = sqlx::query(
                "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_at, expires_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&board_id)
            .bind(&user_id)
            .bind(&role_id)
            .bind(now)
            .bind(Some(now + 86_400_000))
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "board_role_assignments UNIQUE(board,user,role) 必须生效: {dup}"
            );

            // 删除用户 → assignment 级联清理
            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
            let left: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM board_role_assignments WHERE user_id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(left, 0, "删除用户必须级联清理 board_role_assignments");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}
