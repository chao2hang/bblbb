//! M04-SCHEMA-07：内容唯一约束（SQLite）。
//!
//! 1. 主题内楼层：comments (post_id, floor) 唯一（0038）；
//! 2. 板块内 slug：posts (board_id, slug) 唯一（0032）；
//! 3. revision 唯一：post_revisions (post_id, version) 唯一（0033）；
//! 4. 客户端请求 ID：idempotency_records (scope, key) 唯一（0010）。

use std::path::{Path, PathBuf};

use bblbb_backend::content::model::{
    Comment, CommentStatus, Post, PostRevision, PostStatus, PostType,
};
use bblbb_backend::content::repository::{insert_comment, insert_post, insert_post_revision};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-uq-{}", uuid::Uuid::now_v7()));
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

async fn seed_post(pool: &DatabasePool, id: &str, author: &str, board: &str, slug: Option<&str>) {
    let now = now_millis();
    insert_post(
        pool,
        &Post {
            id: id.to_string(),
            board_id: board.to_string(),
            author_id: author.to_string(),
            post_type: PostType::Discussion,
            slug: slug.map(str::to_string),
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

/// 主题内楼层唯一：同主题同楼层拒绝；跨主题同楼层允许。
#[tokio::test]
async fn floor_unique_per_post() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    let tech = board_id_by_slug(&pool, "tech").await;
    seed_post(&pool, "p1", &author, &general, None).await;
    seed_post(&pool, "p2", &author, &tech, None).await;

    let now = now_millis();
    let c1 = Comment {
        id: "c1".into(),
        post_id: "p1".into(),
        author_id: author.clone(),
        parent_id: None,
        quoted_comment_id: None,
        floor: 1,
        status: CommentStatus::Published,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    insert_comment(&pool, &c1).await.unwrap();
    // 同主题同楼层 → 唯一冲突
    let mut c1dup = c1.clone();
    c1dup.id = "c1dup".into();
    assert!(
        insert_comment(&pool, &c1dup).await.is_err(),
        "同主题同楼层必须唯一冲突"
    );
    // 跨主题同楼层 → 允许
    let mut c2 = c1.clone();
    c2.id = "c2".into();
    c2.post_id = "p2".into();
    insert_comment(&pool, &c2).await.unwrap();

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 板块内 slug 唯一 + revision 唯一 + idempotency (scope,key) 唯一。
#[tokio::test]
async fn slug_revision_and_request_id_uniqueness() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    let tech = board_id_by_slug(&pool, "tech").await;
    seed_post(&pool, "p1", &author, &general, Some("same-slug")).await;
    seed_post(&pool, "p2", &author, &tech, Some("same-slug")).await;

    // 板块内 slug 唯一（同板块同 slug 拒绝；跨板块允许——已在 posts_schema 覆盖，
    // 此处用原始 SQL 再断言一次唯一索引存在并生效）
    let now = now_millis();
    let r = match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO posts (id, board_id, author_id, post_type, slug, title, content, status, created_at, updated_at)
                 VALUES ('p3', ?, ?, 'discussion', 'same-slug', 'dup', '', 'published', ?, ?)",
            )
            .bind(&general)
            .bind(&author)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(r.is_err(), "同板块同 slug 必须唯一冲突");

    // revision 唯一（(post_id, version)）
    let r1 = PostRevision {
        id: "rev1".into(),
        post_id: "p1".into(),
        editor_id: author.clone(),
        body_markdown: "v1".into(),
        body_html: "<p>v1</p>".into(),
        restricted_markdown: None,
        restricted_html: None,
        renderer_version: "markdown-v1".into(),
        change_reason: None,
        version: 1,
        created_at: now,
    };
    insert_post_revision(&pool, &r1).await.unwrap();
    let mut r1dup = r1.clone();
    r1dup.id = "rev1dup".into();
    assert!(
        insert_post_revision(&pool, &r1dup).await.is_err(),
        "同 (post_id, version) 修订必须唯一冲突"
    );

    // 客户端请求 ID：idempotency_records (scope, key) 唯一
    let r = match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO idempotency_records (id, scope, key, request_hash, status, expires_at, created_at, updated_at)
                 VALUES ('i1', 'post.create', 'client-req-abc', 'hash1', 'in_progress', ?, ?, ?)",
            )
            .bind(now + 3_600_000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(r.is_ok());
    let r = match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO idempotency_records (id, scope, key, request_hash, status, expires_at, created_at, updated_at)
                 VALUES ('i2', 'post.create', 'client-req-abc', 'hash1', 'in_progress', ?, ?, ?)",
            )
            .bind(now + 3_600_000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(r.is_err(), "同 (scope, key) 幂等记录必须唯一冲突");

    close_pool(&pool).await;
    cleanup(&dir);
}
