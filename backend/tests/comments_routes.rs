//! M04-COMMENTS-07：评论域集成测试（SQLite + 路由层）。
//!
//! 覆盖：创建（201 投影 + private,no-store；锁帖 409；banned/未验证 403；
//! hidden 主题可回复；parent 同主题+可见性；幂等重放/摘要冲突）；列表
//! （keyset 分页、匿名读、ETag/Cache-Control、软删占位、404）；编辑
//! （作者限时 30 分钟窗口、If-Match/body version 守卫、comment_revisions
//! 快照、超窗 409、版本冲突 409）；删除（作者软删、管理员审计、非版主 403、
//! reply_count 不递减）。

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

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-cmr-{}", uuid::Uuid::now_v7()));
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

/// 插入用户（可指定邮箱验证与账号状态）；返回 user_id。
async fn insert_user(pool: &DatabasePool, tag: &str, verified: bool, status: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, display_name, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', ?, 5, ?, ?, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(status)
            .bind(format!("{tag} display"))
            .bind(if verified { 1 } else { 0 })
            // 验证时间须早于 24h 冷静期（ACCOUNT_COOLDOWN_MS），否则内容写入被 InCooldown 拒
            .bind(if verified {
                Some(now - 25 * 3600 * 1000)
            } else {
                None
            })
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

/// 授予全局角色（管理员/版主用）。
async fn assign_global_role(pool: &DatabasePool, user_id: &str, role: &str) {
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 直接经服务层发布一篇帖子，返回 post_id。
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
            client_request_id: format!("crt-{}-{}", title, uuid::Uuid::now_v7().simple()),
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

/// 已认证会话（verified 用户）→ (user_id, session cookie, csrf token)。
async fn authed_session(app: &axum::Router, pool: &DatabasePool) -> (String, String, String) {
    let user = insert_user(pool, "alice", true, "active").await;
    let session = common::direct_session_cookie(pool, &user).await;
    let csrf = session_csrf(app, &session).await;
    (user, session, csrf)
}

async fn authed_post(
    app: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value, axum::http::HeaderMap) {
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
    read_resp(resp).await
}

async fn authed_patch(
    app: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    read_resp(resp).await
}

/// PATCH with If-Match header（body 无 version 字段）。
async fn authed_patch_ifmatch(
    app: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    version: i64,
    body: Value,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("content-type", "application/json")
                .header("if-match", version.to_string())
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    read_resp(resp).await
}

async fn authed_delete(
    app: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Option<Value>,
) -> (StatusCode, Value, axum::http::HeaderMap) {
    let mut builder = Request::builder().method("DELETE").uri(uri);
    builder = builder
        .header("x-csrf-token", csrf)
        .header("cookie", session);
    let body_bytes = match body {
        Some(v) => {
            builder = builder.header("content-type", "application/json");
            v.to_string()
        }
        None => String::new(),
    };
    let resp = app
        .clone()
        .oneshot(builder.body(Body::from(body_bytes)).unwrap())
        .await
        .unwrap();
    read_resp(resp).await
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
    read_resp(resp).await
}

async fn read_resp(resp: axum::response::Response) -> (StatusCode, Value, axum::http::HeaderMap) {
    let status = resp.status();
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value, headers)
}

fn comment_body(markdown: &str, req_id: &str) -> Value {
    json!({
        "markdown": markdown,
        "client_request_id": req_id,
    })
}

fn problem_code(body: &Value) -> String {
    body["code"].as_str().unwrap_or("").to_string()
}

// ── 创建 ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn create_comment_returns_201_with_full_projection() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "回复主题").await;

    let (status, body, headers) = authed_post(
        &app,
        &format!("/api/v1/posts/{post_id}/comments"),
        &session,
        &csrf,
        comment_body("你好 **世界**", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "创建必须 201: {body}");
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("private, no-store"),
        "创建响应必须 private, no-store"
    );

    let comment_id = body["id"].as_str().unwrap();
    assert_eq!(body["post_id"], Value::String(post_id.clone()));
    assert_eq!(body["floor"], 1, "首条回复楼层 1");
    assert_eq!(body["version"], 1);
    assert_eq!(body["status"], "published");
    assert!(body["parent_id"].is_null());
    assert!(
        body["body_html"].as_str().unwrap().contains("世界"),
        "body_html 必须包含渲染后文本: {}",
        body["body_html"]
    );
    assert!(body["created_at"].is_i64() && body["updated_at"].is_i64());
    // 作者卡投影
    assert_eq!(body["author"]["username"], body["author"]["username"]);
    assert_eq!(body["author"]["display_name"], "alice display");
    assert_eq!(body["author"]["level"], 5);
    let profile = body["author"]["profile_url"].as_str().unwrap();
    assert!(profile.starts_with("/users/"), "profile_url: {profile}");

    // 落库：content 存 Markdown 原文；posts.reply_count=1
    let (content, format): (String, String) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT content, content_format FROM comments WHERE id = ?")
                .bind(comment_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(content, "你好 **世界**");
    assert_eq!(format, "markdown");
    let reply_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT reply_count FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(reply_count, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_comment_reply_with_parent_gets_next_floor() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "父子楼").await;

    let (status, root, _) = authed_post(
        &app,
        &format!("/api/v1/posts/{post_id}/comments"),
        &session,
        &csrf,
        comment_body("根回复", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let root_id = root["id"].as_str().unwrap().to_string();

    let mut reply = comment_body("引用根回复", "comment-req-0002");
    reply["parent_id"] = json!(root_id);
    let (status, reply_body, _) = authed_post(
        &app,
        &format!("/api/v1/posts/{post_id}/comments"),
        &session,
        &csrf,
        reply,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "父子回复必须 201: {reply_body}"
    );
    assert_eq!(reply_body["floor"], 2);
    assert_eq!(reply_body["parent_id"], Value::String(root_id));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_comment_rejects_invalid_inputs() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "校验").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    // 空 markdown → 400
    let (status, body, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("  ", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // client_request_id 过短 → 400
    let (status, _, _) =
        authed_post(&app, &uri, &session, &csrf, comment_body("正文", "short")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 不存在的 parent → 400（稳定错误，不泄漏）
    let mut with_parent = comment_body("正文", "comment-req-0002");
    with_parent["parent_id"] = json!(uuid::Uuid::now_v7().to_string());
    let (status, body, _) = authed_post(&app, &uri, &session, &csrf, with_parent).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("parent comment not found or not visible"),
        "稳定 bad_request detail: {body}"
    );

    // 未认证 → 401
    let (status, _, _) = authed_post(
        &app,
        &uri,
        "",
        "x",
        comment_body("正文", "comment-req-0003"),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_comment_rejects_locked_post() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "锁帖").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    // 先有回复
    let (status, _, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("回复", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 锁帖（closed_at 即回复开关）
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET closed_at = ? WHERE id = ?")
                .bind(now_millis())
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 锁帖后回复 → 409（即使已有评论）
    let (status, body, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("锁后回复", "comment-req-0002"),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["detail"].as_str().unwrap().contains("closed"));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_comment_rejects_banned_and_unverified() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let user = insert_user(&pool, "alice", true, "active").await;
    let post_id = publish(&pool, &user, "账号门").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    // banned 用户 → 403（authorize_action 账号状态门）
    let banned = insert_user(&pool, "ban", true, "banned").await;
    let banned_session = common::direct_session_cookie(&pool, &banned).await;
    let banned_csrf = session_csrf(&app, &banned_session).await;
    let (status, body, _) = authed_post(
        &app,
        &uri,
        &banned_session,
        &banned_csrf,
        comment_body("禁止回复", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // 未验证邮箱用户 → 403
    let unverified = insert_user(&pool, "uv", false, "active").await;
    let uv_session = common::direct_session_cookie(&pool, &unverified).await;
    let uv_csrf = session_csrf(&app, &uv_session).await;
    let (status, _, _) = authed_post(
        &app,
        &uri,
        &uv_session,
        &uv_csrf,
        comment_body("未验证", "comment-req-0002"),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_comment_on_hidden_post_is_allowed() {
    // 决策：hidden 主题对公众隐藏，但作者/既有读者仍可回复（create 逻辑允许
    // status IN (published, hidden)；仅 draft/deleted 拒回复）。
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "隐藏主题").await;

    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET status = 'hidden' WHERE id = ?")
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let (status, body, _) = authed_post(
        &app,
        &format!("/api/v1/posts/{post_id}/comments"),
        &session,
        &csrf,
        comment_body("隐藏主题回复", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "hidden 主题仍可回复: {body}");

    // draft 状态 → 404
    let post2 = publish(&pool, &user, "草稿主题").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET status = 'draft' WHERE id = ?")
                .bind(&post2)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let (status, body, _) = authed_post(
        &app,
        &format!("/api/v1/posts/{post2}/comments"),
        &session,
        &csrf,
        comment_body("草稿主题回复", "comment-req-0002"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_comment_rejects_invisible_parent() {
    // 隐藏/已删 parent → 400（稳定 detail，不泄漏 deleted vs hidden）；
    // 跨主题 parent → 400 同主题 detail。
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post1 = publish(&pool, &user, "主题一").await;
    let post2 = publish(&pool, &user, "主题二").await;
    let uri1 = format!("/api/v1/posts/{post1}/comments");

    // 主题一的一条评论
    let (status, root, _) = authed_post(
        &app,
        &uri1,
        &session,
        &csrf,
        comment_body("主题一根", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let root_id = root["id"].as_str().unwrap().to_string();

    // 软删 parent 后引用 → 400
    let (status, _, _) = authed_delete(
        &app,
        &format!("/api/v1/comments/{root_id}"),
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let mut reply_deleted = comment_body("引用已删", "comment-req-0002");
    reply_deleted["parent_id"] = json!(root_id);
    let (status, body, _) = authed_post(&app, &uri1, &session, &csrf, reply_deleted).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("parent comment not found or not visible"),
        "不泄漏 deleted vs hidden: {body}"
    );

    // 跨主题 parent → 400 同主题
    let (status, root2, _) = authed_post(
        &app,
        &format!("/api/v1/posts/{post2}/comments"),
        &session,
        &csrf,
        comment_body("主题二根", "comment-req-0003"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let root2_id = root2["id"].as_str().unwrap().to_string();
    let mut cross = comment_body("跨主题", "comment-req-0004");
    cross["parent_id"] = json!(root2_id);
    let (status, body, _) = authed_post(&app, &uri1, &session, &csrf, cross).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("parent comment must belong to the same post"),
        "{body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_comment_idempotency_replay_and_conflict() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "幂等").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    let body = comment_body("幂等正文", "comment-req-idem-1");
    let (status, first, _) = authed_post(&app, &uri, &session, &csrf, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    let first_id = first["id"].as_str().unwrap().to_string();

    // 同 key + 同摘要 → 重放返回原评论 id
    let (status, replay, _) = authed_post(&app, &uri, &session, &csrf, body.clone()).await;
    assert_eq!(status, StatusCode::CREATED, "重放必须 201: {replay}");
    assert_eq!(
        replay["id"].as_str().unwrap(),
        first_id,
        "同 key+摘要必须返回原评论"
    );

    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM comments")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1, "幂等重放不得产生重复行");

    // 同 key + 不同摘要 → 409
    let mut changed = body.clone();
    changed["markdown"] = json!("不同的正文");
    let (status, body, _) = authed_post(&app, &uri, &session, &csrf, changed).await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(
        body["detail"]
            .as_str()
            .unwrap()
            .contains("idempotency key reused with different request"),
        "{body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── 列表 ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn list_comments_paginates_by_floor() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "分页").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");
    let mut ids = Vec::new();
    for i in 1..=5 {
        let (status, body, _) = authed_post(
            &app,
            &uri,
            &session,
            &csrf,
            comment_body(&format!("第 {i} 层"), &format!("comment-req-page-{i}")),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
        ids.push(body["id"].as_str().unwrap().to_string());
    }

    // 匿名可读（OpenAPI security = 可选会话）+ 响应头
    let (status, page1, headers) = get(&app, &format!("{uri}?limit=2")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("public, max-age=60"),
        "列表必须 public, max-age=60"
    );
    assert!(headers.get("etag").is_some(), "列表必须带 ETag");

    let items1 = page1["items"].as_array().unwrap();
    assert_eq!(items1.len(), 2);
    assert_eq!(items1[0]["floor"], 1);
    assert_eq!(items1[1]["floor"], 2);
    assert_eq!(page1["page"]["has_more"], true);
    let cursor1 = page1["page"]["next_cursor"].as_str().unwrap().to_string();
    assert!(!cursor1.is_empty());

    // 第二页
    let (_, page2, _) = get(&app, &format!("{uri}?limit=2&after={cursor1}")).await;
    let items2 = page2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 2);
    assert_eq!(items2[0]["floor"], 3);
    assert_eq!(items2[1]["floor"], 4);
    assert_eq!(page2["page"]["has_more"], true);
    let cursor2 = page2["page"]["next_cursor"].as_str().unwrap().to_string();

    // 第三页 → 末页
    let (_, page3, _) = get(&app, &format!("{uri}?limit=2&after={cursor2}")).await;
    let items3 = page3["items"].as_array().unwrap();
    assert_eq!(items3.len(), 1);
    assert_eq!(items3[0]["floor"], 5);
    assert_eq!(page3["page"]["has_more"], false);
    assert!(page3["page"]["next_cursor"].is_null());

    // published 评论 body_html 可见且 floor 排序稳定
    assert!(
        items1[0]["body_html"].as_str().unwrap().contains("第 1 层"),
        "published 评论必须返回 body_html"
    );
    // 非法游标 → 400
    let (status, _, _) = get(&app, &format!("{uri}?after=not-a-cursor")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn list_comments_404_when_post_missing_or_hidden_state() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, _, _) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "404").await;

    // 不存在 → 404
    let (status, _, _) = get(
        &app,
        &format!("/api/v1/posts/{}/comments", uuid::Uuid::now_v7()),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // deleted 主题 → 404
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET deleted_at = ? WHERE id = ?")
                .bind(now_millis())
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let (status, _, _) = get(&app, &format!("/api/v1/posts/{post_id}/comments")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn list_comments_shows_deleted_placeholder() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "占位").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    let (status, created, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("敏感正文勿泄漏", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let comment_id = created["id"].as_str().unwrap().to_string();

    // 作者软删
    let (status, _, _) = authed_delete(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, body, _) = get(&app, &uri).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "软删评论以占位投影保留在列表");
    assert_eq!(items[0]["status"], "deleted");
    assert!(items[0]["body_html"].is_null(), "占位投影不得带正文");
    assert_eq!(items[0]["id"], Value::String(comment_id.clone()));
    assert_eq!(items[0]["floor"], 1, "软删保留楼层，不重编号");
    let raw = body.to_string();
    assert!(
        !raw.contains("敏感正文勿泄漏"),
        "列表不得泄漏已删正文: {raw}"
    );

    // reply_count 不递减（占位保留）
    let reply_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT reply_count FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(reply_count, 1, "软删不递减 posts.reply_count");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── 编辑 ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn update_comment_edits_within_window_and_writes_revision() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "编辑").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    let (status, created, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("原始正文", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let comment_id = created["id"].as_str().unwrap().to_string();

    // 窗口内编辑（body version 守卫）
    let (status, body, headers) = authed_patch(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        json!({ "markdown": "编辑后 **新版**", "version": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("private, no-store")
    );
    assert_eq!(body["version"], 2, "编辑后 version 递增");
    assert!(
        body["body_html"].as_str().unwrap().contains("新版"),
        "编辑响应 body_html 为最新渲染: {}",
        body["body_html"]
    );
    assert_eq!(body["floor"], 1);

    // 落库：content 更新 + version=2 + comment_revisions 快照（version=2）
    let (content, version): (String, i64) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT content, version FROM comments WHERE id = ?")
            .bind(&comment_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(content, "编辑后 **新版**");
    assert_eq!(version, 2);
    let (rev_count, rev_version, rev_body, rev_editor): (i64, i64, String, String) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT COUNT(*), MIN(version), MIN(body_markdown), MIN(editor_id) FROM comment_revisions WHERE comment_id = ?",
        )
        .bind(&comment_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(rev_count, 1, "每次编辑产生一条不可变修订");
    assert_eq!(rev_version, 2, "修订 version = old+1");
    assert_eq!(rev_body, "编辑后 **新版**");
    assert_eq!(rev_editor, user);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn update_comment_via_if_match_header() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "If-Match").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    let (status, created, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("原文", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let comment_id = created["id"].as_str().unwrap().to_string();

    let (status, body, _) = authed_patch_ifmatch(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        1,
        json!({ "markdown": "If-Match 编辑" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["version"], 2);

    // 既无 If-Match 也无 body version → 400
    let (status, _, _) = authed_patch(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        json!({ "markdown": "缺版本" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn update_comment_version_conflict_and_non_author() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "冲突").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    let (status, created, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("原文", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let comment_id = created["id"].as_str().unwrap().to_string();

    // 版本过期 → 409 version_conflict
    let (status, body, _) = authed_patch(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        json!({ "markdown": "过期版本", "version": 99 }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(problem_code(&body), "version_conflict");

    // 他人编辑 → 403
    let other = insert_user(&pool, "bob", true, "active").await;
    let other_session = common::direct_session_cookie(&pool, &other).await;
    let other_csrf = session_csrf(&app, &other_session).await;
    let (status, body, _) = authed_patch(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &other_session,
        &other_csrf,
        json!({ "markdown": "他人编辑", "version": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    // 不存在 → 404；已删评论 → 404
    let (status, _, _) = authed_patch(
        &app,
        &format!("/api/v1/comments/{}", uuid::Uuid::now_v7()),
        &session,
        &csrf,
        json!({ "markdown": "x", "version": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (_, _, _) = authed_delete(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        None,
    )
    .await;
    let (status, _, _) = authed_patch(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        json!({ "markdown": "删后编辑", "version": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "已删评论必须 404");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn update_comment_window_expired() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "超窗").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    let (status, created, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("原文", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let comment_id = created["id"].as_str().unwrap().to_string();

    // 模拟 created_at = now-31min（窗口 30 分钟）
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE comments SET created_at = ? WHERE id = ?")
                .bind(now_millis() - 31 * 60 * 1000)
                .bind(&comment_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let (status, body, _) = authed_patch(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        json!({ "markdown": "超窗编辑", "version": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert!(body["detail"]
        .as_str()
        .unwrap()
        .contains("edit window expired"));
    // 不得产生修订
    let rev_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM comment_revisions")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(rev_count, 0, "超窗编辑不得写入修订");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── 删除 ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_comment_author_soft_delete() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "删除").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    let (status, created, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("待删除", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let comment_id = created["id"].as_str().unwrap().to_string();

    // 作者删除 → 204 + private,no-store
    let (status, _, headers) = authed_delete(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("private, no-store")
    );

    // 落库：软删行保留
    let (row_status, deleted_at): (String, Option<i64>) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT status, deleted_at FROM comments WHERE id = ?")
            .bind(&comment_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(row_status, "deleted");
    assert!(deleted_at.is_some(), "软删必须置 deleted_at");

    // 二次删除 → 404
    let (status, body, _) = authed_delete(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn delete_comment_admin_with_audit_and_non_moderator_403() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (author, author_session, author_csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &author, "管理删除").await;

    let (status, created, _) = authed_post(
        &app,
        &format!("/api/v1/posts/{post_id}/comments"),
        &author_session,
        &author_csrf,
        comment_body("违规内容", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let comment_id = created["id"].as_str().unwrap().to_string();

    // 普通第三方用户 → 403
    let (third_session, third_csrf) = {
        let u = insert_user(&pool, "third", true, "active").await;
        let s = common::direct_session_cookie(&pool, &u).await;
        let c = session_csrf(&app, &s).await;
        (s, c)
    };
    let (status, body, _) = authed_delete(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &third_session,
        &third_csrf,
        Some(json!({ "reason": "违规" })),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(problem_code(&body), "forbidden");

    // 全局版主（global_moderator，含 post.moderate）删除他人评论 → 204 + 审计
    let (moderator, mod_session, mod_csrf) = {
        let u = insert_user(&pool, "mod", true, "active").await;
        assign_global_role(&pool, &u, "global_moderator").await;
        // elevated 角色触发强制 TOTP（M02-MFA-05/06）：不 enrollment 会被降级为 member
        common::enroll_totp(&pool, &u).await;
        let s = common::direct_session_cookie(&pool, &u).await;
        let c = session_csrf(&app, &s).await;
        (u, s, c)
    };
    let (status, _, _) = authed_delete(
        &app,
        &format!("/api/v1/comments/{comment_id}"),
        &mod_session,
        &mod_csrf,
        Some(json!({ "reason": "按举报复核" })),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (actor, action, target_type, target_id, role, reason): (
        String,
        String,
        String,
        String,
        Option<String>,
        Option<String>,
    ) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT actor_id, action, target_type, target_id, effective_role, reason
                 FROM audit_logs WHERE action = 'comment.delete' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(actor, moderator, "审计 actor 必须是删除者");
    assert_eq!(action, "comment.delete");
    assert_eq!(target_type, "comment");
    assert_eq!(target_id, comment_id);
    assert_eq!(
        role.as_deref(),
        Some("moderator"),
        "审计必须带 effective_role"
    );
    assert_eq!(reason.as_deref(), Some("按举报复核"));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn delete_comment_with_stale_if_match_is_conflict() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (user, session, csrf) = authed_session(&app, &pool).await;
    let post_id = publish(&pool, &user, "If-Match 删除").await;
    let uri = format!("/api/v1/posts/{post_id}/comments");

    let (status, created, _) = authed_post(
        &app,
        &uri,
        &session,
        &csrf,
        comment_body("原文", "comment-req-0001"),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let comment_id = created["id"].as_str().unwrap().to_string();

    // If-Match 过期版本 → 409 version_conflict
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(format!("/api/v1/comments/{comment_id}"))
                .header("if-match", "99")
                .header("x-csrf-token", &csrf)
                .header("cookie", &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(problem_code(&body), "version_conflict");

    close_pool(&pool).await;
    cleanup(&dir);
}
