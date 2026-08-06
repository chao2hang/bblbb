//! M04-POSTS-06：即时发布 + scheduled 发布 Job（SQLite + 路由/服务层）。
//!
//! 覆盖：即时发布 201（posts/post_contents/post_revisions 事务写、板块计数、
//! 搜索索引 Job 入队）；非法板块 409；scheduled 发布落 draft 态（不计数、不入
//! 索引）；到期 Job 入队 → 执行（再次预检）→ published + 计数 + 索引；执行时
//! 账号被封 → 发布失败。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::content::posts::publish_job::{enqueue_due_publish_jobs, handle_publish_job};
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
    let dir = std::env::temp_dir().join(format!("bblbb-ppb-{}", uuid::Uuid::now_v7()));
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

/// 插入已验证作者（email_verified_at 早于 24h 冷静期）；返回 user_id。
async fn insert_author(pool: &DatabasePool, tag: &str, status: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', ?, 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(status)
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

fn post_body(board_id: &str, scheduled_at: Option<i64>) -> Value {
    json!({
        "type": "article",
        "title": "发布测试帖",
        "markdown": "**正文** <script>alert(1)</script>",
        "board_id": board_id,
        "visibility_level": 1,
        "access_policy": "public",
        "scheduled_at": scheduled_at,
        "client_request_id": "post-req-id-0001",
    })
}

async fn count(pool: &DatabasePool, sql: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn claimed(job_id: &str, payload: Value) -> ClaimedJob {
    ClaimedJob {
        id: job_id.to_string(),
        queue: "default".to_string(),
        kind: "content.publish".to_string(),
        payload,
        payload_version: 1,
        attempts: 1,
        max_attempts: 5,
        locked_until: now_millis() + 60_000,
    }
}

#[tokio::test]
async fn immediate_publish_writes_all_artifacts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice", "active").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;

    let (status, body) = authed_post(
        &app,
        "/api/v1/posts",
        &session,
        &csrf,
        post_body(BOARD_ID, None),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "即时发布必须 201: {body}");
    let post_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["status"], "published");

    // 事务写：posts + post_contents + post_revisions
    let posts = count(
        &pool,
        &format!("SELECT COUNT(*) FROM posts WHERE id = '{post_id}'"),
    )
    .await;
    assert_eq!(posts, 1, "posts 行存在");
    let pc = count(
        &pool,
        &format!("SELECT COUNT(*) FROM post_contents WHERE post_id = '{post_id}'"),
    )
    .await;
    assert_eq!(pc, 1, "post_contents 行存在");
    let rev = count(
        &pool,
        &format!("SELECT COUNT(*) FROM post_revisions WHERE post_id = '{post_id}'"),
    )
    .await;
    assert_eq!(rev, 1, "初始修订存在");
    // 正文渲染 + 清洗
    let html: String = match &pool {
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
        html.contains("<strong>正文</strong>"),
        "Markdown 渲染: {html}"
    );
    assert!(!html.contains("script"), "原始 HTML 清洗: {html}");
    // 板块计数
    let pcnt: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT post_count FROM boards WHERE id = ?")
            .bind(BOARD_ID)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(pcnt, 1, "即时发布必须板块计数 +1");
    // 搜索索引 Job 入队
    let jobs = count(
        &pool,
        "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index'",
    )
    .await;
    assert_eq!(jobs, 1, "即时发布必须入队搜索索引 Job");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn immediate_publish_rejects_unknown_board() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "bob", "active").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;

    let (status, body) = authed_post(
        &app,
        "/api/v1/posts",
        &session,
        &csrf,
        post_body("01911fd5-f999-7561-a2a5-3dd6434157f0", None),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "未知板块必须被预检阻断 409: {body}"
    );
    // 未产生任何行
    assert_eq!(count(&pool, "SELECT COUNT(*) FROM posts").await, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn scheduled_publish_via_job() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "carol", "active").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;

    // 定时发布：未来时间 → 落 draft 态，不计数、不入索引
    let future = now_millis() + 3600 * 1000;
    let (status, body) = authed_post(
        &app,
        "/api/v1/posts",
        &session,
        &csrf,
        post_body(BOARD_ID, Some(future)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "定时发布必须 201: {body}");
    let post_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["status"], "draft", "未到期帖子为 draft 态");
    assert_eq!(
        count(&pool, "SELECT COUNT(*) FROM jobs").await,
        0,
        "未到期不入队索引"
    );
    let pcnt: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT post_count FROM boards WHERE id = ?")
            .bind(BOARD_ID)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(pcnt, 0, "未到期不计数");

    // 未到期：无 Job
    assert_eq!(enqueue_due_publish_jobs(&pool, 10).await.unwrap(), 0);

    // 把 scheduled_at 改到过去 → 到期 → 入队 + 执行
    let past = now_millis() - 1000;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET scheduled_at = ? WHERE id = ?")
                .bind(past)
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    assert_eq!(
        enqueue_due_publish_jobs(&pool, 10).await.unwrap(),
        1,
        "到期必须入队"
    );

    let outcome = handle_publish_job(
        &pool,
        &claimed("j1", json!({ "source": "post", "id": post_id })),
    )
    .await;
    assert!(
        matches!(outcome, JobOutcome::Succeeded),
        "scheduled 发布必须成功: {outcome:?}"
    );

    // 发布后：published + 计数 + 索引
    let status_now: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(status_now, "published", "Job 执行后必须 published");
    let published_at: Option<i64> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT published_at FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(published_at.is_some(), "published_at 必须写入");
    let pcnt: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT post_count FROM boards WHERE id = ?")
            .bind(BOARD_ID)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(pcnt, 1, "发布后板块计数 +1");
    let jobs = count(
        &pool,
        "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index'",
    )
    .await;
    assert_eq!(jobs, 1, "发布后入队搜索索引");

    // 幂等：重复执行同 payload → 非 scheduled → 幂等成功
    let outcome2 = handle_publish_job(
        &pool,
        &claimed("j2", json!({ "source": "post", "id": post_id })),
    )
    .await;
    assert!(
        matches!(outcome2, JobOutcome::Succeeded),
        "已发布重放必须幂等成功"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn scheduled_publish_reruns_preflight_at_execution() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "dave", "active").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;

    let future = now_millis() + 3600 * 1000;
    let (status, body) = authed_post(
        &app,
        "/api/v1/posts",
        &session,
        &csrf,
        post_body(BOARD_ID, Some(future)),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let post_id = body["id"].as_str().unwrap().to_string();

    // 到期前作者被封禁
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET status = 'banned' WHERE id = ?")
                .bind(&author)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET scheduled_at = ? WHERE id = ?")
                .bind(now_millis() - 1000)
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 执行时再次授权 → 阻断 → Job 瞬时失败（不发布）
    let outcome = handle_publish_job(
        &pool,
        &claimed("j1", json!({ "source": "post", "id": post_id })),
    )
    .await;
    assert!(
        matches!(outcome, JobOutcome::Failed { .. }),
        "执行时账号被封必须失败: {outcome:?}"
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
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index'"
        )
        .await,
        0
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn handle_publish_job_rejects_invalid_payload() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    for payload in [
        json!({}),
        json!({ "source": "draft", "id": "x" }),
        json!({ "source": "post" }),
    ] {
        let outcome = handle_publish_job(&pool, &claimed("j-bad", payload)).await;
        assert!(
            matches!(outcome, JobOutcome::Failed { .. }),
            "无效 payload 必须失败"
        );
    }
    close_pool(&pool).await;
    cleanup(&dir);
}
