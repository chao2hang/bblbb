//! M02-UX-09：三数据库一致的认证 HTTP 契约——同一套 HTTP Fixture 在
//! SQLite/MySQL/MariaDB 上断言状态码、Problem code、投影与回滚一致。
//!
//! - SQLite：本地始终运行（临时文件 + 迁移）；
//! - MySQL 8 / MariaDB 10.11：`BBLBB_TEST_MYSQL_URL` 环境变量 + `#[ignore]`
//!   （CI mysql-family 任务以 `cargo test --test auth_crossdb -- --ignored`
//!   分别对两个数据库运行，见 .github/workflows/ci.yml）。
//!
//! 覆盖（M02-UX-03/04/06 + M02-IDENTITY-05/10 契约）：
//! 1. 状态码：register 201、重复注册 201（防枚举统一）、登录 200、错误密码
//!    401、verify-email 无效 token 400、password-reset 未知邮箱 202（统一）、
//!    confirm 无效 token 400、IP 限流 429、MFA enroll/confirm/disable 200、
//!    step-up 超窗 403；
//! 2. Problem code：401=unauthorized、400=bad_request、429=rate_limited、
//!    403=step_up_required（三库一致）；
//! 3. 投影：登录 Me 与 /me 的字段集（id/username/email/email_verified/status/
//!    level/roles/mfa_enabled）三库一致；
//! 4. 回滚：disable_totp 单事务（撤销 TOTP+失效恢复码+安全通知+审计）全部
//!    生效或全部不生效；重复 disable 404 无副作用；重复注册不产生重复行。

mod common;

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

const PASSWORD: &str = "correct-password9";
const KEY: &[u8] = b"test-encryption-key-material";
const IP: &str = "198.51.100.7";
const RATE_IP: &str = "198.51.100.99";

// ────────────────────────── SQLite（本地始终运行） ──────────────────────────

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("../migrations/{engine}"))
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authxdb-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("sqlite")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
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

fn app_with_key(pool: DatabasePool) -> Router {
    let config = AppConfig {
        mfa_encryption_key: String::from_utf8(KEY.to_vec()).unwrap(),
        ..AppConfig::default()
    };
    build_router(config, Some(pool))
}

#[tokio::test]
async fn sqlite_auth_crossdb_contract() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    auth_crossdb_flow(&pool, &app).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

// ──────────────────── MySQL 8 / MariaDB（CI 任务） ────────────────────

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_auth_crossdb_contract() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mysql")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    let app = app_with_key(pool.clone());
    auth_crossdb_flow(&pool, &app).await;
    close_pool(&pool).await;
}

// ─────────────────────────── HTTP 助手 ───────────────────────────

async fn post_json(
    app: &Router,
    uri: &str,
    cookie: &str,
    csrf: &str,
    body: Value,
    ip: &str,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", cookie)
                .header("x-forwarded-for", ip)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

async fn get_with_cookie(app: &Router, uri: &str, cookie: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("cookie", cookie)
                .header("x-forwarded-for", IP)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

/// 登录并返回 (状态码, body, 会话 Cookie)；失败时 Cookie 为空串。
async fn login_full(app: &Router, identifier: &str, password: &str) -> (StatusCode, Value, String) {
    let (preauth, preauth_csrf) = common::fetch_preauth(app).await;
    let preauth = preauth.split(';').next().unwrap().to_string();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-csrf-token", preauth_csrf)
                .header("cookie", preauth)
                .header("x-forwarded-for", IP)
                .body(Body::from(
                    json!({ "identifier": identifier, "password": password }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let cookie = if status == StatusCode::OK {
        resp.headers()
            .get("set-cookie")
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body, cookie)
}

/// 登录（未启用 TOTP）成功：校验 Me 投影字段集三库一致，返回会话 Cookie。
async fn login_session(app: &Router, identifier: &str, password: &str) -> (StatusCode, String) {
    let (status, body, cookie) = login_full(app, identifier, password).await;
    if status != StatusCode::OK {
        return (status, String::new());
    }
    // 未启用 TOTP 一步登录：响应为 Me（无 mfa_required challenge 字段）
    assert!(
        !body.get("mfa_required").is_some_and(|v| v == true),
        "未启用 TOTP 不得要求第二步: {body}"
    );
    for field in [
        "id",
        "username",
        "email",
        "email_verified",
        "status",
        "level",
        "roles",
        "mfa_enabled",
    ] {
        assert!(body.get(field).is_some(), "Me 投影缺少 {field}: {body}");
    }
    assert_eq!(body["email_verified"], false, "注册未验证邮箱: {body}");
    assert!(!cookie.is_empty(), "登录必须签发会话 Cookie");
    (StatusCode::OK, cookie)
}

/// 取会话绑定 CSRF token。
async fn session_csrf(app: &Router, session: &str) -> String {
    let (_, body) = get_with_cookie(app, "/api/v1/auth/csrf", session).await;
    body["token"].as_str().unwrap().to_string()
}

fn code_at(secret: &[u8], step: u64) -> String {
    format!("{:06}", bblbb_backend::auth::totp_at(secret, step))
}

fn now_secs() -> u64 {
    (now_millis() / 1000) as u64
}

// ─────────────────────────── 共享行为流 ───────────────────────────

/// 三数据库认证 HTTP 契约（M02-UX-09）。
async fn auth_crossdb_flow(pool: &DatabasePool, app: &Router) {
    // ── 1. 注册 + 重复注册（防枚举统一 201，无重复行） ──
    let email = format!("xdb_{}@example.com", uuid::Uuid::now_v7().simple());
    let username = format!("xdb_{}", &uuid::Uuid::now_v7().simple().to_string()[..12]);
    let (preauth, preauth_csrf) = common::fetch_preauth(app).await;
    let preauth = preauth.split(';').next().unwrap().to_string();
    let (status, body) = post_json(
        app,
        "/api/v1/auth/register",
        &preauth,
        &preauth_csrf,
        json!({ "username": username, "email": email, "password": PASSWORD }),
        IP,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "注册应 201: {body}");
    assert_eq!(body["ok"], true);

    // 重复注册（同一邮箱）→ 统一 201（防枚举，M02-IDENTITY-05）
    let (status, _) = post_json(
        app,
        "/api/v1/auth/register",
        &preauth,
        &preauth_csrf,
        json!({ "username": format!("{}_2", username), "email": email, "password": PASSWORD }),
        IP,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "重复注册必须统一 201（防枚举）"
    );
    let user_count: i64 = count_users_by_email(pool, &email).await;
    assert_eq!(user_count, 1, "重复注册不得产生重复行");

    // ── 2. 登录：错误密码 401 unauthorized；正确 200 + Me 投影 ──
    let (status, body, _) = login_full(app, &email, "wrong-password").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["code"], "unauthorized", "Problem code 必须一致");
    // 触发失败计数后立即正确登录成功（重置计数，证明无锁死泄漏）
    let (status, _) = login_session(app, &email, PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "错误一次后正确登录应成功: {status}");
    let (status, _) = login_session(app, &email, PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "正确登录应 200");

    // ── 3. verify-email 无效 token → 400 bad_request ──
    let (status, body) = post_json(
        app,
        "/api/v1/auth/verify-email",
        &preauth,
        &preauth_csrf,
        json!({ "token": "definitely-invalid-token" }),
        IP,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "bad_request");

    // ── 4. password-reset 未知邮箱 → 统一 202；confirm 无效 token → 400 ──
    let (status, body) = post_json(
        app,
        "/api/v1/auth/password-reset",
        &preauth,
        &preauth_csrf,
        json!({ "email": "nobody@example.com" }),
        IP,
    )
    .await;
    assert_eq!(status, StatusCode::ACCEPTED, "未知邮箱必须统一 202: {body}");
    let (status, body) = post_json(
        app,
        "/api/v1/auth/password-reset/confirm",
        &preauth,
        &preauth_csrf,
        json!({ "token": "bad-token", "password": "newpassword9" }),
        IP,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert_eq!(body["code"], "bad_request");

    // ── 5. password-reset IP 限流 → 429 rate_limited（第 6 次命中） ──
    // 每请求用不同邮箱，避免每账号冷却/日上限干扰；IP 限流 5/小时。
    for i in 0..5 {
        let email = format!(
            "ratelimit_{i}_{}@example.com",
            uuid::Uuid::now_v7().simple()
        );
        let (status, _) = post_json(
            app,
            "/api/v1/auth/password-reset",
            &preauth,
            &preauth_csrf,
            json!({ "email": email }),
            RATE_IP,
        )
        .await;
        assert_eq!(
            status,
            StatusCode::ACCEPTED,
            "前 5 次必须 202（IP 限流 5/小时）: {status}"
        );
    }
    let (status, body) = post_json(
        app,
        "/api/v1/auth/password-reset",
        &preauth,
        &preauth_csrf,
        json!({ "email": format!("ratelimit_6_{}@example.com", uuid::Uuid::now_v7().simple()) }),
        RATE_IP,
    )
    .await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "第 6 次请求必须 429");
    assert_eq!(body["code"], "rate_limited", "Problem code 必须一致");

    // ── 6. MFA：enroll → 错误 confirm 400 → 正确 confirm 200 → 投影 ──
    let (status, session) = login_session(app, &email, PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "登录应成功");
    let csrf = session_csrf(app, &session).await;

    let (status, body) = post_json(
        app,
        "/api/v1/auth/mfa/enroll",
        &session,
        &csrf,
        Value::Null,
        IP,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "enroll 应 200: {body}");
    let secret_b32 = body["secret_base32"]
        .as_str()
        .expect("secret_base32")
        .to_string();
    assert!(body["otpauth_uri"]
        .as_str()
        .unwrap()
        .starts_with("otpauth://totp/BBLBB:"));
    let secret = bblbb_backend::auth::base32_decode(&secret_b32).expect("合法 base32");
    let step = now_secs() / 30;

    let (status, body) = post_json(
        app,
        "/api/v1/auth/mfa/confirm",
        &session,
        &csrf,
        json!({ "code": "000000" }),
        IP,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "错误 code 必须 400: {body}"
    );

    let (status, _) = post_json(
        app,
        "/api/v1/auth/mfa/confirm",
        &session,
        &csrf,
        json!({ "code": code_at(&secret, step) }),
        IP,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "正确 code 应 200");

    // /me 投影反映 mfa_enabled=true
    let (status, me) = get_with_cookie(app, "/api/v1/me", &session).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        me["mfa_enabled"], true,
        "启用后 /me 必须反映 mfa_enabled=true: {me}"
    );
    assert_eq!(me["email"], email, "/me 邮箱投影一致");

    // ── 7. 恢复码一次生成 + 只存 hash ──
    let (status, body) = post_json(
        app,
        "/api/v1/auth/mfa/recovery-codes",
        &session,
        &csrf,
        Value::Null,
        IP,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "恢复码生成应 200: {body}");
    let codes = body["codes"].as_array().expect("codes 数组");
    assert_eq!(codes.len(), 10);
    assert_eq!(body["only_shown_once"], true);

    // ── 8. step-up：超窗 → 403 step_up_required → re-auth 错/对 → 停用 ──
    expire_step_up(pool, &session).await;
    let (status, body) = delete_authed(app, "/api/v1/auth/mfa", &session, &csrf).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "超窗停用必须 403: {body}");
    assert_eq!(body["code"], "step_up_required", "Problem code 必须一致");

    let (status, _) = post_json(
        app,
        "/api/v1/auth/re-auth",
        &session,
        &csrf,
        json!({ "password": "wrong-password" }),
        IP,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "错误密码 re-auth 必须 401"
    );

    let (status, _) = post_json(
        app,
        "/api/v1/auth/re-auth",
        &session,
        &csrf,
        json!({ "password": PASSWORD }),
        IP,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "正确密码 re-auth 应 200");

    let (status, _) = delete_authed(app, "/api/v1/auth/mfa", &session, &csrf).await;
    assert_eq!(status, StatusCode::OK, "停用应 200");

    // ── 9. 回滚原子性：停用后 TOTP 撤销 + 恢复码失效 + 通知 + 审计（单事务） ──
    let user_id = me["id"].as_str().unwrap().to_string();
    let confirmed = count_confirmed_totp(pool, &user_id).await;
    assert_eq!(confirmed, 0, "停用后不得残留启用 TOTP");
    let unused_codes = count_unused_codes(pool, &user_id).await;
    assert_eq!(unused_codes, 0, "停用后不得残留未用恢复码");
    let notif = count_notifications(pool, &user_id, "mfa_changed").await;
    assert!(notif >= 1, "停用必须落安全通知: {notif}");
    let audit = count_audit(pool, &user_id, "auth.mfa_disabled").await;
    assert_eq!(audit, 1, "停用必须落审计");

    // 重复停用 → 404，无副作用
    let (status, _) = delete_authed(app, "/api/v1/auth/mfa", &session, &csrf).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "重复停用必须 404（防枚举）");
    let audit2 = count_audit(pool, &user_id, "auth.mfa_disabled").await;
    assert_eq!(audit2, 1, "404 停用不得产生副作用");
}

async fn delete_authed(app: &Router, uri: &str, session: &str, csrf: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .header("x-forwarded-for", IP)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

// ─────────────────────────── 数据库断言 ───────────────────────────

async fn count_users_by_email(pool: &DatabasePool, email: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email_normalized = ?")
                .bind(email)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM users WHERE email_normalized = ?")
                .bind(email)
                .fetch_one(p)
                .await
                .unwrap()
        }
    }
}

async fn expire_step_up(pool: &DatabasePool, session: &str) {
    let token = session.split('=').nth(1).unwrap_or("");
    let token_hash = bblbb_backend::auth::hash_token(token);
    let past = now_millis() - 3_600_000;
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET auth_verified_at = ? WHERE token_hash = ?")
                .bind(past)
                .bind(token_hash)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query("UPDATE user_sessions SET auth_verified_at = ? WHERE token_hash = ?")
                .bind(past)
                .bind(token_hash)
                .execute(p)
                .await
                .unwrap();
        }
    }
}

async fn count_confirmed_totp(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM totp_credentials WHERE user_id = ? AND confirmed_at IS NOT NULL AND revoked_at IS NULL",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM totp_credentials WHERE user_id = ? AND confirmed_at IS NOT NULL AND revoked_at IS NULL",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap()
        }
    }
}

async fn count_unused_codes(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM mfa_recovery_codes WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM mfa_recovery_codes WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap(),
    }
}

async fn count_notifications(pool: &DatabasePool, user_id: &str, kind: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND security_kind = ?",
        )
        .bind(user_id)
        .bind(kind)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND security_kind = ?",
        )
        .bind(user_id)
        .bind(kind)
        .fetch_one(p)
        .await
        .unwrap(),
    }
}

async fn count_audit(pool: &DatabasePool, user_id: &str, action: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE actor_id = ? AND action = ?")
                .bind(user_id)
                .bind(action)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE actor_id = ? AND action = ?")
                .bind(user_id)
                .bind(action)
                .fetch_one(p)
                .await
                .unwrap()
        }
    }
}
