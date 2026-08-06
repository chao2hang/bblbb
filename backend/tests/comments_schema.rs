//! M04-SCHEMA-04：comments 元数据扩展与仓储契约（SQLite）。
//!
//! - 0035 后 comments 含：quoted_comment_id（引用回复，删除置空保留占位）、
//!   version（乐观并发）、deleted_at（软删）；
//! - parent_id/floor 为既有列（主题内楼层语义；唯一约束随 M04-SCHEMA-07）；
//! - 软删行保留（占位投影与审计）。

use std::path::{Path, PathBuf};

use bblbb_backend::content::model::{Comment, CommentStatus, Post, PostStatus, PostType};
use bblbb_backend::content::repository::{
    delete_comment, get_comment, insert_comment, insert_post, list_comments_by_post, update_comment,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-cmts-{}", uuid::Uuid::now_v7()));
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

async fn seed_post(pool: &DatabasePool, id: &str, author: &str, board: &str) {
    let now = now_millis();
    insert_post(
        pool,
        &Post {
            id: id.to_string(),
            board_id: board.to_string(),
            author_id: author.to_string(),
            post_type: PostType::Discussion,
            slug: None,
            title: format!("post {id}"),
            status: PostStatus::Published,
            version: 1,
            scheduled_at: None,
            published_at: Some(now),
            pinned_at: None,
            featured_at: None,
            closed_at: None,
            canonical_url: None,
            seo_title: None,
            seo_description: None,
            view_count: 0,
            reply_count: 0,
            last_reply_id: None,
            last_reply_at: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        },
    )
    .await
    .unwrap();
}

fn comment(id: &str, post: &str, author: &str, floor: i64, parent: Option<&str>) -> Comment {
    let now = now_millis();
    Comment {
        id: id.to_string(),
        post_id: post.to_string(),
        author_id: author.to_string(),
        parent_id: parent.map(str::to_string),
        quoted_comment_id: None,
        floor,
        status: CommentStatus::Published,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

/// 0035 新增列存在 + 插入/读取往返（parent/quote/floor/version）。
#[tokio::test]
async fn comments_metadata_columns_and_roundtrip() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let columns: Vec<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT name FROM pragma_table_info('comments')")
            .fetch_all(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    for column in [
        "quoted_comment_id",
        "version",
        "deleted_at",
        "parent_id",
        "floor",
    ] {
        assert!(
            columns.iter().any(|c| c == column),
            "comments 必须含列 {column}"
        );
    }

    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    let root = comment("c1", "p1", &author, 1, None);
    insert_comment(&pool, &root).await.unwrap();
    let mut reply = comment("c2", "p1", &author, 2, Some("c1"));
    reply.quoted_comment_id = Some("c1".to_string());
    insert_comment(&pool, &reply).await.unwrap();

    let got = get_comment(&pool, "c2").await.unwrap().expect("必须读到");
    assert_eq!(got.parent_id.as_deref(), Some("c1"));
    assert_eq!(got.quoted_comment_id.as_deref(), Some("c1"));
    assert_eq!(got.floor, 2);
    assert_eq!(got.version, 1);
    assert!(got.deleted_at.is_none());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 按 floor 升序列出主题评论。
#[tokio::test]
async fn comments_list_ordered_by_floor() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    for (i, id) in ["c1", "c2", "c3"].iter().enumerate() {
        insert_comment(&pool, &comment(id, "p1", &author, 3 - i as i64, None))
            .await
            .unwrap();
    }
    let list = list_comments_by_post(&pool, "p1").await.unwrap();
    assert_eq!(
        list.iter()
            .map(|c| (c.floor, c.id.as_str()))
            .collect::<Vec<_>>(),
        vec![(1, "c3"), (2, "c2"), (3, "c1")],
        "必须按 floor 升序"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 更新递增 version；软删除隐藏但行保留（占位投影）。
#[tokio::test]
async fn comment_version_bump_and_soft_delete() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;
    insert_comment(&pool, &comment("c1", "p1", &author, 1, None))
        .await
        .unwrap();

    let mut c = get_comment(&pool, "c1").await.unwrap().unwrap();
    c.updated_at = now_millis();
    update_comment(&pool, &c).await.unwrap();
    let got = get_comment(&pool, "c1").await.unwrap().unwrap();
    assert_eq!(got.version, 2, "更新必须递增 version");

    delete_comment(&pool, "c1", now_millis()).await.unwrap();
    let deleted = get_comment(&pool, "c1").await.unwrap().unwrap();
    assert!(deleted.deleted_at.is_some(), "软删除必须置 deleted_at");
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM comments WHERE id = 'c1'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1, "软删必须保留行");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// quoted_comment_id 引用删除 → 置空（占位语义），不级联删除引用方。
#[tokio::test]
async fn quoted_comment_delete_sets_null() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    insert_comment(&pool, &comment("c1", "p1", &author, 1, None))
        .await
        .unwrap();
    let mut quote = comment("c2", "p1", &author, 2, None);
    quote.quoted_comment_id = Some("c1".to_string());
    insert_comment(&pool, &quote).await.unwrap();

    // 删除被引用评论（物理删，测试 SET NULL）
    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM comments WHERE id = 'c1'")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let got = get_comment(&pool, "c2").await.unwrap().unwrap();
    assert!(got.quoted_comment_id.is_none(), "引用删除后必须置空");

    close_pool(&pool).await;
    cleanup(&dir);
}
