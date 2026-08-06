//! M04-SCHEMA-03：drafts 表结构与仓储契约（SQLite）。
//!
//! - 独立草稿资源（OpenAPI Draft，与 posts 分离）：owner、可选板块、
//!   article/discussion、标题、Markdown、可见性预设、定时发布时间、版本、
//!   软删除；
//! - owner 维度 cursor 列表（updated_at 降序 + keyset before）；
//! - 更新递增 version（乐观并发）；软删除仅置 deleted_at（行保留）；
//! - board 删除 → 草稿 board_id 置空（ON DELETE SET NULL）。

use std::path::{Path, PathBuf};

use bblbb_backend::content::model::{Draft, PostType};
use bblbb_backend::content::repository::{
    delete_draft, get_draft, insert_draft, list_drafts_cursor, list_scheduled_drafts, update_draft,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-dfts-{}", uuid::Uuid::now_v7()));
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
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
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

async fn board_id_by_slug(pool: &DatabasePool, slug: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM boards WHERE slug = ?")
            .bind(slug)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn draft(id: &str, owner: &str, board: Option<&str>, updated_at: i64) -> Draft {
    Draft {
        id: id.to_string(),
        owner_id: owner.to_string(),
        board_id: board.map(str::to_string),
        post_type: PostType::Discussion,
        title: format!("draft {id}"),
        markdown: format!("body of {id}"),
        visibility_level: None,
        access_policy: None,
        scheduled_at: None,
        version: 1,
        created_at: updated_at,
        updated_at,
        deleted_at: None,
    }
}

/// 插入/读取往返（含 owner 隔离：他人不可读）。
#[tokio::test]
async fn draft_insert_and_get_with_owner_isolation() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "owner").await;
    let other = insert_user(&pool, "other").await;
    let general = board_id_by_slug(&pool, "general").await;
    let now = now_millis();

    let mut d = draft("d1", &owner, Some(&general), now);
    d.scheduled_at = Some(now + 86_400_000);
    insert_draft(&pool, &d).await.unwrap();

    let got = get_draft(&pool, "d1", &owner)
        .await
        .unwrap()
        .expect("owner 必须读到");
    assert_eq!(got.board_id.as_deref(), Some(general.as_str()));
    assert_eq!(got.markdown, "body of d1");
    assert_eq!(got.scheduled_at, Some(now + 86_400_000));
    assert_eq!(got.version, 1);

    // 他人读不到（owner 隔离）
    assert!(get_draft(&pool, "d1", &other).await.unwrap().is_none());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// cursor 列表：按 updated_at 降序 + keyset before 分页；跨 owner 隔离。
#[tokio::test]
async fn draft_cursor_listing_ordered_and_paginated() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "owner").await;
    let other = insert_user(&pool, "other").await;
    let base = now_millis();

    // 按时间倒序插入（updated_at 递增）
    for (i, id) in ["d1", "d2", "d3", "d4", "d5"].iter().enumerate() {
        insert_draft(
            &pool,
            &draft(id, &owner, None, base - 1000 * (5 - i as i64)),
        )
        .await
        .unwrap();
    }
    insert_draft(&pool, &draft("other-1", &other, None, base))
        .await
        .unwrap();

    // 第一页（最新 3 条）：d5, d4, d3
    let page1 = list_drafts_cursor(&pool, &owner, None, 3).await.unwrap();
    assert_eq!(
        page1.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        vec!["d5", "d4", "d3"]
    );
    // 第二页（before = 第一页最后一条 updated_at）：d2, d1
    let cursor = page1.last().unwrap().updated_at;
    let page2 = list_drafts_cursor(&pool, &owner, Some(cursor), 3)
        .await
        .unwrap();
    assert_eq!(
        page2.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        vec!["d2", "d1"]
    );
    // 第三页为空
    let page3 = list_drafts_cursor(&pool, &owner, Some(page2.last().unwrap().updated_at), 3)
        .await
        .unwrap();
    assert!(page3.is_empty());

    // owner 列表不含他人的草稿
    let all_owner = list_drafts_cursor(&pool, &owner, None, 100).await.unwrap();
    assert!(all_owner.iter().all(|d| d.owner_id == owner));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 更新递增 version（乐观并发）；软删除后从 get/list 隐藏但行保留。
#[tokio::test]
async fn draft_update_version_and_soft_delete() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "owner").await;
    let now = now_millis();

    insert_draft(&pool, &draft("d1", &owner, None, now))
        .await
        .unwrap();

    let mut d = get_draft(&pool, "d1", &owner).await.unwrap().unwrap();
    assert_eq!(d.version, 1);
    d.title = "updated".to_string();
    d.updated_at = now + 1;
    update_draft(&pool, &d).await.unwrap();
    let got = get_draft(&pool, "d1", &owner).await.unwrap().unwrap();
    assert_eq!(got.title, "updated");
    assert_eq!(got.version, 2, "更新必须递增 version");

    // 软删除：get 隐藏
    delete_draft(&pool, "d1", &owner, now + 2).await.unwrap();
    assert!(get_draft(&pool, "d1", &owner).await.unwrap().is_none());
    let list = list_drafts_cursor(&pool, &owner, None, 100).await.unwrap();
    assert!(list.is_empty(), "软删草稿不得出现在列表");

    // 行保留（供审计/恢复）
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM drafts WHERE id = 'd1' AND deleted_at IS NOT NULL",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1, "软删必须保留行（deleted_at 置位）");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 定时发布草稿扫描（scheduled_at 到期）与 board 删除置空。
#[tokio::test]
async fn scheduled_drafts_scan_and_board_delete_nullifies() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "owner").await;
    let general = board_id_by_slug(&pool, "general").await;
    let now = now_millis();

    let mut due = draft("d-due", &owner, Some(&general), now);
    due.scheduled_at = Some(now - 1000); // 已到期
    insert_draft(&pool, &due).await.unwrap();
    let mut later = draft("d-later", &owner, Some(&general), now);
    later.scheduled_at = Some(now + 86_400_000); // 未到期
    insert_draft(&pool, &later).await.unwrap();
    insert_draft(&pool, &draft("d-unsched", &owner, None, now))
        .await
        .unwrap();

    let due_list = list_scheduled_drafts(&pool, now, 100).await.unwrap();
    assert_eq!(
        due_list.iter().map(|d| d.id.as_str()).collect::<Vec<_>>(),
        vec!["d-due"],
        "只应返回已到期定时草稿"
    );

    // board 删除 → 草稿 board_id 置空（SET NULL，不级联删除）
    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM boards WHERE id = ?")
                .bind(&general)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let after = get_draft(&pool, "d-due", &owner).await.unwrap().unwrap();
    assert!(
        after.board_id.is_none(),
        "board 删除后草稿 board_id 必须置空"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
