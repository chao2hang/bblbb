//! M03-BOARDS-02：板块 slug/标题/说明/排序/状态/发帖规则校验（真实 DB）。
//!
//! 纯函数规则在 validation.rs 单测覆盖；本文件锁定 DB 交互：
//! - `slug_exists`：种子 slug 命中、新 slug 未命中、插入后命中；
//! - `boards_slug_uq` 唯一索引兜底：同 slug 二次插入失败（unique violation）；
//! - 合法字段（含 readonly posting_mode）可插入且 CHECK 通过；
//! - 非法 posting_mode 被 0022/0025 CHECK 拒绝（服务层先挡，DB 兜底）。

use std::path::{Path, PathBuf};

use bblbb_backend::boards::{
    slug_exists, validate_board_fields, validate_board_update, BoardValidationError,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-bv-{}", uuid::Uuid::now_v7()));
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

/// 插入板块（跳过服务层校验，直接写 DB；校验规则由 validate_* 提供）。
async fn insert_board(pool: &DatabasePool, slug: &str, name: &str, posting_mode: &str) -> String {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, sort_order, posting_mode, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 0, ?, ?, ?)",
            )
            .bind(&board_id)
            .bind(slug)
            .bind(name)
            .bind(name)
            .bind(posting_mode)
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

/// slug_exists 命中种子 slug；新 slug 未命中；插入后命中。
#[tokio::test]
async fn slug_exists_matches_seed_and_new() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    assert!(
        slug_exists(&pool, "general").await.expect("查询必须成功"),
        "种子 slug general 必须已存在"
    );
    assert!(
        !slug_exists(&pool, "brand-new-board").await.unwrap(),
        "未创建的 slug 必须不存在"
    );

    insert_board(&pool, "brand-new-board", "新板块", "normal").await;
    assert!(slug_exists(&pool, "brand-new-board").await.unwrap());
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 合法字段通过服务层校验且可插入（含 readonly posting_mode，CHECK 通过）。
#[tokio::test]
async fn valid_fields_insert_and_checks_pass() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    validate_board_fields(
        "meta",
        "站务公告",
        Some("规则与公告 https://example.com"),
        5,
        true,
        "normal",
    )
    .expect("合法字段必须通过校验");
    validate_board_fields("archive", "归档", None, -3, false, "readonly")
        .expect("readonly 是合法发帖规则");

    insert_board(&pool, "meta", "站务公告", "normal").await;
    insert_board(&pool, "archive", "归档", "readonly").await;
    assert!(slug_exists(&pool, "meta").await.unwrap());
    assert!(slug_exists(&pool, "archive").await.unwrap());
    close_pool(&pool).await;
    cleanup(&dir);
}

/// boards_slug_uq 唯一索引兜底：同 slug 二次插入 → unique violation。
#[tokio::test]
async fn duplicate_slug_hits_unique_index() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    insert_board(&pool, "meta", "站务公告", "normal").await;

    let dup = insert_board_raw(&pool, "meta", "重复板块", "normal").await;
    match dup {
        Ok(_) => panic!("同 slug 二次插入必须被唯一索引拒绝"),
        Err(e) => assert!(
            matches!(e, sqlx::Error::Database(ref db) if db.is_unique_violation()),
            "必须是 unique violation: {e}"
        ),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 非法 posting_mode：服务层拒绝；绕过服务层直写 DB 也被 0022/0025 CHECK 兜底。
#[tokio::test]
async fn invalid_posting_mode_rejected_by_validation_and_db() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    assert_eq!(
        validate_board_update(None, None, None, None, Some("lockdown")),
        Err(BoardValidationError::InvalidPostingMode {
            value: "lockdown".to_string()
        })
    );

    match insert_board_raw(&pool, "bogus-mode", "非法模式", "lockdown").await {
        Ok(_) => panic!("非法 posting_mode 必须被 DB CHECK 拒绝"),
        Err(e) => assert!(
            matches!(e, sqlx::Error::Database(ref db) if db.is_check_violation()),
            "必须是 check violation: {e}"
        ),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

async fn insert_board_raw(
    pool: &DatabasePool,
    slug: &str,
    name: &str,
    posting_mode: &str,
) -> Result<String, sqlx::Error> {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, sort_order, posting_mode, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 0, ?, ?, ?)",
            )
            .bind(&board_id)
            .bind(slug)
            .bind(name)
            .bind(name)
            .bind(posting_mode)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?;
            Ok(board_id)
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}
