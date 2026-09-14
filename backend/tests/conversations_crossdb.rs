//! M17-GAPFIX-07：私信发布级跨数据库契约。
//!
//! SQLite 始终运行；MySQL 8 / MariaDB 10.11 由 CI 的 mysql-family 矩阵以
//! `BBLBB_TEST_MYSQL_URL` + `--ignored` 运行。共享行为流覆盖：
//! - 双人会话并发 find-or-create（不会产生重复会话）；
//! - CSRF、Idempotency-Key、幂等重放/冲突和参与者范围；
//! - 通知恰好一次（重放不重复通知）与 `other` 隐私投影；
//! - 同毫秒复合游标以及并发发送后的分页不重不漏。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{header::CACHE_CONTROL, HeaderMap, Request, StatusCode},
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

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("../migrations/{engine}"))
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-dm-xdb-{}", uuid::Uuid::now_v7()));
    let pool = create_pool(&format!("sqlite://{}", dir.display()))
        .await
        .unwrap();
    let files = read_migration_files(&migrations_dir("sqlite")).unwrap();
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    let sql = "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
               VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
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
        Either::Right(p) => {
            sqlx::query(sql)
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
    }
    (user_id, username)
}

async fn session_csrf(app: &Router, session: &str) -> String {
    let response = app
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
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    body["token"].as_str().unwrap().to_owned()
}

async fn request_json(
    app: &Router,
    method: &str,
    uri: &str,
    session: Option<&str>,
    csrf: Option<&str>,
    idempotency_key: Option<&str>,
    body: Value,
) -> (StatusCode, HeaderMap, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(session) = session {
        builder = builder.header("cookie", session);
    }
    if let Some(csrf) = csrf {
        builder = builder.header("x-csrf-token", csrf);
    }
    if let Some(key) = idempotency_key {
        builder = builder.header("idempotency-key", key);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, headers, body)
}

fn assert_private_no_store(headers: &HeaderMap) {
    assert_eq!(
        headers
            .get(CACHE_CONTROL)
            .and_then(|value| value.to_str().ok()),
        Some("private, no-store")
    );
}

struct UserCtx {
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

async fn notification_count(pool: &DatabasePool, user_id: &str) -> i64 {
    let sql = "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND type = 'mention' AND link = '/messages'";
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
    }
}

async fn scalar_count(pool: &DatabasePool, sql: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        Either::Right(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
    }
}

async fn message_count(pool: &DatabasePool, conversation_id: &str) -> i64 {
    let sql = "SELECT COUNT(*) FROM messages WHERE conversation_id = ?";
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .bind(conversation_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar(sql)
            .bind(conversation_id)
            .fetch_one(p)
            .await
            .unwrap(),
    }
}

async fn execute(pool: &DatabasePool, sql: &str, first: i64, second: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(first)
                .bind(second)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(first)
                .bind(second)
                .execute(p)
                .await
                .unwrap();
        }
    }
}

async fn conversation_crossdb_flow(pool: &DatabasePool, app: &Router) {
    let alice = user_ctx(app, pool, "xdb_alice").await;
    let bob = user_ctx(app, pool, "xdb_bob").await;
    let carol = user_ctx(app, pool, "xdb_carol").await;

    let create_body = json!({
        "username": bob.username,
        "client_request_id": "xdb-create-alice-0001"
    });
    for csrf in [None, Some("wrong-csrf-token")] {
        let (status, _, _) = request_json(
            app,
            "POST",
            "/api/v1/conversations",
            Some(&alice.session),
            csrf,
            Some("xdb-create-alice-0001"),
            create_body.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _, _) = request_json(
        app,
        "POST",
        "/api/v1/conversations",
        Some(&alice.session),
        Some(&alice.csrf),
        None,
        create_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = request_json(
        app,
        "POST",
        "/api/v1/conversations",
        Some(&alice.session),
        Some(&alice.csrf),
        Some("xdb-create-other-header-0001"),
        create_body,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 双方同时点击「发私信」时，MySQL/MariaDB 依赖用户行锁；SQLite 由
    // BEGIN IMMEDIATE 提供同等的 find-or-create 原子性。
    let (alice_create, bob_create) = tokio::join!(
        request_json(
            app,
            "POST",
            "/api/v1/conversations",
            Some(&alice.session),
            Some(&alice.csrf),
            Some("xdb-create-alice-0001"),
            json!({ "username": bob.username, "client_request_id": "xdb-create-alice-0001" }),
        ),
        request_json(
            app,
            "POST",
            "/api/v1/conversations",
            Some(&bob.session),
            Some(&bob.csrf),
            Some("xdb-create-bob-0001"),
            json!({ "username": alice.username, "client_request_id": "xdb-create-bob-0001" }),
        )
    );
    assert_eq!(
        alice_create.0,
        StatusCode::CREATED,
        "alice create: {:?}",
        alice_create.2
    );
    assert_eq!(
        bob_create.0,
        StatusCode::CREATED,
        "bob create: {:?}",
        bob_create.2
    );
    assert_private_no_store(&alice_create.1);
    assert_private_no_store(&bob_create.1);
    let conversation_id = alice_create.2["id"].as_str().unwrap().to_owned();
    assert_eq!(bob_create.2["id"], conversation_id.as_str());
    assert!(alice_create.2["other"].get("email").is_none());
    assert!(bob_create.2["other"].get("email").is_none());
    assert_eq!(
        scalar_count(pool, "SELECT COUNT(*) FROM conversations").await,
        1
    );

    let message_body = json!({
        "body": "跨库私信",
        "client_request_id": "xdb-message-0000001"
    });
    for csrf in [None, Some("wrong-csrf-token")] {
        let (status, _, _) = request_json(
            app,
            "POST",
            &format!("/api/v1/conversations/{conversation_id}/messages"),
            Some(&alice.session),
            csrf,
            Some("xdb-message-0000001"),
            message_body.clone(),
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _, _) = request_json(
        app,
        "POST",
        &format!("/api/v1/conversations/{conversation_id}/messages"),
        Some(&alice.session),
        Some(&alice.csrf),
        None,
        message_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _, _) = request_json(
        app,
        "POST",
        &format!("/api/v1/conversations/{conversation_id}/messages"),
        Some(&alice.session),
        Some(&alice.csrf),
        Some("xdb-message-other-header-0001"),
        message_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let (status, headers, first_message) = request_json(
        app,
        "POST",
        &format!("/api/v1/conversations/{conversation_id}/messages"),
        Some(&alice.session),
        Some(&alice.csrf),
        Some("xdb-message-0000001"),
        message_body.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_private_no_store(&headers);
    let notifications_after_first = notification_count(pool, &bob.id).await;
    assert_eq!(notifications_after_first, 1);

    let (status, headers, replayed) = request_json(
        app,
        "POST",
        &format!("/api/v1/conversations/{conversation_id}/messages"),
        Some(&alice.session),
        Some(&alice.csrf),
        Some("xdb-message-0000001"),
        message_body,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_private_no_store(&headers);
    assert_eq!(replayed["id"], first_message["id"]);
    assert_eq!(
        notification_count(pool, &bob.id).await,
        notifications_after_first
    );

    let (status, _, _) = request_json(
        app,
        "POST",
        &format!("/api/v1/conversations/{conversation_id}/messages"),
        Some(&alice.session),
        Some(&alice.csrf),
        Some("xdb-message-0000001"),
        json!({ "body": "不同请求", "client_request_id": "xdb-message-0000001" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    let (status, headers, bob_conversations) = request_json(
        app,
        "GET",
        "/api/v1/conversations",
        Some(&bob.session),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_private_no_store(&headers);
    assert_eq!(bob_conversations["items"].as_array().unwrap().len(), 1);
    assert!(bob_conversations["items"][0]["other"]
        .get("email")
        .is_none());
    assert_eq!(bob_conversations["items"][0]["unread_count"], 1);

    let (status, headers, bob_messages) = request_json(
        app,
        "GET",
        &format!("/api/v1/conversations/{conversation_id}/messages"),
        Some(&bob.session),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_private_no_store(&headers);
    assert_eq!(bob_messages["items"].as_array().unwrap().len(), 1);

    // 把三条消息压到同一毫秒，验证 `(created_at,id)` 游标在两种 SQL
    // 方言上都不会漏掉或重复记录。
    for (key, body) in [
        ("xdb-message-0000002", "同毫秒二"),
        ("xdb-message-0000003", "同毫秒三"),
    ] {
        let (status, _, _) = request_json(
            app,
            "POST",
            &format!("/api/v1/conversations/{conversation_id}/messages"),
            Some(&alice.session),
            Some(&alice.csrf),
            Some(key),
            json!({ "body": body, "client_request_id": key }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED);
    }
    let fixed_at = now_millis();
    execute(
        pool,
        "UPDATE messages SET created_at = ? WHERE conversation_id = ?",
        fixed_at,
        &conversation_id,
    )
    .await;
    execute(
        pool,
        "UPDATE conversations SET last_message_at = ? WHERE id = ?",
        fixed_at,
        &conversation_id,
    )
    .await;
    assert_eq!(message_count(pool, &conversation_id).await, 3);

    let (status, headers, first_page) = request_json(
        app,
        "GET",
        &format!("/api/v1/conversations/{conversation_id}/messages?limit=2"),
        Some(&alice.session),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_private_no_store(&headers);
    let first_ids: Vec<String> = first_page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(first_ids.len(), 2);
    let cursor = first_page["next_cursor"].as_str().unwrap();
    let (status, _, second_page) = request_json(
        app,
        "GET",
        &format!("/api/v1/conversations/{conversation_id}/messages?limit=2&after={cursor}"),
        Some(&alice.session),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let second_ids: Vec<String> = second_page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(second_ids.len(), 1);
    assert!(first_ids.iter().all(|id| !second_ids.contains(id)));
    assert!(second_page["next_cursor"].is_null());

    // 新会话中的并发发送也必须各自落库，且 limit=1 的连续读取不能重复。
    let (status, _, carol_conversation) = request_json(
        app,
        "POST",
        "/api/v1/conversations",
        Some(&alice.session),
        Some(&alice.csrf),
        Some("xdb-create-carol-0001"),
        json!({ "username": carol.username, "client_request_id": "xdb-create-carol-0001" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let carol_conversation_id = carol_conversation["id"].as_str().unwrap().to_owned();
    let carol_messages_uri = format!("/api/v1/conversations/{carol_conversation_id}/messages");
    let (send_a, send_b) = tokio::join!(
        request_json(
            app,
            "POST",
            &carol_messages_uri,
            Some(&alice.session),
            Some(&alice.csrf),
            Some("xdb-concurrent-message-0001"),
            json!({ "body": "并发 A", "client_request_id": "xdb-concurrent-message-0001" }),
        ),
        request_json(
            app,
            "POST",
            &carol_messages_uri,
            Some(&alice.session),
            Some(&alice.csrf),
            Some("xdb-concurrent-message-0002"),
            json!({ "body": "并发 B", "client_request_id": "xdb-concurrent-message-0002" }),
        )
    );
    assert_eq!(
        send_a.0,
        StatusCode::CREATED,
        "concurrent A: {:?}",
        send_a.2
    );
    assert_eq!(
        send_b.0,
        StatusCode::CREATED,
        "concurrent B: {:?}",
        send_b.2
    );
    let (status, _, page) = request_json(
        app,
        "GET",
        &format!("{carol_messages_uri}?limit=1"),
        Some(&alice.session),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let first_id = page["items"][0]["id"].as_str().unwrap().to_owned();
    let cursor = page["next_cursor"].as_str().unwrap();
    let (status, _, page) = request_json(
        app,
        "GET",
        &format!("{carol_messages_uri}?limit=1&after={cursor}"),
        Some(&alice.session),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_ne!(page["items"][0]["id"], first_id.as_str());
    assert!(page["next_cursor"].is_null());

    // 非参与者对三类资源操作统一 404，不泄露会话存在性。
    let (status, _, _) = request_json(
        app,
        "GET",
        &format!("/api/v1/conversations/{conversation_id}/messages"),
        Some(&carol.session),
        None,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _, _) = request_json(
        app,
        "POST",
        &format!("/api/v1/conversations/{conversation_id}/messages"),
        Some(&carol.session),
        Some(&carol.csrf),
        Some("xdb-intruder-message-0001"),
        json!({ "body": "越权", "client_request_id": "xdb-intruder-message-0001" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    for csrf in [None, Some("wrong-csrf-token")] {
        let (status, _, _) = request_json(
            app,
            "POST",
            &format!("/api/v1/conversations/{conversation_id}/read"),
            Some(&bob.session),
            csrf,
            Some("xdb-read-00000001"),
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::FORBIDDEN);
    }
    let (status, _, _) = request_json(
        app,
        "POST",
        &format!("/api/v1/conversations/{conversation_id}/read"),
        Some(&bob.session),
        Some(&bob.csrf),
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, headers, _) = request_json(
        app,
        "POST",
        &format!("/api/v1/conversations/{conversation_id}/read"),
        Some(&bob.session),
        Some(&bob.csrf),
        Some("xdb-read-00000001"),
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    assert_private_no_store(&headers);
}

#[tokio::test]
async fn sqlite_conversations_crossdb_contract() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    conversation_crossdb_flow(&pool, &app).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_family_conversations_crossdb_contract() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let engine = common::mysql_family_migrations_dir(&pool).await;
    let files = read_migration_files(&migrations_dir(engine)).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    conversation_crossdb_flow(&pool, &app).await;
    close_pool(&pool).await;
}
