//! M03-AUTHZ-05：Handler 统一授权调用模式集成测试——
//! require-action（真实 DB 聚合）+ require-object-scope，默认拒绝语义。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::decision::{
    AccountStatus, Decision, DenyReason, ResourceInfo, ResourceState, AUTHZ_POLICY_VERSION,
};
use bblbb_backend::authz::enforce::{
    decide_action, deny_to_error, require_action, require_object_scope,
};
use bblbb_backend::authz::roles::{aggregate_permissions, seed_builtin_roles};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authz-enf-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("enf_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let email = format!("enf_{}@example.com", uuid::Uuid::now_v7().simple());
    let now = bblbb_backend::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy-hash', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(&email)
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

async fn set_status(pool: &DatabasePool, user_id: &str, status: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET status = ? WHERE id = ?")
                .bind(status)
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// Handler 模式：require-action（member 基线放行）+ require-object-scope（owner+state）。
#[tokio::test]
async fn handler_pattern_action_then_object_scope() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool).await;

    // require-action：member 基线的 post.create 放行
    let decision = require_action(
        &pool,
        &user_id,
        AccountStatus::Active,
        "post.create",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("require_action 必须成功");
    assert!(decision.is_allowed(), "member 创建帖子必须放行");

    // require-object-scope：本人资源（Published）允许
    let own = ResourceInfo {
        owner_id: &user_id,
        state: ResourceState::Published,
    };
    assert!(require_object_scope(&user_id, Some(&own), None, &[ResourceState::Published]).is_ok());

    // 他人资源 → NotResourceOwner（默认拒绝）
    let other = ResourceInfo {
        owner_id: "someone-else",
        state: ResourceState::Published,
    };
    assert_eq!(
        require_object_scope(&user_id, Some(&other), None, &[ResourceState::Published]),
        Err(DenyReason::NotResourceOwner)
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// require-action 默认拒绝：缺少权限 / 账号状态 / 策略版本。
#[tokio::test]
async fn require_action_default_denies() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool).await;

    // member 无 post.moderate → MissingPermission
    let decision = require_action(
        &pool,
        &user_id,
        AccountStatus::Active,
        "post.moderate",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("require_action 必须成功");
    assert!(!decision.is_allowed());

    // 账号状态门槛：banned → AccountNotAllowed（即使权限存在）
    set_status(&pool, &user_id, "banned").await;
    let decision = require_action(
        &pool,
        &user_id,
        AccountStatus::Banned,
        "post.read",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("require_action 必须成功");
    assert!(!decision.is_allowed(), "banned 账号必须拒绝动作");

    // 策略版本过期 → PolicyVersionMismatch
    let roles = aggregate_permissions(&pool, &user_id, None).await.unwrap();
    let decision = decide_action(&roles, "post.read", AccountStatus::Banned, "0.9.0");
    assert!(!decision.is_allowed());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// deny_to_error 映射：未认证 401、其余 403。
#[tokio::test]
async fn deny_reason_maps_to_http() {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let unauthorized = deny_to_error(DenyReason::NotAuthenticated, "req-x").into_response();
    assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
    let forbidden = deny_to_error(DenyReason::DefaultDeny, "req-y").into_response();
    assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
}

/// Decision 的 deny reason 提取（审计用）。
#[test]
fn decision_reason_extraction() {
    let allow = Decision::Allow;
    assert!(bblbb_backend::authz::enforce::denied_reason(&allow).is_none());
    let deny = Decision::Deny {
        reason: DenyReason::MissingPermission,
    };
    assert_eq!(
        bblbb_backend::authz::enforce::denied_reason(&deny),
        Some(DenyReason::MissingPermission)
    );
}
