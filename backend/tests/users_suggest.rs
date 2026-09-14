//! 用户 @提及建议端点测试（GET /api/v1/users/suggest）

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Either;
use tower::ServiceExt;

mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-suggest-{}", uuid::Uuid::now_v7()));
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

async fn insert_custom_user(
    pool: &DatabasePool,
    username: &str,
    display_name: Option<&str>,
    status: &str,
    level: i64,
) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username_normalized = username.to_lowercase();
    let email = format!("{username_normalized}@example.com");
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, display_name, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username_normalized)
            .bind(&email)
            .bind(status)
            .bind(level)
            .bind(display_name)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only in integration tests"),
    }
    user_id
}

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn authed_get(app: &Router, uri: &str, session: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .header("cookie", session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

#[tokio::test]
async fn suggest_mention_users_requires_auth() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/users/suggest")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn suggest_mention_users_returns_defaults_and_excludes_self_and_deleted() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let caller_id = insert_custom_user(&pool, "caller_user", Some("Caller"), "active", 1).await;
    let caller_cookie = common::direct_session_cookie(&pool, &caller_id).await;

    insert_custom_user(&pool, "alice_one", Some("Alice One"), "active", 5).await;
    insert_custom_user(&pool, "bob_two", Some("Bob Two"), "active", 10).await;
    insert_custom_user(&pool, "charlie_three", None, "active", 2).await;
    insert_custom_user(&pool, "deleted_user", Some("Deleted"), "deleted", 10).await;
    insert_custom_user(&pool, "pending_del", Some("Pending"), "pending_delete", 8).await;

    // 默认展示最相近用户（无 query 时按 level/updated_at 降序，最多 5 个，不含本人与已注销）
    let (status, body) = authed_get(&app, "/api/v1/users/suggest", &caller_cookie).await;
    assert_eq!(status, StatusCode::OK);

    let items = body["items"].as_array().expect("items must be array");
    let usernames: Vec<&str> = items
        .iter()
        .filter_map(|i| i["username"].as_str())
        .collect();

    assert!(!usernames.contains(&"caller_user"), "不能推荐本人");
    assert!(!usernames.contains(&"deleted_user"), "已删除账号不推荐");
    assert!(!usernames.contains(&"pending_del"), "删除中账号不推荐");
    assert!(usernames.contains(&"bob_two"));
    assert!(usernames.contains(&"alice_one"));
    assert!(usernames.contains(&"charlie_three"));

    // 等级高的 bob_two (level=10) 排在 alice_one (level=5) 之前
    let bob_idx = usernames.iter().position(|&u| u == "bob_two").unwrap();
    let alice_idx = usernames.iter().position(|&u| u == "alice_one").unwrap();
    assert!(bob_idx < alice_idx, "高等级排在前面");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn suggest_mention_users_fuzzy_search_and_prefix_ranking() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let caller_id = insert_custom_user(&pool, "caller_user", Some("Caller"), "active", 1).await;
    let caller_cookie = common::direct_session_cookie(&pool, &caller_id).await;

    insert_custom_user(&pool, "david_smith", Some("大卫"), "active", 1).await;
    insert_custom_user(&pool, "david", Some("David Exact"), "active", 1).await;
    insert_custom_user(&pool, "super_david_fan", Some("粉丝"), "active", 1).await;
    insert_custom_user(&pool, "john_doe", Some("Little David"), "active", 1).await;

    // 搜索 q=david
    let (status, body) = authed_get(&app, "/api/v1/users/suggest?q=david", &caller_cookie).await;
    assert_eq!(status, StatusCode::OK);

    let items = body["items"].as_array().expect("items must be array");
    let usernames: Vec<&str> = items
        .iter()
        .filter_map(|i| i["username"].as_str())
        .collect();

    // 完全匹配 david 排第一，前缀匹配 david_smith 排第二，包含子串 super_david_fan 与 display_name 包含的排在后面
    assert_eq!(usernames[0], "david", "完全匹配排首位");
    assert_eq!(usernames[1], "david_smith", "用户名首缀匹配排第二");
    assert!(usernames.contains(&"super_david_fan"));
    assert!(usernames.contains(&"john_doe"));

    // 带 @ 前缀搜索 q=@david 应该得到相同结果
    let (status_at, body_at) =
        authed_get(&app, "/api/v1/users/suggest?q=%40david", &caller_cookie).await;
    assert_eq!(status_at, StatusCode::OK);
    let items_at = body_at["items"].as_array().expect("items must be array");
    let usernames_at: Vec<&str> = items_at
        .iter()
        .filter_map(|i| i["username"].as_str())
        .collect();
    assert_eq!(usernames_at, usernames);

    // limit 参数约束
    let (status_lim, body_lim) = authed_get(
        &app,
        "/api/v1/users/suggest?q=david&limit=2",
        &caller_cookie,
    )
    .await;
    assert_eq!(status_lim, StatusCode::OK);
    let items_lim = body_lim["items"].as_array().expect("items must be array");
    assert_eq!(items_lim.len(), 2);

    close_pool(&pool).await;
    cleanup(&dir);
}
