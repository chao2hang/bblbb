//! GAP-FIX 社交域集成测试（私信 + API 密钥；SQLite + 路由层）。
//!
//! 覆盖：会话创建/复用/自谈 422/目标 404、消息发送/列表/未读数/已读、
//! 消息幂等（同 key 重放返回原消息、不同 body 409）、非参与者 404、
//! 私信通知（type='mention'，link='/messages'）、401 权限拒绝；
//! API 密钥：创建（明文一次性返回 + 只存 hash + prefix）、列表（无明文）、
//! 重放 409（明文仅一次）、scopes 白名单 400、撤销 204/幂等/404、401。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::authz::roles::seed_builtin_roles;
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

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-soc2-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
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

/// 插入用户；返回 (user_id, username)。
async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(format!("{username}@example.com"))
            .bind(now - 25 * 3600 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (user_id, username)
}

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

/// 任意方法的已认证请求（会话 + CSRF）；返回 (status, body)。
async fn authed(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
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
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn session_csrf(app: &Router, session: &str) -> String {
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

async fn count(pool: &DatabasePool, sql: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 用户上下文（id + username + session + csrf）。
struct UserCtx {
    #[allow(dead_code)]
    id: String,
    username: String,
    session: String,
    csrf: String,
}

async fn user_ctx(app: &Router, pool: &DatabasePool, tag: &str) -> UserCtx {
    let (id, username) = insert_user(pool, tag).await;
    let session = common::direct_session_cookie(pool, &id).await;
    let csrf = session_csrf(app, &session).await;
    UserCtx {
        id,
        username,
        session,
        csrf,
    }
}

// ───────────────────────────── 私信 ─────────────────────────────

#[tokio::test]
async fn conversation_create_reuse_and_send_flow() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let alice = user_ctx(&app, &pool, "alice").await;
    let bob = user_ctx(&app, &pool, "bob").await;

    // 开会话 → 201 {id, other}
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/conversations",
        &alice.session,
        &alice.csrf,
        json!({ "username": bob.username, "client_request_id": "conv-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "开会话必须 201: {body}");
    let conv_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["other"]["username"], bob.username.as_str());
    assert!(body["other"]["level"].is_number());

    // 复用：两人已有会话返回既有 id
    let (status, body2) = authed(
        &app,
        "POST",
        "/api/v1/conversations",
        &bob.session,
        &bob.csrf,
        json!({ "username": alice.username, "client_request_id": "conv-2" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "复用会话必须 201");
    assert_eq!(
        body2["id"].as_str().unwrap(),
        conv_id,
        "两人已有会话必须返回既有 id"
    );
    let conv_rows = count(&pool, "SELECT COUNT(*) FROM conversations").await;
    assert_eq!(conv_rows, 1, "不重复建会话");

    // 发消息 → 201 {id, sender_username, body, created_at}
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &alice.session,
        &alice.csrf,
        json!({ "body": "你好，Bob！", "client_request_id": "msg-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "发消息必须 201: {body}");
    assert_eq!(body["sender_username"], alice.username.as_str());
    assert_eq!(body["body"], "你好，Bob！");
    assert!(body["created_at"].is_number());
    let msg_id = body["id"].as_str().unwrap().to_string();

    // 幂等重放：同 key + 同 body → 同一消息 id，不重复落库
    let (status, body2) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &alice.session,
        &alice.csrf,
        json!({ "body": "你好，Bob！", "client_request_id": "msg-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "同 key+同 body 重放 201");
    assert_eq!(body2["id"].as_str().unwrap(), msg_id, "重放返回原消息");
    let msg_rows = count(&pool, "SELECT COUNT(*) FROM messages").await;
    assert_eq!(msg_rows, 1, "幂等：只落一条消息");

    // 同 key + 不同 body → 409
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &alice.session,
        &alice.csrf,
        json!({ "body": "不同的内容", "client_request_id": "msg-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "同 key 不同 body 必须 409");

    // bob 视角：会话列表 + 未读数 + last_message
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/conversations",
        &bob.session,
        &bob.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "会话列表必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], conv_id.as_str());
    assert_eq!(items[0]["other"]["username"], alice.username.as_str());
    assert_eq!(items[0]["unread_count"], 1, "bob 有 1 条未读");
    assert_eq!(items[0]["last_message"]["body"], "你好，Bob！");
    assert_eq!(
        items[0]["last_message"]["sender_username"],
        alice.username.as_str()
    );

    // bob 读消息线程（ASC）→ 1 条
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &bob.session,
        &bob.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["sender_username"], alice.username.as_str());

    // 标记已读 → 204；未读清零
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/read"),
        &bob.session,
        &bob.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "已读必须 204");
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/conversations",
        &bob.session,
        &bob.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"][0]["unread_count"], 0, "已读后未读清零");

    // 私信通知：bob 收到 type='mention'，link='/messages'
    let notifies = count(
        &pool,
        &format!(
            "SELECT COUNT(*) FROM notifications WHERE user_id = '{}' AND type = 'mention' AND link = '/messages'",
            bob.id
        ),
    )
    .await;
    assert_eq!(notifies, 1, "收到私信必须插 mention 通知");
    let notify_title: String = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT title FROM notifications WHERE user_id = ? AND type = 'mention'",
        )
        .bind(&bob.id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        notify_title.contains("给你发来私信"),
        "通知标题必须为「{{sender}} 给你发来私信」: {notify_title}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn conversation_self_404_and_participant_gates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let alice = user_ctx(&app, &pool, "alice").await;
    let bob = user_ctx(&app, &pool, "bob").await;
    let carol = user_ctx(&app, &pool, "carol").await;

    // 与自己开会话 → 422
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/conversations",
        &alice.session,
        &alice.csrf,
        json!({ "username": alice.username, "client_request_id": "self" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "自谈必须 422");

    // 目标不存在 → 404
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/conversations",
        &alice.session,
        &alice.csrf,
        json!({ "username": "no_such_user", "client_request_id": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 开会话 + 发消息
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/conversations",
        &alice.session,
        &alice.csrf,
        json!({ "username": bob.username, "client_request_id": "conv" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let conv_id = body["id"].as_str().unwrap().to_string();
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &alice.session,
        &alice.csrf,
        json!({ "body": "hi", "client_request_id": "m1" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);

    // 非参与者（carol）读线程 → 404
    let (status, _) = authed(
        &app,
        "GET",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &carol.session,
        &carol.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "非参与者读必须 404");

    // 非参与者发消息 → 404
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &carol.session,
        &carol.csrf,
        json!({ "body": "intrude", "client_request_id": "m2" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "非参与者发必须 404");

    // 非参与者已读 → 404
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/read"),
        &carol.session,
        &carol.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 消息体校验：空 body / 超长 body → 400
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &alice.session,
        &alice.csrf,
        json!({ "body": "", "client_request_id": "m3" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "空 body 必须 400");
    let long_body = "x".repeat(2001);
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/conversations/{conv_id}/messages"),
        &alice.session,
        &alice.csrf,
        json!({ "body": long_body, "client_request_id": "m4" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "超长 body 必须 400");

    // 未认证 → 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/conversations")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "未认证必须 401");
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/conversations")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────────── API 密钥 ─────────────────────────────

#[tokio::test]
async fn api_key_create_list_revoke_and_one_time_plaintext() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let alice = user_ctx(&app, &pool, "alice").await;

    // 创建 → 201 {id,name,scopes,created_at,key,prefix}
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/me/api-keys",
        &alice.session,
        &alice.csrf,
        json!({
            "name": "我的脚本密钥",
            "scopes": ["posts:read", "me:read"],
            "client_request_id": "key-req-1"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "创建密钥必须 201: {body}");
    let key = body["key"].as_str().unwrap().to_string();
    let key_id = body["id"].as_str().unwrap().to_string();
    let prefix = body["prefix"].as_str().unwrap().to_string();
    assert!(key.starts_with("bblbb_"), "明文格式 bblbb_ 前缀");
    assert_eq!(key.len(), 6 + 32, "明文随机段 32 字符");
    assert!(key[6..].chars().all(|c| c.is_ascii_alphanumeric()));
    assert_eq!(
        prefix,
        key.chars().take(12).collect::<String>(),
        "prefix = 前 12 字符"
    );
    assert_eq!(prefix.len(), 12);

    // 只存 hash：数据库无明文
    let stored_hash: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT secret_hash FROM api_keys WHERE id = ?")
            .bind(&key_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(
        stored_hash.len(),
        64,
        "secret_hash 为 SHA-256 hex（64 字符）"
    );
    assert_ne!(stored_hash, key, "绝不存明文");
    use sha2::Digest;
    let expected = hex::encode(sha2::Sha256::digest(key.as_bytes()));
    assert_eq!(stored_hash, expected, "hash = SHA-256(明文)");

    // 列表 → items（无 key 字段；prefix/scopes/created_at 在）
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/api-keys",
        &alice.session,
        &alice.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "密钥列表必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert!(items[0].get("key").is_none(), "列表绝不返回明文 key");
    assert_eq!(items[0]["prefix"], prefix.as_str());
    assert_eq!(items[0]["name"], "我的脚本密钥");
    assert_eq!(
        items[0]["scopes"].as_array().unwrap().len(),
        2,
        "scopes 数组回显"
    );

    // 重放（同 key + 同 body）→ 409（明文仅首次返回，不产生重复行）
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/me/api-keys",
        &alice.session,
        &alice.csrf,
        json!({
            "name": "我的脚本密钥",
            "scopes": ["posts:read", "me:read"],
            "client_request_id": "key-req-1"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "同 key 重放必须 409（明文一次性）"
    );
    let rows = count(&pool, "SELECT COUNT(*) FROM api_keys").await;
    assert_eq!(rows, 1, "重放不产生重复行");

    // 撤销 → 204；幂等再撤销 → 204；不存在的 → 404
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/me/api-keys/{key_id}"),
        &alice.session,
        &alice.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "撤销必须 204");
    let revoked_at = count(
        &pool,
        &format!("SELECT COUNT(*) FROM api_keys WHERE id = '{key_id}' AND revoked_at IS NOT NULL"),
    )
    .await;
    assert_eq!(revoked_at, 1, "软删除：revoked_at 置位");
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/me/api-keys/{key_id}"),
        &alice.session,
        &alice.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "重复撤销幂等 204");
    let (status, _) = authed(
        &app,
        "DELETE",
        "/api/v1/me/api-keys/00000000-0000-7000-8000-0000000000ff",
        &alice.session,
        &alice.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "撤销不存在密钥必须 404");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn api_key_validation_and_auth_gates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let alice = user_ctx(&app, &pool, "alice").await;
    let bob = user_ctx(&app, &pool, "bob").await;

    // 非法 scope → 400
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/me/api-keys",
        &alice.session,
        &alice.csrf,
        json!({
            "name": "bad scope",
            "scopes": ["admin:everything"],
            "client_request_id": "k1"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非法 scope 必须 400");

    // 名字为空 → 400
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/me/api-keys",
        &alice.session,
        &alice.csrf,
        json!({
            "name": "",
            "scopes": [],
            "client_request_id": "k2"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "空名字必须 400");

    // alice 创建密钥后，bob 无法撤销（他人资源 → 404）
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/me/api-keys",
        &alice.session,
        &alice.csrf,
        json!({
            "name": "alice key",
            "scopes": ["me:read"],
            "client_request_id": "k3"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let alice_key_id = body["id"].as_str().unwrap().to_string();
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/me/api-keys/{alice_key_id}"),
        &bob.session,
        &bob.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "撤销他人密钥必须 404");
    let still_alive = count(
        &pool,
        &format!(
            "SELECT COUNT(*) FROM api_keys WHERE id = '{alice_key_id}' AND revoked_at IS NULL"
        ),
    )
    .await;
    assert_eq!(still_alive, 1, "他人撤销无效");

    // bob 列表看不到 alice 的密钥
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/api-keys",
        &bob.session,
        &bob.csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["items"].as_array().unwrap().len(),
        0,
        "密钥列表按用户隔离"
    );

    // 未认证 → 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me/api-keys")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/me/api-keys")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}
