//! 昵称黑名单与管理员一键随机昵称集成测试。

mod common;

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{header, Request, StatusCode},
    Router,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

const PASSWORD: &str = "correct-password9";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-bl-{}.sqlite", uuid::Uuid::now_v7()));
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

fn test_router(pool: DatabasePool) -> Router {
    let config = AppConfig {
        mfa_encryption_key: String::from_utf8(common::TEST_TOTP_ENC_KEY.to_vec()).unwrap(),
        ..AppConfig::default()
    };
    build_router(config, Some(pool))
}

async fn create_user_with_role(
    pool: &DatabasePool,
    username: &str,
    display_name: Option<&str>,
    role: &str,
) -> (String, String) {
    let id = uuid::Uuid::now_v7().to_string();
    let now = bblbb_backend::outbox::now_millis();
    let hash = bblbb_backend::auth::hash_password(PASSWORD).unwrap();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, display_name, status, email_verified, version, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 'active', 1, 1, ?, ?)",
            )
            .bind(&id)
            .bind(username)
            .bind(format!("{username}@example.com"))
            .bind(&hash)
            .bind(display_name)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();

            let role_id: String = sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
                .bind(role)
                .fetch_one(p)
                .await
                .unwrap();

            sqlx::query("INSERT INTO user_roles (user_id, role_id, granted_at) VALUES (?, ?, ?)")
                .bind(&id)
                .bind(&role_id)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (id, username.to_string())
}

async fn session_csrf(app: &Router, session: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header(header::COOKIE, session)
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

#[tokio::test]
async fn test_randomize_nickname_and_blacklist_enforcement() {
    let (pool, path) = sqlite_pool_with_migrations().await;
    let router = test_router(pool.clone());

    // 1. 创建管理员与违规昵称用户
    let (admin_id, _admin_user) =
        create_user_with_role(&pool, "super_admin", Some("超管"), "administrator").await;
    let (bad_user_id, _bad_username) =
        create_user_with_role(&pool, "bad_user", Some("涉政违规昵称_999"), "member").await;

    common::enroll_totp(&pool, &admin_id).await;
    let admin_session = common::direct_session_cookie(&pool, &admin_id).await;
    let token = admin_session.split('=').nth(1).unwrap().to_string();
    bblbb_backend::auth::session::mark_step_up(&pool, &token)
        .await
        .unwrap();
    let admin_csrf = session_csrf(&router, &admin_session).await;

    let user_session = common::direct_session_cookie(&pool, &bad_user_id).await;
    let user_csrf = session_csrf(&router, &user_session).await;

    // 2. 管理员调用一键随机昵称
    let req = Request::builder()
        .method("POST")
        .uri(format!(
            "/api/v1/admin/users/{bad_user_id}/randomize-nickname"
        ))
        .header(header::COOKIE, &admin_session)
        .header("x-csrf-token", &admin_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "reason": "包含不合规词汇，一键重置" }).to_string(),
        ))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();

    assert_eq!(body["ok"], true);
    assert_eq!(body["old_nickname"], "涉政违规昵称_999");
    let new_nickname = body["new_nickname"].as_str().unwrap().to_string();
    assert!(new_nickname.starts_with("用户_"));

    // 3. 校验数据库中的用户昵称确实已变更为新随机昵称
    let current_dn: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT display_name FROM users WHERE id = ?")
            .bind(&bad_user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(current_dn, Some(new_nickname.clone()));

    // 4. 校验原昵称已被加入黑名单
    let is_bl = bblbb_backend::users::blacklist::is_nickname_blacklisted(&pool, "涉政违规昵称_999")
        .await
        .unwrap();
    assert!(is_bl, "原昵称必须在黑名单中");

    // 大小写/trim 不敏感匹配校验
    let is_bl_case =
        bblbb_backend::users::blacklist::is_nickname_blacklisted(&pool, " 涉政违规昵称_999 ")
            .await
            .unwrap();
    assert!(is_bl_case, "大小写/前后空白归一化后仍应匹配黑名单");

    // 5. 校验用户本人无法再改回原违规昵称
    let req = Request::builder()
        .method("PATCH")
        .uri("/api/v1/me")
        .header(header::COOKIE, &user_session)
        .header("x-csrf-token", &user_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .header("If-Match", "2")
        .body(Body::from(
            json!({ "display_name": "涉政违规昵称_999" }).to_string(),
        ))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    let err_body: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(err_body["detail"].as_str().unwrap().contains("黑名单"));

    // 6. 校验新用户注册使用该名称也被拒绝
    let (preauth, preauth_csrf) = common::fetch_preauth(&router).await;
    let preauth = preauth.split(';').next().unwrap().to_string();
    let req = Request::builder()
        .method("POST")
        .uri("/api/v1/auth/register")
        .header(header::COOKIE, &preauth)
        .header("x-csrf-token", &preauth_csrf)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({
                "username": "涉政违规昵称_999",
                "email": "another_bad@example.com",
                "password": "Password1234"
            })
            .to_string(),
        ))
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::BAD_REQUEST);

    // 7. 管理员查询黑名单列表
    let req = Request::builder()
        .method("GET")
        .uri("/api/v1/admin/nickname-blacklist?limit=10")
        .header(header::COOKIE, &admin_session)
        .body(Body::empty())
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let bl_list: Value =
        serde_json::from_slice(&res.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(bl_list["total"].as_i64().unwrap() >= 1);
    let items = bl_list["items"].as_array().unwrap();
    let found = items.iter().find(|it| it["nickname"] == "涉政违规昵称_999");
    assert!(found.is_some());
    let bl_id = found.unwrap()["id"].as_str().unwrap().to_string();

    // 8. 管理员移出黑名单
    let req = Request::builder()
        .method("DELETE")
        .uri(format!("/api/v1/admin/nickname-blacklist/{bl_id}"))
        .header(header::COOKIE, &admin_session)
        .header("x-csrf-token", &admin_csrf)
        .body(Body::empty())
        .unwrap();

    let res = router.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);

    let is_bl_after =
        bblbb_backend::users::blacklist::is_nickname_blacklisted(&pool, "涉政违规昵称_999")
            .await
            .unwrap();
    assert!(!is_bl_after, "移除后不应在黑名单中");

    close_pool(&pool).await;
    cleanup(&path);
}
