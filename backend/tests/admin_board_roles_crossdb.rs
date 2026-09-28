//! M17-GA-PROD-02：板块角色端点跨数据库契约（GA 生产 MariaDB 实测回归）。
//!
//! 生产发现：`GET /api/v1/admin/boards/{id}/roles` 的查询含 `?` 占位符但
//! **漏绑 board_id**——SQLite 把未绑参数当 NULL 静默返回 0 行（空列表 200，
//! 缺陷被掩盖），MariaDB/MySQL prepared statement 参数数不匹配 →
//! `1210 Incorrect arguments to mysqld_stmt_execute` → 100% 500，管理后台
//! 板块角色列表不可用。本测试在两个引擎上验证：授予 → 列表含该 assignment
//! （非空断言）→ 撤销 → 空列表。
//!
//! SQLite 始终运行；MySQL 8 / MariaDB 10.11 由 CI mysql-family 矩阵以
//! `BBLBB_TEST_MYSQL_URL` + `--ignored` 运行。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::{create_pool, DatabasePool};
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0";

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("../migrations/{engine}"))
}

async fn sqlite_setup() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-abr-xdb-{}", uuid::Uuid::now_v7()));
    let pool = create_pool(&format!("sqlite://{}", dir.display()))
        .await
        .unwrap();
    let files = read_migration_files(&migrations_dir("sqlite")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
    (pool, dir)
}

/// 插入 verified 用户（两引擎）。
async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    let sql = "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
               VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&user_id)
                .bind(&email)
                .bind(&email)
                .bind(now - 25 * 3600 * 1000)
                .bind(now - 30 * 24 * 3600 * 1000)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&user_id)
                .bind(&email)
                .bind(&email)
                .bind(now - 25 * 3600 * 1000)
                .bind(now - 30 * 24 * 3600 * 1000)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
    }
    user_id
}

/// 授予全局角色（两引擎）。
async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role_name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role_name)
            .fetch_one(p)
            .await
            .unwrap(),
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
        Either::Right(p) => {
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
    body["token"].as_str().unwrap().to_owned()
}

async fn request_json(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("cookie", session)
                .header("x-csrf-token", csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let body = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

/// 完整链路：授予 → 列表非空且字段正确 → 撤销 → 列表空。
async fn board_role_assignment_flow(pool: &DatabasePool, app: &Router) {
    let admin_id = insert_user(pool, "abr_admin").await;
    assign_global_role(pool, &admin_id, "administrator").await;
    common::enroll_totp(pool, &admin_id).await; // M02-MFA-05 fail-closed
    let member_id = insert_user(pool, "abr_member").await;
    let session = common::direct_session_cookie(pool, &admin_id).await;
    let csrf = session_csrf(app, &session).await;

    let list_uri = format!("/api/v1/admin/boards/{BOARD_ID}/roles");

    // 1) 空列表
    let (status, body) = request_json(app, "GET", &list_uri, &session, &csrf, json!({})).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "空列表必须 200（修复前 MariaDB 500）: {body}"
    );
    assert_eq!(
        body["assignments"].as_array().unwrap().len(),
        0,
        "初始应为空: {body}"
    );

    // 2) 授予 board_moderator
    let (status, body) = request_json(
        app,
        "POST",
        &list_uri,
        &session,
        &csrf,
        json!({
            "user_id": member_id,
            "role_name": "board_moderator",
            "reason": "crossdb regression"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "授予应 200: {body}");
    assert_eq!(body["status"], "granted");

    // 3) 列表必须含该 assignment（数据存在性断言——漏绑 bug 在此暴露）
    let (status, body) = request_json(app, "GET", &list_uri, &session, &csrf, json!({})).await;
    assert_eq!(status, StatusCode::OK, "列表必须 200: {body}");
    let arr = body["assignments"].as_array().unwrap();
    assert_eq!(arr.len(), 1, "授予后列表必须含 1 条: {body}");
    assert_eq!(arr[0]["user_id"], Value::String(member_id.clone()));
    assert_eq!(arr[0]["role_name"], "board_moderator");
    assert_eq!(arr[0]["board_id"], Value::String(BOARD_ID.to_string()));

    // 4) 撤销
    let (status, body) = request_json(
        app,
        "DELETE",
        &format!("{list_uri}/{member_id}/board_moderator"),
        &session,
        &csrf,
        json!({"reason": "crossdb regression"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "撤销应 200: {body}");

    // 5) 列表恢复空
    let (status, body) = request_json(app, "GET", &list_uri, &session, &csrf, json!({})).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["assignments"].as_array().unwrap().len(),
        0,
        "撤销后应为空: {body}"
    );
}

#[tokio::test]
async fn sqlite_board_role_assignment_contract() {
    let (pool, dir) = sqlite_setup().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    board_role_assignment_flow(&pool, &app).await;
    close(&pool).await;
    let _ = std::fs::remove_file(&dir);
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_family_board_role_assignment_contract() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let engine = common::mysql_family_migrations_dir(&pool).await;
    let files = read_migration_files(&migrations_dir(&engine)).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    board_role_assignment_flow(&pool, &app).await;
    close(&pool).await;
}

async fn close(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}
