//! M04-POSTS-02：草稿创建 / 读取自己的草稿 / cursor 列表（SQLite + 路由层）。
//!
//! 覆盖：201 创建（Cache-Control private,no-store）、服务端字段校验 400、
//! 未认证 401、未验证邮箱 403、读本人 200 / 他人 404 / 不存在 404、
//! cursor 分页（keyset on updated_at DESC、next_cursor 续页）。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
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
    let dir = std::env::temp_dir().join(format!("bblbb-drf-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    // 服务启动时 main.rs 会 seed 内置角色/权限；测试须显式调用
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

/// 直接插入用户（可指定邮箱验证状态）；返回 user_id。
/// 注意：authz 账户门以 `email_verified_at` 为权威字段（0011 迁移），
/// 验证用户需同时写 `email_verified` 与 `email_verified_at`。
async fn insert_user(pool: &DatabasePool, tag: &str, verified: bool) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
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

/// 已认证写请求（会话 Cookie + X-CSRF-Token）。
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

async fn authed_get(app: &axum::Router, uri: &str, session: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("cookie", session)
                .body(Body::empty())
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

fn valid_draft_body() -> Value {
    json!({
        "type": "article",
        "title": "草稿标题",
        "markdown": "草稿正文内容",
        "board_id": BOARD_ID,
        "visibility_level": 1,
        "access_policy": "public",
        "client_request_id": "draft-req-id-0001",
    })
}

/// 建立已认证会话（verified 用户）并返回 (session cookie, csrf token)。
async fn authed_session(app: &axum::Router, pool: &DatabasePool) -> (String, String) {
    let user = insert_user(pool, "alice", true).await;
    let session = common::direct_session_cookie(pool, &user).await;
    let csrf = session_csrf(app, &session).await;
    (session, csrf)
}

#[tokio::test]
async fn create_draft_returns_201_with_private_no_store() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (session, csrf) = authed_session(&app, &pool).await;

    let (status, body, headers) =
        authed_post(&app, "/api/v1/drafts", &session, &csrf, valid_draft_body()).await;
    assert_eq!(status, StatusCode::CREATED, "创建草稿必须 201: {body}");
    assert_eq!(
        headers.get("cache-control").and_then(|v| v.to_str().ok()),
        Some("private, no-store"),
        "草稿响应必须 private, no-store"
    );
    assert!(body["id"].is_string());
    assert_eq!(body["version"], 1, "version 从 1 起");
    assert_eq!(body["title"], "草稿标题");
    assert_eq!(body["type"], "article");
    assert_eq!(body["board_id"], BOARD_ID);
    assert_eq!(body["visibility_level"], 1);
    assert_eq!(body["access_policy"], "public");
    assert!(body["scheduled_at"].is_null());

    // 落库行存在且 owner 正确
    let draft_id = body["id"].as_str().unwrap();
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM drafts WHERE id = ?")
            .bind(draft_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_draft_rejects_invalid_fields() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (session, csrf) = authed_session(&app, &pool).await;

    // 空标题
    let mut bad = valid_draft_body();
    bad["title"] = json!("   ");
    let (status, body, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "空标题必须 400: {body}");

    // 非法 board_id
    let mut bad = valid_draft_body();
    bad["board_id"] = json!("not-a-uuid");
    let (status, _, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非法板块 UUID 必须 400");

    // 幂等键过短
    let mut bad = valid_draft_body();
    bad["client_request_id"] = json!("short");
    let (status, _, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "幂等键过短必须 400");

    // 未知类型
    let mut bad = valid_draft_body();
    bad["type"] = json!("question");
    let (status, _, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "未知类型必须 400");

    // 过期 scheduled_at
    let mut bad = valid_draft_body();
    bad["scheduled_at"] = json!(now_millis() - 1000);
    let (status, _, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, bad).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "过期定时必须 400");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_draft_requires_auth_and_verified_email() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    // 未认证 → 401
    let (status, _, _) = authed_post(&app, "/api/v1/drafts", "", "", valid_draft_body()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 未验证邮箱 → 403
    let user = insert_user(&pool, "unverified", false).await;
    let session = common::direct_session_cookie(&pool, &user).await;
    let csrf = session_csrf(&app, &session).await;
    let (status, body, _) =
        authed_post(&app, "/api/v1/drafts", &session, &csrf, valid_draft_body()).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "未验证邮箱必须 403: {body}");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn get_owned_draft_returns_200() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (session, csrf) = authed_session(&app, &pool).await;

    let (_, created, _) =
        authed_post(&app, "/api/v1/drafts", &session, &csrf, valid_draft_body()).await;
    let draft_id = created["id"].as_str().unwrap().to_string();

    let (status, body) = authed_get(&app, &format!("/api/v1/drafts/{draft_id}"), &session).await;
    assert_eq!(status, StatusCode::OK, "读自己的草稿必须 200: {body}");
    assert_eq!(body["id"], created["id"]);
    assert_eq!(body["title"], "草稿标题");
    assert_eq!(body["markdown"], "草稿正文内容");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn get_others_or_missing_draft_is_404() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (alice_session, alice_csrf) = authed_session(&app, &pool).await;
    let bob = insert_user(&pool, "bob", true).await;
    let bob_session = common::direct_session_cookie(&pool, &bob).await;

    let (_, created, _) = authed_post(
        &app,
        "/api/v1/drafts",
        &alice_session,
        &alice_csrf,
        valid_draft_body(),
    )
    .await;
    let draft_id = created["id"].as_str().unwrap().to_string();

    // 他人草稿 → 404（不泄露存在性）
    let (status, _) = authed_get(&app, &format!("/api/v1/drafts/{draft_id}"), &bob_session).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "他人草稿必须 404");

    // 不存在的草稿 → 404
    let (status, _) = authed_get(
        &app,
        &format!("/api/v1/drafts/{}", uuid::Uuid::now_v7()),
        &alice_session,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在草稿必须 404");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn list_drafts_cursor_pagination() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    // 直接插入用户 + 会话，拿 user_id 与 session cookie
    let user_id = insert_user(&pool, "alice", true).await;
    let session = common::direct_session_cookie(&pool, &user_id).await;

    // 直接插入 3 条受控 updated_at 的草稿（keyset 顺序确定）
    let now = now_millis();
    for (i, ts) in [(3, now - 3_000), (2, now - 2_000), (1, now - 1_000)] {
        match &pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO drafts (id, owner_id, board_id, post_type, title, markdown, visibility_level, access_policy, scheduled_at, version, created_at, updated_at, deleted_at)
                     VALUES (?, ?, ?, 'article', ?, ?, 1, 'public', NULL, 1, ?, ?, NULL)",
                )
                .bind(format!("draft-{i}"))
                .bind(&user_id)
                .bind(BOARD_ID)
                .bind(format!("草稿{i}"))
                .bind(format!("正文{i}"))
                .bind(now - 10_000)
                .bind(ts)
                .execute(p)
                .await
                .unwrap();
            }
            Either::Right(_) => panic!("SQLite only"),
        }
    }

    // 第一页：limit=2 → 最新两条（d1 更新于 now-1000 最先，然后 d2）
    let (status, page1) = authed_get(&app, "/api/v1/drafts?limit=2", &session).await;
    assert_eq!(status, StatusCode::OK, "列表必须 200: {page1}");
    let items1 = page1["items"].as_array().unwrap();
    assert_eq!(items1.len(), 2, "第一页两条: {page1}");
    assert_eq!(items1[0]["title"], "草稿1");
    assert_eq!(items1[1]["title"], "草稿2");
    let cursor = page1["next_cursor"].as_str().unwrap().to_string();
    assert!(!cursor.is_empty(), "还有下一页必须有 next_cursor");

    // 第二页：after=cursor → 剩余一条
    let (_, page2) = authed_get(
        &app,
        &format!("/api/v1/drafts?limit=2&after={cursor}"),
        &session,
    )
    .await;
    let items2 = page2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 1, "第二页一条: {page2}");
    assert_eq!(items2[0]["title"], "草稿3");
    assert_eq!(
        page2["next_cursor"].as_str().unwrap(),
        "",
        "最后一页无 next_cursor"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ──────────────── M04-POSTS-03：更新 / 幂等 / 软删除 ────────────────

/// 已认证 PATCH（带 If-Match 与 body）。
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

/// 已认证 DELETE。
async fn authed_delete(
    app: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .body(Body::empty())
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
async fn update_draft_happy_path_increments_version() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (session, csrf) = authed_session(&app, &pool).await;

    let (_, created, _) =
        authed_post(&app, "/api/v1/drafts", &session, &csrf, valid_draft_body()).await;
    let draft_id = created["id"].as_str().unwrap().to_string();
    assert_eq!(created["version"], 1);

    let (status, body) = authed_patch(
        &app,
        &format!("/api/v1/drafts/{draft_id}"),
        &session,
        &csrf,
        "1",
        json!({ "title": "更新后的标题", "markdown": "更新后的正文" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "更新草稿必须 200: {body}");
    assert_eq!(body["version"], 2, "更新后 version 递增");
    assert_eq!(body["title"], "更新后的标题");
    assert_eq!(body["markdown"], "更新后的正文");
    // 未提交的字段保持不变
    assert_eq!(body["board_id"], BOARD_ID);
    assert_eq!(body["access_policy"], "public");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn update_draft_version_conflict_returns_409() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (session, csrf) = authed_session(&app, &pool).await;

    let (_, created, _) =
        authed_post(&app, "/api/v1/drafts", &session, &csrf, valid_draft_body()).await;
    let draft_id = created["id"].as_str().unwrap().to_string();

    // 错误版本号 → 409
    let (status, body) = authed_patch(
        &app,
        &format!("/api/v1/drafts/{draft_id}"),
        &session,
        &csrf,
        "99",
        json!({ "title": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "版本冲突必须 409: {body}");

    // 缺 If-Match → 400
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/drafts/{draft_id}"))
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &session)
                .body(Body::from(json!({ "title": "x" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::BAD_REQUEST,
        "缺 If-Match 必须 400"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn delete_draft_is_soft_delete() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (session, csrf) = authed_session(&app, &pool).await;

    let (_, created, _) =
        authed_post(&app, "/api/v1/drafts", &session, &csrf, valid_draft_body()).await;
    let draft_id = created["id"].as_str().unwrap().to_string();

    // 软删除 → 204
    let (status, _) =
        authed_delete(&app, &format!("/api/v1/drafts/{draft_id}"), &session, &csrf).await;
    assert_eq!(status, StatusCode::NO_CONTENT, "软删除必须 204");

    // 删除后不可读、列表不含
    let (status, _) = authed_get(&app, &format!("/api/v1/drafts/{draft_id}"), &session).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "软删后读取必须 404");
    let (_, list) = authed_get(&app, "/api/v1/drafts", &session).await;
    assert_eq!(
        list["items"].as_array().unwrap().len(),
        0,
        "列表不得含软删草稿"
    );

    // 行保留（deleted_at 置位），供审计/恢复
    let deleted_at: Option<i64> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT deleted_at FROM drafts WHERE id = ?")
            .bind(&draft_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(deleted_at.is_some(), "软删行必须保留 deleted_at");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_draft_is_idempotent_on_client_request_id() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (session, csrf) = authed_session(&app, &pool).await;

    let body = valid_draft_body();
    let (s1, r1, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, body.clone()).await;
    assert_eq!(s1, StatusCode::CREATED);
    let (s2, r2, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, body).await;
    assert_eq!(s2, StatusCode::CREATED, "同 key+摘要重放必须成功: {r2}");
    assert_eq!(r1["id"], r2["id"], "幂等重放必须返回同一草稿");

    // 行只存在一条
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM drafts")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1, "幂等重放不得产生重复行");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_draft_idempotency_conflict_on_different_body() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (session, csrf) = authed_session(&app, &pool).await;

    let first = valid_draft_body();
    let (s1, _, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, first).await;
    assert_eq!(s1, StatusCode::CREATED);

    // 相同 key、不同正文 → 409
    let mut second = valid_draft_body();
    second["title"] = json!("另一个标题");
    let (s2, body, _) = authed_post(&app, "/api/v1/drafts", &session, &csrf, second).await;
    assert_eq!(s2, StatusCode::CONFLICT, "同 key 不同摘要必须 409: {body}");

    close_pool(&pool).await;
    cleanup(&dir);
}
