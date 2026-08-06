//! M04-POSTS-08：编辑 + 不可变 revision + 管理员代改（reason/recent-auth/审计）。
//!
//! 覆盖：作者编辑（版本递增 + 新 revision + post_contents 更新）；If-Match
//! 版本冲突 409；非作者无权限 403；管理员代改缺 reason 400、未 step-up 403、
//! 完成后写审计日志。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::auth::session::mark_step_up;
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::service::publish_new_post;
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
    let dir = std::env::temp_dir().join(format!("bblbb-ped-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
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

async fn insert_author(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 25 * 3600 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role: &str) {
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role)
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

async fn publish_post(pool: &DatabasePool, author_id: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: "可编辑帖".to_string(),
            markdown: "原始正文".to_string(),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("edit-{}", uuid::Uuid::now_v7().simple()),
        },
        5,
        now_millis(),
    )
    .unwrap();
    publish_new_post(pool, &cmd, author_id, now_millis())
        .await
        .unwrap()
        .post
        .id
}

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn session_csrf(app: &axum::Router, session: &str) -> String {
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

async fn authed_patch(
    app: &axum::Router,
    uri: &str,
    session: &str,
    csrf: &str,
    if_match: &str,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .header("if-match", if_match)
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

#[tokio::test]
async fn owner_edit_creates_immutable_revision() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;
    let post_id = publish_post(&pool, &author).await;

    let (status, body) = authed_patch(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        &session,
        &csrf,
        "1",
        json!({ "markdown": "编辑后的正文" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "作者编辑必须 200: {body}");
    assert_eq!(body["version"], 2, "编辑后版本递增到 2");

    // 不可变 revision：新增一条 (post_id, version=2)
    let rev_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM post_revisions WHERE post_id = ?")
                .bind(&post_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(rev_count, 2, "初始修订 + 编辑修订");
    let rev_body: String = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT body_markdown FROM post_revisions WHERE post_id = ? AND version = 2",
        )
        .bind(&post_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(rev_body, "编辑后的正文", "新修订快照为编辑后正文");

    // post_contents 更新为渲染产物
    let html: String = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT body_html FROM post_contents WHERE post_id = ?")
                .bind(&post_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        html.contains("编辑后的正文"),
        "post_contents 必须更新: {html}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn edit_version_conflict_returns_409() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner2").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let csrf = session_csrf(&app, &session).await;
    let post_id = publish_post(&pool, &author).await;

    let (status, body) = authed_patch(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        &session,
        &csrf,
        "99",
        json!({ "markdown": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "版本冲突必须 409: {body}");
    assert_eq!(body["code"], "version_conflict");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn non_owner_without_permission_is_403() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner3").await;
    let stranger = insert_author(&pool, "stranger").await;
    let author_session = common::direct_session_cookie(&pool, &author).await;
    let author_csrf = session_csrf(&app, &author_session).await;
    let post_id = publish_post(&pool, &author).await;

    let stranger_session = common::direct_session_cookie(&pool, &stranger).await;
    let stranger_csrf = session_csrf(&app, &stranger_session).await;

    let (status, _) = authed_patch(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        &stranger_session,
        &stranger_csrf,
        "1",
        json!({ "markdown": "x", "reason": "代改" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "无 post.edit 权限必须 403");

    // 作者本人仍可编辑
    let (status, _) = authed_patch(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        &author_session,
        &author_csrf,
        "1",
        json!({ "title": "改标题" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "作者本人编辑必须 200");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_delegated_edit_requires_reason_stepup_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "victim").await;
    let admin = insert_author(&pool, "admin").await;
    assign_global_role(&pool, &admin, "administrator").await;
    // elevated 角色触发强制 TOTP（M02-MFA-05/06）：不 enrollment 会被降级为 member
    common::enroll_totp(&pool, &admin).await;
    let admin_session = common::direct_session_cookie(&pool, &admin).await;
    let admin_csrf = session_csrf(&app, &admin_session).await;
    let post_id = publish_post(&pool, &author).await;

    // 缺 reason → 400
    let (status, _) = authed_patch(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        &admin_session,
        &admin_csrf,
        "1",
        json!({ "markdown": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "代改缺 reason 必须 400");

    // 有 reason 但未近期认证（step-up）→ 403
    // （create_session 签发即近期认证；先把会话 auth_verified_at 改为过期）
    let stale = now_millis() - 600_000;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET auth_verified_at = ? WHERE user_id = ?")
                .bind(stale)
                .bind(&admin)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let (status, body) = authed_patch(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        &admin_session,
        &admin_csrf,
        "1",
        json!({ "markdown": "x", "reason": "按举报复核代改" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "未 step-up 必须 403: {body}");
    assert_eq!(body["code"], "step_up_required");

    // 完成近期认证（模拟 5 分钟窗口内重认证）
    let raw_token = admin_session.split('=').nth(1).unwrap().to_string();
    mark_step_up(&pool, &raw_token).await.unwrap();

    // 代改成功 → 200 + 审计日志
    let (status, body) = authed_patch(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        &admin_session,
        &admin_csrf,
        "1",
        json!({ "markdown": "管理员代改正文", "reason": "按举报复核代改" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "管理员代改必须 200: {body}");
    assert_eq!(body["version"], 2);

    // 审计：audit_logs 有一条 post.update 代改记录（actor=admin, target=post）
    let audit_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM audit_logs WHERE action = 'post.update' AND target_type = 'post' AND actor_id = ?",
            )
            .bind(&admin)
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit_count, 1, "代改必须写审计日志");
    let reason: String = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT reason FROM audit_logs WHERE action = 'post.update' AND target_id = ?",
        )
        .bind(&post_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(reason, "按举报复核代改", "审计记录 reason");

    close_pool(&pool).await;
    cleanup(&dir);
}
