//! M03-AUTHZ-09：对象级授权组合测试——
//! 自己/他人 × 草稿/公开/隐藏/删除 × 锁定板块 × 过期 assignment。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::decision::{
    BoardContext, BoardPostingMode, BoardVisibility, DenyReason, ResourceInfo, ResourceState,
    AUTHZ_POLICY_VERSION,
};
use bblbb_backend::authz::enforce::{authorize_action, require_object_scope};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authz-obj-{}", uuid::Uuid::now_v7()));
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
    let now = bblbb_backend::outbox::now_millis();
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

/// 板块角色 assignment，指定 expires_at。
async fn assign_board_role_at(
    pool: &DatabasePool,
    user_id: &str,
    board_id: &str,
    expires_at: Option<i64>,
) {
    let role_id = role_id_by_name(pool, "board_moderator").await;
    let now = bblbb_backend::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, ?, ?, NULL, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(board_id)
            .bind(user_id)
            .bind(&role_id)
            .bind(now - 60_000)
            .bind(expires_at)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 自己/他人 × post.edit_own：owner 边界（对象级默认拒绝）。
#[tokio::test]
async fn owner_boundary_for_edit_own() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let owner = insert_user(&pool, "own").await;
    let other = insert_user(&pool, "oth").await;

    // 权限侧：owner 有 post.edit_own（member 基线）
    let decision = authorize_action(&pool, &owner, "post.edit_own", None, AUTHZ_POLICY_VERSION)
        .await
        .expect("authorize_action 必须成功");
    assert!(decision.is_allowed(), "owner 必须拥有 post.edit_own 权限");

    // 对象侧：自己的 published 帖子可编辑；他人的拒绝
    let own_resource = ResourceInfo {
        owner_id: &owner,
        state: ResourceState::Published,
    };
    let other_resource = ResourceInfo {
        owner_id: &other,
        state: ResourceState::Published,
    };
    assert!(
        require_object_scope(
            &owner,
            Some(&own_resource),
            None,
            &[ResourceState::Published]
        )
        .is_ok(),
        "本人帖子必须可编辑"
    );
    assert_eq!(
        require_object_scope(
            &owner,
            Some(&other_resource),
            None,
            &[ResourceState::Published]
        ),
        Err(DenyReason::NotResourceOwner),
        "他人帖子必须拒绝"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 草稿/公开/隐藏/删除 × owner：状态边界（allowed_states 精确）。
#[tokio::test]
async fn resource_state_boundary() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "st").await;

    let resource = |state| ResourceInfo {
        owner_id: &user_id,
        state,
    };

    // 编辑已发布内容：draft+published 允许，hidden/deleted 拒绝
    assert!(require_object_scope(
        &user_id,
        Some(&resource(ResourceState::Draft)),
        None,
        &[ResourceState::Draft, ResourceState::Published]
    )
    .is_ok());
    assert_eq!(
        require_object_scope(
            &user_id,
            Some(&resource(ResourceState::Hidden)),
            None,
            &[ResourceState::Draft, ResourceState::Published]
        ),
        Err(DenyReason::ResourceStateNotAllowed),
        "hidden 内容不得通过普通编辑路径"
    );
    assert_eq!(
        require_object_scope(
            &user_id,
            Some(&resource(ResourceState::Deleted)),
            None,
            &[ResourceState::Draft, ResourceState::Published]
        ),
        Err(DenyReason::ResourceStateNotAllowed),
        "deleted 内容不得通过普通编辑路径"
    );

    // 隐藏内容读取：仅 moderation 显式路径（require_hidden_read）可用——
    // 对象级普通路径拒绝 hidden 是必要条件
    assert_eq!(
        require_object_scope(
            &user_id,
            Some(&resource(ResourceState::Hidden)),
            None,
            &[ResourceState::Published]
        ),
        Err(DenyReason::ResourceStateNotAllowed)
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 锁定板块（readonly/closed）× post.create：板块谓词与权限判定组合。
#[tokio::test]
async fn locked_board_blocks_content_write() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "lk").await;

    // 权限侧：member 有 post.create（基线），权限判定放行
    let decision = authorize_action(
        &pool,
        &user_id,
        "post.create",
        Some("b-x"),
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(decision.is_allowed(), "权限侧 post.create 必须放行");

    // 板块侧：readonly/closed 锁定 → 禁止新增（M03-BOARDS-03 服务层强制）
    for mode in [BoardPostingMode::Readonly, BoardPostingMode::Closed] {
        let board = BoardContext {
            board_id: "b-x",
            visibility: BoardVisibility::Public,
            posting_mode: mode,
            deleted: false,
        };
        assert!(
            !board.posting_mode.allows_content_write(),
            "锁定板块 {} 禁止新增帖子",
            mode.as_str()
        );
    }
    for mode in [BoardPostingMode::Normal, BoardPostingMode::Approval] {
        assert!(
            mode.allows_content_write(),
            "{} 允许新增帖子",
            mode.as_str()
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 过期 assignment × 自己/他人 × hidden：板块版主到期后失去 post.moderate，
/// 即使读取自己的隐藏内容也拒绝；重新授予后恢复。
#[tokio::test]
async fn expired_assignment_combined_with_object_rules() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let now = bblbb_backend::outbox::now_millis();
    let moderator = insert_user(&pool, "exp").await;
    let general = board_id_by_slug(&pool, "general").await;
    common::enroll_totp(&pool, &moderator).await; // M02-MFA-05：板块版主必须完成 TOTP 才能恢复高权限

    // 已过期 assignment（过去 1 小时）
    assign_board_role_at(&pool, &moderator, &general, Some(now - 3_600_000)).await;

    // 过期后：本板块 post.moderate 拒绝（即使对象是自己/隐藏内容）
    let decision = authorize_action(
        &pool,
        &moderator,
        "post.moderate",
        Some(&general),
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(
        !decision.is_allowed(),
        "过期 assignment 不得授予 post.moderate"
    );
    let own_hidden = ResourceInfo {
        owner_id: &moderator,
        state: ResourceState::Hidden,
    };
    assert_eq!(
        require_object_scope(
            &moderator,
            Some(&own_hidden),
            None,
            &[ResourceState::Draft, ResourceState::Published]
        ),
        Err(DenyReason::ResourceStateNotAllowed),
        "隐藏内容不得通过普通投影读取"
    );

    // 重新授予（未来到期）→ 恢复本板块 post.moderate（同键 UPDATE，唯一约束）
    let role_id = role_id_by_name(&pool, "board_moderator").await;
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE board_role_assignments
                 SET expires_at = ?, granted_at = ?
                 WHERE board_id = ? AND user_id = ? AND role_id = ?",
            )
            .bind(now + 86_400_000)
            .bind(now - 60_000)
            .bind(&general)
            .bind(&moderator)
            .bind(&role_id)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let decision = authorize_action(
        &pool,
        &moderator,
        "post.moderate",
        Some(&general),
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(decision.is_allowed(), "重新授予后板块版主权限必须恢复");

    close_pool(&pool).await;
    cleanup(&dir);
}
