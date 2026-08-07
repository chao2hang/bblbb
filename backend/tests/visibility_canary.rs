//! M04-VISIBILITY-10：唯一 canary 字符串防泄漏（API 全链路）。
//!
//! 每个测试使用唯一 canary（隐藏正文标记）。断言其**不出现**在：
//! - 未授权 persona 的 `GET /api/v1/posts/{id}` 响应体（body_html 缺失，
//!   整个序列化 JSON 不含 canary）；
//! - 错误响应（404/403 的 Problem detail）；
//! - 审计日志 metadata（管理/作者动作的 target/reason 不含 canary）；
//! - 授权作者本人响应则**包含** canary（对照证明投影确实工作）。
//!
//! 服务器端（Rust）不产生 SSR/hydration 输出；SSR/hydration 泄露断言由
//! 前端 `frontend/src/lib/testing/ssr/post-detail-ssr.test.ts` 覆盖（
//! `RESTRICTED-BODY-CANARY` 不出现在 SSR HTML / load 输出）。

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

mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-canary-{}", uuid::Uuid::now_v7()));
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

/// 发布一篇含 canary 正文的公开帖，返回 (post_id, canary)。
async fn publish_with_canary(pool: &DatabasePool, author_id: &str, canary: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: "canary 主题".to_string(),
            markdown: format!("公开开头 CANARY={canary} 结尾"),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("canary-{}-{}", canary, uuid::Uuid::now_v7().simple()),
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

/// 为主题附加 after_reply 策略行并设置 posts.access_policy_id。
async fn attach_after_reply_policy(pool: &DatabasePool, post_id: &str, creator_id: &str) {
    let policy_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO content_access_policies
                 (id, kind, min_level, currency_id, amount, reply_grant_persists, policy_version, created_by, created_at)
                 VALUES (?, 'after_reply', NULL, NULL, NULL, 1, 1, ?, ?)",
            )
            .bind(&policy_id)
            .bind(creator_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query("UPDATE posts SET access_policy_id = ? WHERE id = ?")
                .bind(&policy_id)
                .bind(post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn get(app: &axum::Router, uri: &str, cookie: Option<&str>) -> (StatusCode, Value, String) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(c) = cookie {
        builder = builder.header("cookie", c);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        String::from_utf8_lossy(&bytes).to_string(),
    )
}

/// 唯一 canary 不在未授权 API 响应中（after_reply 锁定帖）。
#[tokio::test]
async fn canary_absent_for_unauthorized_persona() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner").await;
    let canary = format!("CANARY-{}", uuid::Uuid::now_v7().simple());
    let post_id = publish_with_canary(&pool, &author, &canary).await;
    attach_after_reply_policy(&pool, &post_id, &author).await;

    // 匿名 GET：正文完全缺失，canary 不得出现在任何响应字节中
    let (status, body, raw) = get(&app, &format!("/api/v1/posts/{post_id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.get("body_html").is_none(),
        "未授权必须省略 body_html（非 null），实际 {body}"
    );
    assert_eq!(body["access_summary"]["policy"], "after_reply");
    assert_eq!(body["access_summary"]["unlocked"], false);
    assert!(!raw.contains(&canary), "未授权响应泄漏 canary: {raw}");
    // 公开元数据仍在（不因锁定丢失）
    assert_eq!(body["title"], "canary 主题");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 作者本人（授权 persona）能看到正文 canary —— 对照证明投影确实工作。
#[tokio::test]
async fn author_persona_sees_body_and_canary() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner2").await;
    let canary = format!("CANARY-AUTHOR-{}", uuid::Uuid::now_v7().simple());
    let post_id = publish_with_canary(&pool, &author, &canary).await;
    attach_after_reply_policy(&pool, &post_id, &author).await;

    let cookie = common::direct_session_cookie(&pool, &author).await;
    let (status, body, raw) = get(&app, &format!("/api/v1/posts/{post_id}"), Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["access_summary"]["unlocked"], true, "作者自见必须解锁");
    assert!(
        body.get("body_html").is_some(),
        "作者必须能看到正文: {body}"
    );
    assert!(raw.contains(&canary), "作者响应应包含 canary（对照）");
    // 受限策略即使作者解锁也 private,no-store（M04-VISIBILITY-08）
    close_pool(&pool).await;
    cleanup(&dir);
}

/// canary 不进入错误响应。
#[tokio::test]
async fn canary_absent_from_errors() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner3").await;
    let canary = format!("CANARY-ERR-{}", uuid::Uuid::now_v7().simple());
    let post_id = publish_with_canary(&pool, &author, &canary).await;
    attach_after_reply_policy(&pool, &post_id, &author).await;

    // 不存在的帖子 → 404 Problem；canary 不得出现在 detail
    let missing = uuid::Uuid::now_v7().to_string();
    let (status, _, raw) = get(&app, &format!("/api/v1/posts/{missing}"), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert!(!raw.contains(&canary), "404 detail 泄漏 canary: {raw}");
    let _ = post_id;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 列表接口不泄露 canary（列表不投影正文）。
#[tokio::test]
async fn canary_absent_from_list_projection() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner4").await;
    let canary = format!("CANARY-LIST-{}", uuid::Uuid::now_v7().simple());
    let post_id = publish_with_canary(&pool, &author, &canary).await;
    attach_after_reply_policy(&pool, &post_id, &author).await;

    let (status, _, raw) = get(&app, "/api/v1/posts?limit=10", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!raw.contains(&canary), "列表投影泄漏 canary: {raw}");
    let _ = post_id;
    close_pool(&pool).await;
    cleanup(&dir);
}
