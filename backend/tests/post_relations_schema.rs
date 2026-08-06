//! M04-SCHEMA-05：帖子关联数据模型（SQLite）——封面引用、post_tags、
//! post_attachments。
//!
//! - posts.cover_attachment_id：只存附件 UUID（禁止 URL）；
//! - post_tags.created_at 默认 0，按 created_at 稳定排序，随 post 级联删除；
//! - post_attachments：kind（cover/gallery）CHECK、position 排序、级联删除；
//! - 引用回复关联（quoted_comment_id）已在 M04-SCHEMA-04 覆盖。

use std::path::{Path, PathBuf};

use bblbb_backend::content::model::{
    AttachmentKind, Post, PostAttachment, PostStatus, PostTag, PostType,
};
use bblbb_backend::content::repository::{
    insert_post, insert_post_attachment, insert_post_tag, list_post_attachments, list_post_tags,
    set_post_cover,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-prel-{}", uuid::Uuid::now_v7()));
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
            post_type: PostType::Article,
            slug: Some(format!("slug-{id}")),
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

async fn tag_id(pool: &DatabasePool, name: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO tags (id, name, usage_count, created_at, updated_at)
                 VALUES (?, ?, 0, ?, ?) ON CONFLICT(name) DO NOTHING",
            )
            .bind(&id)
            .bind(name)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query_scalar("SELECT id FROM tags WHERE name = ?")
                .bind(name)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 封面引用：只存附件 UUID，可读写可清空。
#[tokio::test]
async fn cover_attachment_reference_roundtrip() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    set_post_cover(&pool, "p1", Some("att-cover-1"))
        .await
        .unwrap();
    let cover: Option<String> = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT cover_attachment_id FROM posts WHERE id = 'p1'")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(cover.as_deref(), Some("att-cover-1"));

    set_post_cover(&pool, "p1", None).await.unwrap();
    let cover: Option<String> = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT cover_attachment_id FROM posts WHERE id = 'p1'")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(cover.is_none(), "清空封面引用必须成功");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// post_tags：created_at 默认 0、稳定排序、随 post 级联删除。
#[tokio::test]
async fn post_tags_link_and_cascade() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    let t1 = tag_id(&pool, "rust").await;
    let t2 = tag_id(&pool, "web-dev").await;
    let now = now_millis();
    insert_post_tag(
        &pool,
        &PostTag {
            post_id: "p1".into(),
            tag_id: t1.clone(),
            created_at: now,
        },
    )
    .await
    .unwrap();
    insert_post_tag(
        &pool,
        &PostTag {
            post_id: "p1".into(),
            tag_id: t2.clone(),
            created_at: now - 1000,
        },
    )
    .await
    .unwrap();

    let list = list_post_tags(&pool, "p1").await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].tag_id, t2, "created_at 早的先排");
    assert_eq!(list[1].tag_id, t1);

    // 重复关联 → 主键冲突
    assert!(insert_post_tag(
        &pool,
        &PostTag {
            post_id: "p1".into(),
            tag_id: t1.clone(),
            created_at: now
        }
    )
    .await
    .is_err());

    // 随 post 级联删除
    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM posts WHERE id = 'p1'")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM post_tags WHERE post_id = 'p1'")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 0, "post_tags 必须随 post 级联删除");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// post_attachments：kind CHECK、position 排序、级联删除。
#[tokio::test]
async fn post_attachments_references() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    let now = now_millis();
    insert_post_attachment(
        &pool,
        &PostAttachment {
            id: "pa1".into(),
            post_id: "p1".into(),
            attachment_id: "att-a".into(),
            kind: AttachmentKind::Cover,
            position: 0,
            created_at: now,
        },
    )
    .await
    .unwrap();
    insert_post_attachment(
        &pool,
        &PostAttachment {
            id: "pa2".into(),
            post_id: "p1".into(),
            attachment_id: "att-b".into(),
            kind: AttachmentKind::Gallery,
            position: 1,
            created_at: now,
        },
    )
    .await
    .unwrap();

    let list = list_post_attachments(&pool, "p1").await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].attachment_id, "att-a");
    assert_eq!(list[0].kind, AttachmentKind::Cover);
    assert_eq!(list[1].position, 1);

    // 非法 kind → CHECK 拒绝
    let r = match &pool {
        Either::Left(p) => sqlx::query(
            "INSERT INTO post_attachments (id, post_id, attachment_id, kind, position, created_at)
                 VALUES ('pa3', 'p1', 'att-c', 'bogus', 2, ?)",
        )
        .bind(now)
        .execute(p)
        .await,
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(r.is_err(), "未知 kind 必须被 CHECK 拒绝");

    // 级联删除
    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM posts WHERE id = 'p1'")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    assert!(list_post_attachments(&pool, "p1").await.unwrap().is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}
