//! M02-UX-03：两步登录 MFA——密码验证后签发一次性 challenge，第二步用
//! TOTP code 或恢复码完成登录（原子消费 challenge、会话 auth_verified_at=now）。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::mfa_login::{complete_mfa_login, MfaLoginError, MFA_CHALLENGE_TTL_MS};
use bblbb_backend::auth::{
    base32_decode, begin_enrollment, confirm_enrollment, has_confirmed_totp, login_user,
    start_mfa_login, LoginLimits, TOTP_PERIOD_SECS,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::ratelimit::RateLimiter;
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
    let dir = std::env::temp_dir().join(format!("bblbb-mfalogin-{}", uuid::Uuid::now_v7()));
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

/// 插入可登录用户（status='active'），返回 (user_id, email_normalized)。
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
            .bind(format!("{tag}_user"))
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

/// 为用户启用 TOTP，返回 (secret, confirm_step)。
async fn enabled_totp(pool: &DatabasePool, user_id: &str) -> (Vec<u8>, u64) {
    let challenge = begin_enrollment(pool, user_id, "BBLBB", "u@example.com", KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let confirm_step = now_secs() / TOTP_PERIOD_SECS;
    confirm_enrollment(
        pool,
        user_id,
        &code_at(&secret, confirm_step),
        KEY,
        now_secs(),
    )
    .await
    .unwrap();
    (secret, confirm_step)
}

/// challenge 行的消费状态：(consumed_at IS NOT NULL, expires_at)。
async fn challenge_state(pool: &DatabasePool, challenge_token: &str) -> (bool, i64) {
    let token_hash = bblbb_backend::auth::hash_token(challenge_token);
    let (consumed, expires): (i64, i64) = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT consumed_at IS NOT NULL, expires_at FROM mfa_login_challenges WHERE token_hash = ?",
        )
        .bind(token_hash)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    (consumed != 0, expires)
}

// ─────────────────────────── start_mfa_login ───────────────────────────

/// 签发一次性 challenge：返回明文 token，数据库只存 hash；有效期 5 分钟。
#[tokio::test]
async fn start_issues_one_time_token_storing_only_hash() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "alice").await;

    let challenge = start_mfa_login(&pool, &user_id).await.unwrap();
    assert!(!challenge.is_empty());

    let (consumed, expires) = challenge_state(&pool, &challenge).await;
    assert!(!consumed);
    let ttl = expires - now_millis();
    assert!(
        ttl > 0 && ttl <= MFA_CHALLENGE_TTL_MS,
        "expires_at 应为 now + 5 分钟: {ttl}"
    );

    // 数据库不存明文 token
    let raw_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM mfa_login_challenges WHERE token_hash = ?")
                .bind(&challenge)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(raw_count, 0, "必须只存 token_hash，不存明文");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 密码步：启用 TOTP 的用户 login_user 返回 mfa_required=true 且不签发会话。
#[tokio::test]
async fn password_step_signals_mfa_required_without_session() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "bob").await;
    assert!(!has_confirmed_totp(&pool, &user_id).await.unwrap());
    enabled_totp(&pool, &user_id).await;

    let outcome = login_user(
        &pool,
        &RateLimiter::new(),
        &email,
        PASSWORD,
        "127.0.0.1",
        None,
        "req-1",
        &LoginLimits::default(),
    )
    .await
    .unwrap();
    assert!(outcome.mfa_required, "启用 TOTP 必须要求第二步");
    assert!(outcome.session_token.is_empty(), "密码步不得签发会话");

    // 未启用 TOTP 的用户：正常一步登录
    let (_, email2) = insert_login_user(&pool, "bobby").await;
    let outcome2 = login_user(
        &pool,
        &RateLimiter::new(),
        &email2,
        PASSWORD,
        "127.0.0.1",
        None,
        "req-2",
        &LoginLimits::default(),
    )
    .await
    .unwrap();
    assert!(!outcome2.mfa_required);
    assert!(!outcome2.session_token.is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── complete_mfa_login ───────────────────────────

/// TOTP 完成登录：签发会话、消费 challenge、会话即刻满足 step-up。
#[tokio::test]
async fn complete_with_totp_issues_session_and_consumes_challenge() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "carol").await;
    let (secret, _) = enabled_totp(&pool, &user_id).await;

    let challenge = start_mfa_login(&pool, &user_id).await.unwrap();
    // complete_mfa_login 内部用系统当前时间；TOTP 窗口 ±1 步，
    // 用当前步 +1 的 code（> last_accepted_step，且仍在窗口内）
    let code = code_at(&secret, (now_secs() / TOTP_PERIOD_SECS) + 1);

    let completed = complete_mfa_login(&pool, &challenge, Some(&code), None, KEY, "req-complete")
        .await
        .unwrap();
    assert!(!completed.session_token.is_empty());
    assert_eq!(completed.user_id, user_id);

    let (consumed, _) = challenge_state(&pool, &challenge).await;
    assert!(consumed, "challenge 必须一次性消费");

    // 会话有效且无需 step-up（auth_verified_at=now）
    assert!(!bblbb_backend::auth::is_step_up_required_for_session(
        &pool,
        &completed.session_token,
        300
    )
    .await
    .unwrap());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 恢复码完成登录：签发会话 + 消费 challenge + 恢复码原子消费。
#[tokio::test]
async fn complete_with_recovery_code_issues_session() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "dave").await;
    enabled_totp(&pool, &user_id).await;
    let codes = bblbb_backend::auth::generate_recovery_codes(&pool, &user_id, 10, "req-1")
        .await
        .unwrap();

    let challenge = start_mfa_login(&pool, &user_id).await.unwrap();
    let completed = complete_mfa_login(
        &pool,
        &challenge,
        None,
        Some(&codes[0]),
        KEY,
        "req-complete",
    )
    .await
    .unwrap();
    assert!(!completed.session_token.is_empty());

    let (consumed, _) = challenge_state(&pool, &challenge).await;
    assert!(consumed);

    // 同一恢复码不可再用
    let challenge2 = start_mfa_login(&pool, &user_id).await.unwrap();
    let err = complete_mfa_login(&pool, &challenge2, None, Some(&codes[0]), KEY, "req-2")
        .await
        .unwrap_err();
    assert!(matches!(err, MfaLoginError::InvalidCode), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 同一 challenge 重复完成 → 第二次 InvalidChallenge（防重放）。
#[tokio::test]
async fn challenge_replay_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "erin").await;
    enabled_totp(&pool, &user_id).await;
    let challenge = start_mfa_login(&pool, &user_id).await.unwrap();

    // 第一次：正确 TOTP code
    let code1 = code_at(
        &secret_of(&pool, &user_id).await,
        (now_secs() / TOTP_PERIOD_SECS) + 1,
    );
    let r1 = complete_mfa_login(&pool, &challenge, Some(&code1), None, KEY, "req-1").await;
    assert!(r1.is_ok(), "{r1:?}");

    // 第二次（新 code，同一 challenge）→ InvalidChallenge（已消费）
    let r2 = complete_mfa_login(&pool, &challenge, Some(&code1), None, KEY, "req-2").await;
    let err = r2.unwrap_err();
    assert!(matches!(err, MfaLoginError::InvalidChallenge), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 过期 challenge → InvalidChallenge。
#[tokio::test]
async fn expired_challenge_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "frank").await;
    enabled_totp(&pool, &user_id).await;
    let challenge = start_mfa_login(&pool, &user_id).await.unwrap();

    // 把 expires_at 回溯到过去
    let token_hash = bblbb_backend::auth::hash_token(&challenge);
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE mfa_login_challenges SET expires_at = ? WHERE token_hash = ?")
                .bind(now_millis() - 1)
                .bind(token_hash)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let code = code_at(
        &secret_of(&pool, &user_id).await,
        (now_secs() / TOTP_PERIOD_SECS) + 1,
    );
    let err = complete_mfa_login(&pool, &challenge, Some(&code), None, KEY, "req-1")
        .await
        .unwrap_err();
    assert!(matches!(err, MfaLoginError::InvalidChallenge), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误 TOTP code → InvalidCode。
#[tokio::test]
async fn wrong_code_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "grace").await;
    enabled_totp(&pool, &user_id).await;
    let challenge = start_mfa_login(&pool, &user_id).await.unwrap();

    let err = complete_mfa_login(&pool, &challenge, Some("000000"), None, KEY, "req-1")
        .await
        .unwrap_err();
    assert!(matches!(err, MfaLoginError::InvalidCode), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 都不给 / 都给 code → InvalidCode（二选一）。
#[tokio::test]
async fn both_or_neither_code_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "heidi").await;
    enabled_totp(&pool, &user_id).await;
    let challenge = start_mfa_login(&pool, &user_id).await.unwrap();

    let err = complete_mfa_login(&pool, &challenge, None, None, KEY, "req-1")
        .await
        .unwrap_err();
    assert!(matches!(err, MfaLoginError::InvalidCode), "{err:?}");

    let challenge2 = start_mfa_login(&pool, &user_id).await.unwrap();
    let err = complete_mfa_login(
        &pool,
        &challenge2,
        Some("123456"),
        Some("SOMECODE123"),
        KEY,
        "req-2",
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MfaLoginError::InvalidCode), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 并发完成同一 challenge + 同一 TOTP step：恰好一个成功。
#[tokio::test]
async fn concurrent_same_challenge_only_one_succeeds() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "ivan").await;
    let (secret, _) = enabled_totp(&pool, &user_id).await;
    let challenge = start_mfa_login(&pool, &user_id).await.unwrap();
    let code = code_at(&secret, (now_secs() / TOTP_PERIOD_SECS) + 1);
    let code1 = code.clone();
    let code2 = code.clone();

    let p1 = pool.clone();
    let p2 = pool.clone();
    let c1 = challenge.clone();
    let c2 = challenge.clone();
    let k1 = KEY.to_vec();
    let k2 = KEY.to_vec();
    let (r1, r2) = tokio::join!(
        async move { complete_mfa_login(&p1, &c1, Some(&code1), None, &k1, "req-1").await },
        async move { complete_mfa_login(&p2, &c2, Some(&code2), None, &k2, "req-2").await },
    );
    let ok_count = [r1, r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 1, "同一 challenge/step 并发必须恰好一个成功");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 读取已启用 TOTP 的明文 secret（测试助手：解密 DB 中密文）。
async fn secret_of(pool: &DatabasePool, user_id: &str) -> Vec<u8> {
    let blob: String = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT encrypted_secret FROM totp_credentials
             WHERE user_id = ? AND confirmed_at IS NOT NULL AND revoked_at IS NULL LIMIT 1",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    bblbb_backend::auth::decrypt_secret(KEY, &blob).expect("解密 TOTP secret")
}

// ─────────────────────────── HTTP 流程 ───────────────────────────

fn app_with_key(pool: DatabasePool) -> Router {
    let config = AppConfig {
        mfa_encryption_key: String::from_utf8(KEY.to_vec()).unwrap(),
        ..AppConfig::default()
    };
    build_router(config, Some(pool))
}

async fn post_json(
    app: &Router,
    uri: &str,
    cookie: &str,
    token: &str,
    body: Value,
) -> (StatusCode, axum::response::Response) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", token)
                .header("cookie", cookie)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    (status, resp)
}

/// 端到端：启用 TOTP 的用户两步登录。
#[tokio::test]
async fn login_flow_two_step_over_http() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "judy").await;
    let (secret, _) = enabled_totp(&pool, &user_id).await;
    let app = app_with_key(pool.clone());

    let (cookie, csrf) = common::fetch_preauth(&app).await;

    // 第一步：密码 → mfa_required + challenge_token（无会话 Cookie）
    let (status, resp) = post_json(
        &app,
        "/api/v1/auth/login",
        &cookie,
        &csrf,
        json!({ "identifier": email, "password": PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(
        body["mfa_required"], true,
        "启用 TOTP 必须要求第二步: {body}"
    );
    let challenge_token = body["challenge_token"].as_str().unwrap().to_string();
    assert!(!challenge_token.is_empty());

    // 第二步：TOTP code → 200 + 会话 Cookie + Me
    let code = code_at(&secret, (now_secs() / TOTP_PERIOD_SECS) + 1);
    let (status, resp) = post_json(
        &app,
        "/api/v1/auth/login/mfa",
        &cookie,
        &csrf,
        json!({ "challenge_token": challenge_token, "totp_code": code }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let set_cookie = resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        set_cookie.contains("__Host-bblbb_session="),
        "第二步必须签发会话 Cookie: {set_cookie}"
    );
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["username"], "judy_user");

    // 会话可用：GET /api/v1/me 200
    let me_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me")
                .header("cookie", &set_cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(me_resp.status(), StatusCode::OK);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误 TOTP → 401 统一 invalid credentials（不泄漏细节）。
#[tokio::test]
async fn login_mfa_wrong_code_returns_401() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "kevin").await;
    enabled_totp(&pool, &user_id).await;
    let app = app_with_key(pool.clone());
    let (cookie, csrf) = common::fetch_preauth(&app).await;

    let (status, resp) = post_json(
        &app,
        "/api/v1/auth/login",
        &cookie,
        &csrf,
        json!({ "identifier": email, "password": PASSWORD }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "密码步必须成功");
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let challenge_token = body["challenge_token"].as_str().unwrap().to_string();

    let (status, resp) = post_json(
        &app,
        "/api/v1/auth/login/mfa",
        &cookie,
        &csrf,
        json!({ "challenge_token": challenge_token, "totp_code": "000000" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(
        !body["detail"].as_str().unwrap_or("").contains("TOTP"),
        "错误信息不得泄漏第二因素细节"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
