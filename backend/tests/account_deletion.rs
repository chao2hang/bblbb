//! M03-PROFILE-07：注销匿名化契约——
//! 1. users 行就地匿名化：username/email 替换为不可识别唯一派生值、
//!    display_name/bio/signature/头像/Cover/last_login_at 清空、
//!    status → deleted、deleted_at 写入、version +1；
//! 2. 断开可识别资料关系：user_preferences/user_privacy 行删除；
//! 3. 立即撤销全部 Session（revoked_at + revoke_reason='account_deleted'）；
//! 4. 公开讨论保留：posts 的 author_id 仍指向该行，内容不删；
//!    公开投影对 deleted 用户 404（前端以"已注销用户"降级展示）；
//! 5. 幂等：再次调用无副作用；审计/修订记录保留。

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
use bblbb_backend::users::deletion::anonymize_user;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

const KEY: &[u8] = b"test-encryption-key-material";
const PASSWORD: &str = "correct-password9";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-del-{}", uuid::Uuid::now_v7()));
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

/// 注册 + 登录（产生一个会话）+ PATCH 写偏好/隐私行。
async fn setup_user(app: &Router, pool: &DatabasePool, tag: &str) -> (String, String, String) {
    let email = format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple());
    let username = format!("{tag}_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
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
                .body(Body::from(
                    json!({ "username": username, "email": email, "password": PASSWORD })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 登录 → 会话
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
                    json!({ "identifier": email, "password": PASSWORD }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let cookie = resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();

    // session CSRF + 写偏好/隐私行
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/auth/csrf")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csrf: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    let csrf = csrf["token"].as_str().unwrap().to_string();

    let (_, me) = request_me(app, &cookie).await;
    let user_id = me["id"].as_str().unwrap().to_string();
    let version = me["version"].as_i64().unwrap();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/me")
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &cookie)
                .header("if-match", &version.to_string())
                .body(Body::from(
                    json!({ "signature": "s", "theme": "default", "email_visible_to": "registered" })
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "PATCH 写偏好/隐私行必须成功");

    let _ = pool;
    (user_id, cookie, username)
}

async fn request_me(app: &Router, cookie: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/me")
                .header("cookie", cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    (status, body)
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

/// 注销匿名化核心流程。
#[tokio::test]
async fn anonymize_user_preserves_discussion_and_disconnects_identity() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with_key(pool.clone());
    let (user_id, cookie, username) = setup_user(&app, &pool, "anon").await;

    // 造一条公开讨论（作者为该用户），内容必须保留
    let board_id: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM boards WHERE slug = 'general'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let post_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO posts (id, board_id, author_id, title, content, created_at, updated_at)
                 VALUES (?, ?, ?, '我的讨论', '正文', ?, ?)",
            )
            .bind(&post_id)
            .bind(board_id)
            .bind(&user_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 前置：会话未撤销、偏好/隐私行存在
    assert_eq!(
        db_scalar(
            &pool,
            "SELECT COUNT(*) FROM user_sessions WHERE user_id = ? AND revoked_at IS NULL",
            &user_id
        )
        .await,
        1,
        "前置：必须存在未撤销会话"
    );
    assert_eq!(
        db_scalar(
            &pool,
            "SELECT COUNT(*) FROM user_preferences WHERE user_id = ?",
            &user_id
        )
        .await,
        1,
        "前置：偏好行必须存在"
    );

    // 执行匿名化
    anonymize_user(&pool, &user_id)
        .await
        .expect("匿名化必须成功");

    // 1. users 行匿名化
    let (status_col, username_col, email_col, display_name, deleted_at): (
        String,
        String,
        String,
        Option<String>,
        Option<i64>,
    ) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT status, username_normalized, email_normalized, display_name, deleted_at FROM users WHERE id = ?",
        )
        .bind(&user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(status_col, "deleted", "状态必须为 deleted（终止态）");
    assert!(
        username_col.starts_with("deleted_user_") && !username_col.contains(&username),
        "用户名必须匿名化且不含原名: {username_col}"
    );
    assert!(
        email_col.ends_with("@deleted.invalid"),
        "邮箱必须匿名化: {email_col}"
    );
    assert!(display_name.is_none(), "display_name 必须清空");
    assert!(deleted_at.is_some(), "deleted_at 必须写入");

    // 2. 断开可识别资料关系
    assert_eq!(
        db_scalar(
            &pool,
            "SELECT COUNT(*) FROM user_preferences WHERE user_id = ?",
            &user_id
        )
        .await,
        0,
        "偏好行必须删除"
    );
    assert_eq!(
        db_scalar(
            &pool,
            "SELECT COUNT(*) FROM user_privacy WHERE user_id = ?",
            &user_id
        )
        .await,
        0,
        "隐私行必须删除"
    );

    // 3. Session 全部撤销
    assert_eq!(
        db_scalar(
            &pool,
            "SELECT COUNT(*) FROM user_sessions WHERE user_id = ? AND revoked_at IS NULL",
            &user_id
        )
        .await,
        0,
        "会话必须全部撤销"
    );

    // 4. 公开讨论保留：posts 仍在且 author_id 未变
    let (author, title): (String, String) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT author_id, title FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(author, user_id, "作者标识保留（指向匿名化行）");
    assert_eq!(title, "我的讨论", "讨论内容必须保留");

    // 公开投影 404（前端降级展示"已注销用户"）
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
    assert_eq!(
        resp.status(),
        StatusCode::NOT_FOUND,
        "已注销用户公开投影必须 404"
    );

    // 5. 幂等：再次调用无副作用
    anonymize_user(&pool, &user_id)
        .await
        .expect("幂等调用必须成功");
    assert_eq!(
        db_scalar(&pool, "SELECT COUNT(*) FROM users WHERE id = ?", &user_id).await,
        1,
        "用户行必须保留（匿名化而非删除）"
    );

    // 6. 修订/审计保留（不可删除）
    let rev = db_scalar(
        &pool,
        "SELECT COUNT(*) FROM profile_revisions WHERE user_id = ?",
        &user_id,
    )
    .await;
    assert!(rev >= 1, "profile_revisions 必须保留");

    let _ = cookie;
    close_pool(&pool).await;
    cleanup(&dir);
}
