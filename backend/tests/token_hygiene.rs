//! M02-IDENTITY-11：验证/重置 token 只以 hash 入库，不出现在 API 响应、
//! 日志、审计 metadata、Outbox payload 与错误诊断中。
//!
//! 方法：测试自己生成**已知明文 token**（DB 只存其 SHA-256 hash），走完整
//! 流程后断言该明文 token 不出现在任何表文本列、任何 API 响应、任何审计
//! metadata、任何 Outbox payload；错误路径 detail 为固定文案，`redact_token`
//! 对含 token 的日志文本脱敏（M01-JOBS-12 机制）。

mod common;

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::token::{generate_token, hash_token};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::payload::redact_token;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::json;
use sqlx::{Either, Row};
use tower::ServiceExt;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-tokhid-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

/// 断言 `cols` 列在 `table` 中的全部值（含 metadata/payload JSON 文本）
/// 都不包含 `token`。
async fn assert_table_columns_token_free(
    pool: &DatabasePool,
    table: &str,
    cols: &[&str],
    token: &str,
) {
    let select = cols.join(", ");
    let rows: Vec<String> = match pool {
        Either::Left(p) => {
            let q = format!("SELECT {select} FROM {table}");
            sqlx::query(&q)
                .fetch_all(p)
                .await
                .unwrap_or_else(|e| panic!("查询 {table} 失败: {e}"))
                .iter()
                .map(|row| {
                    (0..cols.len())
                        .map(|i| match row.try_get::<Option<String>, _>(i) {
                            Ok(Some(v)) => v,
                            Ok(None) => String::new(),
                            Err(_) => String::new(),
                        })
                        .collect::<Vec<_>>()
                        .join("|")
                })
                .collect()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    for (i, text) in rows.iter().enumerate() {
        assert!(
            !text.contains(token),
            "表 {table} 第 {i} 行泄漏明文 token: {text}"
        );
    }
}

/// 断言响应体不包含 token。
async fn body_without_token(response: axum::response::Response, token: &str) -> String {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let text = String::from_utf8_lossy(&bytes).to_string();
    assert!(!text.contains(token), "API 响应泄漏明文 token: {text}");
    text
}

/// 插入一个 pending 用户 + 已知验证 token（DB 只存 hash），返回 (email, token)。
async fn setup_pending_user_with_verify_token(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let now = now_millis();
    let token = generate_token();
    let token_hash = hash_token(&token);
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'pending', ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_user"))
            .bind(&email)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(&token_hash)
            .bind(now + 24 * 60 * 60 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (email, token)
}

/// 发送一次注册请求。
async fn post_register(
    app: &Router,
    username: &str,
    email: &str,
    ip: &str,
) -> axum::response::Response {
    // M02-SESSION-08：注册属预认证写路径，必须先获取匿名预认证 CSRF 状态
    let (cookie, csrf) = common::fetch_preauth(app).await;
    let body = json!({ "username": username, "email": email, "password": "passw0rd9" });
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .header("x-forwarded-for", ip)
                .header("cookie", cookie)
                .header("x-csrf-token", csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap()
}

/// 注册流程：DB 只存 64 位 hex hash，API 响应不含任何 token 形态。
#[tokio::test]
async fn registration_stores_only_hash_and_response_is_token_free() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let resp = post_register(&app, "alice", "alice@example.com", "198.51.100.1").await;
    assert_eq!(resp.status(), StatusCode::CREATED);
    let body = body_without_token(resp, "not-a-token").await;
    assert_eq!(body, r#"{"ok":true}"#);

    // token 列只存 64 位 hex hash（不是 43 字符 base64 token 形态）
    let token_hashes: Vec<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT token_hash FROM email_verification_tokens")
            .fetch_all(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(token_hashes.len(), 1);
    assert_eq!(token_hashes[0].len(), 64);
    assert!(token_hashes[0].chars().all(|c| c.is_ascii_hexdigit()));

    // hash 只出现在 token 表：业务/日志表（users/outbox/audit）不得含它
    for (table, cols) in [
        (
            "users",
            vec!["username_normalized", "email_normalized", "password_hash"],
        ),
        ("outbox_events", vec!["payload"]),
        ("audit_logs", vec!["metadata", "reason", "action"]),
    ] {
        assert_table_columns_token_free(&pool, table, &cols, &token_hashes[0]).await;
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 已知验证 token：不出现在 DB、verify API 响应、审计 metadata 与 Outbox。
#[tokio::test]
async fn known_verify_token_absent_from_db_api_audit_outbox() {
    let (pool, dir) = pool_with_migrations().await;
    let (email, token) = setup_pending_user_with_verify_token(&pool, "bob").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    // 验证前：token 表只存 hash，明文 token 不出现在任何表
    assert_table_columns_token_free(
        &pool,
        "users",
        &["username_normalized", "email_normalized", "password_hash"],
        &token,
    )
    .await;
    assert_table_columns_token_free(&pool, "email_verification_tokens", &["token_hash"], &token)
        .await;

    // verify 成功 → 响应、审计、事件都不含明文 token
    // M02-SESSION-08：verify-email 属预认证写路径，必须先获取预认证 CSRF
    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/verify-email")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.2")
                .header("cookie", cookie)
                .header("x-csrf-token", csrf)
                .body(Body::from(json!({ "token": &token }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    body_without_token(resp, &token).await;

    assert_table_columns_token_free(
        &pool,
        "audit_logs",
        &["metadata", "reason", "action"],
        &token,
    )
    .await;
    assert_table_columns_token_free(&pool, "outbox_events", &["payload"], &token).await;
    // token 消费后仍只存 hash
    assert_table_columns_token_free(&pool, "email_verification_tokens", &["token_hash"], &token)
        .await;

    // email 也不应泄露 token 之外多余信息（响应固定）
    let _ = email;

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 重发流程：Outbox payload 只含 token 引用（token_id），无明文；审计无 token。
#[tokio::test]
async fn resend_flow_payload_and_audit_are_token_free() {
    let (pool, dir) = pool_with_migrations().await;
    let (email, token) = setup_pending_user_with_verify_token(&pool, "carol").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    // M02-SESSION-08：重发属预认证写路径，必须先获取预认证 CSRF 状态
    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/resend-verification")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.3")
                .header("cookie", cookie)
                .header("x-csrf-token", csrf)
                .body(Body::from(json!({ "email": &email }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    body_without_token(resp, &token).await;

    assert_table_columns_token_free(&pool, "outbox_events", &["payload"], &token).await;
    assert_table_columns_token_free(
        &pool,
        "audit_logs",
        &["metadata", "reason", "action"],
        &token,
    )
    .await;

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 找回密码流程：请求与确认的响应、Outbox、审计都不含已知 reset token。
#[tokio::test]
async fn password_reset_flow_outputs_are_token_free() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    // active 用户 + 已知 reset token（DB 只存 hash）
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = "dave@example.com".to_string();
    let reset_token = generate_token();
    let reset_hash = hash_token(&reset_token);
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind("dave_user")
            .bind(&email)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(&reset_hash)
            .bind(now + 30 * 60 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 先确认重置（用已知 token）→ 200，响应/审计无明文 token；
    // token 列仍只存 hash
    // M02-SESSION-08：confirm 属预认证写路径，必须先获取预认证 CSRF
    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/password-reset/confirm")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.4")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .body(Body::from(
                    json!({ "token": &reset_token, "password": "new-passw0rd9" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    body_without_token(resp, &reset_token).await;
    assert_table_columns_token_free(
        &pool,
        "audit_logs",
        &["metadata", "reason", "action"],
        &reset_token,
    )
    .await;
    assert_table_columns_token_free(
        &pool,
        "password_reset_tokens",
        &["token_hash"],
        &reset_token,
    )
    .await;

    // 再请求重置 → 202，响应/Outbox/审计无明文 token（此时已知 token 已消费，
    // 新 token 只以引用形式进入 payload）
    // M02-SESSION-08：请求重置属预认证写路径（预认证状态 TTL 内可复用）
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/password-reset")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.4")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .body(Body::from(json!({ "email": &email }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::ACCEPTED);
    body_without_token(resp, &reset_token).await;
    assert_table_columns_token_free(&pool, "outbox_events", &["payload"], &reset_token).await;
    assert_table_columns_token_free(
        &pool,
        "audit_logs",
        &["metadata", "reason", "action"],
        &reset_token,
    )
    .await;

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误路径：无效 token 的 400 detail 为固定文案（不含用户提交的 token）；
/// 含 token 的日志文本经 redact_token 脱敏。
#[tokio::test]
async fn error_paths_and_log_diagnostics_are_token_free() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let bogus = generate_token();

    // confirm 用无效 token → 400，detail 固定，不含提交的 token
    // M02-SESSION-08：confirm 属预认证写路径（预认证状态 TTL 内可复用）
    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/password-reset/confirm")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.5")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .body(Body::from(
                    json!({ "token": &bogus, "password": "new-passw0rd9" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let body = body_without_token(resp, &bogus).await;
    assert!(body.contains("invalid or expired reset token"));

    // verify 用无效 token → 400，detail 固定
    // M02-SESSION-08：verify-email 属预认证写路径（预认证状态 TTL 内可复用）
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/verify-email")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.5")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .body(Body::from(json!({ "token": &bogus }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    body_without_token(resp, &bogus).await;

    // 日志/错误文本脱敏：任何含 token 的日志字符串都必须经 redact_token
    let raw = format!("smtp 550 rejected, token={bogus}");
    let safe = redact_token(&raw);
    assert!(!safe.contains(&bogus));
    assert!(safe.contains("[REDACTED]"));

    close_pool(&pool).await;
    cleanup(&dir);
}
