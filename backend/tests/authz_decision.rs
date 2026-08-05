//! M03-AUTHZ-04：动作授权输入集成测试——
//! 从真实 DB（种子角色 + 聚合）组装完整 AuthzInput：actor/账号状态/角色/
//! board/resource owner+state/policy version。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::decision::{
    AccountStatus, ActorContext, AuthzInput, BoardContext, BoardPostingMode, BoardVisibility,
    Decision, DenyReason, ResourceInfo, ResourceState, AUTHZ_POLICY_VERSION,
};
use bblbb_backend::authz::roles::{aggregate_permissions, seed_builtin_roles};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authz-dec-{}", uuid::Uuid::now_v7()));
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
    let username = format!("dec_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let email = format!("dec_{}@example.com", uuid::Uuid::now_v7().simple());
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

/// 完整授权输入可组装：真实聚合 + 板块 + 资源 + 策略版本。
#[tokio::test]
async fn full_authz_input_assembles_from_real_db() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool).await;

    let roles = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(roles.has("post.read"), "member 基线含 post.read");

    let actor = ActorContext {
        user_id: &user_id,
        status: AccountStatus::Active,
        roles: &roles,
    };
    let board = BoardContext {
        board_id: "b-1",
        visibility: BoardVisibility::Public,
        posting_mode: BoardPostingMode::Normal,
        deleted: false,
    };
    // 资源所有者 = actor：owner 判定通过
    let resource = ResourceInfo {
        owner_id: &user_id,
        state: ResourceState::Published,
    };

    let input = AuthzInput::new(
        actor,
        "post.edit_own",
        Some(board),
        Some(resource),
        AUTHZ_POLICY_VERSION,
    )
    .expect("已注册权限必须可构造输入");
    assert_eq!(input.permission, "post.edit_own");
    assert_eq!(input.actor.status, AccountStatus::Active);
    assert_eq!(input.policy_version, AUTHZ_POLICY_VERSION);
    assert_eq!(input.board.unwrap().visibility, BoardVisibility::Public);
    let res = input.resource.unwrap();
    assert_eq!(res.state, ResourceState::Published);
    assert_eq!(res.owner_id, user_id);

    // owner 判定与默认拒绝语义
    assert!(bblbb_backend::authz::decision::is_resource_owner(
        input.actor.user_id,
        res.owner_id
    ));
    let deny = Decision::Deny {
        reason: DenyReason::BoardScopeMismatch,
    };
    assert!(!deny.is_allowed());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未注册权限名不得构造输入（默认拒绝的事实来源）。
#[tokio::test]
async fn unregistered_permission_cannot_build_input() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool).await;

    let roles = aggregate_permissions(&pool, &user_id, None).await.unwrap();
    let actor = ActorContext {
        user_id: &user_id,
        status: AccountStatus::Active,
        roles: &roles,
    };
    assert!(
        AuthzInput::new(actor, "ban.hammer", None, None, AUTHZ_POLICY_VERSION).is_none(),
        "未注册权限必须无法构造授权输入"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
