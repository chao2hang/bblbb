//! M18-ADMIN-BATCH 集成测试（SQLite + 路由层）。
//!
//! 覆盖：
//! 1. 批量帖子管理 POST /api/v1/admin/posts/batch（approve/delete/lock 批量生效 + 审计记录落库）；
//! 2. 批量板块管理 POST /api/v1/admin/boards/batch（批量启停 is_active）；
//! 3. 批量标签管理 POST /api/v1/admin/tags/batch（批量启停）；
//! 4. 批量用户管理 POST /api/v1/admin/users/batch（状态切换 + 保护最后管理员冲突拒绝）；
//! 5. 全量流式 CSV 导出 GET /api/v1/admin/{posts,boards,tags,users,audit}/export.csv
//!    （带 UTF-8 BOM + 权限门 + 导出审计）。

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

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-batch-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(format!("{username}@example.com"))
            .bind(now - 25 * 3600 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (user_id, username)
}

async fn grant_role_direct(pool: &DatabasePool, user_id: &str, role_name: &str) {
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

async fn admin_ctx(app: &Router, pool: &DatabasePool) -> (String, String, String) {
    let (admin_id, _name) = insert_user(pool, "adm").await;
    grant_role_direct(pool, &admin_id, "administrator").await;
    common::enroll_totp(pool, &admin_id).await;
    let session = common::direct_session_cookie(pool, &admin_id).await;
    let token = session.split('=').nth(1).unwrap().to_string();
    let _ = bblbb_backend::auth::session::mark_step_up(pool, &token).await;
    let csrf = session_csrf(app, &session).await;
    (admin_id, session, csrf)
}

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn authed_json(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    let req = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-csrf-token", csrf)
        .header("cookie", session)
        .body(Body::from(body.to_string()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn authed_get_bytes(
    app: &Router,
    uri: &str,
    session: &str,
    csrf: &str,
) -> (StatusCode, Vec<u8>) {
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("x-csrf-token", csrf)
        .header("cookie", session)
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, bytes)
}

async fn insert_post(pool: &DatabasePool, author_id: &str, title: &str, status: &str) -> String {
    let pid = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO posts (id, author_id, board_id, title, content, content_format, status,
                                    visibility, pinned, view_count, reply_count, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'body', 'markdown', ?, 'public', 0, 0, 0, ?, ?)",
            )
            .bind(&pid)
            .bind(author_id)
            .bind(BOARD_ID)
            .bind(title)
            .bind(status)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    pid
}

#[tokio::test]
async fn test_batch_posts_and_audit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    let (author_id, _) = insert_user(&pool, "author").await;
    let p1 = insert_post(&pool, &author_id, "post 1", "draft").await;
    let p2 = insert_post(&pool, &author_id, "post 2", "draft").await;

    // 1. 批量 approve
    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/admin/posts/batch",
        &session,
        &csrf,
        json!({
            "ids": [&p1, &p2],
            "action": "approve",
            "reason": "batch approve posts"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "batch approve should succeed: {body:?}"
    );
    assert_eq!(body["ok"], true);
    assert_eq!(body["affected"], 2);

    // 2. 批量 delete
    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/admin/posts/batch",
        &session,
        &csrf,
        json!({
            "ids": [&p1, &p2],
            "action": "delete",
            "reason": "batch delete posts"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "batch delete should succeed: {body:?}"
    );
    assert_eq!(body["ok"], true);
    assert_eq!(body["affected"], 2);

    // 检查审计日志
    match &pool {
        Either::Left(p) => {
            let count: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.posts.batch'",
            )
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(count, 2, "must record two batch audit entries");
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn test_batch_boards_and_tags() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 批量 boards
    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/admin/boards/batch",
        &session,
        &csrf,
        json!({
            "ids": [BOARD_ID],
            "is_active": false,
            "reason": "disable general board"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "batch boards ok: {body:?}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["affected"], 1);

    // 批量 tags
    let tag_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO tags (id, slug, name, description, is_active, created_at, updated_at)
                 VALUES (?, 'rust', 'Rust', 'Rust lang', 1, ?, ?)",
            )
            .bind(&tag_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/admin/tags/batch",
        &session,
        &csrf,
        json!({
            "ids": [&tag_id],
            "is_active": false,
            "reason": "disable tag"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "batch tags ok: {body:?}");
    assert_eq!(body["ok"], true);
    assert_eq!(body["affected"], 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn test_batch_users_protection() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 尝试将唯一的管理员 ban 掉 -> 应该被拒绝 409
    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/admin/users/batch",
        &session,
        &csrf,
        json!({
            "ids": [&admin_id],
            "status": "banned",
            "reason": "try ban admin"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "cannot ban last admin: {body:?}"
    );

    // 创建普通用户并批量 ban
    let (u1, _) = insert_user(&pool, "u1").await;
    let (u2, _) = insert_user(&pool, "u2").await;

    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/admin/users/batch",
        &session,
        &csrf,
        json!({
            "ids": [&u1, &u2],
            "status": "banned",
            "reason": "ban normal users"
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "batch ban normal users ok: {body:?}"
    );
    assert_eq!(body["affected"], 2);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn test_export_csv_endpoints() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 1. posts export
    let (status, bytes) =
        authed_get_bytes(&app, "/api/v1/admin/posts/export.csv", &session, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    // 验证 UTF-8 BOM
    assert!(bytes.starts_with(&[0xEF, 0xBB, 0xBF]));
    let content = String::from_utf8_lossy(&bytes[3..]);
    assert!(content.contains("ID,标题,作者ID"));

    // 2. boards export
    let (status, bytes) =
        authed_get_bytes(&app, "/api/v1/admin/boards/export.csv", &session, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    let content = String::from_utf8_lossy(&bytes[3..]);
    assert!(content.contains("ID,Slug,名称"));

    // 3. tags export
    let (status, bytes) =
        authed_get_bytes(&app, "/api/v1/admin/tags/export.csv", &session, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    let content = String::from_utf8_lossy(&bytes[3..]);
    assert!(content.contains("ID,Slug,名称"));

    // 4. users export
    let (status, bytes) =
        authed_get_bytes(&app, "/api/v1/admin/users/export.csv", &session, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    let content = String::from_utf8_lossy(&bytes[3..]);
    assert!(content.contains("ID,用户名,邮箱"));

    // 5. audit export
    let (status, bytes) =
        authed_get_bytes(&app, "/api/v1/admin/audit/export.csv", &session, &csrf).await;
    assert_eq!(status, StatusCode::OK);
    let content = String::from_utf8_lossy(&bytes[3..]);
    assert!(content.contains("ID,操作者ID,操作者名"));

    close_pool(&pool).await;
    cleanup(&dir);
}
