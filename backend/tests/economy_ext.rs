//! GAP-FIX 管理域 Part B（经济与个人域）集成测试（SQLite + 路由层）。
//!
//! 覆盖：
//! - 积分调整（写流水 + 通知 + 审计 + 权限 403 + 幂等重放 + 非法 amount）；
//! - 管理流水筛选（username/asset/kind/from/to）与 keyset 分页；
//! - 本人流水（me/point-transactions 仅本人可见）；
//! - 等级规则（GET 列表种子、PATCH If-Match 乐观锁 409/400）；
//! - 我的处罚（sanctions 表投影，revoked 不列）；
//! - 修改密码（当前密码错误 401 code=invalid_current_password；成功后
//!   旧会话撤销、当前会话保留、新密码可登录校验）；
//! - OAuth 授权管理（oauth_consents 聚合列表 + 撤销 + token 作废 + 幂等）；
//! - 附件管理（列表 q 过滤 + 软删 + 404）；
//! - 下载计费交易（download_authorizations 联查投影）；
//! - 标签合并（post_tags 关联转移 + usage 同步 + 源标记 merged）；
//! - 付费解锁（paid 帖创建 + price_coin 422 校验 + 扣款 + grant + 作者通知
//!   + 幂等重放 + 余额不足 409 insufficient_funds + 作者免费解锁）。

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
    let dir = std::env::temp_dir().join(format!("bblbb-ecox-{}", uuid::Uuid::now_v7()));
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

/// 设置用户真实密码 hash（insert_user 写入的是占位 'dummy'）。
async fn set_password(pool: &DatabasePool, user_id: &str, password: &str) {
    let hash = bblbb_backend::auth::password::hash_password(password).unwrap();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET password_hash = ? WHERE id = ?")
                .bind(&hash)
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

/// 任意方法已认证请求（会话 + CSRF，可带附加 header）。
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

/// 普通成员上下文（无需 TOTP）；返回 (user_id, username, session, csrf)。
async fn member_ctx(
    app: &Router,
    pool: &DatabasePool,
    tag: &str,
) -> (String, String, String, String) {
    let (user_id, username) = insert_user(pool, tag).await;
    let session = common::direct_session_cookie(pool, &user_id).await;
    let csrf = session_csrf(app, &session).await;
    (user_id, username, session, csrf)
}

/// 管理员上下文（administrator + TOTP enrollment + step-up）。
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

/// 直接 SQL 授予全局角色。
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

/// 通过 API 发布帖子（默认 public；可覆盖字段）；返回 (status, body)。
async fn publish_post(
    app: &Router,
    session: &str,
    csrf: &str,
    crid: &str,
    overrides: Value,
) -> (StatusCode, Value) {
    let mut body = json!({
        "type": "article",
        "title": format!("经济域测试帖-{crid}"),
        "markdown": "正文内容 body with enough text",
        "board_id": BOARD_ID,
        "visibility_level": 1,
        "access_policy": "public",
        "client_request_id": crid,
    });
    if let Some(obj) = body.as_object_mut() {
        if let Some(over) = overrides.as_object() {
            for (k, v) in over {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    authed(app, "POST", "/api/v1/posts", session, csrf, body, &[]).await
}

/// 直接 SQL 插入标签；返回 tag_id。
async fn insert_tag(pool: &DatabasePool, name: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO tags (id, name, usage_count, created_at, slug, description, is_active, updated_at)
                 VALUES (?, ?, 0, ?, ?, '', 1, ?)",
            )
            .bind(&id)
            .bind(name)
            .bind(now)
            .bind(name.to_lowercase())
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    id
}

// ─── 积分调整 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn points_adjust_writes_ledger_notifies_and_rejects_member() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (member_id, member_username, member_session, member_csrf) =
        member_ctx(&app, &pool, "mem").await;
    let (_other_id, other_username, _s, _c) = member_ctx(&app, &pool, "oth").await;

    // 普通成员调用 → 403。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/points/adjust",
        &member_session,
        &member_csrf,
        json!({
            "username": other_username,
            "currency": "coin",
            "amount": 10,
            "reason": "test",
            "client_request_id": "crid-adjust-member-000000001",
        }),
        &[],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "member must be rejected: {body}"
    );

    // amount=0 → 400。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/points/adjust",
        &admin_session,
        &admin_csrf,
        json!({
            "username": member_username,
            "currency": "coin",
            "amount": 0,
            "reason": "test",
            "client_request_id": "crid-adjust-zero-00000000001",
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // 正常调整 → 201 + 余额。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/points/adjust",
        &admin_session,
        &admin_csrf,
        json!({
            "username": member_username,
            "currency": "coin",
            "amount": 100,
            "reason": "活动奖励",
            "client_request_id": "crid-adjust-ok-0000000000001",
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["amount"], json!(100));
    assert_eq!(body["balance"], json!(100));
    assert_eq!(body["currency"], json!("coin"));
    assert_eq!(body["username"], json!(member_username));

    // 账本流水已写（point_transactions + point_operations.kind='adjust'）。
    let tx_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM point_transactions t
                 JOIN point_operations op ON op.id = t.operation_id
                 WHERE t.user_id = ? AND op.kind = 'adjust' AND t.delta_balance = 100",
        )
        .bind(&member_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(
        tx_count, 1,
        "ledger transaction must be written exactly once"
    );

    // 通知已插入（type='system'，title「积分调整」）。
    let notified: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications
                 WHERE user_id = ? AND type = 'system' AND title = '积分调整'",
        )
        .bind(&member_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(notified, 1, "member must be notified");

    // 幂等重放：同 client_request_id → 201 且不重复入账。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/points/adjust",
        &admin_session,
        &admin_csrf,
        json!({
            "username": member_username,
            "currency": "coin",
            "amount": 100,
            "reason": "活动奖励",
            "client_request_id": "crid-adjust-ok-0000000000001",
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["balance"], json!(100), "replay must not double-credit");
    let final_balance: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT balance FROM point_accounts WHERE user_id = ?")
                .bind(&member_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(final_balance, 100);

    // 目标用户不存在 → 404。
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/points/adjust",
        &admin_session,
        &admin_csrf,
        json!({
            "username": "no_such_user_xyz",
            "currency": "coin",
            "amount": 5,
            "reason": "test",
            "client_request_id": "crid-adjust-missing-00000001",
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 管理流水筛选与分页 ──────────────────────────────────────────────────────

#[tokio::test]
async fn admin_points_ledger_filters_and_pagination() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (_m1, u1, _s, _c) = member_ctx(&app, &pool, "u1").await;
    let (_m2, u2, _s2, _c2) = member_ctx(&app, &pool, "u2").await;

    // 3 笔 coin + 1 笔 exp（不同 key）。
    for (i, (username, currency, amount)) in [
        (&u1, "coin", 10),
        (&u1, "coin", 20),
        (&u1, "exp", 5),
        (&u2, "coin", 30),
    ]
    .into_iter()
    .enumerate()
    {
        let (status, body) = authed(
            &app,
            "POST",
            "/api/v1/admin/points/adjust",
            &admin_session,
            &admin_csrf,
            json!({
                "username": username,
                "currency": currency,
                "amount": amount,
                "reason": "筛选测试",
                "client_request_id": format!("crid-ledger-{i}-0000000000000001"),
            }),
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }

    // username 过滤。
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/points/ledger?username={u1}"),
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 3);

    // asset=coin + kind=adjust 组合过滤（u1 只有 2 笔 coin）。
    let (status, body) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/points/ledger?username={u1}&asset=coin&kind=adjust"),
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i["currency"] == json!("coin")));

    // 非法 kind → 400。
    let (status, _) = authed(
        &app,
        "GET",
        "/api/v1/admin/points/ledger?kind=bogus",
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // keyset 分页：limit=2 → has_more（next_cursor 非空），翻页后无更多。
    let (status, page1) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/points/ledger?username={u1}&limit=2"),
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["items"].as_array().unwrap().len(), 2);
    let cursor = page1["next_cursor"].as_str().unwrap().to_string();
    assert!(!cursor.is_empty(), "must have next page");
    let (status, page2) = authed(
        &app,
        "GET",
        &format!("/api/v1/admin/points/ledger?username={u1}&limit=2&after={cursor}"),
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
    assert_eq!(page2["next_cursor"], json!(""));

    // 成员无权限 → 403。
    let (m_session, m_csrf) = {
        let (id, _u, s, c) = member_ctx(&app, &pool, "nop").await;
        let _ = id;
        (s, c)
    };
    let (status, _) = authed(
        &app,
        "GET",
        "/api/v1/admin/points/ledger",
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 本人流水 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn my_point_transactions_visible_only_to_self() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (_m1, u1, s1, c1) = member_ctx(&app, &pool, "vis1").await;
    let (_m2, _u2, s2, c2) = member_ctx(&app, &pool, "vis2").await;

    // 给 u1 调两笔，给 u2 调一笔。
    for (i, (username, amount)) in [(&u1, 7), (&u1, 9)].into_iter().enumerate() {
        let (status, body) = authed(
            &app,
            "POST",
            "/api/v1/admin/points/adjust",
            &admin_session,
            &admin_csrf,
            json!({
                "username": username,
                "currency": "exp",
                "amount": amount,
                "reason": "可见性测试",
                "client_request_id": format!("crid-mytx-{i}-000000000000001"),
            }),
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
    }
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/points/adjust",
        &admin_session,
        &admin_csrf,
        json!({
            "username": _u2,
            "currency": "exp",
            "amount": 3,
            "reason": "可见性测试",
            "client_request_id": "crid-mytx-u2-000000000000001",
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");

    // 本人看到自己的 2 笔。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/point-transactions",
        &s1,
        &c1,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert!(items.iter().all(|i| i["currency"] == json!("exp")));
    assert_eq!(items[0]["amount"], json!(9), "created_at DESC");

    // 他人（u2）只看到自己的 1 笔。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/point-transactions",
        &s2,
        &c2,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["items"].as_array().unwrap().len(), 1);

    // 匿名 → 401。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me/point-transactions")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 等级规则 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn admin_levels_list_and_patch_if_match() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (member_id, _mu, m_session, m_csrf) = member_ctx(&app, &pool, "lvl").await;
    let _ = member_id;

    // 成员 → 403。
    let (status, _) = authed(
        &app,
        "GET",
        "/api/v1/admin/levels",
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 列表：5 级种子 + user_count（admin 是 level 5）。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/levels",
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 5, "seeded level rules");
    assert_eq!(items[0]["level"], json!(1));
    assert_eq!(items[0]["min_exp"], json!(0));
    let level5 = items.iter().find(|i| i["level"] == json!(5)).unwrap();
    // admin_ctx 与 member_ctx 的 insert_user 都写 level=5。
    assert_eq!(
        level5["user_count"],
        json!(2),
        "both test users are level 5"
    );
    assert_eq!(level5["version"], json!(1));

    // PATCH：缺 If-Match → 400。
    let (status, _) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/levels/3",
        &admin_session,
        &admin_csrf,
        json!({ "name": "Lv3 常客改", "reason": "调整等级规则" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // PATCH：If-Match 正确 → 200 + version 递增。
    let (status, body) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/levels/3",
        &admin_session,
        &admin_csrf,
        json!({ "name": "Lv3 常客改", "daily_post_limit": 25, "reason": "调整等级规则" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["name"], json!("Lv3 常客改"));
    assert_eq!(body["daily_post_limit"], json!(25));
    assert_eq!(body["version"], json!(2));

    // 旧版本再 PATCH → 409。
    let (status, _) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/levels/3",
        &admin_session,
        &admin_csrf,
        json!({ "name": "again", "reason": "再次调整" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // 不存在的等级 → 404。
    let (status, _) = authed(
        &app,
        "PATCH",
        "/api/v1/admin/levels/99",
        &admin_session,
        &admin_csrf,
        json!({ "name": "x", "reason": "r" }),
        &[("if-match", "1")],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = admin_id;

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 我的处罚 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn my_sanctions_lists_own_records() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_staff_id, _su, staff_session, staff_csrf) = member_ctx(&app, &pool, "stf").await;
    let (member_id, _mu, m_session, m_csrf) = member_ctx(&app, &pool, "sct").await;

    // 直接 SQL 插入两条处罚（一条有效、一条已撤销）。
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at)
                 VALUES (?, ?, NULL, 'mute', 'active', '灌水', ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&member_id)
            .bind(now - 1000)
            .bind(now + 86_400_000)
            .bind(&_staff_id)
            .bind(now - 500)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at, revoked_at, revoked_by, revoke_reason)
                 VALUES (?, ?, NULL, 'warning', 'revoked', '误报', ?, NULL, ?, ?, ?, ?, '撤销')",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&member_id)
            .bind(now - 2000)
            .bind(&_staff_id)
            .bind(now - 1500)
            .bind(now)
            .bind(&_staff_id)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/sanctions",
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "revoked sanction must be excluded");
    assert_eq!(items[0]["kind"], json!("mute"));
    assert_eq!(items[0]["reason"], json!("灌水"));
    assert_eq!(
        items[0]["expires_at"].as_i64().unwrap() - items[0]["created_at"].as_i64().unwrap(),
        86_400_000 + 500
    );

    // 无处罚的用户 → 空列表。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/sanctions",
        &staff_session,
        &staff_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 修改密码 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn change_password_revokes_other_sessions() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (member_id, _u, _s, _c) = member_ctx(&app, &pool, "pw").await;
    set_password(&pool, &member_id, "OldPass1234").await;

    // 两个会话：A（当前，用于改密）+ B（旧设备）。
    let session_a = common::direct_session_cookie(&pool, &member_id).await;
    let session_b = common::direct_session_cookie(&pool, &member_id).await;
    let csrf_a = session_csrf(&app, &session_a).await;

    // 当前密码错误 → 401 code=invalid_current_password。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/me/password",
        &session_a,
        &csrf_a,
        json!({ "current_password": "WrongPass123", "new_password": "NewPass5678" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{body}");
    assert_eq!(body["code"], json!("invalid_current_password"));

    // 新密码强度不足 → 400。
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/me/password",
        &session_a,
        &csrf_a,
        json!({ "current_password": "OldPass1234", "new_password": "short" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 正确改密 → 204。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/me/password",
        &session_a,
        &csrf_a,
        json!({ "current_password": "OldPass1234", "new_password": "NewPass5678" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT, "{body}");

    // 旧会话 B 被撤销 → 401。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me/point-transactions")
                .header("cookie", &session_b)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "old session revoked"
    );

    // 当前会话 A 仍有效。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me/point-transactions")
                .header("cookie", &session_a)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "current session survives");

    // DB 校验：password_hash 已更新（新密码可验证）。
    let hash: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(&member_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(
        bblbb_backend::auth::password::verify_password("NewPass5678", &hash),
        bblbb_backend::auth::password::VerifyResult::Ok
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── OAuth 授权管理 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn oauth_grants_list_and_revoke() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (admin_id, _au, _as, _ac) = member_ctx(&app, &pool, "oa").await;
    let (member_id, _mu, m_session, m_csrf) = member_ctx(&app, &pool, "oag").await;
    let now = now_millis();

    // 直接 SQL：Client + consents（2 个 scope）+ token（带 last_used_at）。
    let client_row_id = uuid::Uuid::now_v7().to_string();
    let client_id_str = format!("client-{}", uuid::Uuid::now_v7().simple());
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO oauth_clients (id, name, client_type, client_id, redirect_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at)
                 VALUES (?, '测试客户端', 'public', ?, '[]', '[]', 'active', 1, ?, ?, ?, ?)",
            )
            .bind(&client_row_id)
            .bind(&client_id_str)
            .bind(&admin_id)
            .bind(now - 10_000)
            .bind(&admin_id)
            .bind(now - 10_000)
            .execute(p)
            .await
            .unwrap();
            for scope in ["openid", "profile"] {
                // oauth_consents.client_id 存 oauth_clients 行 id（FK → id）。
                sqlx::query(
                    "INSERT INTO oauth_consents (id, user_id, client_id, scope, granted_at)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(uuid::Uuid::now_v7().to_string())
                .bind(&member_id)
                .bind(&client_row_id)
                .bind(scope)
                .bind(now - 5000)
                .execute(p)
                .await
                .unwrap();
            }
            // token family + token（last_used_at 供聚合）。
            let family_id = uuid::Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO oauth_token_families (id, client_id, user_id, scope, created_at)
                 VALUES (?, ?, ?, 'openid', ?)",
            )
            .bind(&family_id)
            .bind(&client_row_id)
            .bind(&member_id)
            .bind(now - 4000)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO oauth_tokens (id, family_id, access_token_hash, client_id, user_id, scope, issued_at, expires_at, last_used_at)
                 VALUES (?, ?, ?, ?, ?, 'openid', ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&family_id)
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&client_row_id)
            .bind(&member_id)
            .bind(now - 4000)
            .bind(now + 3_600_000)
            .bind(now - 1000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 列表：聚合出 1 个 Client，含名称/两个 scope/last_used_at。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/oauth-grants",
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["client_id"], json!(client_id_str));
    assert_eq!(items[0]["client_name"], json!("测试客户端"));
    let scopes = items[0]["scopes"].as_array().unwrap();
    assert_eq!(scopes.len(), 2);
    assert!(scopes.contains(&json!("openid")));
    assert!(scopes.contains(&json!("profile")));
    assert_eq!(items[0]["last_used_at"].as_i64().unwrap(), now - 1000);

    // 撤销 → 204；列表清空；token 作废。
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/me/oauth-grants/{client_id_str}"),
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/me/oauth-grants",
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    let token_revoked: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM oauth_tokens WHERE user_id = ? AND revoked_at IS NOT NULL",
        )
        .bind(&member_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(token_revoked, 1, "active token must be revoked");

    // 幂等：再次 DELETE → 204；未知 Client → 404。
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/me/oauth-grants/{client_id_str}"),
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = authed(
        &app,
        "DELETE",
        "/api/v1/me/oauth-grants/unknown-client",
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 附件管理 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn admin_attachments_list_and_soft_delete() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (owner_id, _ou, _os, _oc) = member_ctx(&app, &pool, "up").await;
    let (member_id, _mu, m_session, m_csrf) = member_ctx(&app, &pool, "upm").await;
    let now = now_millis();

    // 直接 SQL 插入两个附件。
    let att1 = uuid::Uuid::now_v7().to_string();
    let att2 = uuid::Uuid::now_v7().to_string();
    match &pool {
        Either::Left(p) => {
            for (id, name, ts) in [
                (&att1, Some("report.pdf".to_string()), now - 2000),
                (&att2, Some("photo.png".to_string()), now - 1000),
            ] {
                sqlx::query(
                    "INSERT INTO attachments (id, owner_id, storage_backend, storage_key, original_name, media_type, size_bytes, sha256, status, created_at, deleted_at)
                     VALUES (?, ?, 'local', ?, ?, 'application/octet-stream', 100, 'abc', 'ready', ?, NULL)",
                )
                .bind(id)
                .bind(&owner_id)
                .bind(format!("key-{id}"))
                .bind(&name)
                .bind(ts)
                .execute(p)
                .await
                .unwrap();
            }
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 成员 → 403。
    let (status, _) = authed(
        &app,
        "GET",
        "/api/v1/admin/attachments",
        &m_session,
        &m_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let _ = member_id;

    // 列表（created_at DESC）+ q 过滤。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/attachments",
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], json!(att2), "created_at DESC");
    assert_eq!(items[0]["filename"], json!("photo.png"));
    assert_eq!(items[1]["filename"], json!("report.pdf"));

    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/attachments?q=photo",
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], json!(att2));
    assert_eq!(items[0]["filename"], json!("photo.png"));

    // 删除：缺 reason → 400。
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/admin/attachments/{att1}"),
        &admin_session,
        &admin_csrf,
        json!({}),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 删除：正常 → 204，软删列置位。
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/admin/attachments/{att1}"),
        &admin_session,
        &admin_csrf,
        json!({ "reason": "违规附件" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let has_deleted_at: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT deleted_at IS NOT NULL FROM attachments WHERE id = ?")
                .bind(&att1)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    let status_val: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM attachments WHERE id = ?")
            .bind(&att1)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(has_deleted_at, 1, "deleted_at must be set");
    assert_eq!(status_val, "deleted");

    // 列表不再包含已删附件；重复删除 → 404。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/attachments",
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["id"], json!(att2));
    let (status, _) = authed(
        &app,
        "DELETE",
        &format!("/api/v1/admin/attachments/{att1}"),
        &admin_session,
        &admin_csrf,
        json!({ "reason": "再次删除" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 下载计费交易 ────────────────────────────────────────────────────────────

#[tokio::test]
async fn download_billing_transactions_projection() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (member_id, member_username, _ms, _mc) = member_ctx(&app, &pool, "dl").await;
    let now = now_millis();

    let att = uuid::Uuid::now_v7().to_string();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO attachments (id, owner_id, storage_backend, storage_key, original_name, media_type, size_bytes, sha256, status, created_at, deleted_at)
                 VALUES (?, ?, 'local', ?, 'manual.zip', 'application/zip', 500, 'abc', 'ready', ?, NULL)",
            )
            .bind(&att)
            .bind(&member_id)
            .bind(format!("key-{att}"))
            .bind(now - 100)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO download_authorizations (id, attachment_id, user_id, policy_version, point_operation_id, status, charged_amount, currency_id, valid_from, expires_at, created_at)
                 VALUES (?, ?, ?, 1, NULL, 'active', 5, NULL, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&att)
            .bind(&member_id)
            .bind(now - 50)
            .bind(now + 3_600_000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/download-billing/transactions",
        &admin_session,
        &admin_csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["username"], json!(member_username));
    assert_eq!(items[0]["filename"], json!("manual.zip"));
    assert_eq!(items[0]["amount"], json!(5));
    assert!(items[0]["id"].is_string());

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 标签合并 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn tag_merge_moves_associations_and_marks_source() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let (member_id, _mu, m_session, m_csrf) = member_ctx(&app, &pool, "tg").await;

    // 两个标签 + 一篇帖子 + 两行 post_tags 关联（另一篇只挂源标签）。
    let src_tag = insert_tag(&pool, "源标签").await;
    let dst_tag = insert_tag(&pool, "目标标签").await;
    let (status, post1) = publish_post(
        &app,
        &m_session,
        &m_csrf,
        "crid-tag-merge-00000000001",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{post1}");
    let (status, post2) = publish_post(
        &app,
        &m_session,
        &m_csrf,
        "crid-tag-merge-00000000002",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{post2}");
    let post1_id = post1["id"].as_str().unwrap().to_string();
    let post2_id = post2["id"].as_str().unwrap().to_string();
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            for (post_id, tag_id) in [
                (&post1_id, &src_tag),
                (&post1_id, &dst_tag),
                (&post2_id, &src_tag),
            ] {
                sqlx::query("INSERT INTO post_tags (post_id, tag_id, created_at) VALUES (?, ?, ?)")
                    .bind(post_id)
                    .bind(tag_id)
                    .bind(now)
                    .execute(p)
                    .await
                    .unwrap();
            }
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 合并 src → dst：post1 已有 dst 关联（冲突跳过），post2 关联转移。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/tags/{src_tag}/merge"),
        &admin_session,
        &admin_csrf,
        json!({ "target_id": dst_tag, "reason": "重复标签" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["merged"], json!(src_tag));
    assert_eq!(body["into"], json!(dst_tag));
    assert_eq!(body["moved_usage"], json!(2));

    // 源标签关联清零、标记 merged 并停用。
    let src_state: (i64, i64, Option<String>) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT usage_count, is_active, status FROM tags WHERE id = ?")
                .bind(&src_tag)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(src_state.0, 0, "source usage_count zeroed");
    assert_eq!(src_state.1, 0, "source deactivated");
    assert_eq!(src_state.2.as_deref(), Some("merged"));

    // 目标标签 usage = 2（两篇帖子都挂上了）。
    let dst_usage: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM post_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.tag_id = ?",
        )
        .bind(&dst_tag)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(dst_usage, 2);

    // 自我合并 → 400；不存在 → 404。
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/tags/{dst_tag}/merge"),
        &admin_session,
        &admin_csrf,
        json!({ "target_id": dst_tag, "reason": "x" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/tags/no-such-tag/merge",
        &admin_session,
        &admin_csrf,
        json!({ "target_id": dst_tag, "reason": "x" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = member_id;

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 付费解锁 ────────────────────────────────────────────────────────────────

#[tokio::test]
async fn paid_post_create_price_validation_and_unlock_flow() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;

    // 预置两个标签（CreatePostRequest.tags 只接受已存在的标签）。
    let tag_a = insert_tag(&pool, "闲聊").await;
    let tag_b = insert_tag(&pool, "指南").await;

    // 作者 + 两个买家。
    let (author_id, _au, author_session, author_csrf) = member_ctx(&app, &pool, "au").await;
    let (buyer_id, _bu, buyer_session, buyer_csrf) = member_ctx(&app, &pool, "by").await;
    let (poor_id, _pu, poor_session, poor_csrf) = member_ctx(&app, &pool, "po").await;

    // paid 缺 price_coin → 422 code=invalid_price_coin。
    let (status, body) = publish_post(
        &app,
        &author_session,
        &author_csrf,
        "crid-paid-missing-price-0001",
        json!({ "access_policy": "paid" }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], json!("invalid_price_coin"));

    // paid 价格越界（0/1001）→ 422。
    for (i, price) in [0u32, 1001].into_iter().enumerate() {
        let (status, _) = publish_post(
            &app,
            &author_session,
            &author_csrf,
            &format!("crid-paid-bad-price-{i}-00001"),
            json!({ "access_policy": "paid", "price_coin": price }),
        )
        .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    }

    // 非 paid 带价格 → 422。
    let (status, _) = publish_post(
        &app,
        &author_session,
        &author_csrf,
        "crid-free-with-price-000001",
        json!({ "access_policy": "public", "price_coin": 10 }),
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);

    // summary 超长 / tags 超量 → 400。
    let (status, _) = publish_post(
        &app,
        &author_session,
        &author_csrf,
        "crid-long-summary-00000001",
        json!({ "summary": "x".repeat(301) }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = publish_post(
        &app,
        &author_session,
        &author_csrf,
        "crid-many-tags-0000000001",
        json!({ "tags": ["a", "b", "c", "d", "e", "f", "g", "h", "i"] }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 创建 paid 帖（price_coin=50）。
    let (status, post) = publish_post(
        &app,
        &author_session,
        &author_csrf,
        "crid-paid-create-0000000001",
        json!({
            "access_policy": "paid",
            "price_coin": 50,
            "summary": "付费帖摘要",
            "tags": ["闲聊", "指南"],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{post}");
    let post_id = post["id"].as_str().unwrap().to_string();
    assert_eq!(post["price_coin"], json!(50));
    assert_eq!(post["summary"], json!("付费帖摘要"));

    // 落库校验：price_coin/access_policy_id/paid 策略行/标签关联。
    let (price_col, policy_id): (Option<i64>, Option<String>) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT price_coin, access_policy_id FROM posts WHERE id = ?")
                .bind(&post_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(price_col, Some(50));
    let policy = policy_id.clone().unwrap();
    let (kind, amount): (String, i64) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT kind, amount FROM content_access_policies WHERE id = ?")
                .bind(&policy)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(kind, "paid");
    assert_eq!(amount, 50);
    let tag_links: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM post_tags WHERE post_id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(tag_links, 2, "both tags must be linked");
    assert_eq!(post["tags"].as_array().unwrap().len(), 2);

    // 免费帖解锁 → 422 post_not_paid。
    let (status, free_post) = publish_post(
        &app,
        &author_session,
        &author_csrf,
        "crid-free-create-0000000001",
        json!({}),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let free_id = free_post["id"].as_str().unwrap().to_string();
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/posts/{free_id}/unlock"),
        &buyer_session,
        &buyer_csrf,
        json!({ "client_request_id": "crid-unlock-free-000000001" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["code"], json!("post_not_paid"));

    // 余额不足 → 409 code=insufficient_funds（poor 余额 0）。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/posts/{post_id}/unlock"),
        &poor_session,
        &poor_csrf,
        json!({ "client_request_id": "crid-unlock-poor-000000001" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["code"], json!("insufficient_funds"));

    // 给 buyer 充 100 金币。
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/points/adjust",
        &admin_session,
        &admin_csrf,
        json!({
            "username": _bu,
            "currency": "coin",
            "amount": 100,
            "reason": "解锁测试入账",
            "client_request_id": "crid-unlock-fund-000000001",
        }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{body}");
    assert_eq!(body["balance"], json!(100));

    // 解锁成功 → 200 {unlocked:true, coin_balance:50}。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/posts/{post_id}/unlock"),
        &buyer_session,
        &buyer_csrf,
        json!({ "client_request_id": "crid-unlock-buyer-000001" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["unlocked"], json!(true));
    assert_eq!(body["coin_balance"], json!(50));

    // grant 已写入（purchase 来源）+ 作者收到通知。
    let grants: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM content_access_grants
                 WHERE user_id = ? AND post_id = ? AND source_kind = 'purchase'",
        )
        .bind(&buyer_id)
        .bind(&post_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(grants, 1);
    let author_notified: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM notifications
                 WHERE user_id = ? AND type = 'system' AND title = '你的付费内容被解锁'",
        )
        .bind(&author_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(author_notified, 1, "author must be notified");

    // 扣款流水：kind='consume' + source_type='post_unlock'。
    let consumed: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM point_transactions t
                 JOIN point_operations op ON op.id = t.operation_id
                 WHERE t.user_id = ? AND op.kind = 'consume'
                   AND op.source_type = 'post_unlock' AND op.source_id = ?
                   AND t.delta_balance = -50",
        )
        .bind(&buyer_id)
        .bind(&post_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(consumed, 1, "ledger consume must be written");

    // 幂等重放：同 client_request_id → 200，不重复扣款。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/posts/{post_id}/unlock"),
        &buyer_session,
        &buyer_csrf,
        json!({ "client_request_id": "crid-unlock-buyer-000001" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["unlocked"], json!(true));
    assert_eq!(
        body["coin_balance"],
        json!(50),
        "replay must not double-charge"
    );
    let final_balance: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(&buyer_id)
        .bind(bblbb_backend::economy::ledger::service::CURRENCY_COIN)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(final_balance, 50);

    // 解锁后买家可读帖子（可见性 grant 生效）。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/v1/posts/{post_id}"))
                .header("cookie", &buyer_session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "buyer can read after unlock");

    // 作者免费解锁自己的付费帖（evaluate 不给作者放行 paid 内容 → 免费授予）。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/posts/{post_id}/unlock"),
        &author_session,
        &author_csrf,
        json!({ "client_request_id": "crid-unlock-author-000001" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["unlocked"], json!(true));
    assert_eq!(body["coin_balance"], json!(0), "author must not be charged");
    let _ = poor_id;

    close_pool(&pool).await;
    cleanup(&dir);
}
