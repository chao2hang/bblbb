//! M02-SESSION-05：Session 生命周期——idle/absolute timeout、当前登出、
//! 全部登出、设备列表与逐设备撤销（含 CSRF 校验的 DELETE）。

mod common;

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::session::list_sessions as list_sessions_service;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-life-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

const PASSWORD: &str = "correct-password";

async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
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
            .bind(format!("{tag}_user"))
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
    (user_id, email)
}

/// 登录并返回 session cookie 值。
async fn login_cookie(app: &Router, email: &str, ip: &str) -> String {
    // M02-SESSION-08：登录属预认证写路径，必须先获取匿名预认证 CSRF 状态
    let (cookie, csrf) = common::fetch_preauth(app).await;
    let body = json!({ "identifier": email, "password": PASSWORD });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", ip)
                .header("cookie", cookie)
                .header("x-csrf-token", csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

/// 获取 CSRF token（认证用户从会话派生）。
async fn get_csrf(app: &Router, cookie: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", cookie)
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

/// GET /api/v1/auth/sessions
async fn get_sessions(app: &Router, cookie: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/sessions")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

/// DELETE 带 CSRF。
async fn delete_with_csrf(
    app: &Router,
    uri: &str,
    cookie: &str,
    csrf: &str,
) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri(uri)
                .header("cookie", cookie)
                .header("x-csrf-token", csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn session_count(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_sessions WHERE user_id = ?")
                .bind(user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn active_session_count(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_sessions WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// absolute timeout：session 超过最长有效期后认证请求被拒绝（401）。
#[tokio::test]
async fn session_expires_by_absolute_timeout() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "alice").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie = login_cookie(&app, &email, "198.51.100.1").await;

    // 手动把 absolute_expires_at 调到过去
    let past = now_millis() - 1000;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET absolute_expires_at = ? WHERE user_id = ?")
                .bind(past)
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let resp = get_sessions(&app, &cookie).await;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "absolute 过期必须 401"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// idle timeout：滑动窗口在请求时刷新；超过 idle 后认证失败。
#[tokio::test]
async fn session_idle_timeout_slides_on_activity() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "bob").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie = login_cookie(&app, &email, "198.51.100.1").await;

    // 首次请求刷新 last_seen/idle
    let resp = get_sessions(&app, &cookie).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let (last_seen, idle): (i64, i64) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT last_seen_at, idle_expires_at FROM user_sessions WHERE user_id = ?",
        )
        .bind(&user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(idle > now_millis(), "请求后 idle 必须顺延到未来");

    // 把 idle_expires_at 调到过去 → 认证失败
    let past = now_millis() - 1000;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET idle_expires_at = ? WHERE user_id = ?")
                .bind(past)
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let resp = get_sessions(&app, &cookie).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "idle 过期必须 401");

    let _ = last_seen;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 设备列表：多个登录返回全部有效会话（含字段），撤销后不再出现。
#[tokio::test]
async fn list_sessions_shows_active_devices() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "carol").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie_a = login_cookie(&app, &email, "198.51.100.1").await;
    let cookie_b = login_cookie(&app, &email, "198.51.100.2").await;

    let resp = get_sessions(&app, &cookie_a).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let list = body.as_array().unwrap();
    assert_eq!(list.len(), 2, "两个设备登录应有两条会话");
    for item in list {
        assert!(item["id"].as_str().unwrap().len() >= 36);
        assert!(item["created_at"].as_i64().is_some());
        assert!(item["last_seen_at"].as_i64().is_some());
        assert!(item["absolute_expires_at"].as_i64().is_some());
        assert_eq!(item["version"], 0);
    }
    // service 层断言：撤销后列表只剩 1 个
    let sessions = list_sessions_service(&pool, &user_id).await.unwrap();
    assert_eq!(sessions.len(), 2);
    assert!(sessions.iter().any(|s| s.user_agent.is_none()));

    let _ = cookie_b;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 当前登出：只撤销当前 session，其他设备仍有效。
#[tokio::test]
async fn logout_current_revokes_only_current() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "dave").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie_a = login_cookie(&app, &email, "198.51.100.1").await;
    let cookie_b = login_cookie(&app, &email, "198.51.100.2").await;
    assert_eq!(active_session_count(&pool, &user_id).await, 2);
    let csrf = get_csrf(&app, &cookie_a).await;

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/auth/session")
                .header("cookie", &cookie_a)
                .header("x-csrf-token", &csrf)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    assert_eq!(
        active_session_count(&pool, &user_id).await,
        1,
        "只撤销当前设备"
    );
    assert_eq!(session_count(&pool, &user_id).await, 2, "撤销保留历史行");

    // 其他设备仍可访问
    let resp = get_sessions(&app, &cookie_b).await;
    assert_eq!(resp.status(), StatusCode::OK);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 全部登出：撤销全部设备并清 cookie；之后所有 session 失效。
#[tokio::test]
async fn logout_all_revokes_everything() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "erin").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie_a = login_cookie(&app, &email, "198.51.100.1").await;
    let _cookie_b = login_cookie(&app, &email, "198.51.100.2").await;
    let csrf = get_csrf(&app, &cookie_a).await;

    let resp = delete_with_csrf(&app, "/api/v1/auth/sessions", &cookie_a, &csrf).await;
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    assert_eq!(
        active_session_count(&pool, &user_id).await,
        0,
        "全部设备必须撤销"
    );

    let resp = get_sessions(&app, &cookie_a).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 逐设备撤销：撤销指定 session；其他用户的 session 返回 404。
#[tokio::test]
async fn revoke_specific_session() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "frank").await;
    let (other_id, other_email) = insert_user(&pool, "grace").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let cookie_a = login_cookie(&app, &email, "198.51.100.1").await;
    let _cookie_b = login_cookie(&app, &email, "198.51.100.2").await;
    let _other_cookie = login_cookie(&app, &other_email, "198.51.100.3").await;
    // get_csrf 会刷新 cookie_a 的 last_seen → list 第一项即当前 session
    let csrf = get_csrf(&app, &cookie_a).await;

    // 取 A 的「其他设备」session（当前 session 之外的那个）
    let sessions = list_sessions_service(&pool, &user_id).await.unwrap();
    assert_eq!(sessions.len(), 2);
    let current_id = sessions[0].id.clone();
    let target = sessions[1].id.clone();
    assert_ne!(target, current_id);

    let resp = delete_with_csrf(
        &app,
        &format!("/api/v1/auth/sessions/{target}"),
        &cookie_a,
        &csrf,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK, "撤销自己的设备必须成功");
    assert_eq!(
        active_session_count(&pool, &user_id).await,
        1,
        "只剩当前设备"
    );

    // 撤销其他用户的 session → 404（不属于本人）；cookie_a 仍有效
    let other_sessions = list_sessions_service(&pool, &other_id).await.unwrap();
    let other_target = other_sessions[0].id.clone();
    let resp = delete_with_csrf(
        &app,
        &format!("/api/v1/auth/sessions/{other_target}"),
        &cookie_a,
        &csrf,
    )
    .await;
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "他人 session 必须 404"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未认证请求：无 session cookie → 401。
#[tokio::test]
async fn sessions_endpoints_require_authentication() {
    let (pool, dir) = pool_with_migrations().await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    let resp = get_sessions(&app, "bogus").await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 「记住我」（M02-UX-03）：remember=true 的登录签发 30 天绝对 / 7 天空闲
/// 会话；默认登录保持 7 天绝对 / 30 分钟空闲。
#[tokio::test]
async fn remember_me_session_gets_extended_timeouts() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_user(&pool, "remember").await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));

    // 默认登录（无 remember）。
    let _default_cookie = login_cookie(&app, &email, "198.51.100.2").await;
    let (d_idle, d_abs): (i64, i64) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT idle_expires_at - created_at, absolute_expires_at - created_at
             FROM user_sessions WHERE user_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        (d_idle - 30 * 60 * 1000).abs() < 5_000,
        "默认会话空闲超时应约 30 分钟，实际 {d_idle}"
    );
    assert!(
        (d_abs - 7 * 24 * 3600 * 1000).abs() < 5_000,
        "默认会话绝对超时应约 7 天，实际 {d_abs}"
    );

    // remember=true 登录。
    let (cookie, csrf) = common::fetch_preauth(&app).await;
    let body = json!({ "identifier": email, "password": PASSWORD, "remember": true });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-forwarded-for", "198.51.100.2")
                .header("cookie", cookie)
                .header("x-csrf-token", csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let _set_cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();

    let (r_idle, r_abs): (i64, i64) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT idle_expires_at - created_at, absolute_expires_at - created_at
             FROM user_sessions WHERE user_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(&user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        (r_idle - 7 * 24 * 3600 * 1000).abs() < 5_000,
        "记住我空闲超时应约 7 天，实际 {r_idle}"
    );
    assert!(
        (r_abs - 30 * 24 * 3600 * 1000).abs() < 5_000,
        "记住我绝对超时应约 30 天，实际 {r_abs}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
