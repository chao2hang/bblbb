//! M03-BOARDS-01：板块层级读取——最大深度限制与循环父级检测（真实 DB）。
//!
//! 纯函数边界（build_hierarchy / validate_parent）在 hierarchy.rs 单测覆盖；
//! 本文件锁定 DB 加载器与层级语义：
//! - 种子 5 个板块全部为根（深度 1，按 sort_order 稳定排序）；
//! - 嵌套板块反映到 depth/children/descendants；
//! - `is_active=0`（停用）与 `deleted_at`（软删）移出活跃投影；
//! - DB 内环路 / 超深 = 数据完整性故障 → 加载失败；
//! - 父板块软删 → 子板块提升为根并记录悬空引用（读取不中断）；
//! - 有子板块的父级禁止硬删除（has_children / descendant_ids 裁决）。

use std::path::{Path, PathBuf};

use bblbb_backend::boards::load_hierarchy;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-bh-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

/// 插入板块；`parent` 为空即根。
async fn insert_board(
    pool: &DatabasePool,
    slug: &str,
    parent: Option<&str>,
    sort_order: i64,
) -> String {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, parent_id, sort_order, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&board_id)
            .bind(slug)
            .bind(slug)
            .bind(slug)
            .bind(parent)
            .bind(sort_order)
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

/// 种子 5 个板块（0005，经 0006 归一化为合法 UUID v7）全部为根，深度 1，
/// 按 sort_order 稳定排序。
#[tokio::test]
async fn seed_boards_are_roots_in_stable_order() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let h = load_hierarchy(&pool).await.expect("种子层级必须可加载");
    let roots = h.roots();
    assert_eq!(roots.len(), 5, "种子必须是 5 个板块");
    assert_eq!(
        roots,
        &[
            "01911fd5-f000-7561-a2a5-3dd6434157f0".to_string(),
            "01911fd5-f001-758e-a95d-a58489fbb61d".to_string(),
            "01911fd5-f002-7222-8742-68e793fcdbd5".to_string(),
            "01911fd5-f003-7772-b594-c29b2b8c9021".to_string(),
            "01911fd5-f004-7d9c-b6c0-d2c3387e5534".to_string(),
        ],
        "根顺序必须 = sort_order（general/tech/creative/help/news，0006 归一化 id）"
    );
    for root in roots {
        assert_eq!(h.depth_of(root), Some(1));
    }
    assert!(h.dangling().is_empty());
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 嵌套板块：子板块反映到 depth/children/descendants。
#[tokio::test]
async fn nested_board_appears_in_hierarchy() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let general = "01911fd5-f000-7561-a2a5-3dd6434157f0".to_string();
    let child = insert_board(&pool, "general-rust", Some(&general), 0).await;
    let grandchild = insert_board(&pool, "general-rust-unsafe", Some(&child), 0).await;

    let h = load_hierarchy(&pool).await.expect("层级必须可加载");
    assert_eq!(h.depth_of(&child), Some(2));
    assert_eq!(h.depth_of(&grandchild), Some(3));
    assert_eq!(h.parent_of(&child), Some(general.as_str()));
    assert_eq!(h.children_of(&general), std::slice::from_ref(&child));
    assert_eq!(
        h.descendant_ids(&general),
        vec![child.as_str(), grandchild.as_str()],
        "BFS 展开全部后代"
    );
    assert!(h.has_children(&general));
    assert!(!h.has_children(&grandchild));
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 停用（is_active=0）与软删（deleted_at）移出活跃投影。
#[tokio::test]
async fn inactive_and_deleted_boards_are_excluded() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let off = insert_board(&pool, "off", None, 0).await;
    let gone = insert_board(&pool, "gone", None, 0).await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE boards SET is_active = 0 WHERE id = ?")
                .bind(&off)
                .execute(p)
                .await
                .unwrap();
            sqlx::query("UPDATE boards SET deleted_at = ? WHERE id = ?")
                .bind(now_millis())
                .bind(&gone)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let h = load_hierarchy(&pool).await.unwrap();
    assert_eq!(h.roots().len(), 5, "停用/软删板块必须移出活跃投影");
    assert_eq!(h.depth_of(&off), None);
    assert_eq!(h.depth_of(&gone), None);
    close_pool(&pool).await;
    cleanup(&dir);
}

/// DB 内环路（数据完整性故障）→ 加载失败并指明环上节点。
#[tokio::test]
async fn cycle_in_db_fails_load_with_path() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let a = insert_board(&pool, "cycle-a", None, 0).await;
    let b = insert_board(&pool, "cycle-b", Some(&a), 0).await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE boards SET parent_id = ? WHERE id = ?")
                .bind(&b)
                .bind(&a)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let err = load_hierarchy(&pool).await.expect_err("环路必须使加载失败");
    assert!(
        err.contains("cycle") || err.contains("Cycle"),
        "错误必须标明环路: {err}"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

/// DB 内超深链（深度 5 > MAX 4）→ 加载失败。
#[tokio::test]
async fn depth_exceeded_in_db_fails_load() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    // 构造 root → c2 → c3 → c4 → c5（深度 5）
    let ids = [
        insert_board(&pool, "d1", None, 0).await,
        insert_board(&pool, "d2", None, 0).await,
        insert_board(&pool, "d3", None, 0).await,
        insert_board(&pool, "d4", None, 0).await,
        insert_board(&pool, "d5", None, 0).await,
    ];
    match &pool {
        Either::Left(p) => {
            for i in 1..ids.len() {
                sqlx::query("UPDATE boards SET parent_id = ? WHERE id = ?")
                    .bind(&ids[i - 1])
                    .bind(&ids[i])
                    .execute(p)
                    .await
                    .unwrap();
            }
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let err = load_hierarchy(&pool).await.expect_err("超深必须使加载失败");
    assert!(
        err.contains("depth") || err.contains("MAX_BOARD_DEPTH"),
        "错误必须标明深度: {err}"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 父板块软删 → 子板块提升为根并记录悬空引用（读取不中断）。
#[tokio::test]
async fn soft_deleted_parent_promotes_child_and_records_dangling() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let parent = insert_board(&pool, "soft-parent", None, 0).await;
    let child = insert_board(&pool, "surviving-child", Some(&parent), 0).await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE boards SET deleted_at = ? WHERE id = ?")
                .bind(now_millis())
                .bind(&parent)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let h = load_hierarchy(&pool).await.expect("软删父级不得破坏读取");
    assert_eq!(h.depth_of(&parent), None, "软删父级不在活跃投影");
    assert_eq!(h.depth_of(&child), Some(1), "子板块提升为根");
    assert!(h.roots().iter().any(|r| r == &child));
    assert_eq!(
        h.dangling(),
        &[(child.clone(), parent.clone())],
        "悬空引用必须被记录"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 硬删除裁决：有子板块的父级必须被 has_children/descendant_ids 拦截。
#[tokio::test]
async fn hard_delete_rule_uses_has_children_and_descendants() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let parent = insert_board(&pool, "hd-parent", None, 0).await;
    let child = insert_board(&pool, "hd-child", Some(&parent), 0).await;
    let grandchild = insert_board(&pool, "hd-grandchild", Some(&child), 0).await;
    let _ = grandchild;

    let h = load_hierarchy(&pool).await.unwrap();
    // 有子板块 → 禁止硬删除（SCHEMA.md §6 由服务层裁决）
    assert!(h.has_children(&parent));
    assert!(h.has_children(&child));
    assert!(!h.has_children(&grandchild));
    assert_eq!(
        h.descendant_ids(&parent),
        vec![child.as_str(), grandchild.as_str()]
    );
    close_pool(&pool).await;
    cleanup(&dir);
}
