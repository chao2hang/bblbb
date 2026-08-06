//! M04-POSTS-09：pin/feature/close/move/merge 治理命令应用（SQLite）。
//!
//! 覆盖：置顶/精选/关闭时间戳置位与清除；移帖到活跃/停用板块；合并迁移评论
//! + 软删源帖 + reply_count 累加；命令校验矩阵在 govern.rs 单测。

use std::path::{Path, PathBuf};

use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::govern::{
    apply_close, apply_feature, apply_merge, apply_move, apply_pin, CloseCommand, FeatureCommand,
    GovernError, MergeCommand, MoveCommand, PinCommand,
};
use bblbb_backend::content::posts::service::publish_new_post;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const BOARD_GENERAL: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // 'general'
const BOARD_TECH: &str = "01911fd5-f001-758e-a95d-a58489fbb61d"; // 'tech'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-pgov-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
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

async fn insert_author(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 25 * 3600 * 1000)
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

async fn publish(pool: &DatabasePool, author: &str, board: &str, title: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: title.to_string(),
            markdown: format!("正文 {title}"),
            board_id: board.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("g-{}-{}", title, uuid::Uuid::now_v7().simple()),
        },
        5,
        now_millis(),
    )
    .unwrap();
    publish_new_post(pool, &cmd, author, now_millis())
        .await
        .unwrap()
        .post
        .id
}

async fn col_value(pool: &DatabasePool, col: &str, post_id: &str) -> Option<i64> {
    match pool {
        Either::Left(p) => sqlx::query_scalar(&format!("SELECT {col} FROM posts WHERE id = ?"))
            .bind(post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

#[tokio::test]
async fn pin_feature_close_roundtrip() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "a").await;
    let post_id = publish(&pool, &author, BOARD_GENERAL, "治理帖").await;

    let now = now_millis();
    apply_pin(
        &pool,
        &PinCommand {
            post_id: post_id.clone(),
            pin: true,
        }
        .validate()
        .unwrap(),
        now,
    )
    .await
    .unwrap();
    assert!(
        col_value(&pool, "pinned_at", &post_id).await.is_some(),
        "置顶时间戳置位"
    );
    apply_pin(
        &pool,
        &PinCommand {
            post_id: post_id.clone(),
            pin: false,
        }
        .validate()
        .unwrap(),
        now,
    )
    .await
    .unwrap();
    assert!(
        col_value(&pool, "pinned_at", &post_id).await.is_none(),
        "取消置顶置空"
    );

    apply_feature(
        &pool,
        &FeatureCommand {
            post_id: post_id.clone(),
            feature: true,
        }
        .validate()
        .unwrap(),
        now,
    )
    .await
    .unwrap();
    assert!(col_value(&pool, "featured_at", &post_id).await.is_some());
    apply_close(
        &pool,
        &CloseCommand {
            post_id: post_id.clone(),
            close: true,
        }
        .validate()
        .unwrap(),
        now,
    )
    .await
    .unwrap();
    assert!(
        col_value(&pool, "closed_at", &post_id).await.is_some(),
        "关闭锁帖置位"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn move_post_validates_target_board() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "b").await;
    let post_id = publish(&pool, &author, BOARD_GENERAL, "移帖").await;

    // 移到活跃板块
    apply_move(
        &pool,
        &MoveCommand {
            post_id: post_id.clone(),
            target_board_id: BOARD_TECH.to_string(),
        }
        .validate()
        .unwrap(),
        now_millis(),
    )
    .await
    .unwrap();
    let board: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT board_id FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(board, BOARD_TECH, "帖子已移到目标板块");

    // 移到停用板块 → 阻断
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE boards SET is_active = 0 WHERE id = ?")
                .bind(BOARD_TECH)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let r = apply_move(
        &pool,
        &MoveCommand {
            post_id: post_id.clone(),
            target_board_id: BOARD_TECH.to_string(),
        }
        .validate()
        .unwrap(),
        now_millis(),
    )
    .await;
    assert_eq!(r.unwrap_err(), GovernError::TargetBoardNotActive);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn merge_moves_comments_and_soft_deletes_source() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "c").await;
    let source = publish(&pool, &author, BOARD_GENERAL, "源帖").await;
    let target = publish(&pool, &author, BOARD_GENERAL, "目标帖").await;

    // 源帖 2 条评论
    let now = now_millis();
    for i in 0..2 {
        match &pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO comments (id, post_id, author_id, parent_id, content, content_format, status, floor, created_at, updated_at)
                     VALUES (?, ?, ?, NULL, ?, 'markdown', 'published', ?, ?, ?)",
                )
                .bind(format!("cmt-{i}"))
                .bind(&source)
                .bind(&author)
                .bind(format!("评论{i}"))
                .bind(i)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            }
            Either::Right(_) => panic!("SQLite only"),
        }
    }
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET reply_count = 2 WHERE id = ?")
                .bind(&source)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    apply_merge(
        &pool,
        &MergeCommand {
            source_post_id: source.clone(),
            target_post_id: target.clone(),
        }
        .validate()
        .unwrap(),
        now,
    )
    .await
    .unwrap();

    // 评论已迁移到目标帖
    let moved: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM comments WHERE post_id = ?")
            .bind(&target)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(moved, 2, "评论必须迁移到目标帖");
    // 源帖软删
    let src_status: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
            .bind(&source)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(src_status, "deleted", "源帖必须软删");
    // 目标帖 reply_count 累加
    let rc: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT reply_count FROM posts WHERE id = ?")
            .bind(&target)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(rc, 2, "目标帖 reply_count 必须累加源帖回复数");

    // 合并自身 → 校验拒绝
    let r = MergeCommand {
        source_post_id: target.clone(),
        target_post_id: target,
    }
    .validate();
    assert_eq!(r.unwrap_err(), GovernError::MergeIntoSelf);

    close_pool(&pool).await;
    cleanup(&dir);
}
