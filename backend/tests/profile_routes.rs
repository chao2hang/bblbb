//! M03-PROFILE-03：资料读取与更新契约——
//! 1. PATCH /api/v1/me 更新昵称/简介/签名/时区/主题/隐私，200 回显新值；
//! 2. 持久化断言：users（bio/signature）、user_preferences（时区/主题，
//!    行首访惰性创建）、user_privacy（隐私，同上）、profile_revisions
//!    （每次写操作追加修订，UNIQUE(user_id, revision) 连续递增）；
//! 3. PATCH 语义：只更新出现字段，缺失字段保持原值；
//! 4. 非法值：theme/email_visible_to/profile_visible_to 拒绝 400；
//! 5. 主题端点 PUT 持久化 + GET 读取。

mod common;

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
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

const KEY: &[u8] = b"test-encryption-key-material";
const PASSWORD: &str = "correct-password9";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-prof-{}", uuid::Uuid::now_v7()));
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

fn app_with_key(pool: DatabasePool) -> Router {
    let config = AppConfig {
        mfa_encryption_key: String::from_utf8(KEY.to_vec()).unwrap(),
        ..AppConfig::default()
    };
    build_router(config, Some(pool))
}

async fn request(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: &str,
    csrf: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if !cookie.is_empty() {
        builder = builder.header("cookie", cookie);
    }
    if !csrf.is_empty() {
        builder = builder.header("x-csrf-token", csrf);
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let req = builder
        .body(Body::from(body.map(|v| v.to_string()).unwrap_or_default()))
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, body)
}

/// 注册并登录，返回会话 Cookie 与 session CSRF token。
async fn register_and_login(app: &Router, tag: &str) -> (String, String) {
    let email = format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple());
    let username = format!("{tag}_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let (preauth, preauth_csrf) = common::fetch_preauth(app).await;
    let preauth = preauth.split(';').next().unwrap().to_string();
    let (status, _) = request(
        app,
        "POST",
        "/api/v1/auth/register",
        &preauth,
        &preauth_csrf,
        Some(json!({ "username": username, "email": email, "password": PASSWORD })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "注册必须 201");

    let (status, login_body, cookie) = login(app, &email, PASSWORD).await;
    assert_eq!(status, StatusCode::OK, "登录必须 200: {login_body}");
    let (_, csrf_body) = request(app, "GET", "/api/v1/auth/csrf", &cookie, "", None).await;
    let csrf = csrf_body["token"].as_str().unwrap().to_string();
    (cookie, csrf)
}

async fn login(app: &Router, identifier: &str, password: &str) -> (StatusCode, Value, String) {
    let (preauth, preauth_csrf) = common::fetch_preauth(app).await;
    let preauth = preauth.split(';').next().unwrap().to_string();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-csrf-token", preauth_csrf)
                .header("cookie", preauth)
                .body(Body::from(
                    json!({ "identifier": identifier, "password": password }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let cookie = if status == StatusCode::OK {
        resp.headers()
            .get("set-cookie")
            .map(|v| v.to_str().unwrap().split(';').next().unwrap().to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, body, cookie)
}

async fn db_scalar(pool: &DatabasePool, sql: &str, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// PATCH /me 全字段更新：回显 + 三表持久化 + 修订追加。
#[tokio::test]
async fn patch_me_updates_all_profile_fields_persistently() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    let (session, csrf) = register_and_login(&app, "prof").await;
    let user_id = {
        let (_, me) = request(&app, "GET", "/api/v1/me", &session, "", None).await;
        me["id"].as_str().unwrap().to_string()
    };

    let (status, me) = request(
        &app,
        "PATCH",
        "/api/v1/me",
        &session,
        &csrf,
        Some(json!({
            "display_name": "新昵称",
            "bio": "我的简介",
            "signature": "个性签名",
            "timezone": "Asia/Shanghai",
            "theme": "dark",
            "email_visible_to": "registered",
            "profile_visible_to": "nobody"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "PATCH /me 必须 200: {me}");
    assert_eq!(me["display_name"], "新昵称");
    assert_eq!(me["bio"], "我的简介");
    assert_eq!(me["signature"], "个性签名");
    assert_eq!(me["timezone"], "Asia/Shanghai");
    assert_eq!(me["theme_name"], "dark");
    assert_eq!(me["email_visible_to"], "registered");
    assert_eq!(me["profile_visible_to"], "nobody");

    // GET /me 反映持久化
    let (_, me2) = request(&app, "GET", "/api/v1/me", &session, "", None).await;
    assert_eq!(me2["bio"], "我的简介");
    assert_eq!(me2["theme_name"], "dark");
    assert_eq!(me2["profile_visible_to"], "nobody");

    // 三表持久化
    match &pool {
        Either::Left(p) => {
            let (bio, signature): (Option<String>, Option<String>) =
                sqlx::query_as("SELECT bio, signature FROM users WHERE id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(bio.as_deref(), Some("我的简介"));
            assert_eq!(signature.as_deref(), Some("个性签名"));

            let (timezone, theme_name): (String, Option<String>) = sqlx::query_as(
                "SELECT timezone, theme_name FROM user_preferences WHERE user_id = ?",
            )
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(timezone, "Asia/Shanghai");
            assert_eq!(theme_name.as_deref(), Some("dark"));

            let (email_visible_to, profile_visible_to): (String, String) = sqlx::query_as(
                "SELECT email_visible_to, profile_visible_to FROM user_privacy WHERE user_id = ?",
            )
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(email_visible_to, "registered");
            assert_eq!(profile_visible_to, "nobody");
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 修订：第一次写 → revision 1，含 7 个字段摘要
    let rev_count = db_scalar(
        &pool,
        "SELECT COUNT(*) FROM profile_revisions WHERE user_id = ?",
        &user_id,
    )
    .await;
    assert_eq!(rev_count, 1, "每次资料写操作必须追加一条修订");
    let (revision, changes_json): (i64, String) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT revision, changes_json FROM profile_revisions WHERE user_id = ?")
                .bind(&user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(revision, 1);
    assert!(changes_json.contains("display_name"));
    assert!(changes_json.contains("profile_visible_to"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// PATCH 语义：只更新出现字段，缺失字段保持原值；且不产生多余修订。
#[tokio::test]
async fn patch_me_is_partial_update() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    let (session, csrf) = register_and_login(&app, "part").await;
    let user_id = {
        let (_, me) = request(&app, "GET", "/api/v1/me", &session, "", None).await;
        me["id"].as_str().unwrap().to_string()
    };

    let (status, _) = request(
        &app,
        "PATCH",
        "/api/v1/me",
        &session,
        &csrf,
        Some(json!({ "signature": "only-signature", "theme": "light" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // 未出现的字段保持默认/原值；出现的字段已更新
    let (_, me) = request(&app, "GET", "/api/v1/me", &session, "", None).await;
    assert_eq!(me["signature"], "only-signature");
    assert_eq!(me["theme_name"], "light");
    assert_eq!(me["timezone"], "UTC", "缺失时区必须保持原值");
    assert_eq!(me["email_visible_to"], "nobody", "缺失隐私必须保持原值");
    assert_eq!(me["display_name"], Value::Null, "未设置昵称保持 NULL");

    // 惰性建行：即使只写 signature，preferences/privacy 行也已创建
    let pref_rows = db_scalar(
        &pool,
        "SELECT COUNT(*) FROM user_preferences WHERE user_id = ?",
        &user_id,
    )
    .await;
    let privacy_rows = db_scalar(
        &pool,
        "SELECT COUNT(*) FROM user_privacy WHERE user_id = ?",
        &user_id,
    )
    .await;
    assert_eq!(pref_rows, 1, "user_preferences 行必须惰性创建");
    assert_eq!(privacy_rows, 1, "user_privacy 行必须惰性创建");

    // 空 PATCH（无字段）→ 不写库、不写修订
    let before = db_scalar(
        &pool,
        "SELECT COUNT(*) FROM profile_revisions WHERE user_id = ?",
        &user_id,
    )
    .await;
    let (status, _) = request(
        &app,
        "PATCH",
        "/api/v1/me",
        &session,
        &csrf,
        Some(json!({})),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "空 PATCH 应 200 无副作用");
    let after = db_scalar(
        &pool,
        "SELECT COUNT(*) FROM profile_revisions WHERE user_id = ?",
        &user_id,
    )
    .await;
    assert_eq!(before, after, "空 PATCH 不得写修订");
    assert_eq!(after, 1, "仅 signature+theme 一次写 = 1 条修订");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 非法枚举值 → 400；修订号连续递增。
#[tokio::test]
async fn patch_me_rejects_invalid_values() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    let (session, csrf) = register_and_login(&app, "inv").await;
    let user_id = {
        let (_, me) = request(&app, "GET", "/api/v1/me", &session, "", None).await;
        me["id"].as_str().unwrap().to_string()
    };

    for bad in [
        json!({ "theme": "neon" }),
        json!({ "email_visible_to": "enemies" }),
        json!({ "profile_visible_to": "friends-only" }),
        json!({ "display_name": "" }),
    ] {
        let (status, body) = request(&app, "PATCH", "/api/v1/me", &session, &csrf, Some(bad)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "非法值必须 400: {body}");
    }
    // 全部被拒 → 无修订
    let rev = db_scalar(
        &pool,
        "SELECT COUNT(*) FROM profile_revisions WHERE user_id = ?",
        &user_id,
    )
    .await;
    assert_eq!(rev, 0, "非法更新不得写修订");

    // 连续两次合法更新 → revision 1、2
    let (status, _) = request(
        &app,
        "PATCH",
        "/api/v1/me",
        &session,
        &csrf,
        Some(json!({ "signature": "a" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, _) = request(
        &app,
        "PATCH",
        "/api/v1/me",
        &session,
        &csrf,
        Some(json!({ "bio": "b" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (r1, r2): (i64, i64) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT MIN(revision), MAX(revision) FROM profile_revisions WHERE user_id = ?",
        )
        .bind(&user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!((r1, r2), (1, 2), "修订号必须连续递增");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 主题端点：PUT 持久化，GET 读取。
#[tokio::test]
async fn theme_preference_persists_across_requests() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    let (session, csrf) = register_and_login(&app, "theme").await;

    // 默认 default
    let (status, body) = request(
        &app,
        "GET",
        "/api/v1/me/preferences/theme",
        &session,
        "",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["theme"], "default");

    // PUT dark → 持久化
    let (status, body) = request(
        &app,
        "PUT",
        "/api/v1/me/preferences/theme",
        &session,
        &csrf,
        Some(json!({ "theme": "dark" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["theme"], "dark");

    // GET 反映
    let (_, body) = request(
        &app,
        "GET",
        "/api/v1/me/preferences/theme",
        &session,
        "",
        None,
    )
    .await;
    assert_eq!(body["theme"], "dark", "GET 必须读取持久化主题");

    // 非法主题 → 400
    let (status, _) = request(
        &app,
        "PUT",
        "/api/v1/me/preferences/theme",
        &session,
        &csrf,
        Some(json!({ "theme": "neon" })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    close_pool(&pool).await;
    cleanup(&dir);
}
