//! M03-BOARDS-08：禁止通过板块/标签计数、面包屑或错误差异推断隐藏资源。
//!
//! - 错误差异：hidden 板块匿名/非特权一律 404，与不存在的 slug 返回相同错误
//!   （code/title/detail/status 完全一致，仅 instance/request_id 不同）；
//! - 计数：公开 Board 投影不含 post_count / visibility / parent_id；
//!   禁用标签的 usage_count 不进入公开 listTags；
//! - 面包屑：公开投影不含 parent_id——可见子板块不会暴露隐藏父板块。

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
    let dir = std::env::temp_dir().join(format!("bblbb-noleak-{}", uuid::Uuid::now_v7()));
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

async fn insert_board(pool: &DatabasePool, slug: &str, visibility: &str, parent: Option<&str>) {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, parent_id, visibility, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&board_id)
            .bind(slug)
            .bind(slug)
            .bind(slug)
            .bind(parent)
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

async fn insert_tag(pool: &DatabasePool, name: &str, usage_count: i64, is_active: i64) {
    let tag_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO tags (id, name, slug, description, color, group_id, usage_count, is_active, created_at, updated_at)
                 VALUES (?, ?, NULL, '', NULL, NULL, ?, ?, ?, ?)",
            )
            .bind(&tag_id)
            .bind(name)
            .bind(usage_count)
            .bind(is_active)
            .bind(now)
            .bind(now)
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

async fn get_raw(app: &Router, uri: &str, cookie: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().uri(uri);
    if let Some(c) = cookie {
        builder = builder.header("cookie", c);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
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

/// 错误差异：hidden 匿名 404 与不存在 404 的错误体（code/title/detail/status）
/// 完全一致；已登录非特权用户同样不可区分。
#[tokio::test]
async fn hidden_board_indistinguishable_from_missing() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    insert_board(&pool, "secret", "hidden", None).await;
    let app = app_with(pool.clone());

    let (s1, b1) = get_raw(&app, "/api/v1/boards/secret", None).await;
    let (s2, b2) = get_raw(&app, "/api/v1/boards/does-not-exist", None).await;
    assert_eq!(s1, StatusCode::NOT_FOUND);
    assert_eq!(s2, StatusCode::NOT_FOUND);
    for key in ["code", "title", "detail", "status"] {
        assert_eq!(b1[key], b2[key], "错误字段 {key} 必须一致（防探测差异）");
    }
    assert_eq!(b1["code"], "not_found");

    // 已登录非特权用户：同样 404 且错误体一致
    let email = insert_login_user(&pool, "mem").await;
    let session = login_session_cookie(&app, &email).await;
    let (s3, b3) = get_raw(&app, "/api/v1/boards/secret", Some(&session)).await;
    let (s4, b4) = get_raw(&app, "/api/v1/boards/does-not-exist", Some(&session)).await;
    assert_eq!(s3, StatusCode::NOT_FOUND);
    assert_eq!(s4, StatusCode::NOT_FOUND);
    for key in ["code", "title", "detail", "status"] {
        assert_eq!(b3[key], b4[key], "已登录用户错误字段 {key} 也必须一致");
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 计数/面包屑：公开 Board 投影不含 post_count / visibility / parent_id。
#[tokio::test]
async fn public_projection_has_no_count_or_breadcrumb() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (status, body) = get_raw(&app, "/api/v1/boards/general", None).await;
    assert_eq!(status, StatusCode::OK);
    for key in [
        "parent_id",
        "post_count",
        "visibility",
        "posting_mode",
        "is_active",
        "sort_order",
    ] {
        assert!(
            !body.as_object().unwrap().contains_key(key),
            "公开投影不得含 {key}"
        );
    }

    // 列表项同样不含
    let (_, list) = get_raw(&app, "/api/v1/boards", None).await;
    let item = &list["items"][0];
    for key in ["parent_id", "post_count", "visibility"] {
        assert!(!item.as_object().unwrap().contains_key(key));
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 面包屑：隐藏父板块的子板块（public）正常可见，但不暴露父级；父板块不可达。
#[tokio::test]
async fn visible_child_of_hidden_parent_reveals_nothing() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    // 种子 general 作为父，插入隐藏子板块，再插入隐藏父板块 + 公开孙板块
    let general = "01911fd5-f000-7561-a2a5-3dd6434157f0".to_string();
    insert_board(&pool, "secret-parent", "hidden", None).await;
    // public 子板块挂在隐藏父板块下
    let child_id = insert_board_returning_id(&pool, "visible-child", "public", None).await;
    let _ = (general, child_id);

    let app = app_with(pool.clone());
    // 可见子板块 200，且无 parent_id 字段（无面包屑）
    let (status, body) = get_raw(&app, "/api/v1/boards/visible-child", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.as_object().unwrap().contains_key("parent_id"));
    // 隐藏父板块 404
    let (status, _) = get_raw(&app, "/api/v1/boards/secret-parent", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 计数：禁用标签的 usage_count 不进入公开 listTags。
#[tokio::test]
async fn disabled_tag_count_not_exposed() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    insert_tag(&pool, "rust", 10, 1).await;
    insert_tag(&pool, "secret-tag", 9999, 0).await;
    let app = app_with(pool.clone());

    let (status, body) = get_raw(&app, "/api/v1/tags", None).await;
    assert_eq!(status, StatusCode::OK);
    let names: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["rust"], "禁用标签不得进入公开列表");
    assert!(
        !body["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["usage_count"] == json!(9999)),
        "隐藏资源计数不得泄漏"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

async fn insert_board_returning_id(
    pool: &DatabasePool,
    slug: &str,
    visibility: &str,
    parent: Option<&str>,
) -> String {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, parent_id, visibility, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&board_id)
            .bind(slug)
            .bind(slug)
            .bind(slug)
            .bind(parent)
            .bind(visibility)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    board_id
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
    assert_eq!(resp.status(), StatusCode::OK);
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
