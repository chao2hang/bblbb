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

async fn set_email_verified_at(pool: &DatabasePool, user_id: &str, verified_at: Option<i64>) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET email_verified_at = ? WHERE id = ?")
                .bind(verified_at)
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

/// AUTHZ-06：authorize_action 组合（状态门 + 动作门）真实 DB 判定。
#[tokio::test]
async fn authorize_action_respects_account_status() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool).await;
    let now = bblbb_backend::outbox::now_millis();

    // 未验证（email_verified_at=NULL）：内容写入拒，读取放
    let decision = bblbb_backend::authz::enforce::authorize_action(
        &pool,
        &user_id,
        "post.create",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(!decision.is_allowed(), "未验证不得创建内容");
    let decision = bblbb_backend::authz::enforce::authorize_action(
        &pool,
        &user_id,
        "post.read",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(decision.is_allowed(), "未验证可读公开内容");

    // 验证但冷静期未过（verified_at = now → cooldown = now+24h）：内容写入拒
    set_email_verified_at(&pool, &user_id, Some(now)).await;
    let decision = bblbb_backend::authz::enforce::authorize_action(
        &pool,
        &user_id,
        "post.create",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(!decision.is_allowed(), "冷静期内不得创建内容");

    // 冷静期过后（verified_at = now-2 天）：内容写入放
    set_email_verified_at(&pool, &user_id, Some(now - 2 * 86_400_000)).await;
    let decision = bblbb_backend::authz::enforce::authorize_action(
        &pool,
        &user_id,
        "post.create",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(decision.is_allowed(), "冷静期过后 member 可创建内容");

    // banned：一律拒（读也拒）
    set_status(&pool, &user_id, "banned").await;
    let decision = bblbb_backend::authz::enforce::authorize_action(
        &pool,
        &user_id,
        "post.read",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(!decision.is_allowed(), "banned 必须拒绝一切动作");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// AUTHZ-06：load_account_gates 从 DB 读取 status/email_verified/冷静期。
#[tokio::test]
async fn load_account_gates_reads_user_row() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user_id = insert_user(&pool).await;
    let now = bblbb_backend::outbox::now_millis();

    let gates = bblbb_backend::authz::enforce::load_account_gates(&pool, &user_id)
        .await
        .expect("加载状态门必须成功");
    assert_eq!(gates.status, AccountStatus::Active);
    assert!(!gates.email_verified);
    assert_eq!(gates.cooldown_until, None);

    set_email_verified_at(&pool, &user_id, Some(now - 60_000)).await;
    let gates = bblbb_backend::authz::enforce::load_account_gates(&pool, &user_id)
        .await
        .expect("加载状态门必须成功");
    assert!(gates.email_verified);
    assert_eq!(
        gates.cooldown_until,
        Some(now - 60_000 + bblbb_backend::authz::enforce::ACCOUNT_COOLDOWN_MS)
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
