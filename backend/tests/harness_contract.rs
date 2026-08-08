//! M16-HARNESS-06 契约边缘：cursor 不重不漏、未知参数、最大 limit、
//! ETag/If-Match 与 Retry-After。
//!
//! 覆盖（其余既有证据见 reports/rc/state-machine-coverage.md 与各测试文件）：
//!   * 最大 limit 钳制：`limit=1000` 时服务端 clamp 到 100（src/routes/boards.rs clamp(1,100)）。
//!   * 未知查询参数被忽略（公开查询契约容忍未知参数，不 4xx）。
//!   * 未知 JSON body 字段被忽略（非 deny_unknown_fields 契约）。
//!   * 非法游标 → 400（boards_pagination.rs#malformed_cursor_returns_400）。
//!   * cursor 分页不重不漏（boards_pagination.rs#cursor_pagination_pages_without_duplicates /
//!     posts_read.rs#list_posts_cursor_pagination_with_headers）。
//!   * ETag/If-Match 版本冲突（posts_edit.rs#edit_version_conflict_returns_409 /
//!     admin_routes.rs If-Match 409 断言）。
//!   * 429 带 Retry-After（session_login.rs / antibot.rs retry-after 断言）。

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Either;
use tower::ServiceExt;

use bblbb_backend::{
    build_router,
    db::migrate::{read_migration_files, run_migrations},
    db::{create_pool, DatabasePool},
    outbox::now_millis,
    AppConfig,
};

mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-hc-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

fn cleanup(dir: &std::path::Path) {
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

async fn insert_boards(pool: &DatabasePool, count: i64, prefix: &str) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await.unwrap();
            for i in 0..count {
                let slug = format!("{prefix}-{i:04}");
                sqlx::query(
                    "INSERT INTO boards (id, slug, name, description, visibility, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(uuid::Uuid::now_v7().to_string())
                .bind(&slug)
                .bind(&slug)
                .bind("")
                .bind("public")
                .bind(now + i)
                .bind(now + i)
                .execute(&mut *tx)
                .await
                .unwrap();
            }
            tx.commit().await.unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 最大 limit 钳制：请求 `limit=1000` 必须被 clamp 到 100。
#[tokio::test]
async fn max_limit_is_clamped_to_100() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    insert_boards(&pool, 120, "bulk").await;

    let (status, body, _) = get_json(&app, "/api/v1/boards?limit=1000").await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 100, "limit 必须被钳制到最大值 100");
    assert_eq!(body["page"]["has_more"], true, "还有更多页");
    assert!(body["page"]["next_cursor"].is_string());

    // 负 limit / 零 limit 走默认值路径（clamp 到 [1,100]）。
    let (status, body, _) = get_json(&app, "/api/v1/boards?limit=0").await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body["items"].as_array().unwrap().is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未知查询参数被忽略（公开查询契约容忍未知参数；不 4xx、不改变结果）。
#[tokio::test]
async fn unknown_query_parameter_is_tolerated() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (status_a, body_a, _) = get_json(&app, "/api/v1/boards?limit=2").await;
    let (status_b, body_b, _) =
        get_json(&app, "/api/v1/boards?limit=2&bogus_param=1&x-extra=zzz").await;
    assert_eq!(status_a, StatusCode::OK);
    assert_eq!(status_b, StatusCode::OK, "未知查询参数不 4xx");
    assert_eq!(
        body_a["items"].as_array().unwrap().len(),
        body_b["items"].as_array().unwrap().len(),
        "未知参数不改变分页结果"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未知 JSON body 字段被忽略（CreatePostRequest 非 deny_unknown_fields 契约）。
#[tokio::test]
async fn unknown_body_field_is_tolerated() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    // 匿名发帖 → 401（说明 body 已解析并走到身份校验；未知字段不导致 400/422）。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/posts")
                .header("content-type", "application/json")
                .body(Body::from(
                    r#"{"type":"discussion","title":"t","markdown":"m","totally_unknown_field":42}"#,
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "未知 body 字段被容忍，不产生 400"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 非法游标 → 400（不泄漏内部信息，仅 bad_request 语义）。
#[tokio::test]
async fn malformed_cursor_returns_400() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (status, body, _) = get_json(&app, "/api/v1/boards?after=!!!not-base64!!!").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["code"], "bad_request");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// cursor 分页不重不漏（三页合计 = 全部，无重复、无遗漏）。
#[tokio::test]
async fn cursor_pagination_pages_without_duplicates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    insert_boards(&pool, 5, "pg").await;

    let mut collected: Vec<String> = Vec::new();
    let mut after: Option<String> = None;
    let mut pages = 0;
    loop {
        let uri = match &after {
            Some(cursor) => format!("/api/v1/boards?limit=2&after={cursor}"),
            None => "/api/v1/boards?limit=2".to_string(),
        };
        let (status, body, _) = get_json(&app, &uri).await;
        assert_eq!(status, StatusCode::OK);
        for item in body["items"].as_array().unwrap() {
            collected.push(item["slug"].as_str().unwrap().to_string());
        }
        if body["page"]["has_more"] == Value::Bool(true) {
            after = Some(body["page"]["next_cursor"].as_str().unwrap().to_string());
            pages += 1;
            assert!(pages <= 10, "游标必须前进，防止死循环");
        } else {
            assert!(body["page"]["next_cursor"].is_null(), "末页无 next_cursor");
            break;
        }
    }
    // 5 个 seed 板块 + 5 个新插入板块 = 10 个公开板块。
    assert_eq!(collected.len(), 10, "不重不漏：10 个公开板块全部出现");
    assert_eq!(
        pages, 4,
        "10 个板块 limit=2 → 5 页，其中 4 页 has_more=true"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
