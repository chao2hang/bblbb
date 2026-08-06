//! M04-SCHEMA-01：posts 元数据表结构、约束与仓储契约（SQLite）。
//!
//! - 迁移 0032 后 posts 表含：post_type、slug（板块内唯一）、version、
//!   scheduled_at/published_at/pinned_at/featured_at/closed_at、SEO 字段、
//!   deleted_at、last_reply_id；
//! - 板块内 slug 唯一（同 slug 跨板块允许；slug 可空，多 NULL 不冲突）；
//! - post_type CHECK 拒绝未知值；status CHECK 保持 0003 值域；
//! - 骨架遗留列（content/content_format/visibility/pinned/last_reply_by）
//!   仍存在（M04-POSTS 收口前兼容）；
//! - comments(post_id → posts.id) 外键仍可用。

use std::path::{Path, PathBuf};

use bblbb_backend::content::model::{Post, PostStatus, PostType};
use bblbb_backend::content::repository::{get_post, insert_post};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-psch-{}", uuid::Uuid::now_v7()));
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

fn post(
    id: &str,
    board_id: &str,
    author_id: &str,
    post_type: PostType,
    slug: Option<&str>,
    status: PostStatus,
) -> Post {
    let now = now_millis();
    Post {
        id: id.to_string(),
        board_id: board_id.to_string(),
        author_id: author_id.to_string(),
        post_type,
        slug: slug.map(str::to_string),
        title: format!("post {id}"),
        status,
        version: 1,
        scheduled_at: None,
        published_at: if status == PostStatus::Published {
            Some(now)
        } else {
            None
        },
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
    }
}

/// posts 表包含 0032 新增的全部元数据列（PRAGMA table_info）。
#[tokio::test]
async fn posts_table_has_metadata_columns() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let expected = [
        "post_type",
        "slug",
        "excerpt",
        "version",
        "scheduled_at",
        "published_at",
        "pinned_at",
        "featured_at",
        "closed_at",
        "canonical_url",
        "seo_title",
        "seo_description",
        "last_reply_id",
        "deleted_at",
    ];
    let columns: Vec<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT name FROM pragma_table_info('posts')")
            .fetch_all(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    for column in expected {
        assert!(
            columns.iter().any(|c| c == column),
            "posts 表必须包含列 {column}"
        );
    }
    // 骨架遗留列保持（M04-POSTS 收口前兼容）
    for legacy in [
        "content",
        "content_format",
        "visibility",
        "pinned",
        "last_reply_by",
    ] {
        assert!(
            columns.iter().any(|c| c == legacy),
            "骨架遗留列 {legacy} 必须保留"
        );
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 板块内 slug 唯一：同板块同 slug 冲突；跨板块同 slug 允许；slug 可空。
#[tokio::test]
async fn board_scoped_slug_uniqueness() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    let tech = board_id_by_slug(&pool, "tech").await;

    let p1 = post(
        "p1",
        &general,
        &author,
        PostType::Article,
        Some("hello-world"),
        PostStatus::Published,
    );
    insert_post(&pool, &p1).await.unwrap();

    // 同板块同 slug → 唯一冲突
    let p2 = post(
        "p2",
        &general,
        &author,
        PostType::Article,
        Some("hello-world"),
        PostStatus::Published,
    );
    assert!(
        insert_post(&pool, &p2).await.is_err(),
        "同板块同 slug 必须唯一冲突"
    );

    // 跨板块同 slug → 允许
    let p3 = post(
        "p3",
        &tech,
        &author,
        PostType::Article,
        Some("hello-world"),
        PostStatus::Published,
    );
    insert_post(&pool, &p3).await.unwrap();

    // 同板块两个 NULL slug（草稿）→ 允许
    let p4 = post(
        "p4",
        &general,
        &author,
        PostType::Discussion,
        None,
        PostStatus::Draft,
    );
    let p5 = post(
        "p5",
        &general,
        &author,
        PostType::Discussion,
        None,
        PostStatus::Draft,
    );
    insert_post(&pool, &p4).await.unwrap();
    insert_post(&pool, &p5).await.unwrap();

    close_pool(&pool).await;
    cleanup(&dir);
}

/// post_type CHECK 拒绝未知值；status CHECK 接受稳定状态值。
#[tokio::test]
async fn post_type_and_status_checks() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;

    // 合法 post_type
    let p = post(
        "p1",
        &general,
        &author,
        PostType::Article,
        None,
        PostStatus::Draft,
    );
    insert_post(&pool, &p).await.unwrap();

    // 非法 post_type → CHECK 拒绝
    let now = now_millis();
    let r = match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO posts (id, board_id, author_id, post_type, title, content, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'blog', 'x', '', 'draft', ?, ?)",
            )
            .bind("p-bad-type")
            .bind(&general)
            .bind(&author)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(r.is_err(), "未知 post_type 必须被 CHECK 拒绝");

    // 合法状态值均可写
    for (i, status) in ["draft", "published", "hidden", "deleted"]
        .iter()
        .enumerate()
    {
        let id = format!("p-st-{i}");
        let r = match &pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO posts (id, board_id, author_id, post_type, title, content, status, created_at, updated_at)
                     VALUES (?, ?, ?, 'discussion', 'x', '', ?, ?, ?)",
                )
                .bind(&id)
                .bind(&general)
                .bind(&author)
                .bind(status)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
            }
            Either::Right(_) => panic!("SQLite only"),
        };
        assert!(r.is_ok(), "状态 {status} 必须可写");
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 仓储插入/读取往返（含发布时间、版本、slug 与软删时间）。
#[tokio::test]
async fn repository_insert_and_read_roundtrip() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    let published_at = now_millis();
    let mut p = post(
        "p-roundtrip",
        &general,
        &author,
        PostType::Article,
        Some("guide"),
        PostStatus::Published,
    );
    p.published_at = Some(published_at);
    p.view_count = 3;
    p.reply_count = 2;
    insert_post(&pool, &p).await.unwrap();

    let got = get_post(&pool, "p-roundtrip")
        .await
        .unwrap()
        .expect("必须读到");
    assert_eq!(got.post_type, PostType::Article);
    assert_eq!(got.status, PostStatus::Published);
    assert_eq!(got.slug.as_deref(), Some("guide"));
    assert_eq!(got.published_at, Some(published_at));
    assert_eq!(got.version, 1);
    assert_eq!(got.view_count, 3);
    assert_eq!(got.reply_count, 2);
    assert!(got.replies_open());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// comments 外键仍引用 posts（0032 后不破坏既有约束）。
#[tokio::test]
async fn comments_foreign_key_remains_intact() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    let p = post(
        "p-fk",
        &general,
        &author,
        PostType::Discussion,
        None,
        PostStatus::Published,
    );
    insert_post(&pool, &p).await.unwrap();

    let now = now_millis();
    let r = match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO comments (id, post_id, author_id, content, content_format, status, floor, created_at, updated_at)
                 VALUES (?, ?, ?, 'reply', 'markdown', 'published', 1, ?, ?)",
            )
            .bind("c1")
            .bind("p-fk")
            .bind(&author)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(r.is_ok(), "comments 外键必须仍可用");

    close_pool(&pool).await;
    cleanup(&dir);
}
