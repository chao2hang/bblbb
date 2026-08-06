//! M02-MFA-05/06：强制 TOTP enrollment 集成测试。
//!
//! - 普通 member 可选 TOTP；administrator / moderator（全局与板块）/
//!   高风险账务账号（sensitive/system 权限）**强制启用**（MFA-05）；
//! - 未完成强制 enrollment 的账号聚合被降级为 member 基线：
//!   不得取得高权限 Session（会话 roles 与 `/me` 投影不宣称高权限），
//!   不得执行高风险操作（authorize_action / HTTP 管理员端点 403）（MFA-06）；
//! - 完成 enrollment 后聚合、会话与操作立即恢复（fail-closed 语义，
//!   无需重建 Session——判定实时依赖 TOTP 状态）。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::auth::hash_password;
use bblbb_backend::authz::decision::{DenyReason, AUTHZ_POLICY_VERSION};
use bblbb_backend::authz::enforce::authorize_action;
use bblbb_backend::authz::roles::{aggregate_permissions, seed_builtin_roles};
use bblbb_backend::authz::PERMISSION_REGISTRY;
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

const PASSWORD: &str = "correct-password";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-mfaf-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let email = format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy-hash', 'active', ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(&email)
            .bind(Some(now - 2 * 86_400_000))
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

async fn role_id_by_name(pool: &DatabasePool, name: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn board_id_by_slug(pool: &DatabasePool, slug: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM boards WHERE slug = ?")
            .bind(slug)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id = role_id_by_name(pool, role_name).await;
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

async fn assign_board_role(pool: &DatabasePool, user_id: &str, board_id: &str, role_name: &str) {
    let role_id = role_id_by_name(pool, role_name).await;
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO board_roles (board_id, role_id, granted_at) VALUES (?, ?, ?)")
                .bind(board_id)
                .bind(&role_id)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            sqlx::query(
                "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, ?, ?, NULL, ?, NULL)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(board_id)
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

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn insert_login_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let hash = hash_password(PASSWORD).unwrap();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&email)
            .bind(&email)
            .bind(&hash)
            .bind(Some(now - 2 * 86_400_000))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (user_id, email)
}

async fn login_session_cookie(app: &Router, email: &str) -> (String, Value) {
    let (cookie, csrf) = common::fetch_preauth(app).await;
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/auth/login")
                .header("content-type", "application/json")
                .header("x-csrf-token", &csrf)
                .header("cookie", &cookie)
                .body(Body::from(
                    json!({ "identifier": email, "password": PASSWORD }).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK, "登录必须 200");
    let session = resp
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let me: Value = serde_json::from_slice(&bytes).unwrap();
    (session, me)
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

async fn get_me(app: &Router, session: &str) -> Value {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me")
                .header("cookie", session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap()
}

async fn admin_create_board(app: &Router, session: &str, csrf: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/admin/boards")
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .body(Body::from(
                    json!({
                        "slug": "meta-2",
                        "name": "站务公告 2",
                        "description": "规则",
                        "reason": "v1 初始化"
                    })
                    .to_string(),
                ))
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

/// MFA-05：administrator 未完成 TOTP enrollment → 聚合降级为 member 基线
/// （不宣称任何高权限）；普通 member 无 TOTP 保持可选、聚合不变。
#[tokio::test]
async fn forced_user_without_totp_aggregates_as_member() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let admin = insert_user(&pool, "adm").await;
    let member = insert_user(&pool, "mem").await;
    assign_global_role(&pool, &admin, "administrator").await;

    // administrator 未完成 enrollment → member 基线（TOTP 可选路径不受影响）
    let agg = aggregate_permissions(&pool, &admin, None)
        .await
        .expect("聚合必须成功");
    assert_eq!(
        agg.global_roles,
        vec!["member".to_string()],
        "未完成强制 enrollment 的管理员会话角色必须降级为 member"
    );
    assert!(agg.has("post.read"), "member 基线保留");
    assert!(!agg.has("admin.manage"), "不得宣称 admin.manage");
    assert!(!agg.has("user.manage"), "不得宣称 user.manage");
    assert!(!agg.has("role.manage"), "不得宣称 role.manage");
    assert!(!agg.has("storage.manage"), "不得宣称高风险敏感权限");

    // 普通 member（无任何 elevated 内容）不受影响
    let member_agg = aggregate_permissions(&pool, &member, None)
        .await
        .expect("聚合必须成功");
    assert!(
        member_agg.global_roles.is_empty(),
        "普通 member 无 TOTP 仍是 member（TOTP 可选），角色保持为空"
    );
    assert!(member_agg.has("post.read"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// MFA-05：完成 enrollment 后 administrator 聚合恢复全部注册权限。
#[tokio::test]
async fn admin_after_enrollment_holds_full_permissions() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let admin = insert_user(&pool, "adm").await;
    assign_global_role(&pool, &admin, "administrator").await;
    common::enroll_totp(&pool, &admin).await;

    let agg = aggregate_permissions(&pool, &admin, None)
        .await
        .expect("聚合必须成功");
    assert_eq!(
        agg.permissions.len(),
        PERMISSION_REGISTRY.len(),
        "完成 enrollment 后 administrator 必须聚合全部注册权限"
    );
    assert_eq!(agg.global_roles, vec!["administrator".to_string()]);
    assert!(agg.has("admin.manage"));
    assert!(agg.has("role.manage"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// MFA-06：未完成强制 enrollment 的账号不得执行高风险操作（authorize_action
/// 拒绝），完成 enrollment 后实时恢复（同一账号无需重建任何状态）。
#[tokio::test]
async fn forced_user_operations_gated_until_enrollment() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let admin = insert_user(&pool, "adm").await;
    assign_global_role(&pool, &admin, "administrator").await;

    // 未完成 enrollment → 高风险操作拒绝（fail-closed，默认拒绝语义）
    for permission in [
        "admin.manage",
        "user.manage",
        "role.manage",
        "board.manage",
        "points.adjust",
        "marketplace.refund_admin",
    ] {
        let decision = authorize_action(&pool, &admin, permission, None, AUTHZ_POLICY_VERSION)
            .await
            .expect("authorize_action 必须成功");
        assert_eq!(
            decision,
            bblbb_backend::authz::decision::Decision::Deny {
                reason: DenyReason::MissingPermission
            },
            "未完成强制 enrollment 的 {permission} 必须拒绝"
        );
    }

    // 完成 enrollment → 同一账号立即恢复（判定实时依赖 TOTP 状态）
    common::enroll_totp(&pool, &admin).await;
    for permission in [
        "admin.manage",
        "user.manage",
        "board.manage",
        "points.adjust",
    ] {
        assert!(
            authorize_action(&pool, &admin, permission, None, AUTHZ_POLICY_VERSION)
                .await
                .expect("authorize_action 必须成功")
                .is_allowed(),
            "完成 enrollment 后 {permission} 必须放行"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// MFA-05：板块版主同样强制启用——未完成 enrollment 时在本板块也不得执行
/// 审核操作；完成 enrollment 后板块范围权限恢复。
#[tokio::test]
async fn board_moderator_requires_totp_for_board_authority() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let moderator = insert_user(&pool, "bmod").await;
    let general = board_id_by_slug(&pool, "general").await;
    assign_board_role(&pool, &moderator, &general, "board_moderator").await;

    // 未完成 enrollment → 本板块审核拒绝
    assert_eq!(
        authorize_action(
            &pool,
            &moderator,
            "post.moderate",
            Some(&general),
            AUTHZ_POLICY_VERSION
        )
        .await
        .expect("authorize_action 必须成功"),
        bblbb_backend::authz::decision::Decision::Deny {
            reason: DenyReason::MissingPermission
        },
        "未完成强制 enrollment 的板块版主不得行使本板块审核权限"
    );

    // 完成 enrollment → 本板块审核恢复
    common::enroll_totp(&pool, &moderator).await;
    let agg = aggregate_permissions(&pool, &moderator, Some(&general))
        .await
        .expect("聚合必须成功");
    assert_eq!(
        agg.board_roles,
        vec!["board_moderator".to_string()],
        "完成 enrollment 后板块角色恢复"
    );
    assert!(
        authorize_action(
            &pool,
            &moderator,
            "post.moderate",
            Some(&general),
            AUTHZ_POLICY_VERSION
        )
        .await
        .expect("authorize_action 必须成功")
        .is_allowed(),
        "完成 enrollment 后本板块审核必须放行"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// MFA-06（HTTP）：未完成强制 enrollment 的管理员登录后：会话 / 投影不宣称
/// 高权限（roles=["member"]），管理员端点 403；完成 enrollment 后同一会话
/// 实时恢复——/me roles=["administrator"]、管理员端点 200。
#[tokio::test]
async fn http_admin_gated_until_totp_enrollment() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let app = app_with(pool.clone());
    let (user_id, email) = insert_login_user(&pool, "adm").await;
    assign_global_role(&pool, &user_id, "administrator").await;

    // 未完成 enrollment：登录成功但 Me.roles 不宣称高权限
    let (session, me) = login_session_cookie(&app, &email).await;
    assert_eq!(
        me["roles"],
        json!(["member"]),
        "未完成强制 enrollment 的登录响应不得宣称高权限角色"
    );
    let me_from_get = get_me(&app, &session).await;
    assert_eq!(
        me_from_get["roles"],
        json!(["member"]),
        "GET /me 同样只宣称 member"
    );

    // 管理员端点 → 403（board.manage 不在降级后的权限集）
    let csrf = session_csrf(&app, &session).await;
    let (status, body) = admin_create_board(&app, &session, &csrf).await;
    assert_eq!(
        status,
        StatusCode::FORBIDDEN,
        "未完成 enrollment 必须 403: {body}"
    );

    // 完成 enrollment → 同一会话实时恢复（判定不依赖 Session 重建）
    common::enroll_totp(&pool, &user_id).await;
    let me_after = get_me(&app, &session).await;
    assert_eq!(
        me_after["roles"],
        json!(["administrator"]),
        "完成 enrollment 后 /me 宣称 administrator"
    );
    let (status, body) = admin_create_board(&app, &session, &csrf).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "完成 enrollment 后管理员端点放行: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
