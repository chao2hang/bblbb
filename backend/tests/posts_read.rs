//! M04-POSTS-07：详情/列表/板块列表/作者列表——cursor/ETag/Cache-Control（SQLite）。
//!
//! 覆盖：列表 keyset 分页（has_more/next_cursor）；Cache-Control+ETag 头；
//! 详情投影（body_html/author/access_summary）；404；板块列表过滤；作者过滤。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::service::publish_new_post;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Either;
use tower::ServiceExt;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-prd-{}", uuid::Uuid::now_v7()));
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

/// 直接经服务层发布一篇帖子，返回 post_id。
async fn publish(pool: &DatabasePool, author_id: &str, title: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: title.to_string(),
            markdown: format!("正文 {title}"),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("read-{}-{}", title, uuid::Uuid::now_v7().simple()),
        },
        5,
        now_millis(),
    )
    .unwrap();
    let published = publish_new_post(pool, &cmd, author_id, now_millis())
        .await
        .unwrap();
    published.post.id
}

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Value, axum::http::HeaderMap) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    (status, value, headers)
}

#[tokio::test]
async fn list_posts_cursor_pagination_with_headers() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice").await;
    // 三帖（created_at 递增，latest 排序 → p3 最新）
    let p1 = publish(&pool, &author, "帖一").await;
    let p2 = publish(&pool, &author, "帖二").await;
    let p3 = publish(&pool, &author, "帖三").await;

    let (status, body, headers) = get(&app, "/api/v1/posts?limit=2").await;
    assert_eq!(status, StatusCode::OK, "列表必须 200");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "第一页两条");
    assert_eq!(items[0]["id"], Value::String(p3.clone()), "最新帖在最前");
    assert_eq!(items[1]["id"], Value::String(p2.clone()));
    assert_eq!(body["page"]["has_more"], true, "还有更多");
    let cursor = body["page"]["next_cursor"].as_str().unwrap().to_string();
    assert!(!cursor.is_empty(), "has_more 必须有 next_cursor");
    // 响应头
    assert!(
        headers.get("cache-control").is_some(),
        "必须带 Cache-Control"
    );
    assert!(headers.get("etag").is_some(), "必须带 ETag");

    // 第二页
    let (_, body2, _) = get(&app, &format!("/api/v1/posts?limit=2&after={cursor}")).await;
    let items2 = body2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 1, "第二页一条");
    assert_eq!(items2[0]["id"], Value::String(p1.clone()));
    assert_eq!(body2["page"]["has_more"], false, "末页无更多");
    assert!(body2["page"]["next_cursor"].is_null(), "末页无 next_cursor");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn get_post_detail_returns_projection() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "bob").await;
    let post_id = publish(&pool, &author, "详情帖").await;

    let (status, body, headers) = get(&app, &format!("/api/v1/posts/{post_id}")).await;
    assert_eq!(status, StatusCode::OK, "详情必须 200");
    assert_eq!(body["id"], Value::String(post_id.clone()));
    assert_eq!(body["title"], "详情帖");
    assert_eq!(body["status"], "published");
    assert!(
        body["author"]["id"] == Value::String(author.clone()),
        "作者投影"
    );
    assert!(
        body["body_html"].as_str().unwrap().contains("正文 详情帖"),
        "body_html 必须可见: {}",
        body["body_html"]
    );
    assert_eq!(body["access_summary"]["policy"], "public");
    assert!(
        headers.get("cache-control").is_some(),
        "必须带 Cache-Control"
    );
    assert!(headers.get("etag").is_some(), "必须带 ETag");

    // 不存在 → 404
    let (status, _, _) = get(&app, &format!("/api/v1/posts/{}", uuid::Uuid::now_v7())).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在必须 404");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn list_board_posts_filters_by_board() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "carol").await;
    publish(&pool, &author, "板块帖").await;

    let (status, body, _) = get(&app, "/api/v1/boards/general/posts").await;
    assert_eq!(status, StatusCode::OK, "板块列表必须 200");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "板块帖一条");
    assert_eq!(items[0]["title"], "板块帖");

    // 未知板块 → 404
    let (status, _, _) = get(&app, "/api/v1/boards/no-such-board/posts").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "未知板块必须 404");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn list_posts_author_filter() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let alice = insert_author(&pool, "alice2").await;
    let bob = insert_author(&pool, "bob2").await;
    publish(&pool, &alice, "A1").await;
    publish(&pool, &alice, "A2").await;
    publish(&pool, &bob, "B1").await;

    let (_, body, _) = get(&app, &format!("/api/v1/posts?author_id={alice}")).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "作者列表只含该作者帖子");
    for it in items {
        assert_eq!(
            it["author"]["id"],
            Value::String(alice.clone()),
            "作者过滤生效"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}
