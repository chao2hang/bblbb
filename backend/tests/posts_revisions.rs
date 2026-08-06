//! M04-POSTS-11：revisions 列表/详情——作者看自身版本（含正文）、普通成员只见
//! 元数据、管理查看写审计（SQLite）。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::service::{edit_post, publish_new_post, EditPostInput};
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

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-prv-{}", uuid::Uuid::now_v7()));
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

async fn insert_author(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 25 * 3600 * 1000)
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

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role: &str) {
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role)
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

async fn publish(pool: &DatabasePool, author: &str, title: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: title.to_string(),
            markdown: "v1 正文".to_string(),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("rv-{}-{}", title, uuid::Uuid::now_v7().simple()),
        },
        5,
        now_millis(),
    )
    .unwrap();
    publish_new_post(pool, &cmd, author, now_millis())
        .await
        .unwrap()
        .post
        .id
}

/// 作者编辑一次 → 产生第二个修订（version=2）。
async fn edit_once(pool: &DatabasePool, author: &str, post_id: &str) {
    edit_post(
        pool,
        post_id,
        author,
        &EditPostInput {
            title: None,
            markdown: Some("v2 正文".to_string()),
            expected_version: 1,
            change_reason: Some("修订二".to_string()),
        },
        now_millis(),
    )
    .await
    .unwrap();
}

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn get(app: &axum::Router, uri: &str, session: &str) -> (StatusCode, Value) {
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
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn author_sees_own_revisions_with_body() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let post_id = publish(&pool, &author, "历史帖").await;
    edit_once(&pool, &author, &post_id).await;

    let (status, body) = get(
        &app,
        &format!("/api/v1/posts/{post_id}/revisions"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "作者列表必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "两个修订");
    assert_eq!(items[0]["version"], 1);
    assert_eq!(items[1]["version"], 2);
    assert!(
        items.iter().all(|it| it["body_html"].is_string()),
        "作者可见正文: {body}"
    );

    // 详情（作者）
    let rev_id = items[1]["id"].as_str().unwrap().to_string();
    let (status, detail) = get(
        &app,
        &format!("/api/v1/posts/{post_id}/revisions/{rev_id}"),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "作者详情必须 200");
    assert_eq!(detail["version"], 2);
    assert!(
        detail["body_html"].as_str().unwrap().contains("v2 正文"),
        "详情含 v2 正文: {detail}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn stranger_sees_metadata_without_body() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner2").await;
    let stranger = insert_author(&pool, "reader").await;
    let stranger_session = common::direct_session_cookie(&pool, &stranger).await;
    let post_id = publish(&pool, &author, "公开帖").await;
    edit_once(&pool, &author, &post_id).await;

    let (status, body) = get(
        &app,
        &format!("/api/v1/posts/{post_id}/revisions"),
        &stranger_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "成员列表必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(
        items.iter().all(|it| it["body_html"].is_null()),
        "非作者只见元数据（无正文）: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn moderator_view_writes_audit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner3").await;
    let admin = insert_author(&pool, "mod").await;
    assign_global_role(&pool, &admin, "administrator").await;
    common::enroll_totp(&pool, &admin).await;
    let admin_session = common::direct_session_cookie(&pool, &admin).await;
    let post_id = publish(&pool, &author, "被审帖").await;
    edit_once(&pool, &author, &post_id).await;

    let (status, body) = get(
        &app,
        &format!("/api/v1/posts/{post_id}/revisions"),
        &admin_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let rev_id = body["items"][1]["id"].as_str().unwrap().to_string();

    let (status, detail) = get(
        &app,
        &format!("/api/v1/posts/{post_id}/revisions/{rev_id}"),
        &admin_session,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "管理查看详情必须 200");
    assert!(
        detail["body_html"].as_str().unwrap().contains("v2 正文"),
        "管理可见正文: {detail}"
    );

    // 审计：管理查看修订写 post.revision.read
    let audit: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM audit_logs WHERE action = 'post.revision.read' AND actor_id = ? AND target_id = ?",
            )
            .bind(&admin)
            .bind(&rev_id)
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit, 1, "管理查看修订必须写审计日志");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn revision_detail_404_for_wrong_post_or_missing() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner4").await;
    let session = common::direct_session_cookie(&pool, &author).await;
    let post_id = publish(&pool, &author, "详情404").await;

    // 不存在的修订 → 404
    let (status, _) = get(
        &app,
        &format!("/api/v1/posts/{post_id}/revisions/{}", uuid::Uuid::now_v7()),
        &session,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在修订必须 404");

    close_pool(&pool).await;
    cleanup(&dir);
}
