//! 成就图标集成测试（GAP-FIX 社交域·管理侧 icon 上传；SQLite + 路由层）。
//!
//! 覆盖（`routes/achievements.rs` icon 端点，storage_dir 指向临时目录）：
//! - 上传（魔数嗅探/大小/reason）→ 201 + icon_url + version 递增 + 本地落盘；
//! - 公开读取 200/Content-Type/ETag/304；公开目录与管理目录投影 icon_url；
//! - 替换清理旧文件（内容寻址）；
//! - 校验失败：空体/超限/非图片/缺 reason → 400，未知 code → 404，
//!   非 admin → 403，未认证 → 401；
//! - 移除图标与删除成就的级联清理。
//!
//! 成就图标刻意不走 S3/附件域：直写 `{storage_dir}/achievements/`。

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
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

mod common;

/// 最小 PNG（魔数 + 尾部填充；后端只嗅探头部魔数）。
const PNG_BYTES: &[u8] = &[
    0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    1, 2, 3, 4,
];
/// 最小 GIF（GIF89a 头 + 填充）。
const GIF_BYTES: &[u8] = b"GIF89a\x02\x00\x03\x00";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-achicon-{}", uuid::Uuid::now_v7()));
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
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 插入已验证用户；返回 (user_id, username)。
async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(format!("{username}@example.com"))
            .bind(now - 25 * 3600 * 1000)
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

/// 应用实例：storage_dir 指向临时目录（图标落盘隔离，不污染仓库 uploads/）。
fn app_with(pool: DatabasePool, storage_dir: &Path) -> Router {
    let config = AppConfig {
        storage_dir: storage_dir.to_path_buf(),
        ..AppConfig::default()
    };
    build_router(config, Some(pool))
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

/// 管理员上下文（administrator + TOTP enrollment + step-up）。
async fn admin_ctx(app: &Router, pool: &DatabasePool) -> (String, String) {
    let (admin_id, _admin_name) = insert_user(pool, "adm").await;
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = 'administrator'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(&admin_id)
            .bind(&role_id)
            .bind(now_millis() - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    common::enroll_totp(pool, &admin_id).await;
    let session = common::direct_session_cookie(pool, &admin_id).await;
    let token = session.split('=').nth(1).unwrap().to_string();
    bblbb_backend::auth::session::mark_step_up(pool, &token)
        .await
        .unwrap();
    let csrf = session_csrf(app, &session).await;
    (session, csrf)
}

/// JSON 已认证请求；返回 (status, body)。
async fn authed_json(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .body(Body::from(body.to_string()))
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

/// 原始字节已认证 POST（icon 上传）；返回 (status, body)。
async fn authed_bytes(
    app: &Router,
    uri: &str,
    content_type: &str,
    session: &str,
    csrf: &str,
    body: &'static [u8],
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .header("content-type", content_type)
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .body(Body::from(body.to_vec()))
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

/// 匿名 GET（二进制响应）；返回 (status, headers, bytes)。
async fn anon_get_raw(app: &Router, uri: &str) -> (StatusCode, Vec<(String, String)>, Vec<u8>) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let headers = resp
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
        .collect();
    let bytes = resp
        .into_body()
        .collect()
        .await
        .unwrap()
        .to_bytes()
        .to_vec();
    (status, headers, bytes)
}

/// 管理侧创建一个成就；返回 version（=1）。
async fn create_achievement(app: &Router, session: &str, csrf: &str, code: &str) -> Value {
    let (status, body) = authed_json(
        app,
        "POST",
        "/api/v1/admin/achievements",
        session,
        csrf,
        json!({
            "code": code,
            "name": format!("图标测试成就-{code}"),
            "description": "成就图标集成测试成就",
            "category": "test",
            "condition_type": "manual",
            "condition_threshold": 1,
            "reward_coin": 0,
            "is_hidden": false,
            "is_enabled": true,
            "sort_order": 0,
            "reason": "测试创建"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "创建成就必须 201: {body}");
    body
}

#[tokio::test]
async fn icon_upload_serve_replace_and_catalog_projection() {
    let (pool, db_dir) = sqlite_pool_with_migrations().await;
    let storage_dir =
        std::env::temp_dir().join(format!("bblbb-achicon-store-{}", uuid::Uuid::now_v7()));
    let app = app_with(pool.clone(), &storage_dir);
    let (admin_session, admin_csrf) = admin_ctx(&app, &pool).await;

    create_achievement(&app, &admin_session, &admin_csrf, "icon_badge").await;

    // 未上传：公开读取 404
    let (status, _, _) = anon_get_raw(&app, "/api/v1/achievements/icon_badge/icon").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "未上传图标必须 404");

    // 上传 PNG（Content-Type 伪装成 html 也不影响：服务端魔数嗅探）→ 201
    let (status, body) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/icon_badge/icon?reason=%E6%9B%B4%E6%8D%A2%E5%9B%BE%E6%A0%87",
        "text/html",
        &admin_session,
        &admin_csrf,
        PNG_BYTES,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "上传图标必须 201: {body}");
    assert_eq!(body["icon_url"], "/api/v1/achievements/icon_badge/icon");
    assert_eq!(body["content_type"], "image/png");
    assert_eq!(body["bytes"], PNG_BYTES.len() as i64);
    assert_eq!(body["version"], 2, "上传递增 version");

    // 公开读取：200 + image/png + 字节一致 + ETag
    let (status, headers, bytes) = anon_get_raw(&app, "/api/v1/achievements/icon_badge/icon").await;
    assert_eq!(status, StatusCode::OK);
    let ct = headers
        .iter()
        .find(|(k, _)| k == "content-type")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    assert!(
        ct.starts_with("image/png"),
        "content-type 必须为 image/png: {ct}"
    );
    assert_eq!(bytes, PNG_BYTES, "回读字节一致");
    let etag = headers
        .iter()
        .find(|(k, _)| k == "etag")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    assert!(
        etag.starts_with('"') && etag.len() >= 18,
        "ETag 为内容哈希: {etag}"
    );
    let cache = headers
        .iter()
        .find(|(k, _)| k == "cache-control")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    assert!(cache.contains("public"), "公开资源缓存: {cache}");

    // If-None-Match 命中 → 304
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/achievements/icon_badge/icon")
                .header("if-none-match", etag.as_str())
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED, "ETag 命中必须 304");

    // 公共目录与管理目录投影 icon_url
    let (_, body) = authed_json(
        &app,
        "GET",
        "/api/v1/achievements",
        &admin_session,
        &admin_csrf,
        Value::Null,
    )
    .await;
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["code"] == "icon_badge")
        .unwrap()
        .clone();
    assert_eq!(item["icon_url"], "/api/v1/achievements/icon_badge/icon");
    let (_, body) = authed_json(
        &app,
        "GET",
        "/api/v1/admin/achievements",
        &admin_session,
        &admin_csrf,
        Value::Null,
    )
    .await;
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["code"] == "icon_badge")
        .unwrap()
        .clone();
    assert_eq!(item["icon_url"], "/api/v1/achievements/icon_badge/icon");

    // 替换为 GIF：201 + 旧 PNG 文件被清理（内容寻址）
    let icon_dir = storage_dir.join("achievements");
    let files_before: Vec<_> = std::fs::read_dir(&icon_dir).unwrap().collect();
    assert_eq!(files_before.len(), 1, "上传后目录只有 1 个文件");
    let (status, body) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/icon_badge/icon?reason=switch",
        "image/gif",
        &admin_session,
        &admin_csrf,
        GIF_BYTES,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "替换图标必须 201: {body}");
    assert_eq!(body["content_type"], "image/gif");
    assert_eq!(body["version"], 3);
    let files_after: Vec<_> = std::fs::read_dir(&icon_dir).unwrap().collect();
    assert_eq!(
        files_after.len(),
        1,
        "替换后旧文件必须清理: {:?}",
        files_after
            .iter()
            .filter_map(|f| f.as_ref().ok().map(|e| e.file_name()))
            .collect::<Vec<_>>()
    );
    let (_, headers, bytes) = anon_get_raw(&app, "/api/v1/achievements/icon_badge/icon").await;
    let ct = headers
        .iter()
        .find(|(k, _)| k == "content-type")
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    assert!(ct.starts_with("image/gif"));
    assert_eq!(bytes, GIF_BYTES);

    // 审计：icon_upload 落两条（首次 + 替换）
    let pool_ref = &pool;
    let audits: i64 = match pool_ref {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.achievement.icon_upload'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audits, 2, "两次上传都写审计");

    close_pool(&pool).await;
    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn icon_upload_validation_and_permissions() {
    let (pool, db_dir) = sqlite_pool_with_migrations().await;
    let storage_dir =
        std::env::temp_dir().join(format!("bblbb-achicon-val-{}", uuid::Uuid::now_v7()));
    let app = app_with(pool.clone(), &storage_dir);
    let (admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (alice_id, _alice_name) = insert_user(&pool, "alice").await;
    let alice_session = common::direct_session_cookie(&pool, &alice_id).await;
    let alice_csrf = session_csrf(&app, &alice_session).await;

    create_achievement(&app, &admin_session, &admin_csrf, "val_badge").await;

    // 未认证 → 401
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/val_badge/icon?reason=x",
        "image/png",
        "session=none",
        "none",
        PNG_BYTES,
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "未认证必须 401");

    // 非 admin → 403
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/val_badge/icon?reason=x",
        "image/png",
        &alice_session,
        &alice_csrf,
        PNG_BYTES,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "非管理员必须 403");

    // 未知 code → 404
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/no_such/icon?reason=x",
        "image/png",
        &admin_session,
        &admin_csrf,
        PNG_BYTES,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "未知成就必须 404");

    // 缺 reason → 400
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/val_badge/icon",
        "image/png",
        &admin_session,
        &admin_csrf,
        PNG_BYTES,
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 reason 必须 400");

    // 空体 → 400
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/val_badge/icon?reason=x",
        "image/png",
        &admin_session,
        &admin_csrf,
        b"",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "空体必须 400");

    // 非图片 → 400（魔数嗅探拒绝）
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/val_badge/icon?reason=x",
        "image/png",
        &admin_session,
        &admin_csrf,
        b"<html><script>alert(1)</script></html>",
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非图片必须 400");

    // 超限（2MB+1）→ 413：axum `DefaultBodyLimit`（默认 2MB，与
    // MAX_ICON_BYTES 同值）先于业务校验触发；handler 内的 400 检查仅为兜底。
    let oversized: &'static [u8] = Box::leak(vec![0u8; 2 * 1024 * 1024 + 1].into_boxed_slice());
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/val_badge/icon?reason=x",
        "image/png",
        &admin_session,
        &admin_csrf,
        oversized,
    )
    .await;
    assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE, "超过 2MB 必须 413");

    // 全部失败后：目录仍为空、icon_path 为 NULL
    let (status, _, _) = anon_get_raw(&app, "/api/v1/achievements/val_badge/icon").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "校验失败不得产生图标");

    close_pool(&pool).await;
    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}

#[tokio::test]
async fn icon_delete_and_achievement_delete_cascade() {
    let (pool, db_dir) = sqlite_pool_with_migrations().await;
    let storage_dir =
        std::env::temp_dir().join(format!("bblbb-achicon-del-{}", uuid::Uuid::now_v7()));
    let app = app_with(pool.clone(), &storage_dir);
    let (admin_session, admin_csrf) = admin_ctx(&app, &pool).await;

    create_achievement(&app, &admin_session, &admin_csrf, "del_badge").await;
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/del_badge/icon?reason=x",
        "image/png",
        &admin_session,
        &admin_csrf,
        PNG_BYTES,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let icon_dir = storage_dir.join("achievements");
    assert_eq!(std::fs::read_dir(&icon_dir).unwrap().count(), 1);

    // 缺 reason → 400
    let (status, _) = authed_json(
        &app,
        "DELETE",
        "/api/v1/admin/achievements/del_badge/icon",
        &admin_session,
        &admin_csrf,
        json!({}),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "移除图标缺 reason 必须 400"
    );

    // 正常移除：文件删除 + icon_path 清空 + version 递增
    let (status, body) = authed_json(
        &app,
        "DELETE",
        "/api/v1/admin/achievements/del_badge/icon",
        &admin_session,
        &admin_csrf,
        json!({ "reason": "下架图标" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "移除图标必须 200: {body}");
    assert_eq!(body["icon_url"], Value::Null);
    assert_eq!(body["version"], 3);
    assert_eq!(
        std::fs::read_dir(&icon_dir).unwrap().count(),
        0,
        "文件必须删除"
    );
    let (status, _, _) = anon_get_raw(&app, "/api/v1/achievements/del_badge/icon").await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // 幂等边界：无图标再移除 → 404
    let (status, _) = authed_json(
        &app,
        "DELETE",
        "/api/v1/admin/achievements/del_badge/icon",
        &admin_session,
        &admin_csrf,
        json!({ "reason": "again" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "无图标移除必须 404");

    // 再上传后删除整个成就：图标文件级联清理
    let (status, _) = authed_bytes(
        &app,
        "/api/v1/admin/achievements/del_badge/icon?reason=x",
        "image/png",
        &admin_session,
        &admin_csrf,
        PNG_BYTES,
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(std::fs::read_dir(&icon_dir).unwrap().count(), 1);
    let (status, _) = authed_json(
        &app,
        "DELETE",
        "/api/v1/admin/achievements/del_badge",
        &admin_session,
        &admin_csrf,
        json!({ "reason": "删除成就" }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "删除成就必须 204");
    assert_eq!(
        std::fs::read_dir(&icon_dir).unwrap().count(),
        0,
        "删除成就必须级联清理图标文件"
    );

    close_pool(&pool).await;
    cleanup(&db_dir);
    let _ = std::fs::remove_dir_all(&storage_dir);
}
