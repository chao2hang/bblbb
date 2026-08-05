//! M03-BOARDS-04：板块列表/详情 cursor 分页、稳定排序与 Cache-Control（HTTP）。
//!
//! 纯函数（游标编码/解码/字典序）在 pagination.rs 单测覆盖；本文件锁定路由：
//! - 匿名列表返回全部种子板块（稳定排序 = sort_order）；
//! - limit + after 游标分页不重不漏（limit=2 → 3 页）；
//! - 非法游标 → 400 invalid_request；
//! - 可见性过滤接入：hidden 不进匿名列表；hidden/members 匿名详情 401/404；
//! - Cache-Control：匿名公开列表 public max-age=60；已认证列表 private
//!   no-store；公开板块详情 public，members 板块详情 private。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
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

const PASSWORD: &str = "correct-password";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-bpage-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
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

async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value, axum::http::HeaderMap) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap()
    };
    (status, value, headers)
}

async fn insert_board(pool: &DatabasePool, slug: &str, visibility: &str) {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, visibility, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&board_id)
            .bind(slug)
            .bind(slug)
            .bind(slug)
            .bind(visibility)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 插入可登录用户（password=PASSWORD）。
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
        .expect("登录必须签发会话 Cookie")
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

/// 匿名列表：5 个种子板块，稳定排序（sort_order），无 next_cursor，has_more=false，
/// Cache-Control public。
#[tokio::test]
async fn anonymous_list_returns_seed_boards_stable_sorted() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (status, body, headers) = get_json(&app, "/api/v1/boards").await;
    assert_eq!(status, StatusCode::OK);
    let slugs: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["slug"].as_str().unwrap())
        .collect();
    assert_eq!(
        slugs,
        vec!["general", "tech", "creative", "help", "news"],
        "稳定排序 = sort_order"
    );
    assert!(body["page"]["next_cursor"].is_null());
    assert_eq!(body["page"]["has_more"], false);
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap(),
        "public, max-age=60",
        "匿名公开列表可缓存"
    );
    // Board 投影包含 ResourceMeta 字段
    assert!(body["items"][0]["id"].is_string());
    assert!(body["items"][0]["version"].is_number());
    assert!(body["items"][0]["created_at"].is_number());
    assert!(body["items"][0]["updated_at"].is_number());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// limit + after 游标分页：limit=2 → 3 页，不重不漏。
#[tokio::test]
async fn cursor_pagination_pages_without_duplicates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let mut collected: Vec<String> = Vec::new();
    let mut after: Option<String> = None;
    let mut pages = 0;
    loop {
        let uri = match &after {
            None => "/api/v1/boards?limit=2".to_string(),
            Some(c) => format!("/api/v1/boards?limit=2&after={c}"),
        };
        let (status, body, _) = get_json(&app, &uri).await;
        assert_eq!(status, StatusCode::OK, "page {pages}");
        let items = body["items"].as_array().unwrap();
        assert!(items.len() <= 2, "limit=2 每页至多 2 条");
        for item in items {
            collected.push(item["slug"].as_str().unwrap().to_string());
        }
        let has_more = body["page"]["has_more"].as_bool().unwrap();
        let next = body["page"]["next_cursor"].as_str().map(str::to_string);
        pages += 1;
        if !has_more {
            assert!(next.is_none(), "无更多时 next_cursor 必须为空");
            break;
        }
        assert!(next.is_some(), "has_more=true 必须有 next_cursor");
        after = next;
        assert!(pages <= 10, "游标必须前进，防止死循环");
    }
    assert_eq!(
        collected,
        vec![
            "general".to_string(),
            "tech".to_string(),
            "creative".to_string(),
            "help".to_string(),
            "news".to_string()
        ],
        "三页合计 = 全部 5 个板块且不重不漏"
    );
    assert_eq!(pages, 3, "5 个板块 limit=2 → 3 页");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 非法游标 → 400 invalid_request。
#[tokio::test]
async fn malformed_cursor_returns_400() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (status, body, _) = get_json(&app, "/api/v1/boards?after=!!!not-base64!!!").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(
        body["code"], "bad_request",
        "AppError::bad_request 既有 code 约定"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 可见性接入列表与详情：hidden 不进匿名列表；hidden 匿名 401、members 匿名 401。
#[tokio::test]
async fn visibility_applies_to_list_and_detail() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    insert_board(&pool, "secret", "hidden").await;
    insert_board(&pool, "lounge", "members").await;
    let app = app_with(pool.clone());

    let (status, body, _) = get_json(&app, "/api/v1/boards").await;
    assert_eq!(status, StatusCode::OK);
    let slugs: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs.len(), 5, "hidden/members 不进匿名列表");
    assert!(!slugs.contains(&"secret") && !slugs.contains(&"lounge"));

    let (status, _, _) = get_json(&app, "/api/v1/boards/secret").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "hidden 匿名 401");
    let (status, _, _) = get_json(&app, "/api/v1/boards/lounge").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "members 匿名 401");
    let (status, _, _) = get_json(&app, "/api/v1/boards/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Cache-Control：已认证列表 private；公开板块详情 public；members 板块详情 private。
#[tokio::test]
async fn cache_control_depends_on_auth_and_visibility() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    insert_board(&pool, "lounge", "members").await;
    let app = app_with(pool.clone());

    // 已认证列表 → private
    let email = insert_login_user(&pool, "pag").await;
    let session = login_session_cookie(&app, &email).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/boards")
                .header("cookie", session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap(),
        "private, no-store",
        "按请求方裁剪的列表必须私有"
    );

    // 匿名公开板块详情 → public
    let (status, _, headers) = get_json(&app, "/api/v1/boards/general").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get(header::CACHE_CONTROL)
            .unwrap()
            .to_str()
            .unwrap(),
        "public, max-age=60"
    );
    let detail = get_json(&app, "/api/v1/boards/general").await.1;
    assert_eq!(detail["slug"], "general");
    assert!(detail["version"].is_number(), "Board 投影带 version");

    // members 板块匿名详情 → 401（不设缓存头路径）
    let (status, _, _) = get_json(&app, "/api/v1/boards/lounge").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}
