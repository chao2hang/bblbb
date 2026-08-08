//! M03-PROFILE cover 端点集成测试（真实 HTTP 路径，SQLite）。
//!
//! 覆盖：POST/DELETE /api/v1/me/profile-cover 与
//! GET /api/v1/users/{user_id}/profile-cover —— 附件归属/就绪校验、设置、
//! 清除、公开投影与 204 语义。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

#[path = "common/mod.rs"]
mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-cover-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

/// 直接插入一个 ready 附件（跳过上传流程；状态机由 storage 测试覆盖）。
async fn insert_ready_attachment(pool: &DatabasePool, owner_id: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO attachments (id, owner_id, storage_backend, storage_key, original_name, media_type, size_bytes, sha256, status, quota_bytes_charged, is_public, ref_count, created_at)
                 VALUES (?, ?, 'local', ?, 'cover.jpg', 'image/jpeg', 1024, 'x', 'ready', 1024, 0, 0, ?)",
            )
            .bind(&id)
            .bind(owner_id)
            .bind(format!("cover/{id}.jpg"))
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    id
}

async fn session_csrf(app: &axum::Router, session: &str) -> String {
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

#[tokio::test]
async fn profile_cover_set_get_clear_lifecycle() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user_id = insert_user(&pool, "cover").await;
    let other_id = insert_user(&pool, "other").await;
    let attachment_id = insert_ready_attachment(&pool, &user_id).await;
    let other_attachment_id = insert_ready_attachment(&pool, &other_id).await;

    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let session = common::direct_session_cookie(&pool, &user_id).await;
    let csrf = session_csrf(&app, &session).await;

    // POST 设置（他人附件 → 400；本人附件 → 204）。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/me/profile-cover")
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &session)
                .body(Body::from(
                    json!({"attachment_id": other_attachment_id, "alt_text": "x", "position": "center"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "他人附件必须拒绝");

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/me/profile-cover")
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &session)
                .body(Body::from(
                    json!({"attachment_id": attachment_id, "alt_text": "风景封面", "position": "center top"}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "本人附件设置成功");

    // GET 公开投影（本人视角）。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/users/{user_id}/profile-cover"))
                .header("cookie", &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["attachment_id"], attachment_id);
    assert_eq!(body["alt_text"], "风景封面");
    assert!(body["content_url"]
        .as_str()
        .unwrap()
        .contains(&attachment_id));

    // 数据库落库校验（cover_attachment_id + 引用计数）。
    match &pool {
        Either::Left(p) => {
            let stored: Option<String> =
                sqlx::query_scalar("SELECT cover_attachment_id FROM users WHERE id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(stored.as_deref(), Some(attachment_id.as_str()));
            let refs: i64 = sqlx::query_scalar("SELECT ref_count FROM attachments WHERE id = ?")
                .bind(&attachment_id)
                .fetch_one(p)
                .await
                .unwrap();
            assert_eq!(refs, 1, "封面引用必须计入 ref_count");
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // DELETE 清除 → 204；再 GET → 204。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("DELETE")
                .uri("/api/v1/me/profile-cover")
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &session)
                .body(Body::from(
                    json!({"attachment_id": attachment_id, "alt_text": "x", "position": "x"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/users/{user_id}/profile-cover"))
                .header("cookie", &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT, "清除后封面投影 204");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn profile_cover_requires_auth_and_validates_body() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user_id = insert_user(&pool, "authz").await;
    let attachment_id = insert_ready_attachment(&pool, &user_id).await;

    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let session = common::direct_session_cookie(&pool, &user_id).await;
    let csrf = session_csrf(&app, &session).await;

    // 未认证 → 401。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/me/profile-cover")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({"attachment_id": attachment_id}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 缺少必填字段 → 400。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/me/profile-cover")
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &session)
                .body(Body::from("{}".to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    // 非 uuid attachment_id → 400。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/me/profile-cover")
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &session)
                .body(Body::from(
                    json!({"attachment_id": "not-a-uuid", "alt_text": "x", "position": "x"})
                        .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

    close_pool(&pool).await;
    cleanup(&dir);
}
