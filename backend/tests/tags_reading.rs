//! M03-BOARDS-06：标签组、标签 slug、展示名与禁用状态读取（HTTP + DB）。
//!
//! - 迁移 0028：tags.is_active（默认 1）；0 = 禁用 → 移出公开投影；
//! - load_tag_groups / load_active_tags / load_all_tags 服务读取；
//! - listTags：启用标签（slug/name/description/color/group_id/usage_count）+
//!   标签组；禁用标签不出现；
//! - listAdminTags：全部标签（含 is_active 禁用状态）；member 403。

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
use bblbb_backend::tags::{load_active_tags, load_all_tags, load_tag_groups};
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

mod common;

const PASSWORD: &str = "correct-password";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-tags-{}", uuid::Uuid::now_v7()));
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

async fn insert_tag_group(pool: &DatabasePool, name: &str, slug: &str, sort_order: i64) -> String {
    let group_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO tag_groups (id, name, slug, sort_order, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&group_id)
            .bind(name)
            .bind(slug)
            .bind(sort_order)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    group_id
}

#[allow(clippy::too_many_arguments)]
async fn insert_tag(
    pool: &DatabasePool,
    name: &str,
    slug: Option<&str>,
    group_id: Option<&str>,
    usage_count: i64,
    is_active: i64,
) -> String {
    let tag_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO tags (id, name, slug, description, color, group_id, usage_count, is_active, created_at)
                 VALUES (?, ?, ?, '默认', NULL, ?, ?, ?, ?)",
            )
            .bind(&tag_id)
            .bind(name)
            .bind(slug)
            .bind(group_id)
            .bind(usage_count)
            .bind(is_active)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    tag_id
}

/// 迁移 0028：is_active 默认 1；禁用标签移出活跃读取。
#[tokio::test]
async fn disabled_tags_excluded_from_active_reads() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let group = insert_tag_group(&pool, "技术", "tech", 0).await;
    insert_tag(&pool, "rust", Some("rust"), Some(&group), 10, 1).await;
    insert_tag(&pool, "gone", Some("gone"), Some(&group), 99, 0).await;

    let active = load_active_tags(&pool).await.unwrap();
    assert_eq!(active.len(), 1, "禁用标签必须移出活跃读取");
    assert_eq!(active[0].name, "rust");
    assert_eq!(active[0].slug.as_deref(), Some("rust"));
    assert!(active[0].enabled());

    let all = load_all_tags(&pool).await.unwrap();
    assert_eq!(all.len(), 2, "管理端读取必须含禁用");
    let gone = all.iter().find(|t| t.name == "gone").unwrap();
    assert!(!gone.enabled(), "禁用状态必须可见");

    let groups = load_tag_groups(&pool).await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].slug, "tech");

    close_pool(&pool).await;
    cleanup(&dir);
}

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn get_json(app: &Router, uri: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, body)
}

/// listTags：只含启用标签（slug/展示名/组/颜色）+ 标签组；禁用不出现。
#[tokio::test]
async fn list_tags_shows_active_tags_and_groups() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let group = insert_tag_group(&pool, "技术", "tech", 0).await;
    insert_tag(&pool, "rust", Some("rust"), Some(&group), 10, 1).await;
    insert_tag(&pool, "gone", Some("gone"), Some(&group), 99, 0).await;
    let app = app_with(pool.clone());

    let (status, body) = get_json(&app, "/api/v1/tags").await;
    assert_eq!(status, StatusCode::OK);
    let slugs: Vec<&str> = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["slug"].as_str().unwrap())
        .collect();
    assert_eq!(slugs, vec!["rust"], "禁用标签不得进入公开列表");
    let tag = &body["items"][0];
    assert_eq!(tag["name"], "rust");
    assert_eq!(tag["group_id"], group);
    assert!(tag["usage_count"].is_number());
    let groups: Vec<&str> = body["groups"]
        .as_array()
        .unwrap()
        .iter()
        .map(|g| g["slug"].as_str().unwrap())
        .collect();
    assert_eq!(groups, vec!["tech"]);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// listAdminTags：全部标签含禁用状态；member 403。
#[tokio::test]
async fn list_admin_tags_shows_disabled_state_and_gates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    insert_tag(&pool, "rust", Some("rust"), None, 10, 1).await;
    insert_tag(&pool, "gone", Some("gone"), None, 99, 0).await;

    // member → 403
    let email = insert_login_user(&pool, "mem").await;
    let app = app_with(pool.clone());
    let session = login_session_cookie(&app, &email).await;
    let csrf = session_csrf(&app, &session).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/tags")
                .header("cookie", session)
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN, "member 必须 403");

    // admin → 200，全部标签含 is_active
    let (admin_session, admin_csrf) = admin_session(&app, &pool).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/admin/tags")
                .header("cookie", admin_session)
                .header("x-csrf-token", admin_csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "管理端必须含禁用标签");
    let gone = items.iter().find(|t| t["name"] == "gone").unwrap();
    assert_eq!(gone["is_active"], false, "禁用状态必须可读");

    close_pool(&pool).await;
    cleanup(&dir);
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

async fn admin_session(app: &Router, pool: &DatabasePool) -> (String, String) {
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
    (session, csrf)
}
