//! GAP-FIX 社交域集成测试（收藏 + 关注 + 成就；SQLite + 路由层）。
//!
//! 覆盖：收藏创建/列表/幂等/404/401、关注创建/取消/幂等/自我关注 422/
//! 粉丝与关注列表/板块关注/me 汇总、PublicProfile 社交统计与 is_following、
//! 成就公共目录（隐藏脱敏）、本人成就视图/装备（409 槽位）、管理侧
//! grant（手工授予 + badge 通知）、发帖后 first_post 自动解锁（评价钩子）。

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

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'
const BOARD_SLUG: &str = "general";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-soc1-{}", uuid::Uuid::now_v7()));
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
        Either::Right(p) => p.close().await,
    }
}

/// 插入已验证用户（email_verified_at 早于 24h 冷静期）；返回 (user_id, username)。
async fn insert_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
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

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

/// 任意方法的已认证请求（会话 + CSRF）；返回 (status, body)。
async fn authed(
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

/// 匿名 GET；返回 (status, body)。
async fn anon_get(app: &Router, uri: &str) -> (StatusCode, Value) {
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
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
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

/// 通过 API 发布一篇帖子（成就钩子 + 收藏目标）；返回 post_id。
async fn publish_post(app: &Router, session: &str, csrf: &str, client_request_id: &str) -> String {
    let (status, body) = authed(
        app,
        "POST",
        "/api/v1/posts",
        session,
        csrf,
        json!({
            "type": "article",
            "title": format!("社交域测试帖-{client_request_id}"),
            "markdown": "正文内容",
            "board_id": BOARD_ID,
            "visibility_level": 1,
            "access_policy": "public",
            "client_request_id": client_request_id,
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "发布帖子必须 201: {body}");
    body["id"].as_str().unwrap().to_string()
}

async fn count(pool: &DatabasePool, sql: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 管理员上下文（administrator + TOTP enrollment + step-up）。
async fn admin_ctx(app: &Router, pool: &DatabasePool) -> (String, String) {
    let (admin_id, _admin_name) = insert_user(pool, "adm").await;
    // 指派 administrator 全局角色
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
    // elevated 角色未完成 TOTP enrollment 会被降级为 member 基线
    common::enroll_totp(pool, &admin_id).await;
    let session = common::direct_session_cookie(pool, &admin_id).await;
    let token = session.split('=').nth(1).unwrap().to_string();
    bblbb_backend::auth::session::mark_step_up(pool, &token)
        .await
        .unwrap();
    let csrf = session_csrf(app, &session).await;
    (session, csrf)
}

// ───────────────────────────── 收藏 ─────────────────────────────

#[tokio::test]
async fn favorite_create_list_and_idempotency() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (alice_id, _alice_name) = insert_user(&pool, "alice").await;
    let (bob_id, _bob_name) = insert_user(&pool, "bob").await;
    let alice_session = common::direct_session_cookie(&pool, &alice_id).await;
    let bob_session = common::direct_session_cookie(&pool, &bob_id).await;
    let alice_csrf = session_csrf(&app, &alice_session).await;
    let bob_csrf = session_csrf(&app, &bob_session).await;

    let post1 = publish_post(&app, &alice_session, &alice_csrf, "fav-post-00000001").await;
    let post2 = publish_post(&app, &alice_session, &alice_csrf, "fav-post-00000002").await;

    // 创建 → 200 {favorited:true, favorite_count:1}
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/posts/{post1}/favorite"),
        &bob_session,
        &bob_csrf,
        json!({ "client_request_id": "fav-req-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "收藏必须 200: {body}");
    assert_eq!(body["favorited"], true);
    assert_eq!(body["favorite_count"], 1);

    // 幂等：重复收藏 → 计数不变
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/posts/{post1}/favorite"),
        &bob_session,
        &bob_csrf,
        json!({ "client_request_id": "fav-req-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["favorite_count"], 1, "重复收藏幂等");
    let rows = count(&pool, "SELECT COUNT(*) FROM favorites").await;
    assert_eq!(rows, 1, "收藏表只允许一行");

    // 第二个收藏
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/posts/{post2}/favorite"),
        &bob_session,
        &bob_csrf,
        json!({ "client_request_id": "fav-req-2" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "第二个收藏必须 200");

    // 列表：GET /me/favorites 返回 PostSummary 投影
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/favorites?limit=30",
        &bob_session,
        &bob_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "收藏列表必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "两个收藏: {body}");
    for item in items {
        for key in [
            "id",
            "board_id",
            "title",
            "post_type",
            "created_at",
            "favorited_at",
        ] {
            assert!(item.get(key).is_some(), "PostSummary 缺少 {key}");
        }
    }

    // 取消收藏 → 200 {favorited:false, favorite_count:0}；重复幂等
    let (status, body) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/posts/{post1}/favorite"),
        &bob_session,
        &bob_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["favorited"], false);
    assert_eq!(body["favorite_count"], 0);
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/posts/{post1}/favorite"),
        &bob_session,
        &bob_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "重复取消收藏幂等");

    // 帖子详情含 favorite_count 与 viewer_favorited
    let (status, body) = anon_get(&app, &format!("/api/v1/posts/{post2}")).await;
    assert_eq!(status, StatusCode::OK, "匿名读帖子: {body}");
    assert_eq!(body["favorite_count"], 1, "详情聚合收藏数");
    assert_eq!(
        body["viewer_favorited"], false,
        "匿名 viewer_favorited=false"
    );
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/posts/{post2}"),
        &bob_session,
        &bob_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["viewer_favorited"], true,
        "登录收藏者 viewer_favorited=true"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn favorite_requires_auth_and_missing_post() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    // 未认证 → 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/posts/00000000-0000-7000-8000-000000000001/favorite")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "未认证收藏必须 401"
    );

    // 已认证但帖子不存在 → 404
    let (alice_id, _) = insert_user(&pool, "alice").await;
    let alice_session = common::direct_session_cookie(&pool, &alice_id).await;
    let alice_csrf = session_csrf(&app, &alice_session).await;
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/posts/00000000-0000-7000-8000-000000000001/favorite",
        &alice_session,
        &alice_csrf,
        json!({ "client_request_id": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "收藏不存在帖子必须 404");

    // 列表未认证 → 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me/favorites")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────────── 关注 ─────────────────────────────

#[tokio::test]
async fn follow_user_flow_and_lists() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (alice_id, alice_name) = insert_user(&pool, "alice").await;
    let (_bob_id, bob_name) = insert_user(&pool, "bob").await;
    let alice_session = common::direct_session_cookie(&pool, &alice_id).await;
    let alice_csrf = session_csrf(&app, &alice_session).await;

    // 关注 → 200 {following:true, followers:1}
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/users/{bob_name}/follow"),
        &alice_session,
        &alice_csrf,
        json!({ "client_request_id": "follow-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "关注必须 200: {body}");
    assert_eq!(body["following"], true);
    assert_eq!(body["followers"], 1);

    // 幂等：重复关注不产生重复行
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/users/{bob_name}/follow"),
        &alice_session,
        &alice_csrf,
        json!({ "client_request_id": "follow-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["followers"], 1, "重复关注幂等");
    let rows = count(&pool, "SELECT COUNT(*) FROM user_follows").await;
    assert_eq!(rows, 1);

    // 粉丝列表（bob 视角）
    let (status, body) = anon_get(&app, &format!("/api/v1/users/{bob_name}/followers")).await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["username"], alice_name.as_str());
    for key in ["username", "display_name", "level", "created_at"] {
        assert!(items[0].get(key).is_some(), "粉丝投影缺少 {key}");
    }

    // 关注列表（alice 视角）
    let (status, body) = anon_get(&app, &format!("/api/v1/users/{alice_name}/following")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);
    assert_eq!(body["items"][0]["username"], bob_name.as_str());

    // me/following 汇总
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/following",
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["users"].as_array().unwrap().len(), 1, "关注用户汇总");
    assert_eq!(body["boards"].as_array().unwrap().len(), 0);

    // PublicProfile 社交统计 + is_following
    let (status, body) = anon_get(&app, &format!("/api/v1/users/{bob_name}")).await;
    assert_eq!(status, StatusCode::OK, "公开主页: {body}");
    assert_eq!(body["followers"], 1);
    assert_eq!(body["following"], 0);
    assert_eq!(body["is_following"], false, "匿名 is_following=false");
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/users/{bob_name}"),
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["is_following"], true, "关注者视角 is_following=true");

    // 取关 → 200 {following:false, followers:0}
    let (status, body) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/users/{bob_name}/follow"),
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["following"], false);
    assert_eq!(body["followers"], 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn follow_self_is_422_and_board_follow_flow() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (alice_id, alice_name) = insert_user(&pool, "alice").await;
    let alice_session = common::direct_session_cookie(&pool, &alice_id).await;
    let alice_csrf = session_csrf(&app, &alice_session).await;

    // 禁止关注自己 → 422
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/users/{alice_name}/follow"),
        &alice_session,
        &alice_csrf,
        json!({ "client_request_id": "self-follow" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "关注自己必须 422: {body}"
    );

    // 板块关注 → {following:true}；重复幂等；取消 → {following:false}
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/boards/{BOARD_SLUG}/follow"),
        &alice_session,
        &alice_csrf,
        json!({ "client_request_id": "bf-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "关注板块必须 200: {body}");
    assert_eq!(body["following"], true);
    let (status, _body) = authed(
        &app,
        "POST",
        &format!("/api/v1/boards/{BOARD_SLUG}/follow"),
        &alice_session,
        &alice_csrf,
        json!({ "client_request_id": "bf-1" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "重复关注板块幂等");
    let (status, body) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/boards/{BOARD_SLUG}/follow"),
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["following"], false);

    // me/following：取消后 boards 为空
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/following",
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["boards"].as_array().unwrap().len(), 0);

    // 关注未认证 → 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/users/nobody/follow")
                .header("content-type", "application/json")
                .body(Body::from(json!({}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────────── 成就 ─────────────────────────────

#[tokio::test]
async fn achievements_public_catalog_and_my_view() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (alice_id, _alice_name) = insert_user(&pool, "alice").await;
    let alice_session = common::direct_session_cookie(&pool, &alice_id).await;
    let alice_csrf = session_csrf(&app, &alice_session).await;

    // 公共目录：8 条种子成就；隐藏成就描述脱敏
    let (status, body) = anon_get(&app, "/api/v1/achievements").await;
    assert_eq!(status, StatusCode::OK, "成就目录必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 8, "8 条种子成就: {body}");
    for item in items {
        for key in [
            "code",
            "name",
            "description",
            "category",
            "reward_exp",
            "reward_coin",
            "is_hidden",
            "sort_order",
        ] {
            assert!(item.get(key).is_some(), "成就投影缺少 {key}");
        }
    }
    let streak30 = items.iter().find(|i| i["code"] == "streak_30").unwrap();
    assert_eq!(streak30["is_hidden"], true, "streak_30 为隐藏成就");
    assert_eq!(streak30["description"], "隐藏成就", "隐藏成就描述脱敏");

    // 本人视图：全部成就 + stats
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/achievements",
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "本人成就必须 200: {body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 8);
    assert_eq!(body["stats"]["total"], 8);
    assert_eq!(body["stats"]["unlocked"], 0);
    assert_eq!(body["stats"]["equipped"], 0);
    assert_eq!(body["stats"]["max_slots"], 3);

    // 未认证 → 401
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me/achievements")
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
async fn post_publish_unlocks_first_post_with_notification() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (alice_id, _alice_name) = insert_user(&pool, "alice").await;
    let alice_session = common::direct_session_cookie(&pool, &alice_id).await;
    let alice_csrf = session_csrf(&app, &alice_session).await;

    let _post_id = publish_post(&app, &alice_session, &alice_csrf, "ach-post-0000001").await;

    // 钩子解锁：first_post 达标（post_count=1 >= 1）
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/achievements",
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    let first_post = items.iter().find(|i| i["code"] == "first_post").unwrap();
    assert!(first_post["unlocked_at"].is_number(), "first_post 应已解锁");
    assert_eq!(first_post["progress"], 1);
    assert_eq!(first_post["target"], 1);
    assert_eq!(body["stats"]["unlocked"], 1);

    // badge 通知
    let badges = count(
        &pool,
        &format!(
            "SELECT COUNT(*) FROM notifications WHERE user_id = '{alice_id}' AND type = 'badge'"
        ),
    )
    .await;
    assert_eq!(badges, 1, "解锁必须写一条 badge 通知");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_grant_equip_and_slot_limit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (alice_id, alice_name) = insert_user(&pool, "alice").await;
    let alice_session = common::direct_session_cookie(&pool, &alice_id).await;
    let alice_csrf = session_csrf(&app, &alice_session).await;

    // 管理侧目录（admin.manage 门）
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/achievements",
        &admin_session,
        &admin_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "管理成就目录必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 8);
    for key in ["code", "condition_type", "unlocked_count", "version"] {
        assert!(items[0].get(key).is_some(), "管理投影缺少 {key}");
    }

    // 普通用户访问管理端点 → 403
    let (status, _) = authed(
        &app,
        "GET",
        "/api/v1/admin/achievements",
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "非管理员必须 403");

    // 手工授予 community_elder（manual）→ 201 {code, unlocked_at}
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/achievements/community_elder/grant",
        &admin_session,
        &admin_csrf,
        json!({ "username": alice_name, "reason": "test grant" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "授予必须 201: {body}");
    assert_eq!(body["code"], "community_elder");
    assert!(body["unlocked_at"].is_number());

    // 重复授予幂等：返回既有 unlocked_at，不重复通知
    let (status, body2) = authed(
        &app,
        "POST",
        "/api/v1/admin/achievements/community_elder/grant",
        &admin_session,
        &admin_csrf,
        json!({ "username": alice_name, "reason": "test grant again" }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body2["unlocked_at"], body["unlocked_at"], "重复授予幂等");
    let badges = count(
        &pool,
        &format!(
            "SELECT COUNT(*) FROM notifications WHERE user_id = '{alice_id}' AND type = 'badge'"
        ),
    )
    .await;
    assert_eq!(badges, 1, "重复授予不重复通知");

    // 装备 → 200 {equipped:true}；卸下 → 200 {equipped:false}
    let (status, body) = authed(
        &app,
        "PUT",
        "/api/v1/me/achievements/community_elder/equip",
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "装备必须 200: {body}");
    assert_eq!(body["equipped"], true);
    let (status, body) = authed(
        &app,
        "DELETE",
        "/api/v1/me/achievements/community_elder/equip",
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["equipped"], false);

    // 未解锁成就装备 → 409
    let (status, _) = authed(
        &app,
        "PUT",
        "/api/v1/me/achievements/first_post/equip",
        &alice_session,
        &alice_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "未解锁装备必须 409");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_create_patch_delete_achievement() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (admin_session, admin_csrf) = admin_ctx(&app, &pool).await;

    // 创建 → 201；重复 code → 409
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/achievements",
        &admin_session,
        &admin_csrf,
        json!({
            "code": "test_badge",
            "name": "测试徽章",
            "description": "管理侧创建的测试成就",
            "category": "special",
            "condition_type": "manual",
            "condition_threshold": 0,
            "reward_exp": 1,
            "reward_coin": 1,
            "is_hidden": false,
            "is_enabled": true,
            "sort_order": 99,
            "reason": "test create"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "创建成就必须 201: {body}");
    assert_eq!(body["version"], 1);
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/achievements",
        &admin_session,
        &admin_csrf,
        json!({
            "code": "test_badge",
            "name": "重复",
            "description": "重复 code",
            "category": "special",
            "condition_type": "manual",
            "condition_threshold": 0,
            "reason": "dup"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "重复 code 必须 409");

    // 缺 reason → 400
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/achievements",
        &admin_session,
        &admin_csrf,
        json!({
            "code": "no_reason_badge",
            "name": "无理由",
            "description": "缺 reason",
            "category": "special",
            "condition_type": "manual",
            "condition_threshold": 0
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 reason 必须 400");

    // PATCH（If-Match）→ version 递增；错误 If-Match → 409
    let (status, _body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/achievements/test_badge",
        &admin_session,
        &admin_csrf,
        json!({ "name": "改名徽章", "reason": "test update" }),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "缺 If-Match 必须 400: {_body}"
    );
    // 错误 If-Match → 409 版本冲突
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/v1/admin/achievements/test_badge")
                .header("content-type", "application/json")
                .header("x-csrf-token", &admin_csrf)
                .header("cookie", &admin_session)
                .header("if-match", "999")
                .body(Body::from(
                    json!({ "name": "改名徽章", "reason": "test update" }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT, "版本冲突必须 409");
    let (status, body) = authed_with_if_match(
        &app,
        "PATCH",
        "/api/v1/admin/achievements/test_badge",
        &admin_session,
        &admin_csrf,
        "1",
        json!({ "name": "改名徽章", "reason": "test update" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "更新成就必须 200: {body}");
    assert_eq!(body["name"], "改名徽章");
    assert_eq!(body["version"], 2, "version 递增");

    // DELETE → 204；目录变 9→8（种子 8 + 新建 1 - 删除 1 = 8）
    let (status, _) = authed(
        &app,
        "DELETE",
        "/api/v1/admin/achievements/test_badge",
        &admin_session,
        &admin_csrf,
        json!({ "reason": "test delete" }),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "删除成就必须 204");
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/achievements",
        &admin_session,
        &admin_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 8);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 带 If-Match 头的已认证请求。
async fn authed_with_if_match(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    if_match: &str,
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
                .header("if-match", if_match)
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
