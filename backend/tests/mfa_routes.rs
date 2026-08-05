//! M02-UX-06：MFA 管理 HTTP 路由——enrollment（enroll/confirm/cancel）、
//! 恢复码一次展示、停用 MFA 与 re-auth step-up 交互。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::{
    base32_decode, begin_enrollment, confirm_enrollment, has_confirmed_totp, hash_token,
    TOTP_PERIOD_SECS,
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

mod common;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";
const KEY: &[u8] = b"test-encryption-key-material";
const PASSWORD: &str = "correct-password";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-mfaroutes-{}", uuid::Uuid::now_v7()));
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

async fn insert_login_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let hash = bblbb_backend::auth::hash_password(PASSWORD).unwrap();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(&email)
            .bind(&email)
            .bind(&hash)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (user_id, email)
}

fn now_secs() -> u64 {
    (now_millis() / 1000) as u64
}

fn code_at(secret: &[u8], step: u64) -> String {
    format!("{:06}", bblbb_backend::auth::totp_at(secret, step))
}

fn app_with_key(pool: DatabasePool) -> Router {
    let config = AppConfig {
        mfa_encryption_key: String::from_utf8(KEY.to_vec()).unwrap(),
        ..AppConfig::default()
    };
    build_router(config, Some(pool))
}

/// 未启用 TOTP 用户通过 HTTP 登录，返回会话 Cookie（Set-Cookie 值）。
async fn login_session_cookie(app: &Router, email: &str) -> String {
    let (cookie, csrf) = common::fetch_preauth(app).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({ "identifier": email, "password": PASSWORD }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "未启用 TOTP 一步登录应 200");
    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .expect("登录必须签发会话 Cookie")
        .to_str()
        .unwrap()
        .to_string();
    // 只保留 __Host-bblbb_session=...; 段
    set_cookie
        .split(';')
        .next()
        .expect("Set-Cookie 第一段为 name=value")
        .to_string()
}

/// 携带会话 Cookie 获取会话绑定 synchronizer token。
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

async fn send_authed(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-csrf-token", csrf)
        .header("cookie", session);
    let req = match body {
        Some(value) => builder.body(Body::from(value.to_string())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

/// 让当前会话的 auth_verified_at 回溯超窗（模拟 step-up 过期）。
async fn expire_step_up(pool: &DatabasePool, session: &str) {
    let token_hash = hash_token(&session[session.find('=').map(|i| i + 1).unwrap_or(0)..]);
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET auth_verified_at = ? WHERE token_hash = ?")
                .bind(now_millis() - 3_600_000)
                .bind(token_hash)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

// ─────────────────────────── enrollment ───────────────────────────

/// 完整 enrollment 流程：登录 → enroll（otpauth+secret）→ 正确 code confirm。
#[tokio::test]
async fn enroll_confirm_flow_over_http() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "alice").await;
    assert!(!has_confirmed_totp(&pool, &user_id).await.unwrap());
    let app = app_with_key(pool.clone());
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;

    // enroll → 二维码最小数据
    let (status, body) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/mfa/enroll",
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let secret_b32 = body["secret_base32"]
        .as_str()
        .expect("secret_base32")
        .to_string();
    let uri = body["otpauth_uri"].as_str().expect("otpauth_uri");
    assert!(uri.starts_with("otpauth://totp/BBLBB:"), "{uri}");
    assert_eq!(body["issuer"], "BBLBB");
    assert_eq!(body["account"], email);

    let secret = base32_decode(&secret_b32).expect("合法 base32");
    let code = code_at(&secret, now_secs() / TOTP_PERIOD_SECS);

    // confirm → 启用
    let (status, body) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/mfa/confirm",
        &session,
        &csrf,
        Some(json!({ "code": code })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(has_confirmed_totp(&pool, &user_id).await.unwrap());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误 code → 400（统一，不泄漏校验细节）。
#[tokio::test]
async fn confirm_wrong_code_rejected_400() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, email) = insert_login_user(&pool, "bob").await;
    let app = app_with_key(pool.clone());
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;

    let (status, body) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/mfa/enroll",
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let _ = body;

    let (status, body) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/mfa/confirm",
        &session,
        &csrf,
        Some(json!({ "code": "000000" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 取消未完成 enrollment；无 pending → 404。
#[tokio::test]
async fn cancel_enrollment_and_404() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, email) = insert_login_user(&pool, "carol").await;
    let app = app_with_key(pool.clone());
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;

    // 无 pending → 404
    let (status, _) = send_authed(
        &app,
        "DELETE",
        "/api/v1/auth/mfa/enrollment",
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // enroll → cancel → 200
    let (status, _) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/mfa/enroll",
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = send_authed(
        &app,
        "DELETE",
        "/api/v1/auth/mfa/enrollment",
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未认证 → 401。
#[tokio::test]
async fn enroll_requires_auth() {
    let (pool, dir) = pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    let (status, _) = send_authed(&app, "POST", "/api/v1/auth/mfa/enroll", "", "x", None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── 恢复码 ───────────────────────────

/// 恢复码一次生成：只展示一次（响应明文），DB 只存 hash。
#[tokio::test]
async fn recovery_codes_generated_once() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "dave").await;
    let app = app_with_key(pool.clone());
    // 先登录（会话 auth_verified_at=now，step-up 即刻满足），再启用 TOTP
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBL", &email, KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;
    confirm_enrollment(&pool, &user_id, &code_at(&secret, step), KEY, now_secs())
        .await
        .unwrap();

    let (status, body) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/mfa/recovery-codes",
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let codes = body["codes"].as_array().expect("codes 数组");
    assert_eq!(codes.len(), 10, "默认 10 个恢复码");
    for code in codes {
        let c = code.as_str().unwrap();
        assert_eq!(c.len(), 16, "恢复码为 16 位 base32");
        assert!(c
            .chars()
            .all(|ch| "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567".contains(ch)));
    }

    // DB 只存 hash：明文不落库
    let raw_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM mfa_recovery_codes WHERE code_hash = ?")
                .bind(codes[0].as_str().unwrap())
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(raw_count, 0, "恢复码明文不得入库");
    let hash_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM mfa_recovery_codes WHERE user_id = ?")
                .bind(&user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(hash_count, 10, "应存 10 条 hash");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── step-up 与停用 ───────────────────────────

/// 停用 MFA 需要近期认证：超窗 → 403 step_up_required → re-auth（错/对密码）
/// → 停用成功。
#[tokio::test]
async fn disable_requires_step_up_and_reauth() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "erin").await;
    let app = app_with_key(pool.clone());
    // 先登录再启用 TOTP（登录后会话 step-up 即刻满足）
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBL", &email, KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;
    confirm_enrollment(&pool, &user_id, &code_at(&secret, step), KEY, now_secs())
        .await
        .unwrap();

    // 超窗 → 停用被拒 403 step_up_required
    expire_step_up(&pool, &session).await;
    let (status, body) =
        send_authed(&app, "DELETE", "/api/v1/auth/mfa", &session, &csrf, None).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(
        body["code"], "step_up_required",
        "必须用可识别 code 通知前端"
    );

    // 错误密码 re-auth → 401
    let (status, body) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/re-auth",
        &session,
        &csrf,
        Some(json!({ "password": "wrong-password" })),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");

    // 正确密码 re-auth → 200
    let (status, body) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/re-auth",
        &session,
        &csrf,
        Some(json!({ "password": PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    // 停用 → 200，TOTP 撤销 + 恢复码失效
    let (status, body) =
        send_authed(&app, "DELETE", "/api/v1/auth/mfa", &session, &csrf, None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!has_confirmed_totp(&pool, &user_id).await.unwrap());
    let active_codes: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM mfa_recovery_codes WHERE user_id = ? AND consumed_at IS NULL",
        )
        .bind(&user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(active_codes, 0, "停用 MFA 必须失效全部未用恢复码");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 恢复码生成同样受 step-up 保护。
#[tokio::test]
async fn recovery_codes_requires_step_up() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "frank").await;
    let app = app_with_key(pool.clone());
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBL", &email, KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;
    confirm_enrollment(&pool, &user_id, &code_at(&secret, step), KEY, now_secs())
        .await
        .unwrap();

    expire_step_up(&pool, &session).await;
    let (status, body) = send_authed(
        &app,
        "POST",
        "/api/v1/auth/mfa/recovery-codes",
        &session,
        &csrf,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert_eq!(body["code"], "step_up_required");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未启用 TOTP 时停用 → 404（防枚举）。
#[tokio::test]
async fn disable_when_not_enabled_404() {
    let (pool, dir) = pool_with_migrations().await;
    let (_, email) = insert_login_user(&pool, "grace").await;
    let app = app_with_key(pool.clone());
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;

    let (status, _) = send_authed(&app, "DELETE", "/api/v1/auth/mfa", &session, &csrf, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 服务层直连验证：disable_totp 单事务（撤销 TOTP + 失效恢复码 + 通知）。
#[tokio::test]
async fn disable_totp_service_atomic() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "henry").await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBL", &email, KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;
    confirm_enrollment(&pool, &user_id, &code_at(&secret, step), KEY, now_secs())
        .await
        .unwrap();
    bblbb_backend::auth::generate_recovery_codes(&pool, &user_id, 10, "req-1")
        .await
        .unwrap();

    let disabled = bblbb_backend::auth::disable_totp(&pool, &user_id, "req-2")
        .await
        .unwrap();
    assert!(disabled, "已启用 TOTP 应返回 true");

    // 安全通知与审计落库
    let notif_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = ? AND security_kind = 'mfa_changed'")
                .bind(&user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        notif_count >= 1,
        "停用必须发 mfa_changed 安全通知: {notif_count}"
    );
    let audit_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE actor_id = ? AND action = 'auth.mfa_disabled'",
        )
        .bind(&user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit_count, 1, "停用必须写 auth.mfa_disabled 审计");

    // 再次调用 → false（无启用 TOTP）
    let again = bblbb_backend::auth::disable_totp(&pool, &user_id, "req-3")
        .await
        .unwrap();
    assert!(!again);

    close_pool(&pool).await;
    cleanup(&dir);
}
