//! M02-IDENTITY-05：注册在同一事务创建 pending 用户、一次性验证 token（hash）、
//! 审计与验证邮件 Outbox；唯一约束冲突整事务回滚（无半完成状态）；
//! token 只以 hash 入库，Outbox payload 只含引用不含明文。

mod common;

use std::path::{Path, PathBuf};
use std::time::Instant;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::{register_user, RegisterUserError};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::domain::registration::{validate_register, RegisterRequest};
use bblbb_backend::jobs::payload::validate_mail_payload;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-reg-tx-{}", uuid::Uuid::now_v7()));
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

async fn table_count(pool: &DatabasePool, table: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn valid_reg(
    username: &str,
    email: &str,
) -> bblbb_backend::domain::registration::NormalizedRegistration {
    validate_register(&RegisterRequest {
        username: username.to_string(),
        email: email.to_string(),
        password: "passw0rd9".to_string(),
    })
    .unwrap()
}

/// 发送一次注册请求；`ip` 用于模拟客户端地址（x-forwarded-for 首跳）。
async fn post_register(
    app: &Router,
    username: &str,
    email: &str,
    ip: &str,
) -> axum::response::Response {
    // M02-SESSION-08：注册属预认证写路径，必须先获取匿名预认证 CSRF 状态
    let (cookie, csrf) = common::fetch_preauth(app).await;
    let body = json!({
        "username": username,
        "email": email,
        "password": "passw0rd9",
    });
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

/// 成功路径：同一事务写入用户 + token（hash）+ 审计 + Outbox，且 token
/// 只存 hash、Outbox payload 只含引用（M02-IDENTITY-05 契约）。
#[tokio::test]
async fn register_creates_user_token_audit_and_outbox_in_one_tx() {
    let (pool, dir) = pool_with_migrations().await;

    let outcome = register_user(
        &pool,
        &valid_reg(" Alice ", "Alice@Example.COM"),
        "req-reg-1",
    )
    .await
    .expect("注册事务必须成功");

    // 1) 用户：pending + 规范化列
    assert_eq!(table_count(&pool, "users").await, 1);
    let (username, email, status, phash, verified): (String, String, String, String, i64) =
        match &pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT username_normalized, email_normalized, status, password_hash, email_verified FROM users",
            )
            .fetch_one(p)
            .await
            .unwrap(),
            Either::Right(_) => panic!("SQLite only"),
        };
    assert_eq!(username, "alice", "用户名必须规范化入库");
    assert_eq!(email, "alice@example.com", "邮箱必须规范化入库");
    assert_eq!(status, "pending");
    assert_eq!(verified, 0);
    assert!(phash.starts_with("$argon2id$"), "密码必须为 Argon2id PHC");
    assert!(!phash.contains("passw0rd9"), "密码明文不得入库");

    // 2) 验证 token：只存 64 位 hex hash，且与用户关联
    assert_eq!(table_count(&pool, "email_verification_tokens").await, 1);
    let (token_hash, user_id, consumed_at): (String, String, Option<i64>) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT token_hash, user_id, consumed_at FROM email_verification_tokens")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(user_id, outcome.user_id);
    assert_eq!(consumed_at, None);
    assert_eq!(token_hash.len(), 64, "SHA-256 hex 长度");
    assert!(
        token_hash.chars().all(|c| c.is_ascii_hexdigit()),
        "token 列必须是 hex hash，而非可逆明文"
    );

    // 3) 审计：同事务写入注册审计
    assert_eq!(table_count(&pool, "audit_logs").await, 1);
    let (action, target_type, target_id, request_id): (String, String, String, String) = match &pool
    {
        Either::Left(p) => {
            sqlx::query_as("SELECT action, target_type, target_id, request_id FROM audit_logs")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(action, "auth.register");
    assert_eq!(target_type, "user");
    assert_eq!(target_id, outcome.user_id);
    assert_eq!(request_id, "req-reg-1");

    // 4) Outbox：user.registered.v1 事件，payload 只含 token 引用
    assert_eq!(table_count(&pool, "outbox_events").await, 1);
    let (event_type, payload): (String, String) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT event_type, payload FROM outbox_events")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(event_type, "user.registered.v1");
    assert_eq!(outcome.event_id.len(), 36, "UUID v7 事件 ID");
    let payload: Value = serde_json::from_str(&payload).unwrap();
    assert_eq!(
        payload["email_verification_token_id"],
        outcome.verify_token_id
    );
    assert_eq!(payload["user_id"], outcome.user_id);
    assert!(
        validate_mail_payload(&payload).is_ok(),
        "Outbox payload 不得携带明文 token（M01-JOBS-12 契约）"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 重复用户名：唯一约束冲突 → AlreadyExists，且整事务回滚，四表无新增。
#[tokio::test]
async fn register_duplicate_username_rolls_back_entire_transaction() {
    let (pool, dir) = pool_with_migrations().await;

    register_user(&pool, &valid_reg("alice", "alice@example.com"), "req-1")
        .await
        .expect("首次注册必须成功");

    // 相同规范化用户名、不同邮箱、不同密码 → 唯一约束冲突
    let err = register_user(&pool, &valid_reg("ALICE", "other@example.com"), "req-2")
        .await
        .unwrap_err();
    assert!(
        matches!(err, RegisterUserError::AlreadyExists),
        "必须报告 AlreadyExists（不泄漏是用户名还是邮箱），实际: {err}"
    );

    // 无半完成状态：四表都保持首次注册后的计数
    assert_eq!(table_count(&pool, "users").await, 1);
    assert_eq!(table_count(&pool, "email_verification_tokens").await, 1);
    assert_eq!(table_count(&pool, "audit_logs").await, 1);
    assert_eq!(table_count(&pool, "outbox_events").await, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 重复邮箱：同样整事务回滚，无半完成状态。
#[tokio::test]
async fn register_duplicate_email_rolls_back_entire_transaction() {
    let (pool, dir) = pool_with_migrations().await;

    register_user(&pool, &valid_reg("alice", "alice@example.com"), "req-1")
        .await
        .expect("首次注册必须成功");

    let err = register_user(&pool, &valid_reg("bob", "Alice@Example.com"), "req-2")
        .await
        .unwrap_err();
    assert!(matches!(err, RegisterUserError::AlreadyExists));

    assert_eq!(table_count(&pool, "users").await, 1);
    assert_eq!(table_count(&pool, "email_verification_tokens").await, 1);
    assert_eq!(table_count(&pool, "audit_logs").await, 1);
    assert_eq!(table_count(&pool, "outbox_events").await, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// HTTP 层：注册成功与重复注册返回完全一致的 201 响应（不泄漏标识已存在）。
#[tokio::test]
async fn register_endpoint_returns_unified_response() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let first = post_register(&app, "alice", "alice@example.com", "198.51.100.7").await;
    assert_eq!(first.status(), StatusCode::CREATED);
    let first_body = first.into_body().collect().await.unwrap().to_bytes();

    let second = post_register(&app, "ALICE", "other@example.com", "198.51.100.7").await;
    assert_eq!(second.status(), StatusCode::CREATED);
    let second_body = second.into_body().collect().await.unwrap().to_bytes();

    assert_eq!(
        first_body, second_body,
        "重复注册必须与成功注册返回相同响应（防枚举）"
    );
    assert_eq!(&first_body[..], br#"{"ok":true}"#);

    let err_body = post_register(&app, "bob", "not-an-email", "198.51.100.7").await;
    assert_eq!(
        err_body.status(),
        StatusCode::BAD_REQUEST,
        "非法请求仍要 400"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 每 IP 限流（M02-IDENTITY-06）：同一 IP 第 4 次注册 → 429 `rate_limited`，
/// 带 `Retry-After` 与 `RateLimit-Limit/Remaining/Reset` 头。
#[tokio::test]
async fn register_ip_rate_limit_returns_429_with_headers() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    // 前 3 次（不同账号、同一 IP）成功
    for i in 0..3 {
        let resp = post_register(
            &app,
            &format!("user{i}"),
            &format!("user{i}@example.com"),
            "198.51.100.1",
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED, "第 {} 次应放行", i + 1);
    }

    // 第 4 次 → 429 + 限流头 + 契约错误码
    let resp = post_register(&app, "user4", "user4@example.com", "198.51.100.1").await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
    let retry_after = resp
        .headers()
        .get("retry-after")
        .expect("429 必须带 Retry-After");
    assert!(retry_after.to_str().unwrap().parse::<u64>().unwrap() >= 1);
    assert_eq!(resp.headers().get("ratelimit-limit").unwrap(), "3");
    assert_eq!(resp.headers().get("ratelimit-remaining").unwrap(), "0");
    let reset = resp
        .headers()
        .get("ratelimit-reset")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        reset.parse::<i64>().unwrap() > 0,
        "RateLimit-Reset 为 Unix 秒"
    );

    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["code"], "rate_limited", "错误码须与 OpenAPI 契约一致");
    assert_eq!(body["status"], 429);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 账号维度限流：同一规范化邮箱（不同 IP）第 4 次 → 429，即使 IP 额度未耗尽。
#[tokio::test]
async fn register_account_rate_limit_is_per_normalized_email() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    // 同一邮箱 spam@example.com，每次换 IP（各 IP 只消耗 1/3 额度）
    for i in 0..3 {
        let resp = post_register(
            &app,
            &format!("spam{i}"),
            "Spam@Example.com",
            &format!("203.0.113.{i}"),
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED, "第 {} 次应放行", i + 1);
    }

    // 新 IP（IP 额度未用）→ 仍因账号维度命中 429
    let resp = post_register(&app, "spam4", "spam@example.com", "203.0.113.99").await;
    assert_eq!(
        resp.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "账号维度额度耗尽必须 429"
    );
    assert_eq!(resp.headers().get("ratelimit-limit").unwrap(), "3");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 不同 IP 的额度相互独立（不误伤共享出口）。
#[tokio::test]
async fn register_rate_limit_is_isolated_per_ip() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    for i in 0..3 {
        let resp = post_register(
            &app,
            &format!("aaa{i}"),
            &format!("aaa{i}@example.com"),
            "10.0.0.1",
        )
        .await;
        assert_eq!(resp.status(), StatusCode::CREATED);
    }
    // 10.0.0.1 已满
    let resp = post_register(&app, "aaa9", "aaa9@example.com", "10.0.0.1").await;
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);

    // 其他 IP 不受影响
    let resp = post_register(&app, "bbb0", "bbb0@example.com", "10.0.0.2").await;
    assert_eq!(resp.status(), StatusCode::CREATED);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 耗时不泄漏：已存在账号路径仍执行完整 Argon2id 哈希（不短路），
/// 防止未来把哈希移到唯一约束检查之后造成账号枚举时序侧信道。
#[tokio::test]
async fn duplicate_register_does_not_short_circuit_expensive_hash() {
    let (pool, dir) = pool_with_migrations().await;

    register_user(&pool, &valid_reg("alice", "alice@example.com"), "req-1")
        .await
        .expect("首次注册必须成功");

    // 已存在路径（唯一约束冲突）耗时下限：若短路（不哈希）会是微秒级。
    // Argon2id m=19456 在 debug 构建下实测 ≥10ms；取 5ms 保守下限。
    let start = Instant::now();
    let err = register_user(&pool, &valid_reg("alice", "again@example.com"), "req-2")
        .await
        .unwrap_err();
    let elapsed_ms = start.elapsed().as_millis() as i64;
    assert!(
        matches!(err, RegisterUserError::AlreadyExists),
        "已存在路径必须报告 AlreadyExists"
    );
    assert!(
        elapsed_ms >= 5,
        "已存在路径必须执行完整 Argon2 哈希（防枚举时序泄漏），实际耗时 {elapsed_ms}ms"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
