//! M03-SCHEMA-06：role、permission、board 与 assignment 的删除/停用语义——
//! - boards 软删除（deleted_at）与停用（is_active=0）投影，及 SCHEMA.md §6
//!   记录的 (parent_id, sort_order)、(visibility, deleted_at) 索引；
//! - 删除级联链：删角色 → role_permissions/user_roles/board_roles/
//!   board_role_assignments；删权限 → role_permissions；删板块 →
//!   board_roles/board_role_assignments/board_tags；
//! - is_system=1 的 role/permission 在数据库层仍可物理删除，证明"系统行
//!   不可删"必须作为应用约束由服务层（M03-AUTHZ）强制，而非依赖数据库。

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
    let dir = std::env::temp_dir().join(format!("bblbb-del-{}", uuid::Uuid::now_v7()));
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

async fn insert_role(pool: &DatabasePool, name: &str, is_system: bool) -> String {
    let role_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO roles (id, name, display_name, is_system, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&role_id)
            .bind(name)
            .bind(name)
            .bind(is_system as i64)
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
    let permission_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO permissions (id, name, created_at) VALUES (?, ?, ?)")
                .bind(&permission_id)
                .bind(name)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    permission_id
}

async fn insert_tag(pool: &DatabasePool, name: &str) -> String {
    let tag_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
                .bind(&tag_id)
                .bind(name)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    tag_id
}

async fn count_rows(pool: &DatabasePool, sql: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// boards 软删除/停用投影与 SCHEMA.md §6 索引。
#[tokio::test]
async fn boards_soft_delete_disable_and_indexes() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "boards").await;
    assert!(
        columns.iter().any(|c| c == "deleted_at"),
        "boards 缺少 deleted_at，实际: {columns:?}"
    );

    let board_id = insert_board(&pool, "announcements").await;
    match &pool {
        Either::Left(p) => {
            // 索引落地
            let indexes: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type = 'index'
                 AND name IN ('boards_parent_sort_idx', 'boards_visibility_deleted_idx')",
            )
            .fetch_all(p)
            .await
            .unwrap();
            assert_eq!(indexes.len(), 2, "boards 文档索引缺失: {indexes:?}");

            // 活跃投影：is_active=1 且 deleted_at IS NULL（routes/boards.rs 语义）
            let active: i64 = count_rows(
                &pool,
                "SELECT COUNT(*) FROM boards WHERE slug = 'announcements'
                 AND is_active = 1 AND deleted_at IS NULL",
            )
            .await;
            assert_eq!(active, 1, "新建板块必须在活跃投影内");

            // 停用（is_active=0）：移出活跃投影
            sqlx::query("UPDATE boards SET is_active = 0 WHERE id = ?")
                .bind(&board_id)
                .execute(p)
                .await
                .unwrap();
            let active: i64 = count_rows(
                &pool,
                "SELECT COUNT(*) FROM boards WHERE slug = 'announcements'
                 AND is_active = 1 AND deleted_at IS NULL",
            )
            .await;
            assert_eq!(active, 0, "停用板块必须移出活跃投影");

            // 恢复并软删除（deleted_at）：同样移出活跃投影
            sqlx::query("UPDATE boards SET is_active = 1, deleted_at = ? WHERE id = ?")
                .bind(now_millis())
                .bind(&board_id)
                .execute(p)
                .await
                .unwrap();
            let active: i64 = count_rows(
                &pool,
                "SELECT COUNT(*) FROM boards WHERE slug = 'announcements'
                 AND is_active = 1 AND deleted_at IS NULL",
            )
            .await;
            assert_eq!(active, 0, "软删除板块必须移出活跃投影");

            // 软删除行本身仍可查（软删除 = 保留行）
            let deleted_at: Option<i64> =
                sqlx::query_scalar("SELECT deleted_at FROM boards WHERE id = ?")
                    .bind(&board_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert!(
                deleted_at.is_some(),
                "软删除必须写入 deleted_at 而非物理删除行"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 删角色 → 级联清理 role_permissions/user_roles/board_roles/board_role_assignments。
#[tokio::test]
async fn role_delete_cascades_all_assignment_tables() {
    let (pool, dir) = pool_with_migrations().await;
    let role_id = insert_role(&pool, "board_mod", false).await;
    let user_id = insert_user(&pool, "role").await;
    let board_id = insert_board(&pool, "gaming").await;
    let permission_id = insert_permission(&pool, "post.edit_any").await;
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)")
                .bind(&role_id)
                .bind(&permission_id)
                .execute(p)
                .await
                .unwrap();
            sqlx::query("INSERT INTO user_roles (user_id, role_id, granted_at) VALUES (?, ?, ?)")
                .bind(&user_id)
                .bind(&role_id)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            sqlx::query("INSERT INTO board_roles (board_id, role_id, granted_at) VALUES (?, ?, ?)")
                .bind(&board_id)
                .bind(&role_id)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&board_id)
            .bind(&user_id)
            .bind(&role_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();

            sqlx::query("DELETE FROM roles WHERE id = ?")
                .bind(&role_id)
                .execute(p)
                .await
                .unwrap();

            let left_rp = count_rows(&pool, "SELECT COUNT(*) FROM role_permissions").await;
            let left_ur = count_rows(&pool, "SELECT COUNT(*) FROM user_roles").await;
            let left_br = count_rows(&pool, "SELECT COUNT(*) FROM board_roles").await;
            let left_bra = count_rows(&pool, "SELECT COUNT(*) FROM board_role_assignments").await;
            assert_eq!(
                (left_rp, left_ur, left_br, left_bra),
                (0, 0, 0, 0),
                "删角色必须级联清理全部 assignment 表"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 删权限 → 级联清理 role_permissions。
#[tokio::test]
async fn permission_delete_cascades_role_permissions() {
    let (pool, dir) = pool_with_migrations().await;
    let role_id = insert_role(&pool, "member", false).await;
    let permission_id = insert_permission(&pool, "board.read").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)")
                .bind(&role_id)
                .bind(&permission_id)
                .execute(p)
                .await
                .unwrap();

            sqlx::query("DELETE FROM permissions WHERE id = ?")
                .bind(&permission_id)
                .execute(p)
                .await
                .unwrap();
            let left = count_rows(&pool, "SELECT COUNT(*) FROM role_permissions").await;
            assert_eq!(left, 0, "删权限必须级联清理 role_permissions");

            // 角色本身保留（删权限不应波及角色）
            let roles = count_rows(&pool, "SELECT COUNT(*) FROM roles").await;
            assert_eq!(roles, 1, "删权限不应删除角色");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 删板块 → 级联清理 board_roles/board_role_assignments/board_tags。
#[tokio::test]
async fn board_delete_cascades_all_board_relations() {
    let (pool, dir) = pool_with_migrations().await;
    let board_id = insert_board(&pool, "devlog").await;
    let role_id = insert_role(&pool, "board_mod", false).await;
    let user_id = insert_user(&pool, "board").await;
    let tag_id = insert_tag(&pool, "rust").await;
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO board_roles (board_id, role_id, granted_at) VALUES (?, ?, ?)")
                .bind(&board_id)
                .bind(&role_id)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&board_id)
            .bind(&user_id)
            .bind(&role_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query("INSERT INTO board_tags (board_id, tag_id) VALUES (?, ?)")
                .bind(&board_id)
                .bind(&tag_id)
                .execute(p)
                .await
                .unwrap();

            sqlx::query("DELETE FROM boards WHERE id = ?")
                .bind(&board_id)
                .execute(p)
                .await
                .unwrap();

            let left_br = count_rows(&pool, "SELECT COUNT(*) FROM board_roles").await;
            let left_bra = count_rows(&pool, "SELECT COUNT(*) FROM board_role_assignments").await;
            let left_bt = count_rows(&pool, "SELECT COUNT(*) FROM board_tags").await;
            assert_eq!(
                (left_br, left_bra, left_bt),
                (0, 0, 0),
                "删板块必须级联清理 board_roles/board_role_assignments/board_tags"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 系统 role/permission 在数据库层仍可物理删除——证明"系统行不可删"必须由
/// 应用层（M03-AUTHZ）强制，数据库仅负责级联完整性。
#[tokio::test]
async fn system_rows_are_db_deletable_so_guard_must_be_service_layer() {
    let (pool, dir) = pool_with_migrations().await;
    let system_role = insert_role(&pool, "administrator", true).await;
    let system_permission = insert_permission(&pool, "admin.*").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)")
                .bind(&system_role)
                .bind(&system_permission)
                .execute(p)
                .await
                .unwrap();

            // 数据库允许删除 is_system=1 行（无触发器防护）
            sqlx::query("DELETE FROM permissions WHERE id = ?")
                .bind(&system_permission)
                .execute(p)
                .await
                .unwrap();
            let left = count_rows(&pool, "SELECT COUNT(*) FROM permissions").await;
            assert_eq!(left, 0, "数据库层允许物理删除系统权限（预期行为）");

            // 若服务层把关，这里本来不应发生；本测试锁定"数据库不背锅"的事实，
            // 让 M03-AUTHZ 的应用约束测试成为唯一防线。
            sqlx::query("DELETE FROM roles WHERE id = ?")
                .bind(&system_role)
                .execute(p)
                .await
                .unwrap();
            let left = count_rows(&pool, "SELECT COUNT(*) FROM roles").await;
            assert_eq!(left, 0, "数据库层允许物理删除系统角色（预期行为）");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}
