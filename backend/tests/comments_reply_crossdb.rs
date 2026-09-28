//! M17-GA-PROD-01：public 帖子回复的跨数据库契约（生产 MariaDB 500 回归）。
//!
//! 根因：`grant_reply_if_after_reply_*` 用 `LEFT JOIN content_access_policies`
//! 解码进非 Option `(String, String)`。public 帖子 `access_policy_id IS NULL`
//! 时 LEFT JOIN 产出 (NULL, NULL) 行——sqlx 的 MySQL/MariaDB 驱动把 NULL 解码
//! 进非 Option String 直接 500；SQLite 驱动宽容地把 NULL 解成 ""，掩盖了缺陷。
//! 修复：改为 INNER JOIN（无策略行 → 零行 → 无 grant）。
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
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::pool::DatabasePool;
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
    let dir = std::env::temp_dir().join(format!("bblbb-crt-xdb-{}", uuid::Uuid::now_v7()));
    let pool = create_pool(&format!("sqlite://{}", dir.display()))
        .await
        .unwrap();
    let files = read_migration_files(&migrations_dir("sqlite")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
    (pool, dir)
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    let sql = "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
               VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&user_id)
                .bind(&username)
                .bind(format!("{username}@example.com"))
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
                .bind(&username)
                .bind(format!("{username}@example.com"))
                .bind(now - 25 * 3600 * 1000)
                .bind(now - 30 * 24 * 3600 * 1000)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
    }
    (user_id, username)
}

async fn session_csrf(app: &Router, session: &str) -> String {
    let response = app
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
    assert_eq!(response.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    body["token"].as_str().unwrap().to_owned()
}

async fn request_json(
    app: &Router,
    method: &str,
    uri: &str,
    session: Option<&str>,
    csrf: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json");
    if let Some(session) = session {
        builder = builder.header("cookie", session);
    }
    if let Some(csrf) = csrf {
        builder = builder.header("x-csrf-token", csrf);
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
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

async fn scalar_i64(pool: &DatabasePool, sql: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        Either::Right(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
    }
}

/// public 帖子（access_policy_id NULL）回复必须 201，且恰好落一条评论。
/// 这是生产 MariaDB 上 100% 500 的路径。
async fn public_post_reply_flow(pool: &DatabasePool, app: &Router, tag: &str) {
    let run = uuid::Uuid::now_v7().simple().to_string();
    let (user_id, _username) = insert_user(pool, tag).await;
    let session = common::direct_session_cookie(pool, &user_id).await;
    let csrf = session_csrf(app, &session).await;

    let (status, body) = request_json(
        app,
        "POST",
        "/api/v1/posts",
        Some(&session),
        Some(&csrf),
        json!({
            "type": "discussion",
            "title": format!("pub post {run}"),
            "markdown": format!("pub body {run}"),
            "board_id": BOARD_ID,
            "access_policy": "public",
            "client_request_id": format!("crt-pub-{run}0001")
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "public 发帖应 201: {body}");
    assert_eq!(
        body["status"], "published",
        "public 帖应直接 published（作者 30 天前创建，无冷静/风险命中）: {body}"
    );
    let post_id = body["id"].as_str().unwrap().to_owned();

    let before = scalar_i64(pool, "SELECT COUNT(*) FROM comments").await;
    let (status, body) = request_json(
        app,
        "POST",
        &format!("/api/v1/posts/{post_id}/comments"),
        Some(&session),
        Some(&csrf),
        json!({
            "markdown": format!("pub reply {run}"),
            "client_request_id": format!("crt-crep-{run}0001")
        }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "public 帖子回复必须 201（修复前 MariaDB 500: {body}）"
    );
    assert_eq!(body["post_id"], Value::String(post_id.clone()));
    assert_eq!(body["floor"], 1);
    let after = scalar_i64(pool, "SELECT COUNT(*) FROM comments").await;
    assert_eq!(after - before, 1, "应恰好新增一条评论");
}

#[tokio::test]
async fn sqlite_public_post_reply_contract() {
    let (pool, dir) = sqlite_setup().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    public_post_reply_flow(&pool, &app, "sqlite_pub").await;
    close(&pool).await;
    let _ = std::fs::remove_file(&dir);
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_family_public_post_reply_contract() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let engine = common::mysql_family_migrations_dir(&pool).await;
    let files = read_migration_files(&migrations_dir(engine)).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    public_post_reply_flow(&pool, &app, "xdb_pub").await;
    close(&pool).await;
}

async fn close(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}
