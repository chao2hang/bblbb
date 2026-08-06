//! M04-SCHEMA-08：内容数据模型并发与边界测试（SQLite）。
//!
//! - 并发楼层：同主题同楼层并发插入恰好一个成功（UNIQUE(post_id, floor)）；
//! - slug 冲突：同板块同 slug 并发恰好一个成功；
//! - 非法 parent：parent/quote 必须属于同一主题（服务层校验 + DB 外键）；
//! - 孤儿附件引用：帖子删除级联清理 post_attachments/post_tags（不留孤儿）；
//! - 软删恢复：deleted_at 置位后可恢复（行保留 + 清除 deleted_at 即恢复）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use bblbb_backend::content::model::{
    Comment, CommentStatus, Post, PostRevision, PostStatus, PostType,
};
use bblbb_backend::content::repository::{
    get_comment, insert_comment, insert_post, insert_post_revision,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-ccur-{}", uuid::Uuid::now_v7()));
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

fn comment(id: &str, post: &str, author: &str, floor: i64) -> Comment {
    let now = now_millis();
    Comment {
        id: id.to_string(),
        post_id: post.to_string(),
        author_id: author.to_string(),
        parent_id: None,
        quoted_comment_id: None,
        floor,
        status: CommentStatus::Published,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

/// 并发楼层：同主题同楼层并发插入恰好一个成功（唯一约束兜底）。
#[tokio::test]
async fn concurrent_floor_allocation_only_one_wins() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general, None).await;

    let pool = Arc::new(pool);
    let c1 = comment("c-a", "p1", &author, 7);
    let c2 = comment("c-b", "p1", &author, 7);
    let (r1, r2) = tokio::join!(
        async { insert_comment(pool.as_ref(), &c1).await },
        async { insert_comment(pool.as_ref(), &c2).await },
    );
    let ok_count = [r1, r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 1, "同主题同楼层并发必须恰好一个成功");

    let rows: i64 = match &*pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM comments WHERE post_id = 'p1' AND floor = 7")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(rows, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// slug 冲突：同板块同 slug 并发插入恰好一个成功。
#[tokio::test]
async fn concurrent_same_slug_only_one_wins() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    let now = now_millis();

    let pool = Arc::new(pool);
    let mk = |id: &str| Post {
        id: id.to_string(),
        board_id: general.clone(),
        author_id: author.clone(),
        post_type: PostType::Article,
        slug: Some("race-slug".to_string()),
        title: id.to_string(),
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
    };
    let (r1, r2) = tokio::join!(
        async { insert_post(pool.as_ref(), &mk("p-a")).await },
        async { insert_post(pool.as_ref(), &mk("p-b")).await },
    );
    let ok_count = [r1, r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 1, "同板块同 slug 并发必须恰好一个成功");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 非法 parent：跨主题引用必须被服务层校验拒绝；合法同主题引用通过。
#[tokio::test]
async fn cross_post_parent_is_rejected() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general, None).await;
    seed_post(&pool, "p2", &author, &general, None).await;

    insert_comment(&pool, &comment("parent-c", "p1", &author, 1))
        .await
        .unwrap();

    // 同主题引用 → 通过
    let mut ok_comment = comment("child-ok", "p1", &author, 2);
    ok_comment.parent_id = Some("parent-c".to_string());
    let parent_post = get_comment(&pool, "parent-c")
        .await
        .unwrap()
        .unwrap()
        .post_id;
    assert!(ok_comment
        .validate_quote_scope(Some(&parent_post), None)
        .is_ok());

    // 跨主题引用（parent 在 p1，child 在 p2）→ 拒绝
    let mut bad_comment = comment("child-bad", "p2", &author, 1);
    bad_comment.parent_id = Some("parent-c".to_string());
    let parent_post = get_comment(&pool, "parent-c")
        .await
        .unwrap()
        .unwrap()
        .post_id;
    assert_eq!(
        bad_comment.validate_quote_scope(Some(&parent_post), None),
        Err("parent comment must belong to the same post"),
        "跨主题 parent 必须拒绝"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 孤儿附件引用：帖子删除级联清理 post_attachments/post_tags（不留孤儿）；
/// 软删恢复：deleted_at 置位后清除即恢复（行保留）。
#[tokio::test]
async fn no_orphan_references_and_soft_delete_restore() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general, None).await;
    let now = now_millis();

    // 关联附件与修订
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO post_attachments (id, post_id, attachment_id, kind, position, created_at)
                 VALUES ('pa1', 'p1', 'att-1', 'gallery', 0, ?)",
            )
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO post_tags (post_id, tag_id, created_at)
                 SELECT 'p1', id, ? FROM tags WHERE name = 'rust'",
            )
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    insert_post_revision(
        &pool,
        &PostRevision {
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
        },
    )
    .await
    .unwrap();

    // 删除帖子 → 引用行级联清除（不留孤儿）
    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM posts WHERE id = 'p1'")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let orphans: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM post_attachments WHERE post_id = 'p1'
                 UNION ALL SELECT COUNT(*) FROM post_tags WHERE post_id = 'p1'
                 UNION ALL SELECT COUNT(*) FROM post_revisions WHERE post_id = 'p1'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(orphans, 0, "帖子删除后不得留下孤儿引用");

    // 软删恢复：评论软删（deleted_at 置位）→ 清除 deleted_at 即恢复
    seed_post(&pool, "p2", &author, &general, None).await;
    insert_comment(&pool, &comment("c1", "p2", &author, 1))
        .await
        .unwrap();
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE comments SET deleted_at = ? WHERE id = 'c1'")
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let soft = get_comment(&pool, "c1").await.unwrap().unwrap();
    assert!(soft.deleted_at.is_some(), "软删必须置 deleted_at");
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE comments SET deleted_at = NULL WHERE id = 'c1'")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let restored = get_comment(&pool, "c1").await.unwrap().unwrap();
    assert!(restored.deleted_at.is_none(), "清除 deleted_at 即恢复");

    close_pool(&pool).await;
    cleanup(&dir);
}
