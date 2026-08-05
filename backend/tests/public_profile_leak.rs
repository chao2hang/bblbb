//! M03-PROFILE-02：公开用户投影泄漏测试——
//! 注册用户并填充敏感字段（邮箱、封禁状态、最后登录、注销请求时间），
//! 经公开端点 `GET /api/v1/users/{username}` 断言：
//! 1. 响应键集 ⊆ `PUBLIC_PROFILE_ALLOWLIST`（邮箱/IP/Session/处罚/审计字段
//!    不得出现在响应 JSON 键中）；
//! 2. 响应文本不含敏感值（邮箱字符串、状态码、IP、session 字样）；
//! 3. 已注销（deleted）用户公开查询返回 404（不泄漏存在性之外的任何信息）。

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
use bblbb_backend::outbox::now_millis;
use bblbb_backend::users::dto::PUBLIC_PROFILE_ALLOWLIST;
use bblbb_backend::{build_router, AppConfig};
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
    let config = AppConfig {
        mfa_encryption_key: String::from_utf8(KEY.to_vec()).unwrap(),
        ..AppConfig::default()
    };
    build_router(config, Some(pool))
}

/// 注册用户（预认证 CSRF + 防枚举统一 201），返回 username。
async fn register_user(app: &Router, username: &str, email: &str) {
    let (preauth, preauth_csrf) = common::fetch_preauth(app).await;
    let preauth = preauth.split(';').next().unwrap().to_string();
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/register")
                .header("content-type", "application/json")
                .header("x-csrf-token", preauth_csrf)
                .header("cookie", preauth)
                .header("x-forwarded-for", "203.0.113.99")
                .body(Body::from(
                    json!({ "username": username, "email": email, "password": PASSWORD })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED, "注册必须 201");
}

async fn get_public_user(app: &Router, username: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(format!("/api/v1/users/{username}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string())
}

/// 直接改库填充敏感字段：邮箱（仅 email_normalized 列）、封禁状态、
/// 最后登录、注销请求时间、头像/Cover 附件引用。
async fn mark_sensitive(pool: &DatabasePool, username: &str, email: &str, status: &str) {
    let now = now_millis();
    let email_normalized = email.to_lowercase();
    let avatar = uuid::Uuid::now_v7().to_string();
    let cover = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users
                 SET email_normalized = ?, status = ?, last_login_at = ?, delete_requested_at = ?,
                     avatar_attachment_id = ?, cover_attachment_id = ?
                 WHERE username_normalized = ?",
            )
            .bind(&email_normalized)
            .bind(status)
            .bind(now)
            .bind(now)
            .bind(&avatar)
            .bind(&cover)
            .bind(username)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 公开端点必须 200，响应键集 ⊆ allowlist，且文本不含任何敏感值。
#[tokio::test]
async fn public_user_never_leaks_sensitive_fields() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    let username = format!("leak_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let email = format!(
        "secret-{}@example.com",
        &uuid::Uuid::now_v7().simple().to_string()[..8]
    );
    register_user(&app, &username, &email).await;

    // 填充敏感字段：封禁 + 邮箱 + 登录/注销时间（模拟"最坏情况"的库内容）
    mark_sensitive(&pool, &username, &email, "banned").await;

    let (status, body) = get_public_user(&app, &username).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "封禁用户公开资料仍可读（不泄漏状态）: {body}"
    );
    let parsed: Value = serde_json::from_str(&body).expect("响应必须为 JSON");

    // 1. 键集 ⊆ allowlist
    let keys = parsed.as_object().unwrap();
    for key in keys.keys() {
        assert!(
            PUBLIC_PROFILE_ALLOWLIST.contains(&key.as_str()),
            "公开投影出现 allowlist 之外字段: {key}"
        );
    }

    // 1b. Cover/头像只返回稳定附件 UUID（M03-PROFILE-05），绝不返回 URL
    for field in ["avatar_attachment_id", "cover_attachment_id"] {
        let value = parsed[field].as_str().expect("附件引用必须存在");
        assert!(
            uuid::Uuid::parse_str(value).is_ok(),
            "{field} 必须是 UUID: {value}"
        );
        assert!(
            !value.contains("://") && !value.contains("signed"),
            "{field} 不得是远程/签名 URL: {value}"
        );
    }

    // 2. 文本不含敏感值
    for needle in [
        email.as_str(),        // 邮箱
        "banned",              // 封禁状态码
        "session",             // Session
        "203.0.113.99",        // 注册 IP
        "password_hash",       // 凭据列名
        "audit",               // 审计
        "sanction",            // 处罚
        "delete_requested_at", // 注销请求
        "last_login_at",       // 最后登录
    ] {
        assert!(
            !body.to_lowercase().contains(needle),
            "公开投影文本泄漏敏感值: {needle}"
        );
    }
    // 用户名/昵称/等级本身必须可见（allowlist 内公开字段）
    assert!(body.contains(&username), "公开投影必须含用户名");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 已注销（deleted）用户公开查询必须 404，且响应不含用户标识。
#[tokio::test]
async fn deleted_user_public_lookup_returns_404() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    let username = format!("gone_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let email = format!(
        "gone-{}@example.com",
        &uuid::Uuid::now_v7().simple().to_string()[..8]
    );
    register_user(&app, &username, &email).await;
    mark_sensitive(&pool, &username, &email, "deleted").await;

    let (status, body) = get_public_user(&app, &username).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "已注销用户必须 404: {body}");
    // instance 按 RFC 7807 回显请求路径（请求方自供的用户名），不算泄漏；
    // 断言 404 为标准 Problem 且不含邮箱等敏感值。
    assert!(body.contains("not_found"), "404 必须是标准 Problem: {body}");
    assert!(
        !body.contains(&email.to_lowercase()),
        "404 响应不得泄漏邮箱: {body}"
    );
    assert!(!body.contains("password_hash"), "404 响应不得泄漏凭据列名");

    close_pool(&pool).await;
    cleanup(&dir);
}
