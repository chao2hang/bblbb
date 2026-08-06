//! M03-BOARDS-07：标签创建/更新——唯一性、版本冲突、权限与审计（HTTP + DB）。
//!
//! - 创建：权限门（tag.manage 仅管理员）+ reason 必填 + name/slug 唯一 +
//!   group_id 存在性 + 审计 admin.tag_create；
//! - 更新：If-Match 版本冲突（tags.updated_at 0029）+ 部分字段 + 唯一性 +
//!   审计 admin.tag_update（before/after 白名单字段）；
//! - is_active=false 禁用 → 移出公开 listTags。

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
    let dir = std::env::temp_dir().join(format!("bblbb-tagadm-{}", uuid::Uuid::now_v7()));
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
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
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
    common::enroll_totp(pool, &user_id).await; // M02-MFA-05：管理员必须完成 TOTP 才能持有高权限
    let session = common::direct_session_cookie(pool, &user_id).await;
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

async fn tag_version(pool: &DatabasePool, tag_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT updated_at FROM tags WHERE id = ?")
            .bind(tag_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 创建标签：200 + DB 落库 + 审计 admin.tag_create。
#[tokio::test]
async fn create_tag_creates_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "rust", "slug": "rust", "description": "Rust 语言", "reason": "初始化" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let tag_id = body["id"].as_str().unwrap().to_string();
    assert!(body["version"].as_i64().is_some());

    // 审计
    let (action, reason, role): (String, Option<String>, Option<String>) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT action, reason, effective_role FROM audit_logs WHERE target_type = 'tag' AND target_id = ?",
        )
        .bind(&tag_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(action, "admin.tag_create");
    assert_eq!(reason.as_deref(), Some("初始化"));
    assert_eq!(role.as_deref(), Some("administrator"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 唯一性：name 冲突 409、slug 冲突 409；非法 group_id 400；member 403。
#[tokio::test]
async fn create_tag_uniqueness_and_group_and_permission() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    // member 403
    let email = insert_login_user(&pool, "mem").await;
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &session,
        &csrf,
        None,
        json!({ "name": "x", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member 必须 403");

    // 创建 rust
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "rust", "slug": "rust", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // name 冲突
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "rust", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "name 冲突必须 409");

    // slug 冲突（不同 name）
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "rust-lang", "slug": "rust", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "slug 冲突必须 409");

    // 非法 group_id
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "other", "group_id": "no-such-group", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非法 group_id 必须 400");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 更新：正确 If-Match → 200 + 版本递增 + 审计 before/after；过期 → 409。
#[tokio::test]
async fn update_tag_success_and_version_conflict() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "rust", "reason": "创建" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tag_id = body["id"].as_str().unwrap().to_string();
    let v1 = tag_version(&pool, &tag_id).await;

    // 正确 If-Match
    let (status, body) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/tags/{tag_id}"),
        &admin.session,
        &admin.csrf,
        Some(v1),
        json!({ "name": "rust-lang", "description": "Rust 生态", "reason": "改名" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert!(tag_version(&pool, &tag_id).await > v1, "版本必须递增");

    let (name, description): (String, String) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT name, description FROM tags WHERE id = ?")
            .bind(&tag_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(name, "rust-lang");
    assert_eq!(description, "Rust 生态");

    // 审计 before/after
    let metadata: String = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT metadata FROM audit_logs WHERE target_type = 'tag' AND target_id = ? AND action = 'admin.tag_update'",
        )
        .bind(&tag_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let metadata: Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(metadata["before"]["name"], "rust");
    assert_eq!(metadata["after"]["name"], "rust-lang");

    // 过期 If-Match → 409
    let (status, _) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/tags/{tag_id}"),
        &admin.session,
        &admin.csrf,
        Some(v1),
        json!({ "name": "x", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "过期 If-Match 必须 409");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 更新唯一性：改名撞已存在 name → 409。
#[tokio::test]
async fn update_tag_rename_to_existing_conflicts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "rust", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tag_id = body["id"].as_str().unwrap().to_string();
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "go", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/tags/{tag_id}"),
        &admin.session,
        &admin.csrf,
        Some(tag_version(&pool, &tag_id).await),
        json!({ "name": "go", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "改名撞已存在 name 必须 409");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 禁用（is_active=false）→ 移出公开 listTags。
#[tokio::test]
async fn disable_tag_leaves_public_list() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "temp", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let tag_id = body["id"].as_str().unwrap().to_string();

    let (status, _) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/tags/{tag_id}"),
        &admin.session,
        &admin.csrf,
        Some(tag_version(&pool, &tag_id).await),
        json!({ "is_active": false, "reason": "停用" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/tags")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let names: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert!(!names.contains(&"temp"), "禁用标签必须移出公开列表");

    close_pool(&pool).await;
    cleanup(&dir);
}
