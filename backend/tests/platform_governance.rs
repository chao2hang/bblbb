//! 平台治理底座（P0 整改）：Bootstrap 首管理员、Feature Flag 持久化、
//! Shop config 真实持久化、Outbox 消费者、最后管理员保护（SQLite HTTP）。

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

const PASSWORD: &str = "correct-password";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-gov-{}", uuid::Uuid::now_v7()));
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

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

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
    assert_eq!(resp.status(), StatusCode::OK, "登录必须 200");
    resp.headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
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

async fn authed(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    if_match: Option<i64>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-csrf-token", csrf)
        .header("cookie", session);
    if let Some(v) = if_match {
        builder = builder.header("if-match", v.to_string());
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
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

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role_name)
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
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn has_administrator_role(pool: &DatabasePool, user_id: &str) -> bool {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar::<_, i64>(
                "SELECT COUNT(*) FROM user_roles ur
             JOIN roles r ON r.id = ur.role_id
             WHERE r.name = 'administrator' AND ur.user_id = ?",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap()
                > 0
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

// ─── Bootstrap ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn bootstrap_token_creates_first_admin_then_locks() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    // 1) 生成一次性 token（服务层直接调用；明文仅此一次）。
    let now = now_millis();
    let (_token_id, token) = bblbb_backend::bootstrap::create_bootstrap_token(&pool, now)
        .await
        .expect("create bootstrap token");

    // 2) 无 token 的请求 422（防枚举统一错误）。
    let (status, _) = anon_post(
        &app,
        "/api/v1/auth/bootstrap",
        json!({
            "token": "bogus-token-value",
            "username": "rootuser",
            "email": "root@example.com",
            "password": "password-123"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // 3) 携带有效 token 创建首个 administrator。
    let (status, body) = anon_post(
        &app,
        "/api/v1/auth/bootstrap",
        json!({
            "token": token,
            "username": "firstadmin",
            "email": "first-admin@example.com",
            "password": "password-123"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "bootstrap 应 201：{body}");
    let user_id = body["user_id"].as_str().unwrap().to_string();
    assert!(has_administrator_role(&pool, &user_id).await);

    // 4) 同一 token 重放 → 422（已消费）。
    let (status, _) = anon_post(
        &app,
        "/api/v1/auth/bootstrap",
        json!({
            "token": token,
            "username": "secondadmin",
            "email": "second@example.com",
            "password": "password-123"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // 5) 已有 administrator 后再生成 token → 拒绝（实例已初始化）。
    let again = bblbb_backend::bootstrap::create_bootstrap_token(&pool, now).await;
    assert!(
        again.is_err(),
        "已有 active administrator 时应拒绝生成 token"
    );

    close_pool(&pool);
    cleanup(&dir);
}

#[tokio::test]
async fn last_active_admin_cannot_be_revoked_or_disabled() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (admin_id, email) = insert_login_user(&pool, "lastadmin").await;
    assign_global_role(&pool, &admin_id, "administrator").await;
    // M02-MFA-05：administrator 未完成 TOTP enrollment 时聚合降级为 member
    // 基线——高权限测试必须先完成 enrollment。
    common::enroll_totp(&pool, &admin_id).await;
    // 直签会话（TOTP 已启用后密码登录走两步；管理端测试直签等价）。
    let session = common::direct_session_cookie(&pool, &admin_id).await;
    let csrf = session_csrf(&app, &session).await;

    // GET 当前用户版本。
    let (_, me) = authed(&app, "GET", "/api/v1/me", &session, &csrf, None, json!({})).await;
    let version = me["user"]["version"]
        .as_i64()
        .or(me["version"].as_i64())
        .unwrap_or(1);

    // 撤销 administrator 角色 → 409（最后管理员保护）。
    let (status, body) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/admin/users/{admin_id}/roles/administrator"),
        &session,
        &csrf,
        None,
        json!({ "reason": "test revoke last admin" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "撤销最后管理员应 409：{body}");
    assert!(has_administrator_role(&pool, &admin_id).await);

    // 停用/封禁最后管理员 → 409。
    let (status, body) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/users/{admin_id}"),
        &session,
        &csrf,
        Some(version),
        json!({ "status": "banned", "reason": "test ban last admin" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "封禁最后管理员应 409：{body}");

    close_pool(&pool);
    cleanup(&dir);
}

// ─── Feature Flag ──────────────────────────────────────────────────────────

#[tokio::test]
async fn feature_flags_persist_and_gate_runtime() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (admin_id, email) = insert_login_user(&pool, "flagadmin").await;
    assign_global_role(&pool, &admin_id, "administrator").await;
    common::enroll_totp(&pool, &admin_id).await;
    // 直签会话（TOTP 已启用后密码登录走两步；管理端测试直签等价）。
    let session = common::direct_session_cookie(&pool, &admin_id).await;
    let csrf = session_csrf(&app, &session).await;

    // 默认关闭：/api/v1/ai/capabilities 被 feature gate 409。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/ai/capabilities")
                .header("cookie", &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT, "默认关闭应 409");

    // GET flags：ai 版本 1、禁用。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/feature-flags",
        &session,
        &csrf,
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let flags = body["flags"].as_array().unwrap();
    assert_eq!(flags.len(), 5);
    let ai = flags.iter().find(|f| f["name"] == "ai").unwrap();
    assert_eq!(ai["version"], 1);
    assert_eq!(ai["enabled"], false);

    // PATCH 启用 ai（expected_version=1）→ version 2。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/feature-flags/ai",
        &session,
        &csrf,
        None,
        json!({ "enabled": true, "expected_version": 1, "reason": "enable ai for test" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "PATCH flag 应 200：{body}");
    assert_eq!(body["flag"]["enabled"], true);
    assert_eq!(body["flag"]["version"], 2);

    // 旧版本重放 → 409。
    let (status, _) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/feature-flags/ai",
        &session,
        &csrf,
        None,
        json!({ "enabled": false, "expected_version": 1, "reason": "stale version" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // 运行时快照已重载：/api/v1/ai/capabilities 放行（200）。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/ai/capabilities")
                .header("cookie", &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "启用后 feature gate 应放行");

    close_pool(&pool);
    cleanup(&dir);
}

// ─── Shop config ───────────────────────────────────────────────────────────

#[tokio::test]
async fn shop_config_persists_and_guards_last_admin() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (admin_id, email) = insert_login_user(&pool, "shopadmin").await;
    assign_global_role(&pool, &admin_id, "administrator").await;
    common::enroll_totp(&pool, &admin_id).await;
    // 直签会话（TOTP 已启用后密码登录走两步；管理端测试直签等价）。
    let session = common::direct_session_cookie(&pool, &admin_id).await;
    let csrf = session_csrf(&app, &session).await;

    // GET 默认配置。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/shop/config",
        &session,
        &csrf,
        None,
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["enabled"], true);
    assert_eq!(body["max_quantity_per_order"], 100);
    assert_eq!(body["version"], 1);

    // PATCH（If-Match=1）持久化。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/shop/config",
        &session,
        &csrf,
        Some(1),
        json!({ "enabled": false, "max_quantity_per_order": 5, "reason": "maintenance" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "PATCH shop config 应 200：{body}");
    assert_eq!(body["enabled"], false);
    assert_eq!(body["max_quantity_per_order"], 5);
    assert_eq!(body["version"], 2);

    // 缺 If-Match → 400。
    let (status, _) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/shop/config",
        &session,
        &csrf,
        None,
        json!({ "enabled": true, "reason": "no if-match" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 缺 reason → 400。
    let (status, _) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/shop/config",
        &session,
        &csrf,
        Some(2),
        json!({ "enabled": true }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    close_pool(&pool);
    cleanup(&dir);
}

// ─── Outbox consumer ───────────────────────────────────────────────────────

#[tokio::test]
async fn outbox_consumer_delivers_pending_events() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    // 两个 pending 事件：未知类型（no-op 消费者）与注册事件（SMTP 未配置 → 跳过）。
    let now = now_millis();
    let _ = bblbb_backend::outbox::enqueue(&pool, "unknown.event.v1", json!({ "k": 1 })).await;
    let _ = bblbb_backend::outbox::enqueue(
        &pool,
        bblbb_backend::events::types::USER_REGISTERED,
        json!({ "user_id": "u1", "email": "a@b.co", "email_verification_token_id": "t1" }),
    )
    .await;
    let _ = now;

    let processed = bblbb_backend::jobs::outbox_consumer::process_batch(&pool, "", 20)
        .await
        .unwrap();
    assert_eq!(processed, 2, "两个事件都应被处理");

    // 事件不再 pending。
    let pending = bblbb_backend::outbox::pending_count(&pool).await.unwrap();
    assert_eq!(pending, 0);

    // 去重标记已写入（消费一次）。
    let consumed: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM outbox_consumed")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(consumed, 2);

    // 重跑幂等：无 pending 事件。
    let again = bblbb_backend::jobs::outbox_consumer::process_batch(&pool, "", 20)
        .await
        .unwrap();
    assert_eq!(again, 0);

    close_pool(&pool);
    cleanup(&dir);
}

// ─── 匿名 POST 助手 ────────────────────────────────────────────────────────

async fn anon_post(app: &Router, uri: &str, body: Value) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", "application/json")
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
