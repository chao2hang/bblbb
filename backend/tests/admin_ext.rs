//! GAP-FIX 管理域 Part A 集成测试（SQLite + 路由层）。
//!
//! 覆盖：仪表盘统计（空库基线 + 401/403）、系统设置 GET/PATCH/If-Match
//! 乐观锁（缺头 400、错版本 409、并发竞态 409）、帖子管理列表与动作
//! （feature/hide/restore 全链路 + 审计落库 + member 403）、通知广播
//! （创建/幂等重放/发件箱/撤回保留已读）、审计日志 keyset 分页、
//! 角色授予/撤销（幂等 + 自我分配 403）、公开站点统计（匿名可读）。

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

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-admx-{}", uuid::Uuid::now_v7()));
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

/// 插入已验证 active 用户；返回 (user_id, username)。
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

/// 任意方法已认证请求（会话 + CSRF，可带附加 header）；返回 (status, body)。
async fn authed(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-csrf-token", csrf)
        .header("cookie", session);
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
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

/// 匿名 GET（含响应头检查用返回值）；返回 (status, body, cache_control)。
async fn anon_get(app: &Router, uri: &str) -> (StatusCode, Value, String) {
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
    let cache_control = resp
        .headers()
        .get("cache-control")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value, cache_control)
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

/// 管理员上下文（administrator + TOTP enrollment + step-up）；
/// 返回 (admin_id, session, csrf)。
async fn admin_ctx(app: &Router, pool: &DatabasePool) -> (String, String, String) {
    let (admin_id, _name) = insert_user(pool, "adm").await;
    grant_role_direct(pool, &admin_id, "administrator").await;
    common::enroll_totp(pool, &admin_id).await;
    let session = common::direct_session_cookie(pool, &admin_id).await;
    let token = session.split('=').nth(1).unwrap().to_string();
    bblbb_backend::auth::session::mark_step_up(pool, &token)
        .await
        .unwrap();
    let csrf = session_csrf(app, &session).await;
    (admin_id, session, csrf)
}

/// 直接 SQL 授予全局角色（绕过 API，测试夹具用）。
async fn grant_role_direct(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role_name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now_millis() - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 通过 API 发布一篇帖子；返回 post_id。
async fn publish_post(app: &Router, session: &str, csrf: &str, crid: &str) -> String {
    let (status, body) = authed(
        app,
        "POST",
        "/api/v1/posts",
        session,
        csrf,
        json!({
            "type": "article",
            "title": format!("管理域测试帖-{crid}"),
            "markdown": "正文内容",
            "board_id": BOARD_ID,
            "visibility_level": 1,
            "access_policy": "public",
            "client_request_id": crid,
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "发布帖子必须 201: {body}");
    body["id"].as_str().unwrap().to_string()
}

async fn count(pool: &DatabasePool, sql: &str, arg: Option<&str>) -> i64 {
    match pool {
        Either::Left(p) => match arg {
            Some(a) => sqlx::query_scalar(sql).bind(a).fetch_one(p).await.unwrap(),
            None => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        },
        Either::Right(_) => panic!("SQLite only"),
    }
}

// ───────────────────── 仪表盘统计 ─────────────────────

#[tokio::test]
async fn admin_stats_requires_admin_and_reports_counts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;
    let (member_id, _m) = insert_user(&pool, "mem").await;
    let member_session = common::direct_session_cookie(&pool, &member_id).await;
    let member_csrf = session_csrf(&app, &member_session).await;

    // 匿名 401。
    let (status, _body, _cc) = anon_get(&app, "/api/v1/admin/stats").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // member 403（无 admin.manage）。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/stats",
        &member_session,
        &member_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member 必须 403: {body}");

    // admin 200：members 计数 = admin + member（均 active）。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/stats",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin 必须 200: {body}");
    assert_eq!(body["members"].as_i64().unwrap(), 2, "members 计数: {body}");
    assert!(body["members_delta_7d"].is_i64());
    assert!(body["posts_today"].is_i64());
    assert!(body["posts_yesterday"].is_i64());
    assert!(body["reports_pending"].is_i64());
    assert!(body["active_today"].is_i64());
    // 空库时今日/昨日帖数为 0，环 比 delta 为 null（无除零）。
    assert!(body["posts_delta_day"].is_null() || body["posts_delta_day"].is_number());

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────── 系统设置 ─────────────────────

#[tokio::test]
async fn admin_settings_get_patch_optimistic_lock() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // GET → 200 + version=1（迁移种子）。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "GET 设置必须 200: {body}");
    assert_eq!(body["version"].as_i64().unwrap(), 1);
    // 0061 种子默认值：open_registration=1 / anonymous_replies=0。
    assert!(body["settings"]["open_registration"].as_bool().unwrap());
    assert!(!body["settings"]["anonymous_replies"].as_bool().unwrap());
    assert_eq!(body["settings"]["site_name"].as_str().unwrap(), "BBLBB");
    // 0063 种子默认值：public_source = 原型默认公开源。
    assert_eq!(
        body["settings"]["public_source"].as_str().unwrap(),
        "https://bblbb.local"
    );

    // PATCH 缺 If-Match → 400。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "open_registration": true }, "reason": "开启注册" }),
        &[],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "缺 If-Match 必须 400: {body}"
    );

    // PATCH 错版本 → 409。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "open_registration": true }, "reason": "开启注册" }),
        &[("if-match", "999")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "错版本必须 409: {body}");

    // PATCH 缺 reason → 400。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "open_registration": true } }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "缺 reason 必须 400: {body}"
    );

    // PATCH 正确版本 → 200 + version=2 + 持久化。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({
            "settings": {
                "open_registration": false,
                "site_name": "BBLBB 测试站",
                "public_source": "https://settings.example.com",
                "site_description": "测试站描述",
                "login_eyebrow": "HELLO",
                "login_title": "登录测试站",
                "login_subtitle": "测试登录说明",
                "register_eyebrow": "JOIN US",
                "register_title": "加入测试站",
                "register_subtitle": "测试注册说明"
            },
            "reason": "关闭注册并改名"
        }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "PATCH 设置必须 200: {body}");
    assert_eq!(body["version"].as_i64().unwrap(), 2, "版本必须 +1: {body}");
    assert!(
        !body["settings"]["open_registration"].as_bool().unwrap(),
        "设置必须生效: {body}"
    );
    assert_eq!(
        body["settings"]["public_source"].as_str().unwrap(),
        "https://settings.example.com",
        "公开源必须生效: {body}"
    );
    assert_eq!(
        body["settings"]["site_name"].as_str().unwrap(),
        "BBLBB 测试站"
    );
    // 0065 站点文案：PATCH 后回读一致（trim 后落库）。
    assert_eq!(
        body["settings"]["site_description"].as_str().unwrap(),
        "测试站描述"
    );
    assert_eq!(
        body["settings"]["login_title"].as_str().unwrap(),
        "登录测试站"
    );
    assert_eq!(
        body["settings"]["login_subtitle"].as_str().unwrap(),
        "测试登录说明"
    );
    assert_eq!(
        body["settings"]["register_eyebrow"].as_str().unwrap(),
        "JOIN US"
    );
    assert_eq!(
        body["settings"]["register_title"].as_str().unwrap(),
        "加入测试站"
    );

    // 旧版本重放 → 409（乐观锁把门）。
    let (status, _body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "open_registration": true }, "reason": "回滚" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "旧版本必须 409");

    // 再 GET 确认持久化 + 审计落库。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"].as_i64().unwrap(), 2);
    assert!(!body["settings"]["open_registration"].as_bool().unwrap());
    let audit = count(
        &pool,
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.settings.update'",
        None,
    )
    .await;
    assert_eq!(audit, 1, "设置更新必须写审计");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 0065 站点公开信息（GET /api/v1/site）：匿名可读、只含公开投影字段
/// （不含 SMTP/注册开关）、空文案 = 前端兜底语义原样透传、private no-store。
#[tokio::test]
async fn public_site_projection_anonymous() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 匿名 GET → 200：0061/0065 种子默认值。
    let (status, body, cache_control) = anon_get(&app, "/api/v1/site").await;
    assert_eq!(status, StatusCode::OK, "匿名必须 200: {body}");
    assert_eq!(body["site_name"].as_str().unwrap(), "BBLBB");
    assert_eq!(
        body["site_description"].as_str().unwrap(),
        "",
        "空文案原样透传（前端兜底）"
    );
    assert_eq!(body["login_title"].as_str().unwrap(), "");
    assert_eq!(body["register_subtitle"].as_str().unwrap(), "");
    assert!(!body["maintenance_mode"].as_bool().unwrap());
    assert!(body["version"].is_i64());
    assert_eq!(
        cache_control, "private, no-store",
        "公开站点信息禁止共享缓存"
    );
    // 公开投影不得泄漏 SMTP / 注册开关等运营字段。
    for key in [
        "smtp_host",
        "smtp_pass",
        "smtp_enabled",
        "open_registration",
        "email_verification",
        "public_source",
        "api_rate_limit",
    ] {
        assert!(body.get(key).is_none(), "公开投影不得包含 {key}: {body}");
    }

    // 管理台改文案 → 匿名立即读到新值（空串 = 兜底语义保留）。
    let (status, _body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({
            "settings": {
                "site_name": "开源论坛",
                "site_description": "一个开源的论坛程序",
                "login_title": "欢迎回来",
                "login_subtitle": "登录以继续"
            },
            "reason": "全站文案统一"
        }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    let (status, body, _cc) = anon_get(&app, "/api/v1/site").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["site_name"].as_str().unwrap(), "开源论坛");
    assert_eq!(
        body["site_description"].as_str().unwrap(),
        "一个开源的论坛程序"
    );
    assert_eq!(body["login_title"].as_str().unwrap(), "欢迎回来");
    assert_eq!(body["login_subtitle"].as_str().unwrap(), "登录以继续");
    // 未改的字段保持空串兜底语义。
    assert_eq!(body["register_title"].as_str().unwrap(), "");

    // 超长文案 → 400（login_title 上限 100 字符）。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({
            "settings": { "login_title": "标".repeat(101) },
            "reason": "超长文案"
        }),
        &[("if-match", "2")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "超长站点文案必须 400: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 运营趋势（M17-GAPFIX-07）：8 桶时间序列、内容口径过滤（published 帖子
/// 落桶）、非法 period → 400。
#[tokio::test]
async fn admin_stats_trend_buckets() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 铸一篇 published 帖子（内容口径必须落桶）。
    let _post_id = publish_post(&app, &session, &csrf, "admx-trend-000000000001").await;

    // day → 8 桶，结构完整，内容桶合计 ≥ 1（刚发布的帖子）。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/stats/trend?period=day",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "trend 必须 200: {body}");
    assert_eq!(body["period"], "day");
    let buckets = body["buckets"].as_array().unwrap();
    assert_eq!(buckets.len(), 8, "必须 8 个时间桶: {body}");
    let mut posts_sum = 0i64;
    for bucket in buckets {
        assert!(
            bucket["start"].is_i64() && bucket["end"].is_i64(),
            "桶必须带毫秒边界"
        );
        assert!(bucket["posts"].is_i64());
        assert!(bucket["comments"].is_i64());
        assert!(bucket["active_users"].is_i64());
        assert!(bucket["reports"].is_i64());
        posts_sum += bucket["posts"].as_i64().unwrap();
    }
    assert!(posts_sum >= 1, "published 帖子必须落桶: {body}");

    // week → 8 桶 + period 回显。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/stats/trend?period=week",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["period"], "week");
    assert_eq!(body["buckets"].as_array().unwrap().len(), 8);

    // 非法 period → 400。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/stats/trend?period=decade",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "非法 period 必须 400: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 0063 public_source（公开源）：非法 URL / 空串 → 400；合法值回读一致；
/// 缺 key 保持原值（部分更新语义）。
#[tokio::test]
async fn admin_settings_public_source_validation() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 非法 URL（缺 http(s) 前缀）→ 400。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "public_source": "bblbb.example.com" }, "reason": "改公开源" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "非 http(s) 公开源必须 400: {body}"
    );

    // 空串 → 400（必填，与原型一致）。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "public_source": "   " }, "reason": "改公开源" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "空公开源必须 400: {body}");

    // 含空白 → 400。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "public_source": "https://bblbb example.com" }, "reason": "改公开源" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "含空白必须 400: {body}");

    // 非字符串 → 400。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "public_source": 12345 }, "reason": "改公开源" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非字符串必须 400: {body}");

    // 以上 400 均为校验前置：version 不应推进，值保持种子默认。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"].as_i64().unwrap(), 1, "校验失败不得推进版本");
    assert_eq!(
        body["settings"]["public_source"].as_str().unwrap(),
        "https://bblbb.local"
    );

    // 合法值（http:// 亦接受）→ 200 + 回读一致 + 版本推进。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "public_source": "http://192.168.8.9:8080/site" }, "reason": "内网公开源" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "合法公开源必须 200: {body}");
    assert_eq!(body["version"].as_i64().unwrap(), 2);
    assert_eq!(
        body["settings"]["public_source"].as_str().unwrap(),
        "http://192.168.8.9:8080/site"
    );

    // 缺 key = 保持原值（部分更新语义）。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "default_lang": "en" }, "reason": "切默认语言" }),
        &[("if-match", "2")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "缺 key 的部分更新必须 200: {body}");
    assert_eq!(
        body["settings"]["public_source"].as_str().unwrap(),
        "http://192.168.8.9:8080/site",
        "缺 key 必须保持原值"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 0064 SMTP 配置：GET 脱敏回读（密码不外泄）；PATCH 校验（端口/加密/邮箱）；密码持久化与按需保留。
#[tokio::test]
async fn admin_settings_smtp_configuration_and_masking() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 1) 初始状态：未启用，默认端口 587，密码未配置，绝不泄露明文密码
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let s = &body["settings"];
    assert!(!s["smtp_enabled"].as_bool().unwrap());
    assert_eq!(s["smtp_port"].as_i64().unwrap(), 587);
    assert_eq!(s["smtp_encryption"].as_str().unwrap(), "starttls");
    assert!(!s["smtp_pass_configured"].as_bool().unwrap());
    assert!(
        s.get("smtp_pass").is_none(),
        "GET 返回绝不得包含 smtp_pass 字段"
    );

    // 2) 校验测试：非法端口 (0 或 >65535) → 400
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "smtp_port": 70000 }, "reason": "测试端口超限" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "端口 >65535 必须 400: {body}"
    );

    // 3) 校验测试：非法加密方式 → 400
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "smtp_encryption": "invalid_mode" }, "reason": "测试加密方式" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "未知加密模式必须 400: {body}"
    );

    // 4) 校验测试：非法发件人邮箱 → 400
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({ "settings": { "smtp_from_email": "not-an-email" }, "reason": "测试邮箱" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "非法邮箱必须 400: {body}");

    // 5) 合法配置更新 + 设置密码 → 200 + 密码脱敏为 pass_configured=true
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({
            "settings": {
                "smtp_enabled": true,
                "smtp_host": "smtp.example.com",
                "smtp_port": 465,
                "smtp_user": "notify@example.com",
                "smtp_pass": "secret_auth_code_xyz",
                "smtp_from_email": "noreply@example.com",
                "smtp_from_name": "BBLBB Community",
                "smtp_encryption": "tls"
            },
            "reason": "配置生产 SMTP 发信服务"
        }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "合法更新必须 200: {body}");
    assert_eq!(body["version"].as_i64().unwrap(), 2);
    let s = &body["settings"];
    assert!(s["smtp_enabled"].as_bool().unwrap());
    assert_eq!(s["smtp_host"].as_str().unwrap(), "smtp.example.com");
    assert_eq!(s["smtp_port"].as_i64().unwrap(), 465);
    assert_eq!(s["smtp_user"].as_str().unwrap(), "notify@example.com");
    assert_eq!(
        s["smtp_from_email"].as_str().unwrap(),
        "noreply@example.com"
    );
    assert_eq!(s["smtp_from_name"].as_str().unwrap(), "BBLBB Community");
    assert_eq!(s["smtp_encryption"].as_str().unwrap(), "tls");
    assert!(
        s["smtp_pass_configured"].as_bool().unwrap(),
        "密码必须标记已配置"
    );
    assert!(s.get("smtp_pass").is_none(), "PATCH 返回绝不得泄露明文密码");

    // 6) 从 DB 辅助函数直接验证持久化的 SMTP 配置
    let db_conf = bblbb_backend::email::service::load_smtp_config_from_db(&pool)
        .await
        .expect("load smtp from db")
        .expect("singleton exists");
    assert!(db_conf.enabled);
    assert_eq!(db_conf.host, "smtp.example.com");
    assert_eq!(db_conf.port, 465);
    assert_eq!(db_conf.user, "notify@example.com");
    assert_eq!(db_conf.pass, "secret_auth_code_xyz");
    assert_eq!(db_conf.from_email, "noreply@example.com");
    assert_eq!(db_conf.from_name, "BBLBB Community");
    assert_eq!(db_conf.encryption, "tls");

    // 7) 再次 PATCH 其它字段（缺 smtp_pass）→ 原密码保留，不被清空
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({
            "settings": {
                "smtp_port": 587,
                "smtp_encryption": "starttls"
            },
            "reason": "切换到 STARTTLS 端口"
        }),
        &[("if-match", "2")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["version"].as_i64().unwrap(), 3);
    assert!(
        body["settings"]["smtp_pass_configured"].as_bool().unwrap(),
        "未传密码时原密码应保持"
    );

    let db_conf2 = bblbb_backend::email::service::load_smtp_config_from_db(&pool)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        db_conf2.pass, "secret_auth_code_xyz",
        "数据库中的密码值保持不变"
    );
    assert_eq!(db_conf2.port, 587);
    assert_eq!(db_conf2.encryption, "starttls");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 0066 第三方 OAuth 配置：GET 脱敏回读（Secret 不外泄）；PATCH 更新；Secret 持久化与按需保留。
#[tokio::test]
async fn admin_settings_oauth_configuration_and_masking() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 1) 初始状态：未启用，Secret 未配置，绝不泄露明文 Secret
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let s = &body["settings"];
    assert!(!s["google_auth_enabled"].as_bool().unwrap());
    assert!(!s["google_client_secret_configured"].as_bool().unwrap());
    assert!(s.get("google_client_secret").is_none());
    assert!(!s["github_auth_enabled"].as_bool().unwrap());
    assert!(!s["github_client_secret_configured"].as_bool().unwrap());
    assert!(s.get("github_client_secret").is_none());

    // 2) 更新 OAuth 配置与 Client Secret
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({
            "settings": {
                "google_auth_enabled": true,
                "google_client_id": "google-client-id-123",
                "google_client_secret": "google-secret-456",
                "github_auth_enabled": true,
                "github_client_id": "github-client-id-789",
                "github_client_secret": "github-secret-abc"
            },
            "reason": "配置 Google 和 GitHub 登录"
        }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "PATCH OAuth 配置必须 200: {body}");
    let s = &body["settings"];
    assert!(s["google_auth_enabled"].as_bool().unwrap());
    assert_eq!(s["google_client_id"].as_str().unwrap(), "google-client-id-123");
    assert!(s["google_client_secret_configured"].as_bool().unwrap());
    assert!(s.get("google_client_secret").is_none(), "PATCH 返回绝不泄露 Secret");
    assert!(s["github_auth_enabled"].as_bool().unwrap());
    assert_eq!(s["github_client_id"].as_str().unwrap(), "github-client-id-789");
    assert!(s["github_client_secret_configured"].as_bool().unwrap());
    assert!(s.get("github_client_secret").is_none(), "PATCH 返回绝不泄露 Secret");

    // 3) 再次 PATCH 其它字段（缺 secret 字段）→ 原 Secret 保留，不被清空
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/settings",
        &session,
        &csrf,
        json!({
            "settings": {
                "site_name": "New BBLBB"
            },
            "reason": "修改站点名称"
        }),
        &[("if-match", "2")],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let s = &body["settings"];
    assert!(s["google_client_secret_configured"].as_bool().unwrap());
    assert!(s["github_client_secret_configured"].as_bool().unwrap());

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────── 帖子管理 ─────────────────────

#[tokio::test]
async fn admin_posts_list_and_actions_flow() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;
    let (author_id, _a) = insert_user(&pool, "aut").await;
    let author_session = common::direct_session_cookie(&pool, &author_id).await;
    let author_csrf = session_csrf(&app, &author_session).await;
    let (member_id, _m) = insert_user(&pool, "mem").await;
    let member_session = common::direct_session_cookie(&pool, &member_id).await;
    let member_csrf = session_csrf(&app, &member_session).await;

    let post_id = publish_post(&app, &author_session, &author_csrf, "admx-pub-000000000001").await;

    // 管理列表能按 status 找到 published 帖。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/posts?status=published",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "管理帖子列表必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    let hit = items
        .iter()
        .find(|it| it["id"].as_str() == Some(post_id.as_str()));
    assert!(hit.is_some(), "管理列表必须包含目标帖: {body}");

    // member 无 post.moderate → 403。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/posts",
        &member_session,
        &member_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member 必须 403: {body}");

    // feature → is_featured=true（featured_at 非空）。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/posts/{post_id}/action"),
        &session,
        &csrf,
        json!({ "action": "feature", "reason": "精选测试" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "feature 必须 200: {body}");
    assert!(body["is_featured"].as_bool().unwrap(), "精华标记: {body}");

    // pin → is_pinned=true（pinned 列）。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/posts/{post_id}/action"),
        &session,
        &csrf,
        json!({ "action": "pin", "reason": "置顶测试" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "pin 必须 200: {body}");
    assert!(body["is_pinned"].as_bool().unwrap(), "置顶标记: {body}");

    // hide → status=hidden。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/posts/{post_id}/action"),
        &session,
        &csrf,
        json!({ "action": "hide", "reason": "下架测试" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "hide 必须 200: {body}");
    assert_eq!(
        body["status"].as_str().unwrap(),
        "hidden",
        "下架状态: {body}"
    );

    // 对 published 帖 restore → 400（守卫：仅 hidden/deleted 可恢复——当前是
    // hidden，所以 restore 应成功；改为先验证未过审场景在 approve 测试里）。
    // 对已 hidden 帖 restore → 200 回 published。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/posts/{post_id}/action"),
        &session,
        &csrf,
        json!({ "action": "restore", "reason": "恢复测试" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "restore 必须 200: {body}");
    assert_eq!(
        body["status"].as_str().unwrap(),
        "published",
        "恢复后状态: {body}"
    );

    // 未知动作 → 400。
    let (status, _body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/posts/{post_id}/action"),
        &session,
        &csrf,
        json!({ "action": "explode", "reason": "未知动作" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "未知动作必须 400");

    // 404：不存在的帖子。
    let (status, _body) = authed(
        &app,
        "POST",
        "/api/v1/admin/posts/01911111-1111-7111-8111-111111111111/action",
        &session,
        &csrf,
        json!({ "action": "hide", "reason": "不存在" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在的帖子必须 404");

    // 审计落库：每个动作一条 admin.post.<action>。
    for action in ["feature", "pin", "hide", "restore"] {
        let n = count(
            &pool,
            &format!("SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.post.{action}'"),
            None,
        )
        .await;
        assert_eq!(n, 1, "admin.post.{action} 必须有审计");
    }

    // member 直接调动作 → 403。
    let (status, _body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/posts/{post_id}/action"),
        &member_session,
        &member_csrf,
        json!({ "action": "hide", "reason": "越权" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member 动作必须 403");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────── 通知广播 ─────────────────────

/// 广播 target（M17-GAPFIX-07）：admins 只投递管理员；非法 target → 400。
#[tokio::test]
async fn broadcast_target_admins_and_validation() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;
    let (member, _m) = insert_user(&pool, "bct").await;
    let _ = member;

    // 非法 target → 400。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/notifications/broadcast",
        &session,
        &csrf,
        json!({ "title": "t", "body": "b", "client_request_id": "vt-e1", "target": "everyone" }),
        &[],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "非法 target 必须 400: {body}"
    );

    // target=admins → 201，只有管理员收到（member 不在收件人）。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/notifications/broadcast",
        &session,
        &csrf,
        json!({ "title": "目标验证", "body": "admins 广播", "client_request_id": "vt-e2", "target": "admins" }),
        &[],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CREATED,
        "target=admins 必须 201: {body}"
    );
    let target_count = body["target_count"].as_i64().unwrap();
    assert!(target_count >= 1, "管理员必须收到: {body}");

    // member 未收到（notifications 按 user_id 计数）。
    let member_notifications = count(
        &pool,
        "SELECT COUNT(*) FROM notifications n WHERE n.broadcast_id IS NOT NULL",
        None,
    )
    .await;
    assert_eq!(
        member_notifications, target_count,
        "广播通知数必须等于 target_count（member 未收）"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn broadcast_create_replay_outbox_and_recall() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;
    let (u1, _n1) = insert_user(&pool, "bc1").await;
    let (u2, _n2) = insert_user(&pool, "bc2").await;

    // 广播 → 201 target_count = 3（admin + 2 用户，均 active）。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/notifications/broadcast",
        &session,
        &csrf,
        json!({
            "title": "全站公告",
            "body": "维护通知正文",
            "client_request_id": "admx-bc-000000000001"
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "广播必须 201: {body}");
    assert_eq!(
        body["target_count"].as_i64().unwrap(),
        3,
        "广播目标数: {body}"
    );
    let broadcast_id = body["id"].as_str().unwrap().to_string();

    // 通知行：type/category='system'，broadcast_id 关联。
    let n = count(
        &pool,
        "SELECT COUNT(*) FROM notifications WHERE broadcast_id = ? AND type = 'system' AND category = 'system'",
        Some(&broadcast_id),
    )
    .await;
    assert_eq!(n, 3, "广播通知必须按用户展开: {broadcast_id}");

    // 幂等重放：同 client_request_id → 201 同一广播，不重复插入。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/notifications/broadcast",
        &session,
        &csrf,
        json!({
            "title": "全站公告",
            "body": "维护通知正文",
            "client_request_id": "admx-bc-000000000001"
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "幂等重放必须 201: {body}");
    assert_eq!(
        body["id"].as_str().unwrap(),
        broadcast_id,
        "重放返回原广播 id"
    );
    let n = count(
        &pool,
        "SELECT COUNT(*) FROM notifications WHERE broadcast_id = ?",
        Some(&broadcast_id),
    )
    .await;
    assert_eq!(n, 3, "重放不得重复插入通知");

    // 发件箱：包含该广播行。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/notifications/outbox",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "发件箱必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    let hit = items
        .iter()
        .find(|it| it["id"].as_str() == Some(broadcast_id.as_str()));
    assert!(hit.is_some(), "发件箱必须包含广播: {body}");
    assert_eq!(hit.unwrap()["target_count"].as_i64().unwrap(), 3);

    // 用户侧可见该系统通知（category 过滤）。
    let u1_session = common::direct_session_cookie(&pool, &u1).await;
    let u1_csrf = session_csrf(&app, &u1_session).await;
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/notifications?category=system",
        &u1_session,
        &u1_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "用户通知必须 200: {body}");
    let items = body["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|it| it["title"].as_str() == Some("全站公告")),
        "用户必须收到广播: {body}"
    );

    // 标记 u1 已读，撤回：未读删除（2 条）、已读保留（1 条）。
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE notifications SET is_read = 1 WHERE broadcast_id = ? AND user_id = ?",
            )
            .bind(&broadcast_id)
            .bind(&u1)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    // u2 读取自己的通知列表把广播置为已读——用直接 SQL（撤回语义测试）。
    let (status, _body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/notifications/outbox/{broadcast_id}/recall"),
        &session,
        &csrf,
        json!({ "reason": "撤回公告" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "撤回必须 204");

    let unread = count(
        &pool,
        "SELECT COUNT(*) FROM notifications WHERE broadcast_id = ? AND is_read = 0",
        Some(&broadcast_id),
    )
    .await;
    assert_eq!(unread, 0, "撤回后未读必须清零");
    let read = count(
        &pool,
        "SELECT COUNT(*) FROM notifications WHERE broadcast_id = ? AND is_read = 1",
        Some(&broadcast_id),
    )
    .await;
    assert_eq!(read, 1, "已读通知必须保留");
    let _ = u2; // u2 保持未读（撤回删除）

    // 撤回不存在 → 404；撤回审计。
    let (status, _body) = authed(
        &app,
        "POST",
        "/api/v1/admin/notifications/outbox/01911111-1111-7111-8111-111111111111/recall",
        &session,
        &csrf,
        json!({ "reason": "不存在" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "撤回不存在必须 404");
    let audit = count(
        &pool,
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.notification.recall'",
        None,
    )
    .await;
    assert_eq!(audit, 1, "撤回必须写审计");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────── 审计日志分页 ─────────────────────

#[tokio::test]
async fn audit_logs_keyset_pagination() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;
    let (author_id, _a) = insert_user(&pool, "aut").await;
    let author_session = common::direct_session_cookie(&pool, &author_id).await;
    let author_csrf = session_csrf(&app, &author_session).await;

    // 制造 5 条审计（4 个帖子动作 + 1 条设置更新）。
    let p1 = publish_post(&app, &author_session, &author_csrf, "admx-aud-000000000001").await;
    for (i, action) in ["feature", "unfeature", "pin", "unpin"].iter().enumerate() {
        let (status, body) = authed(
            &app,
            "POST",
            &format!("/api/v1/admin/posts/{p1}/action"),
            &session,
            &csrf,
            json!({ "action": action, "reason": format!("审计分页-{i}") }),
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::OK, "动作 {action} 必须 200: {body}");
    }

    // 第一页 limit=2。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/audit-logs?limit=2",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "审计列表必须 200: {body}");
    let page1 = body["items"].as_array().unwrap().clone();
    assert_eq!(page1.len(), 2, "limit=2 必须返回 2 条: {body}");
    assert!(
        !body["next_cursor"].as_str().unwrap_or("").is_empty(),
        "必须有下一页游标"
    );
    let cursor1 = body["next_cursor"].as_str().unwrap().to_string();

    // keyset 翻页：无重叠。
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/audit-logs?limit=2&after={cursor1}"),
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "第二页必须 200: {body}");
    let page2 = body["items"].as_array().unwrap().clone();
    assert_eq!(page2.len(), 2, "第二页 2 条: {body}");
    let ids1: Vec<&str> = page1.iter().filter_map(|i| i["id"].as_str()).collect();
    let ids2: Vec<&str> = page2.iter().filter_map(|i| i["id"].as_str()).collect();
    assert!(
        ids1.iter().all(|i| !ids2.contains(i)),
        "翻页不得重叠: {ids1:?} vs {ids2:?}"
    );

    // actor 过滤（按用户名）。
    let admin_username: String = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT u.username_normalized FROM audit_logs a JOIN users u ON u.id = a.actor_id LIMIT 1",
            )
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/audit-logs?actor={admin_username}"),
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        !body["items"].as_array().unwrap().is_empty(),
        "actor 过滤必须命中"
    );

    // member 403。
    let (member_id, _m) = insert_user(&pool, "mem").await;
    let member_session = common::direct_session_cookie(&pool, &member_id).await;
    let member_csrf = session_csrf(&app, &member_session).await;
    let (status, _body) = authed(
        &app,
        "GET",
        "/api/v1/admin/audit-logs",
        &member_session,
        &member_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member 必须无权读审计");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────── 角色授予/撤销 ─────────────────────

#[tokio::test]
async fn role_assign_revoke_and_guards() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (admin_id, session, csrf) = admin_ctx(&app, &pool).await;
    let (target_id, _t) = insert_user(&pool, "tgt").await;
    let (member_id, _m) = insert_user(&pool, "mem").await;
    let member_session = common::direct_session_cookie(&pool, &member_id).await;
    let member_csrf = session_csrf(&app, &member_session).await;

    // 授予 moderator → 200 返回当前角色列表。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{target_id}/roles"),
        &session,
        &csrf,
        json!({ "role_name": "global_moderator", "reason": "晋升测试" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "授予角色必须 200: {body}");
    let roles = body["roles"].as_array().unwrap();
    assert!(
        roles.iter().any(|r| r.as_str() == Some("global_moderator")),
        "角色列表必须包含 moderator: {body}"
    );

    // 幂等：重复授予同一角色 → 200 且不重复。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{target_id}/roles"),
        &session,
        &csrf,
        json!({ "role_name": "global_moderator", "reason": "重复授予" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "重复授予必须 200: {body}");
    let n = count(
        &pool,
        "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = ? AND r.name = 'global_moderator'",
        Some(&target_id),
    )
    .await;
    assert_eq!(n, 1, "user_roles 唯一约束不得重复");

    // 自我分配 → 403。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{admin_id}/roles"),
        &session,
        &csrf,
        json!({ "role_name": "global_moderator", "reason": "自我提权" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "自我分配必须 403: {body}");

    // 不存在角色 → 404；不存在用户 → 404。
    let (status, _body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{target_id}/roles"),
        &session,
        &csrf,
        json!({ "role_name": "no-such-role", "reason": "x" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let (status, _body) = authed(
        &app,
        "POST",
        "/api/v1/admin/users/01911111-1111-7111-8111-111111111111/roles",
        &session,
        &csrf,
        json!({ "role_name": "global_moderator", "reason": "x" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    // member 无 role.manage → 403。
    let (status, _body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{target_id}/roles"),
        &member_session,
        &member_csrf,
        json!({ "role_name": "global_moderator", "reason": "越权" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member 授予必须 403");

    // 撤销 → 200 移除 + 审计。
    let (status, body) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/admin/users/{target_id}/roles/global_moderator"),
        &session,
        &csrf,
        json!({ "reason": "卸任测试" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "撤销角色必须 200: {body}");
    let roles = body["roles"].as_array().unwrap();
    assert!(
        !roles.iter().any(|r| r.as_str() == Some("global_moderator")),
        "撤销后不得保留: {body}"
    );
    let grant_audit = count(
        &pool,
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.user.role.grant'",
        None,
    )
    .await;
    assert_eq!(grant_audit, 1, "授予必须写一次审计（幂等不重复）");
    let revoke_audit = count(
        &pool,
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.user.role.revoke'",
        None,
    )
    .await;
    assert_eq!(revoke_audit, 1, "撤销必须写审计");

    // 撤销未持有角色 → 200 幂等（当前角色列表）。
    let (status, _body) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/admin/users/{target_id}/roles/global_moderator"),
        &session,
        &csrf,
        json!({ "reason": "再次撤销" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "重复撤销幂等 200");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────── 公开统计 ─────────────────────

#[tokio::test]
async fn public_stats_anonymous_and_cached() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (author_id, _a) = insert_user(&pool, "aut").await;
    let author_session = common::direct_session_cookie(&pool, &author_id).await;
    let author_csrf = session_csrf(&app, &author_session).await;

    // 1 个 active 用户 + 1 个 published 帖。
    publish_post(&app, &author_session, &author_csrf, "admx-pst-000000000001").await;

    // 匿名可读，计数正确，可缓存。
    let (status, body, cache_control) = anon_get(&app, "/api/v1/stats").await;
    assert_eq!(status, StatusCode::OK, "公开统计必须 200: {body}");
    assert_eq!(body["members"].as_i64().unwrap(), 1, "公开 members: {body}");
    assert_eq!(body["posts"].as_i64().unwrap(), 1, "公开 posts: {body}");
    assert!(body["comments"].is_i64());
    assert!(body["boards"].is_i64());
    assert!(body["tags"].is_i64());
    assert_eq!(
        cache_control, "public, max-age=60",
        "公开统计缓存头: {cache_control}"
    );

    // 隐藏帖不计入：管理动作下架后公开计数下降。
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;
    let post_id = match &pool {
        Either::Left(p) => sqlx::query_scalar::<_, String>(
            "SELECT id FROM posts WHERE status = 'published' LIMIT 1",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let _ = post_id;
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/posts/{post_id}/action"),
        &session,
        &csrf,
        json!({ "action": "hide", "reason": "下架验证" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "下架必须 200: {body}");
    let (status, body, _cc) = anon_get(&app, "/api/v1/stats").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body["posts"].as_i64().unwrap(),
        0,
        "隐藏帖不得计入公开统计: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_ai_provider_add_update_delete() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, session, csrf) = admin_ctx(&app, &pool).await;

    // 1. GET /api/v1/admin/ai/config
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/ai/config",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let version = body["version"].as_i64().expect("version integer");

    // 2. Add provider via PATCH /api/v1/admin/ai/config
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/ai/config",
        &session,
        &csrf,
        json!({
            "name": "新模型渠道",
            "base_url": "https://api.openai.com/v1",
            "adapter_type": "openai_compatible",
            "default_model": "gpt-4o",
            "status": "enabled",
            "api_key": "sk-test-secret-key",
            "expected_version": version,
            "reason": "添加新测试渠道"
        }),
        &[("if-match", &format!("\"{version}\""))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "添加渠道必须 200: {body}");
    let providers = body["providers"].as_array().expect("providers list");
    let added = providers
        .iter()
        .find(|p| p["name"] == "新模型渠道")
        .expect("added provider must exist");
    assert_eq!(added["base_url"], "https://api.openai.com/v1");
    assert_eq!(added["secret_configured"], true);
    let provider_id = added["id"].as_str().expect("provider id").to_string();
    let new_version = body["version"].as_i64().expect("new version");

    // 3. Update provider
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/ai/config",
        &session,
        &csrf,
        json!({
            "id": provider_id,
            "name": "新模型渠道",
            "base_url": "https://api.deepseek.com/v1",
            "default_model": "deepseek-chat",
            "status": "disabled",
            "expected_version": new_version,
            "reason": "更新渠道配置"
        }),
        &[("if-match", &format!("\"{new_version}\""))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "更新渠道必须 200: {body}");
    let updated = body["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == provider_id.as_str())
        .expect("updated provider must exist");
    assert_eq!(updated["base_url"], "https://api.deepseek.com/v1");
    assert_eq!(updated["default_model"], "deepseek-chat");
    assert_eq!(updated["status"], "disabled");
    assert_eq!(updated["secret_configured"], true); // secret retained
    let final_version = body["version"].as_i64().expect("final version");

    // 4. Delete provider
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/ai/config",
        &session,
        &csrf,
        json!({
            "delete_provider_id": provider_id,
            "expected_version": final_version,
            "reason": "删除测试渠道"
        }),
        &[("if-match", &format!("\"{final_version}\""))],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "删除渠道必须 200: {body}");
    let remaining = body["providers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == provider_id.as_str());
    assert!(remaining.is_none(), "deleted provider must not exist");

    close_pool(&pool).await;
    cleanup(&dir);
}
