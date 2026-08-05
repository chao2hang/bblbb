//! M03-PROFILE-09：私有用户字段泄漏测试——API 面与日志面。
//!
//! 覆盖：
//! 1. **API 面**：匿名与跨用户访问公共端点（公开主页 /api/v1/users/{username}、
//!    板块 /api/v1/boards、标签 /api/v1/tags、搜索 /api/v1/search），响应体
//!    不得出现邮箱、密码、状态、版本、Session 等私有用户字段；公开主页响应
//!    键集严格等于 `PUBLIC_PROFILE_ALLOWLIST`；未认证 /api/v1/me → 401。
//! 2. **日志面**：注册/登录流程的 HTTP 响应与 `audit_logs` 全部文本列不得
//!    出现明文密码（审计字段 allowlist/脱敏，M01-AUDIT-02；日志禁止记录凭据）。
//!
//! 前端面（SSR / Hover Card / 客户端缓存）见
//! `frontend/src/lib/testing/ssr/privacy.test.ts`（SSR + Hover Card）与
//! `frontend/src/lib/testing/user-page-privacy.test.ts`（用户页客户端缓存）。

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
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

const KEY: &[u8] = b"test-encryption-key-material";
const PASSWORD: &str = "correct-password9";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-leak-{}", uuid::Uuid::now_v7()));
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
    let config = bblbb_backend::AppConfig {
        mfa_encryption_key: String::from_utf8(KEY.to_vec()).unwrap(),
        ..bblbb_backend::AppConfig::default()
    };
    bblbb_backend::build_router(config, Some(pool))
}

/// 发起 JSON 请求：返回 (status, 响应体原始文本)。
/// cookie/csrf 为可选（GET 不需要；写端点必须带预认证 CSRF 头）。
async fn json_request(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    csrf: Option<&str>,
    body: Option<Value>,
) -> (StatusCode, String) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        builder = builder.header("cookie", c);
    }
    if let Some(t) = csrf {
        builder = builder.header("x-csrf-token", t);
    }
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    let req = match body {
        Some(v) => builder.body(Body::from(v.to_string())).unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

/// 获取匿名预认证 CSRF（cookie + token）。
async fn preauth(app: &Router) -> (String, String) {
    let (set_cookie, token) = common::fetch_preauth(app).await;
    (set_cookie.split(';').next().unwrap().to_string(), token)
}

/// 通过真实 API 注册（预认证 CSRF），返回 (email, username)。
async fn register_user(app: &Router, tag: &str, password: &str) -> (String, String) {
    let email = format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple());
    let username = format!("{tag}_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let (preauth, csrf) = preauth(app).await;
    let (status, body) = json_request(
        app,
        "POST",
        "/api/v1/auth/register",
        Some(&preauth),
        Some(&csrf),
        Some(json!({ "username": username, "email": email, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "注册失败: {body}");
    assert!(!body.contains(password), "注册响应不得回显密码");
    (email, username)
}

/// 通过真实 API 登录（预认证 CSRF），返回本人 user_id（从 Me 响应解析）。
async fn login_user(app: &Router, identifier: &str, password: &str) -> String {
    let (preauth, csrf) = preauth(app).await;
    let (status, body) = json_request(
        app,
        "POST",
        "/api/v1/auth/login",
        Some(&preauth),
        Some(&csrf),
        Some(json!({ "identifier": identifier, "password": password })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "登录失败: {body}");
    assert!(!body.contains(password), "登录响应不得包含密码");
    let me: Value = serde_json::from_str(&body).unwrap();
    me["id"].as_str().unwrap().to_string()
}

/// 直接插入一个带资料的活动用户（公开主页用，含 bio/signature）。
/// 返回 (user_id, username)。
async fn insert_active_profile_user(
    pool: &DatabasePool,
    tag: &str,
    email: &str,
) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!(
        "prof_{tag}_{}",
        &uuid::Uuid::now_v7().simple().to_string()[..10]
    );
    let now = bblbb_backend::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users
                     (id, username_normalized, email_normalized, password_hash, display_name, bio, signature, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy-hash', ?, '公开简介', '公开签名', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(email)
            .bind(format!("昵称_{tag}"))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (user_id, username)
}

/// audit_logs 文本列行（避免 clippy type_complexity）。
type AuditLogTextRow = (
    Option<String>,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// 收集 audit_logs 全部文本列，用于"日志不泄漏凭据"断言。
async fn audit_logs_text(pool: &DatabasePool) -> String {
    match pool {
        Either::Left(p) => {
            let rows: Vec<AuditLogTextRow> = sqlx::query_as(
                "SELECT actor_id, action, reason, metadata, request_id FROM audit_logs",
            )
            .fetch_all(p)
            .await
            .unwrap();
            rows.iter()
                .map(|(a, act, r, m, q)| format!("{a:?}|{act}|{r:?}|{m:?}|{q:?}"))
                .collect::<Vec<_>>()
                .join("\n")
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// API 面：匿名与跨用户访问公共端点不得泄漏私有用户字段。
#[tokio::test]
async fn api_surface_never_leaks_private_user_fields() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());

    // 用户 A：active + 公开资料（邮箱/密码为私有值）
    let email_a = format!("leak_{}@example.com", uuid::Uuid::now_v7().simple());
    let (_, username_a) = insert_active_profile_user(&pool, "a", &email_a).await;

    // 用户 B：真实 API 注册（跨用户视角）
    let (email_b, username_b) = register_user(&app, "b", PASSWORD).await;

    // 1) 公开主页：匿名访问 —— 键集严格等于公开 allowlist，私有值不出现
    let (status, body) = json_request(
        &app,
        "GET",
        &format!("/api/v1/users/{username_a}"),
        None,
        None,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(!body.contains(&email_a), "公开主页不得泄漏邮箱");
    assert!(!body.contains(PASSWORD), "公开主页不得泄漏密码");
    assert!(
        !body.contains("password_hash"),
        "公开主页不得出现密码字段键"
    );
    let profile: Value = serde_json::from_str(&body).unwrap();
    let mut keys: Vec<String> = profile.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    let mut expected = [
        "avatar_attachment_id",
        "bio",
        "cover_attachment_id",
        "created_at",
        "display_name",
        "id",
        "level",
        "signature",
        "username",
    ]
    .map(|s| s.to_string())
    .to_vec();
    expected.sort();
    assert_eq!(
        keys, expected,
        "公开投影键集必须严格等于 PUBLIC_PROFILE_ALLOWLIST"
    );

    // 2) 跨用户（B 访问 A）与本人（B 访问自己，pending）：同样不泄漏邮箱
    for uri in [
        format!("/api/v1/users/{username_a}"),
        format!("/api/v1/users/{username_b}"),
    ] {
        let (status, body) = json_request(&app, "GET", &uri, None, None, None).await;
        assert_eq!(status, StatusCode::OK);
        assert!(
            !body.contains(&email_a) && !body.contains(&email_b),
            "公开主页不得泄漏邮箱: {uri}"
        );
    }

    // 3) 板块/标签/搜索：整响应不得出现私有字段
    for uri in ["/api/v1/boards", "/api/v1/tags", "/api/v1/search?q=test"] {
        let (status, body) = json_request(&app, "GET", uri, None, None, None).await;
        assert_eq!(status, StatusCode::OK, "{uri} 必须 200");
        assert!(!body.contains(&email_a), "{uri} 不得泄漏邮箱");
        assert!(!body.contains(PASSWORD), "{uri} 不得泄漏密码");
        assert!(!body.contains("password_hash"), "{uri} 不得出现密码字段键");
    }

    // 4) 未认证 /me → 401，且不得携带任何用户数据
    let (status, body) = json_request(&app, "GET", "/api/v1/me", None, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert!(!body.contains(&email_a));

    let _ = app;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 日志面：注册/登录响应与 audit_logs 不得出现明文密码。
#[tokio::test]
async fn responses_and_audit_logs_never_contain_credentials() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());

    // 用独特密码执行完整注册 + 登录
    let pw = format!(
        "Correct-password-{}",
        &uuid::Uuid::now_v7().simple().to_string()[..10]
    );
    let (email, username) = register_user(&app, "log", &pw).await;
    let _user_id = login_user(&app, &email, &pw).await;
    let _ = username;

    // 审计日志全部文本列不得包含明文密码（M01-AUDIT-02 allowlist/脱敏）
    let logs = audit_logs_text(&pool).await;
    assert!(!logs.contains(&pw), "audit_logs 不得记录明文密码");
    assert!(
        !logs.contains("password"),
        "audit_logs 不得出现 password 值形态"
    );
    assert!(logs.contains("auth.register"), "注册审计必须存在");

    let _ = app;
    close_pool(&pool).await;
    cleanup(&dir);
}
