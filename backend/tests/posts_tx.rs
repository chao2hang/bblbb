//! M04-POSTS-10（P0）：发布事务回滚 / 重复请求 / 定时执行降级 / 旧 revision
//! 并发覆盖（SQLite）。
//!
//! 覆盖：
//! 1. 重复请求：createPost 同 client_request_id+摘要重放返回原帖，只落一行；
//! 2. 事务回滚：编辑中途修订插入失败（旧 revision 已存在）→ 整体回滚，
//!    帖子标题/版本/post_contents 均不变（无部分状态）；
//! 3. 旧 revision 并发覆盖：同 (post_id, version) 修订插入冲突 → 稳定 409
//!    version_conflict（唯一约束兜底）；
//! 4. 定时执行降级：执行时作者等级下调 → 预检阻断 → Job 失败、保持 draft。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::publish_job::{handle_publish_job, PUBLISH_JOB_KIND};
use bblbb_backend::content::posts::service::publish_new_post;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::worker::ClaimedJob;
use bblbb_backend::jobs::worker_loop::JobOutcome;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-ptx-{}", uuid::Uuid::now_v7()));
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

async fn insert_author(pool: &DatabasePool, tag: &str, level: i64) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(level)
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

async fn publish(pool: &DatabasePool, author: &str, title: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: title.to_string(),
            markdown: format!("正文 {title}"),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("tx-{}-{}", title, uuid::Uuid::now_v7().simple()),
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

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
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

async fn authed_post(
    app: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
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
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

async fn authed_patch(
    app: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    if_match: &str,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .header("if-match", if_match)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn duplicate_create_request_returns_same_post() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "dup", 5).await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;

    let body = json!({
        "type": "article",
        "title": "重复请求帖",
        "markdown": "正文",
        "board_id": BOARD_ID,
        "visibility_level": 1,
        "access_policy": "public",
        "client_request_id": "post-req-dup-0001",
    });

    let (s1, r1) = authed_post(&app, "/api/v1/posts", &session, &csrf, body.clone()).await;
    assert_eq!(s1, StatusCode::CREATED);
    let (s2, r2) = authed_post(&app, "/api/v1/posts", &session, &csrf, body).await;
    assert_eq!(s2, StatusCode::CREATED, "同 key+摘要重放必须成功: {r2}");
    assert_eq!(r1["id"], r2["id"], "重放必须返回同一帖子");

    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM posts")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1, "重复请求不得产生重复帖子");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn edit_mid_tx_conflict_rolls_back_fully() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "rb", 5).await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;
    let post_id = publish(&pool, &author, "回滚帖").await;

    // 预置一条 (post_id, version=2) 修订 → 模拟旧 revision 已被并发覆盖
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO post_revisions (id, post_id, editor_id, body_markdown, body_html, restricted_markdown, restricted_html, renderer_version, change_reason, version, created_at)
                 VALUES (?, ?, ?, 'x', 'x', NULL, NULL, 'markdown-v1+ammonia-v1', 'concurrent', 2, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&post_id)
            .bind(&author)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 用 If-Match=1 编辑 → 修订插入 (version=2) 唯一冲突 → 事务整体回滚 → 409
    let (status, body) = authed_patch(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        &session,
        &csrf,
        "1",
        json!({ "title": "不应生效", "markdown": "不应生效的正文" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "旧 revision 冲突必须 409: {body}"
    );
    assert_eq!(body["code"], "version_conflict");

    // 回滚验证：无任何部分状态（标题/版本/post_contents/修订数均不变）
    let (title, version, updated_at): (String, i64, i64) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT title, version, updated_at FROM posts WHERE id = ?")
                .bind(&post_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(title, "回滚帖", "失败编辑不得改写标题");
    assert_eq!(version, 1, "失败编辑不得递增版本");
    let body_html: String = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT body_html FROM post_contents WHERE post_id = ?")
                .bind(&post_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        body_html.contains("正文 回滚帖"),
        "失败编辑不得改写 post_contents: {body_html}"
    );
    let revs: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM post_revisions WHERE post_id = ?")
                .bind(&post_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(revs, 2, "修订数不变（初始 + 预置并发）");
    let _ = updated_at;

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn scheduled_execution_board_degradation_blocks_publish() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "degrade", 5).await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;

    // 定时发布（未来时间）
    let future = now_millis() + 3600 * 1000;
    let body = json!({
        "type": "article",
        "title": "降级帖",
        "markdown": "正文",
        "board_id": BOARD_ID,
        "visibility_level": 1,
        "access_policy": "public",
        "scheduled_at": future,
        "client_request_id": "post-req-deg-0001",
    });
    let (status, created) = authed_post(&app, "/api/v1/posts", &session, &csrf, body).await;
    assert_eq!(status, StatusCode::CREATED, "定时创建必须 201: {created}");
    let post_id = created["id"].as_str().unwrap().to_string();

    // 到期前板块被降级为只读 → 执行时预检重读板块规则阻断
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE boards SET posting_mode = 'readonly' WHERE id = ?")
                .bind(BOARD_ID)
                .execute(p)
                .await
                .unwrap();
            sqlx::query("UPDATE posts SET scheduled_at = ? WHERE id = ?")
                .bind(now_millis() - 1000)
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 执行时预检：板块只读 → 阻断，Job 失败、保持 draft
    let outcome = handle_publish_job(
        &pool,
        &ClaimedJob {
            id: "j1".into(),
            queue: "default".into(),
            kind: PUBLISH_JOB_KIND.into(),
            payload: json!({ "source": "post", "id": post_id.clone() }),
            payload_version: 1,
            attempts: 1,
            max_attempts: 5,
            locked_until: now_millis() + 60_000,
        },
    )
    .await;
    assert!(
        matches!(outcome, JobOutcome::Failed { .. }),
        "板块降级必须阻断定时发布: {outcome:?}"
    );
    let status_now: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(status_now, "draft", "预检失败不得发布");

    close_pool(&pool).await;
    cleanup(&dir);
}
