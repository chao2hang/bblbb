//! M04-COMMENTS-03/07：并发楼层分配（SQLite + 路由层，tokio::join!）。
//!
//! `tokio::join!` 8 个并发创建请求 → 断言楼层为 1..=8（唯一、连续、无重复）、
//! `posts.reply_count == 8`、列表返回 8 条。楼层分配在写事务内完成
//! （MAX(floor)+1），`UNIQUE(post_id, floor)`（0038）兜底并发冲突。

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
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'
const CONCURRENCY: usize = 8;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-cmc-{}", uuid::Uuid::now_v7()));
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

async fn publish(pool: &DatabasePool, author_id: &str, title: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "discussion".to_string(),
            title: title.to_string(),
            markdown: format!("正文 {title}"),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("cmc-{}-{}", title, uuid::Uuid::now_v7().simple()),
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

async fn session_csrf(app: &axum::Router, session: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
async fn concurrent_creates_allocate_unique_contiguous_floors() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let author = insert_author(&pool, "racer").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;
    let post_id = publish(&pool, &author, "并发楼层").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    // 构造 CONCURRENCY 个并发请求（各自独立 client_request_id；JoinSet 并发执行）
    let mut set = tokio::task::JoinSet::new();
    for i in 0..CONCURRENCY {
        let app = app.clone();
        let uri = uri.clone();
        let session = session.clone();
        let csrf = csrf.clone();
        let body = json!({
            "markdown": format!("并发回复 {i}"),
            "client_request_id": format!("cmc-req-{i}-{}", uuid::Uuid::now_v7().simple()),
        });
        set.spawn(async move {
            let resp = app
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(&uri)
                        .header("content-type", "application/json")
                        .header("x-csrf-token", csrf)
                        .header("cookie", session)
                        .body(Body::from(body.to_string()))
                        .unwrap(),
                )
                .await
                .unwrap();
            let status = resp.status();
            let bytes = resp.into_body().collect().await.unwrap().to_bytes();
            let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
            (status, value)
        });
    }

    let mut results = Vec::new();
    while let Some(res) = set.join_next().await {
        results.push(res.expect("并发任务必须正常结束"));
    }
    assert_eq!(results.len(), CONCURRENCY);
    for (status, body) in &results {
        assert_eq!(*status, StatusCode::CREATED, "并发创建必须全部 201: {body}");
    }

    // 楼层唯一 + 连续 + 无重复
    let mut floors: Vec<i64> = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT floor FROM comments WHERE post_id = ? ORDER BY floor ASC")
                .bind(&post_id)
                .fetch_all(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    floors.sort_unstable();
    let expected: Vec<i64> = (1..=CONCURRENCY as i64).collect();
    assert_eq!(
        floors, expected,
        "并发下楼层必须为 1..={CONCURRENCY}，唯一且连续"
    );

    // reply_count == 8
    let reply_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT reply_count FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(
        reply_count, CONCURRENCY as i64,
        "reply_count 必须精确等于并发创建数"
    );

    // 列表返回 8 条（楼层升序）
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(&uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    let items = body["items"].as_array().unwrap();
    assert_eq!(
        items.len(),
        CONCURRENCY,
        "列表必须返回全部 {CONCURRENCY} 条"
    );
    let list_floors: Vec<i64> = items.iter().map(|c| c["floor"].as_i64().unwrap()).collect();
    assert_eq!(list_floors, expected, "列表楼层必须按升序连续");

    close_pool(&pool).await;
    cleanup(&dir);
}
