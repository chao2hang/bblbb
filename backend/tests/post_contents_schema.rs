//! M04-SCHEMA-02：post_contents / post_revisions 结构与仓储契约（SQLite）。
//!
//! - post_contents：帖子当前正文（1:1 with posts）——Markdown、清洗 HTML、
//!   renderer version、安全摘要、受限正文可空；upsert 覆盖不产生重复行；
//! - post_revisions：不可变修订快照，`UNIQUE(post_id, version)`，按 version
//!   升序读取；编辑历史随 post 级联删除。

use std::path::{Path, PathBuf};

use bblbb_backend::content::model::{Post, PostContent, PostRevision, PostStatus, PostType};
use bblbb_backend::content::repository::{
    get_post, insert_post, insert_post_revision, list_post_revisions, load_post_content,
    save_post_content,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-pcnt-{}", uuid::Uuid::now_v7()));
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
    let post = Post {
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
    };
    insert_post(pool, &post).await.unwrap();
}

/// post_contents 保存/读取往返（Markdown、清洗 HTML、renderer version、摘要、
/// 受限正文）。
#[tokio::test]
async fn post_contents_save_and_load_roundtrip() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    let content = PostContent {
        post_id: "p1".to_string(),
        body_markdown: "# 你好\n\n正文".to_string(),
        body_html: "<h1>你好</h1>\n<p>正文</p>".to_string(),
        restricted_markdown: Some("仅付费可见".to_string()),
        restricted_html: Some("<p>仅付费可见</p>".to_string()),
        renderer_version: "markdown-v1".to_string(),
        excerpt: "你好，正文摘要".to_string(),
        updated_at: now_millis(),
    };
    save_post_content(&pool, &content).await.unwrap();

    let got = load_post_content(&pool, "p1")
        .await
        .unwrap()
        .expect("必须读到");
    assert_eq!(got.body_markdown, "# 你好\n\n正文");
    assert_eq!(got.body_html, "<h1>你好</h1>\n<p>正文</p>");
    assert_eq!(got.renderer_version, "markdown-v1");
    assert_eq!(got.excerpt, "你好，正文摘要");
    assert_eq!(got.restricted_markdown.as_deref(), Some("仅付费可见"));
    assert_eq!(got.restricted_html.as_deref(), Some("<p>仅付费可见</p>"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// post_contents 与 posts 1:1：重复保存覆盖而非新增。
#[tokio::test]
async fn post_contents_upsert_is_single_row() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    let v1 = PostContent {
        post_id: "p1".to_string(),
        body_markdown: "v1".to_string(),
        body_html: "<p>v1</p>".to_string(),
        restricted_markdown: None,
        restricted_html: None,
        renderer_version: "markdown-v1".to_string(),
        excerpt: "v1".to_string(),
        updated_at: now_millis(),
    };
    save_post_content(&pool, &v1).await.unwrap();
    let mut v2 = v1.clone();
    v2.body_markdown = "v2".to_string();
    v2.body_html = "<p>v2</p>".to_string();
    v2.excerpt = "v2".to_string();
    save_post_content(&pool, &v2).await.unwrap();

    let count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM post_contents WHERE post_id = 'p1'")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1, "post_contents 必须与 post 1:1");
    let got = load_post_content(&pool, "p1").await.unwrap().unwrap();
    assert_eq!(got.body_markdown, "v2");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 修订快照：不可变追加、按 version 升序、同 (post_id, version) 冲突。
#[tokio::test]
async fn post_revisions_immutable_and_ordered() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    let now = now_millis();
    let r1 = PostRevision {
        id: "rev1".to_string(),
        post_id: "p1".to_string(),
        editor_id: author.clone(),
        body_markdown: "第一版".to_string(),
        body_html: "<p>第一版</p>".to_string(),
        restricted_markdown: None,
        restricted_html: None,
        renderer_version: "markdown-v1".to_string(),
        change_reason: Some("创建".to_string()),
        version: 1,
        created_at: now - 1000,
    };
    let r2 = PostRevision {
        id: "rev2".to_string(),
        post_id: "p1".to_string(),
        editor_id: author.clone(),
        body_markdown: "第二版".to_string(),
        body_html: "<p>第二版</p>".to_string(),
        restricted_markdown: Some("受限".to_string()),
        restricted_html: Some("<p>受限</p>".to_string()),
        renderer_version: "markdown-v1".to_string(),
        change_reason: Some("补充".to_string()),
        version: 2,
        created_at: now,
    };
    insert_post_revision(&pool, &r1).await.unwrap();
    insert_post_revision(&pool, &r2).await.unwrap();

    // 重复 (post_id, version) → 唯一冲突（不可覆盖）
    let dup = PostRevision {
        id: "rev3".to_string(),
        ..r1.clone()
    };
    assert!(
        insert_post_revision(&pool, &dup).await.is_err(),
        "同版修订必须唯一"
    );

    // 按 version 升序读取
    let list = list_post_revisions(&pool, "p1").await.unwrap();
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].version, 1);
    assert_eq!(list[1].version, 2);
    assert_eq!(list[0].body_markdown, "第一版");
    assert_eq!(list[1].change_reason.as_deref(), Some("补充"));
    assert_eq!(list[1].restricted_markdown.as_deref(), Some("受限"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 删除帖子 → post_contents 与 post_revisions 级联清除。
#[tokio::test]
async fn cascade_delete_removes_contents_and_revisions() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &author, &general).await;

    save_post_content(
        &pool,
        &PostContent {
            post_id: "p1".to_string(),
            body_markdown: "正文".to_string(),
            body_html: "<p>正文</p>".to_string(),
            restricted_markdown: None,
            restricted_html: None,
            renderer_version: "markdown-v1".to_string(),
            excerpt: "摘要".to_string(),
            updated_at: now_millis(),
        },
    )
    .await
    .unwrap();
    insert_post_revision(
        &pool,
        &PostRevision {
            id: "rev1".to_string(),
            post_id: "p1".to_string(),
            editor_id: author.clone(),
            body_markdown: "正文".to_string(),
            body_html: "<p>正文</p>".to_string(),
            restricted_markdown: None,
            restricted_html: None,
            renderer_version: "markdown-v1".to_string(),
            change_reason: None,
            version: 1,
            created_at: now_millis(),
        },
    )
    .await
    .unwrap();

    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM posts WHERE id = 'p1'")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    assert!(load_post_content(&pool, "p1").await.unwrap().is_none());
    assert!(list_post_revisions(&pool, "p1").await.unwrap().is_empty());
    assert!(get_post(&pool, "p1").await.unwrap().is_none());

    close_pool(&pool).await;
    cleanup(&dir);
}
