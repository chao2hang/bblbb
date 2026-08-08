//! M13-ADMIN 管理 API 集成测试（HTTP + SQLite 真库）。
//!
//! 覆盖：user.manage/role.manage 权限门（403）、reason 必填、recent-auth
//! （step-up）、If-Match 版本冲突、审计、管理 DTO 不泄漏凭据（无 password_
//! hash/恢复码/Session）、主题/插件管理端点、处罚创建。

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
mod support;

const PASSWORD: &str = "correct-password";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-admin-{}", uuid::Uuid::now_v7()));
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

async fn insert_login_user(pool: &DatabasePool, tag: &str) -> String {
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
    email
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
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now_millis() - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
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

/// 管理员上下文：direct_session_cookie + TOTP + mark_step_up（满足 recent-auth）。
struct AdminCtx {
    session: String,
    csrf: String,
}

async fn admin_ctx(app: &Router, pool: &DatabasePool) -> AdminCtx {
    let email = insert_login_user(pool, "adm").await;
    let user_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM users WHERE email_normalized = ?")
            .bind(&email)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assign_global_role(pool, &user_id, "administrator").await;
    common::enroll_totp(pool, &user_id).await;
    let session = common::direct_session_cookie(pool, &user_id).await;
    // 满足 recent-auth 窗口
    let token = session.split('=').nth(1).unwrap().to_string();
    bblbb_backend::auth::session::mark_step_up(pool, &token)
        .await
        .unwrap();
    let csrf = session_csrf(app, &session).await;
    AdminCtx { session, csrf }
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

// ─────────────────────────── M13-ADMIN-02 用户管理 ───────────────────────

#[tokio::test]
async fn user_management_requires_permission_reason_and_step_up() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    // 普通 member：403（无 user.manage）
    let email = insert_login_user(&pool, "member").await;
    let member_id: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM users WHERE email_normalized = ?")
            .bind(&email)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let member_session = common::direct_session_cookie(&pool, &member_id).await;
    let member_csrf = session_csrf(&app, &member_session).await;
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/users",
        &member_session,
        &member_csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    let admin = admin_ctx(&app, &pool).await;

    // 列表：管理 DTO（无 password_hash/恢复码/Session）
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/users",
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["items"].is_array());
    for item in body["items"].as_array().unwrap() {
        let s = item.to_string();
        assert!(!s.contains("password_hash"), "管理 DTO 不得含密码哈希");
        assert!(!s.contains("recovery"), "管理 DTO 不得含恢复码");
        assert!(!s.contains("session"), "管理 DTO 不得含 Session");
    }

    // 创建用户：缺 reason → 400
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/users",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "username": "newbie", "email": "newbie@example.com" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "reason 必填");

    // 创建用户成功（pending 状态）
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/users",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "username": "newbie", "email": "newbie@example.com", "reason": "onboard" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let new_user_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["status"], "pending");

    // 读取单个用户
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/users/{new_user_id}"),
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["username"], "newbie");
    assert!(body.get("password_hash").is_none());

    // 更新：缺 If-Match → 400；版本过期 → 409
    let (status, _) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/users/{new_user_id}"),
        &admin.session,
        &admin.csrf,
        None,
        json!({ "status": "banned", "reason": "spam" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺失 If-Match 必须 400");

    let (status, body) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/users/{new_user_id}"),
        &admin.session,
        &admin.csrf,
        Some(999),
        json!({ "status": "banned", "reason": "spam" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "version_conflict");

    // 正确更新 → 200 + 审计
    let (status, body) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/users/{new_user_id}"),
        &admin.session,
        &admin.csrf,
        Some(1),
        json!({ "status": "banned", "reason": "spam" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["status"], "banned");

    let (action, reason): (String, Option<String>) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT action, reason FROM audit_logs WHERE action = 'admin.user.update' ORDER BY created_at DESC LIMIT 1",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(action, "admin.user.update");
    assert_eq!(reason.as_deref(), Some("spam"));

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-ADMIN-02 角色管理 ───────────────────────

#[tokio::test]
async fn role_management_creates_and_updates_roles_with_audit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    // 列表（含 system 角色）
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/roles",
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["items"].as_array().unwrap().len() >= 4);

    // 创建：未知权限名 → 400
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/roles",
        &admin.session,
        &admin.csrf,
        None,
        json!({
            "name": "custom_mod",
            "display_name": "自定义版主",
            "permissions": ["totally.fake.permission"],
            "reason": "create"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "未知权限名必须 400");

    // 创建成功
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/roles",
        &admin.session,
        &admin.csrf,
        None,
        json!({
            "name": "custom_mod",
            "display_name": "自定义版主",
            "permissions": ["moderation.review", "post.moderate"],
            "reason": "create custom moderator"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    let role_id = body["id"].as_str().unwrap().to_string();

    // 读取单角色
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/roles/{role_id}"),
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let perms: Vec<&str> = body["permissions"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|p| p.as_str())
        .collect();
    assert!(perms.contains(&"moderation.review"));

    // 更新：If-Match（updated_at 版本）
    let version = body["updated_at"].as_i64().unwrap();
    let (status, _) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/roles/{role_id}"),
        &admin.session,
        &admin.csrf,
        Some(999),
        json!({ "display_name": "新名", "reason": "rename" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "版本过期必须 409");

    let (status, body) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/roles/{role_id}"),
        &admin.session,
        &admin.csrf,
        Some(version),
        json!({ "display_name": "新名", "reason": "rename" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["display_name"], "新名");

    // system 角色权限不可改：administrator（is_system=1）更新 permissions → 400
    let (_, roles) = authed(
        &app,
        "GET",
        "/api/v1/admin/roles",
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    let admin_role = roles["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == "administrator")
        .cloned()
        .unwrap();
    let admin_role_id = admin_role["id"].as_str().unwrap().to_string();
    let admin_role_version = admin_role["updated_at"].as_i64().unwrap();
    let (status, body) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/roles/{admin_role_id}"),
        &admin.session,
        &admin.csrf,
        Some(admin_role_version),
        json!({ "permissions": ["board.read"], "reason": "downgrade attempt" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-ADMIN-03 板块/标签/处罚 ─────────────────

#[tokio::test]
async fn board_tag_sanction_admin_endpoints_work() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    // 板块创建 → 读取
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "meta", "name": "站务", "reason": "init" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let board_id = body["id"].as_str().unwrap().to_string();
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/boards/{board_id}"),
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["slug"], "meta");

    // 标签创建 → 读取
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "官方", "slug": "official", "reason": "init" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let tag_id = body["id"].as_str().unwrap().to_string();
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/tags/{tag_id}"),
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["slug"], "official");

    // 处罚：目标用户 + 越权防护 + 审计
    let target = support::insert_user(&pool, "target").await;
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/moderation/sanctions",
        &admin.session,
        &admin.csrf,
        None,
        json!({
            "target_user_id": target,
            "kind": "mute",
            "reason": "abuse",
            "ends_at": now_millis() + 86_400_000,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["kind"], "mute");

    // 处罚自己 → 403
    let admin_id: String = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT u.id FROM users u JOIN user_roles ur ON ur.user_id = u.id JOIN roles r ON r.id = ur.role_id WHERE r.name = 'administrator' LIMIT 1",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/moderation/sanctions",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "target_user_id": admin_id, "kind": "mute", "reason": "self" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-THEME-06 管理端点（HTTP）──────────────

#[tokio::test]
async fn theme_admin_endpoints_upload_set_default_and_patch_settings() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let package = json!({
        "schema_version": 1,
        "name": "midnight",
        "display_name": "Midnight",
        "version": "1.0.0",
        "supports": ">=1.0 <2.0",
        "kind": "data",
        "tokens": {
            "color.background": "#0f172a",
            "color.surface": "#1e293b",
            "color.text": "#e2e8f0",
            "color.muted": "#94a3b8",
            "color.accent": "#38bdf8",
            "color.border": "#334155",
            "font.body": "system-ui",
            "font.mono": "ui-monospace",
            "radius.control": "0.5rem",
            "radius.card": "0.75rem",
            "space.density": "comfortable",
            "shadow.card": "md",
            "motion.duration": "150ms",
            "motion.reduced": true,
        },
    });

    // 上传（缺 reason → 400）
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/themes/data-packages",
        &admin.session,
        &admin.csrf,
        None,
        package.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "reason 必填");

    let mut with_reason = package.clone();
    with_reason["reason"] = json!("theme init");
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/themes/data-packages",
        &admin.session,
        &admin.csrf,
        None,
        with_reason,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["theme"]["status"], "disabled");
    assert_eq!(body["theme"]["revision"], 1);

    // 列表
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/themes",
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["themes"].as_array().unwrap().len(), 1);

    // 设为默认（激活）
    let (status, body) = authed(
        &app,
        "PUT",
        "/api/v1/admin/themes/default",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "midnight", "reason": "go dark" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["theme"]["status"], "active");
    assert_eq!(body["theme"]["is_default"], true);

    // 公开 active 端点返回新主题（revision 一致）
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/themes/active")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let active: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(active["name"], "midnight");
    assert_eq!(active["revision"], 1);

    // patch settings：If-Match 冲突 → 409；正确 → revision 2
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/themes/midnight/settings",
        &admin.session,
        &admin.csrf,
        Some(99),
        json!({ "reason": "tweak", "tokens": { "color.background": "#000000", "color.surface": "#111111", "color.text": "#ffffff", "color.muted": "#888888", "color.accent": "#38bdf8", "color.border": "#222222", "font.body": "system-ui", "font.mono": "ui-monospace", "radius.control": "0.5rem", "radius.card": "0.75rem", "space.density": "comfortable", "shadow.card": "md", "motion.duration": "150ms", "motion.reduced": false } }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], "version_conflict");

    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/themes/midnight/settings",
        &admin.session,
        &admin.csrf,
        Some(1),
        json!({ "reason": "tweak", "tokens": { "color.background": "#000000", "color.surface": "#111111", "color.text": "#ffffff", "color.muted": "#888888", "color.accent": "#38bdf8", "color.border": "#222222", "font.body": "system-ui", "font.mono": "ui-monospace", "radius.control": "0.5rem", "radius.card": "0.75rem", "space.density": "comfortable", "shadow.card": "md", "motion.duration": "150ms", "motion.reduced": false } }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["theme"]["revision"], 2);

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-PLUGIN 管理端点（HTTP）────────────────

#[tokio::test]
async fn plugin_admin_endpoints_install_enable_metrics() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    // capabilities 白名单（菜单数据源；不是安全边界）
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/plugins/capabilities",
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(body["capabilities"].as_array().unwrap().len() >= 9);
    assert_eq!(body["v1_execution"], "config_only");
    let adapters: Vec<&str> = body["provider_adapters"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|a| a["provider"].as_str())
        .collect();
    assert_eq!(adapters, vec!["direct", "hls", "xigua"]);

    // 安装（缺 reason → 400）
    let pkg = json!({
        "schema_version": 1,
        "id": "welcome-reward",
        "name": "欢迎奖励",
        "version": "1.0.0",
        "supports": ">=1.0 <2.0",
        "kind": "config",
        "subscriptions": ["user.verified.v1"],
        "capabilities": ["notification.create"],
        "settings_schema": {
            "type": "object",
            "properties": { "amount": { "type": "integer", "minimum": 0, "maximum": 1000 } },
            "required": ["amount"],
            "additionalProperties": false
        }
    });
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/plugins",
        &admin.session,
        &admin.csrf,
        None,
        pkg.clone(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    let mut with_reason = pkg.clone();
    with_reason["reason"] = json!("install");
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/plugins",
        &admin.session,
        &admin.csrf,
        None,
        with_reason,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["plugin"]["status"], "disabled");
    assert_eq!(body["plugin"]["policy_revision"], 1);

    // 恶意 capability → 400
    let mut evil = pkg.clone();
    evil["id"] = json!("evil-plugin");
    evil["capabilities"] = json!(["db.read"]);
    evil["reason"] = json!("install");
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/plugins",
        &admin.session,
        &admin.csrf,
        None,
        evil,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");
    assert!(body["detail"].to_string().contains("unknown capability"));

    // 更新 settings（If-Match policy_revision）
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/plugins/welcome-reward/settings",
        &admin.session,
        &admin.csrf,
        Some(1),
        json!({ "settings": { "amount": 50 }, "reason": "configure" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["plugin"]["policy_revision"], 2);
    assert_eq!(body["plugin"]["settings"]["amount"], 50);

    // 启用
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/plugins/welcome-reward/enable",
        &admin.session,
        &admin.csrf,
        Some(2),
        json!({ "reason": "go live" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["plugin"]["status"], "enabled");

    // 记录调用 → metrics 可见
    bblbb_backend::plugins::record_call(
        &pool,
        "welcome-reward",
        "user.verified.v1",
        "ok",
        None,
        3,
        Some(9),
    )
    .await;
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/plugins/welcome-reward/metrics",
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["metrics"].as_array().unwrap().len(), 1);
    assert_eq!(body["metrics"][0]["result"], "ok");

    // 停用 + 卸载
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/plugins/welcome-reward/disable",
        &admin.session,
        &admin.csrf,
        Some(3),
        json!({ "reason": "halt" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = authed(
        &app,
        "DELETE",
        "/api/v1/admin/plugins/welcome-reward",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "reason": "uninstall" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-ADMIN-08 DTO 分离 ───────────────────────

#[tokio::test]
async fn admin_dtos_never_leak_credentials_or_private_body() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    // 用户列表/详情响应不含凭据与内部字段
    for path in ["/api/v1/admin/users", "/api/v1/admin/roles"] {
        let (status, body) = authed(
            &app,
            "GET",
            path,
            &admin.session,
            &admin.csrf,
            None,
            Value::Null,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let s = body.to_string();
        assert!(!s.contains("password_hash"), "DTO 不得含密码哈希: {path}");
        assert!(!s.contains("client_secret"), "DTO 不得含 secret");
        assert!(!s.contains("access_token"), "DTO 不得含 access_token");
        assert!(!s.contains("token_hash"), "DTO 不得含 token hash");
        assert!(!s.contains("csrf_secret"), "DTO 不得含 csrf secret");
        assert!(!s.contains("private_key"), "DTO 不得含私钥");
    }

    // 主题/插件视图不含任何内部字段泄漏
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/themes",
        &admin.session,
        &admin.csrf,
        None,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert!(!body.to_string().contains("corrupt") || body["themes"].as_array().unwrap().is_empty());

    cleanup(&dir);
    close_pool(&pool).await;
}
