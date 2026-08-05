//! M03-SCHEMA-05：标签数据模型迁移契约——
//! - tag_groups：标签分组（slug 全局唯一、sort_order 默认 0）；
//! - tags 演进（0003 骨架升级）：新增 group_id（软引用 tag_groups，无 FK）、
//!   slug（可空，非空时全局唯一）、description（默认 ''）与 color；
//!   usage_count 保留为可重建缓存；
//! - board_tags：板块启用标签（复合主键，删板块/标签级联）。

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
    let dir = std::env::temp_dir().join(format!("bblbb-tags-{}", uuid::Uuid::now_v7()));
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

async fn insert_tag_group(pool: &DatabasePool, name: &str, slug: &str) -> String {
    let group_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO tag_groups (id, name, slug, created_at) VALUES (?, ?, ?, ?)")
                .bind(&group_id)
                .bind(name)
                .bind(slug)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    group_id
}

/// tag_groups：列契约 + slug 全局唯一 + sort_order 默认 0。
#[tokio::test]
async fn tag_groups_slug_unique_and_default_sort() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "tag_groups").await;
    for required in ["id", "name", "slug", "sort_order", "created_at"] {
        assert!(
            columns.iter().any(|c| c == required),
            "tag_groups 缺少列 {required}，实际: {columns:?}"
        );
    }

    let g1 = insert_tag_group(&pool, "编程", "programming").await;
    match &pool {
        Either::Left(p) => {
            // sort_order 默认 0
            let sort: i64 = sqlx::query_scalar("SELECT sort_order FROM tag_groups WHERE id = ?")
                .bind(&g1)
                .fetch_one(p)
                .await
                .unwrap();
            assert_eq!(sort, 0, "tag_groups.sort_order 默认必须 0");

            // slug 全局唯一
            let dup = sqlx::query(
                "INSERT INTO tag_groups (id, name, slug, created_at) VALUES (?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind("编程二号")
            .bind("programming")
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "tag_groups.slug 唯一约束必须生效: {dup}"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// tags 演进：新列存在、description 默认 ''、usage_count 保留、slug 非空唯一。
#[tokio::test]
async fn tags_evolution_columns_and_slug_uniqueness() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "tags").await;
    for required in [
        "id",
        "name",
        "usage_count",
        "created_at",
        "group_id",
        "slug",
        "description",
        "color",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "tags 缺少列 {required}，实际: {columns:?}"
        );
    }

    let t1 = insert_tag(&pool, "rust").await;
    match &pool {
        Either::Left(p) => {
            // 新列默认值：description ''、slug NULL、color NULL、usage_count 0
            let (description, slug, color, usage_count): (
                String,
                Option<String>,
                Option<String>,
                i64,
            ) = sqlx::query_as(
                "SELECT description, slug, color, usage_count FROM tags WHERE id = ?",
            )
            .bind(&t1)
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(description, "", "tags.description 默认必须为空字符串");
            assert_eq!(slug, None, "存量 tags.slug 默认必须为 NULL");
            assert_eq!(color, None, "tags.color 默认必须为 NULL");
            assert_eq!(usage_count, 0, "tags.usage_count 默认必须为 0");

            // 多个 NULL slug 允许（存量行不受唯一约束影响）
            sqlx::query("INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)")
                .bind(uuid::Uuid::now_v7().to_string())
                .bind("go")
                .bind(now_millis())
                .execute(p)
                .await
                .unwrap();

            // 非空 slug 全局唯一
            sqlx::query("UPDATE tags SET slug = 'rust' WHERE id = ?")
                .bind(&t1)
                .execute(p)
                .await
                .unwrap();
            let dup = sqlx::query("UPDATE tags SET slug = 'rust' WHERE name = 'go'")
                .execute(p)
                .await
                .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "tags.slug 非空时全局唯一必须生效: {dup}"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// group_id 软引用：可指向不存在的分组（无 FK 强约束），往返可读。
#[tokio::test]
async fn tags_group_id_is_soft_reference() {
    let (pool, dir) = pool_with_migrations().await;
    let t1 = insert_tag(&pool, "svelte").await;
    match &pool {
        Either::Left(p) => {
            // 软引用：指向不存在分组的 UUID 也能写入（ALTER 不能带 FK）
            let ghost = uuid::Uuid::now_v7().to_string();
            sqlx::query("UPDATE tags SET group_id = ? WHERE id = ?")
                .bind(&ghost)
                .bind(&t1)
                .execute(p)
                .await
                .unwrap();
            let read: Option<String> = sqlx::query_scalar("SELECT group_id FROM tags WHERE id = ?")
                .bind(&t1)
                .fetch_one(p)
                .await
                .unwrap();
            assert_eq!(read.as_deref(), Some(ghost.as_str()));
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// board_tags：复合主键 + 删板块/删标签级联。
#[tokio::test]
async fn board_tags_composite_pk_and_cascade() {
    let (pool, dir) = pool_with_migrations().await;
    let board_id = insert_board(&pool, "devlog").await;
    let tag_id = insert_tag(&pool, "rust").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO board_tags (board_id, tag_id) VALUES (?, ?)")
                .bind(&board_id)
                .bind(&tag_id)
                .execute(p)
                .await
                .unwrap();
            let dup = sqlx::query("INSERT INTO board_tags (board_id, tag_id) VALUES (?, ?)")
                .bind(&board_id)
                .bind(&tag_id)
                .execute(p)
                .await
                .unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "board_tags 复合主键必须生效: {dup}"
            );

            // 删板块 → 关联清理
            sqlx::query("DELETE FROM boards WHERE id = ?")
                .bind(&board_id)
                .execute(p)
                .await
                .unwrap();
            let left: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM board_tags WHERE tag_id = ?")
                .bind(&tag_id)
                .fetch_one(p)
                .await
                .unwrap();
            assert_eq!(left, 0, "删除板块必须级联清理 board_tags");

            // 删标签 → 关联清理
            sqlx::query("INSERT INTO board_tags (board_id, tag_id) VALUES (?, ?)")
                .bind(&board_id)
                .bind(&tag_id)
                .execute(p)
                .await
                .unwrap_err(); // board 已删除，先重建 board 才能再次验证
            let board2 = insert_board(&pool, "devlog2").await;
            sqlx::query("INSERT INTO board_tags (board_id, tag_id) VALUES (?, ?)")
                .bind(&board2)
                .bind(&tag_id)
                .execute(p)
                .await
                .unwrap();
            sqlx::query("DELETE FROM tags WHERE id = ?")
                .bind(&tag_id)
                .execute(p)
                .await
                .unwrap();
            let left: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM board_tags WHERE board_id = ?")
                    .bind(&board2)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(left, 0, "删除标签必须级联清理 board_tags");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}
