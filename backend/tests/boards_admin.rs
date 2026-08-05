//! M03-BOARDS-05：管理员创建/更新板块——版本冲突、reason 与审计（HTTP + DB）。
//!
//! - 创建：权限门（board.manage 仅管理员）+ reason 必填 + slug 唯一/格式 +
//!   审计 admin.board_create（事务内，effective_role=administrator）；
//! - 更新：If-Match 版本冲突（boards.updated_at 为版本）+ 部分字段 + 父级循环
//!   拒绝 + 审计 admin.board_update（before/after 白名单字段）；
//! - is_active=false 停用 → 移出公开列表（活跃投影）。

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
    let dir = std::env::temp_dir().join(format!("bblbb-badm-{}", uuid::Uuid::now_v7()));
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

/// 已认证写请求（会话 Cookie + X-CSRF-Token + 可选 If-Match）。
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

async fn board_version(pool: &DatabasePool, board_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT updated_at FROM boards WHERE id = ?")
            .bind(board_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
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
    let session = login_session_cookie(app, &email).await;
    let csrf = session_csrf(app, &session).await;
    AdminCtx { session, csrf }
}

/// 创建板块：200 + DB 落库 + 审计 admin.board_create。
#[tokio::test]
async fn create_board_creates_board_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "meta", "name": "站务公告", "description": "规则", "reason": "v1 初始化" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    let board_id = body["id"].as_str().unwrap().to_string();
    assert!(body["version"].as_i64().is_some(), "创建返回版本");

    // DB 落库（含默认 visibility=public / posting_mode=normal / sort_order=0）
    let (visibility, posting_mode): (String, String) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT visibility, posting_mode FROM boards WHERE id = ?")
                .bind(&board_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(visibility, "public");
    assert_eq!(posting_mode, "normal");

    // 审计：admin.board_create + reason + effective_role + policy_version
    let (action, reason, role, policy): (String, Option<String>, Option<String>, Option<String>) =
        match &pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT action, reason, effective_role, policy_version FROM audit_logs WHERE target_type = 'board' AND target_id = ?",
            )
            .bind(&board_id)
            .fetch_one(p)
            .await
            .unwrap(),
            Either::Right(_) => panic!("SQLite only"),
        };
    assert_eq!(action, "admin.board_create");
    assert_eq!(reason.as_deref(), Some("v1 初始化"));
    assert_eq!(role.as_deref(), Some("administrator"));
    assert_eq!(
        policy.as_deref(),
        Some(bblbb_backend::authz::decision::AUTHZ_POLICY_VERSION)
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 权限门：member 403；reason 缺失 400。
#[tokio::test]
async fn create_board_requires_permission_and_reason() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let email = insert_login_user(&pool, "mem").await;
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &session,
        &csrf,
        None,
        json!({ "slug": "x", "name": "X", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member 必须 403");

    let admin = admin_ctx(&app, &pool).await;
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "meta", "name": "站务" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "缺 reason 必须 400: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// slug 冲突 409 / 非法 slug 400。
#[tokio::test]
async fn create_board_slug_conflict_and_invalid() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    // 与种子 slug 冲突
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "general", "name": "重复", "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "slug 冲突必须 409");

    // 非法 slug（大写）
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "Meta-Board", "name": "大写", "reason": "t" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "非法 slug 必须 400: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 更新：正确 If-Match → 200 + 版本递增 + 字段变更 + 审计 before/after。
#[tokio::test]
async fn update_board_success_bumps_version_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "meta", "name": "站务", "reason": "创建" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let board_id = body["id"].as_str().unwrap().to_string();
    let v1 = board_version(&pool, &board_id).await;

    // PATCH name + is_active=false
    let (status, body) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/boards/{board_id}"),
        &admin.session,
        &admin.csrf,
        Some(v1),
        json!({ "name": "站务公告", "is_active": false, "reason": "改名并停用" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body: {body}");
    assert_eq!(
        board_version(&pool, &board_id).await,
        body["version"].as_i64().unwrap()
    );
    assert!(board_version(&pool, &board_id).await > v1, "版本必须递增");

    let (name, is_active): (String, i64) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT name, is_active FROM boards WHERE id = ?")
            .bind(&board_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(name, "站务公告");
    assert_eq!(is_active, 0);

    // 审计 admin.board_update：before/after 白名单字段
    let (action, metadata): (String, String) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT action, metadata FROM audit_logs WHERE target_type = 'board' AND target_id = ? AND action = 'admin.board_update'",
        )
        .bind(&board_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(action, "admin.board_update");
    let metadata: Value = serde_json::from_str(&metadata).unwrap();
    assert_eq!(metadata["before"]["name"], "站务");
    assert_eq!(metadata["after"]["name"], "站务公告");
    assert_eq!(metadata["after"]["is_active"], false);
    assert_eq!(metadata["before"]["is_active"], true);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 过期 If-Match → 409 version_conflict；缺 If-Match → 400。
#[tokio::test]
async fn update_board_stale_if_match_conflicts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "meta", "name": "站务", "reason": "创建" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let board_id = body["id"].as_str().unwrap().to_string();
    let stale = board_version(&pool, &board_id).await - 1;

    // 无 If-Match → 400
    let (status_no_match, _) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/boards/{board_id}"),
        &admin.session,
        &admin.csrf,
        None,
        json!({ "name": "新名", "reason": "t" }),
    )
    .await;
    assert_eq!(
        status_no_match,
        StatusCode::BAD_REQUEST,
        "缺 If-Match 必须 400"
    );

    // 过期 If-Match → 409
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/admin/boards/{board_id}"))
                .header("content-type", "application/json")
                .header("x-csrf-token", &admin.csrf)
                .header("cookie", &admin.session)
                .header("if-match", stale.to_string())
                .body(Body::from(
                    json!({ "name": "新名", "reason": "t" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::CONFLICT,
        "过期 If-Match 必须 409 version_conflict"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 父级循环拒绝：把根板块移动到自己的子板块下 → 400。
#[tokio::test]
async fn update_board_parent_cycle_rejected() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "root-b", "name": "根", "reason": "创建" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let root_id = body["id"].as_str().unwrap().to_string();

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "child-b", "name": "子", "parent_id": root_id, "reason": "创建" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let child_id = body["id"].as_str().unwrap().to_string();

    // 把 root 移到 child 下 → 循环
    let root_version = board_version(&pool, &root_id).await;
    let (status, body) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/boards/{root_id}"),
        &admin.session,
        &admin.csrf,
        Some(root_version),
        json!({ "parent_id": child_id, "reason": "t" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "父级循环必须 400: {body}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 并发 slug：同时创建同 slug 的两个请求只允许一个成功（另一个 409，
/// 唯一索引兜底触发，绝不 500 也绝不产生重复）。
#[tokio::test]
async fn concurrent_create_same_slug_only_one_wins() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let body = json!({ "slug": "race", "name": "竞态", "reason": "t" });
    let (sa, sb) = tokio::join!(
        authed(
            &app,
            "POST",
            "/api/v1/admin/boards",
            &admin.session,
            &admin.csrf,
            None,
            body.clone(),
        ),
        authed(
            &app,
            "POST",
            "/api/v1/admin/boards",
            &admin.session,
            &admin.csrf,
            None,
            body.clone(),
        ),
    );
    let statuses = [sa.0, sb.0];
    assert_eq!(
        statuses.iter().filter(|s| **s == StatusCode::OK).count(),
        1,
        "同 slug 并发创建必须恰好一个成功"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::CONFLICT)
            .count(),
        1,
        "另一个必须 409 conflict（唯一索引兜底）"
    );
    assert_eq!(
        statuses
            .iter()
            .filter(|s| **s == StatusCode::INTERNAL_SERVER_ERROR)
            .count(),
        0,
        "并发竞态不得 500"
    );

    // 库中恰好一行
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM boards WHERE slug = 'race'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// is_active=false（停用）→ 移出公开列表（活跃投影）。
#[tokio::test]
async fn deactivated_board_leaves_public_list() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let admin = admin_ctx(&app, &pool).await;

    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/boards",
        &admin.session,
        &admin.csrf,
        None,
        json!({ "slug": "temp", "name": "临时", "reason": "创建" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let board_id = body["id"].as_str().unwrap().to_string();

    let (status, _) = authed(
        &app,
        "PATCH",
        &format!("/api/v1/admin/boards/{board_id}"),
        &admin.session,
        &admin.csrf,
        Some(board_version(&pool, &board_id).await),
        json!({ "is_active": false, "reason": "停用" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 匿名公开列表不含 temp
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/boards")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let slugs: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["slug"].as_str().unwrap())
        .collect();
    assert!(!slugs.contains(&"temp"), "停用板块必须移出公开列表");

    close_pool(&pool).await;
    cleanup(&dir);
}
