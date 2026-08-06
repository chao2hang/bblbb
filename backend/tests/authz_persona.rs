//! M03-AUTHZ-08：persona × role × board 正负边界测试——
//! 当前板块版主 / 其他板块版主 / 全局版主 / 管理员 / member 在
//! `post.moderate`、`moderation.review`、`admin.manage`、`post.read` 上的
//! 允许/拒绝边界（经 authorize_action + 聚合，板块范围实时生效）。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::decision::{Decision, AUTHZ_POLICY_VERSION};
use bblbb_backend::authz::enforce::authorize_action;
use bblbb_backend::authz::roles::{aggregate_permissions, seed_builtin_roles};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authz-per-{}", uuid::Uuid::now_v7()));
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

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id = role_id_by_name(pool, role_name).await;
    let now = bblbb_backend::outbox::now_millis();
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

async fn assign_board_role(pool: &DatabasePool, user_id: &str, board_id: &str) {
    let role_id = role_id_by_name(pool, "board_moderator").await;
    let now = bblbb_backend::outbox::now_millis();
    match pool {
        Either::Left(p) => {
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

/// 构造 5 个 persona：admin / global_mod / board_mod_general / board_mod_tech / member。
struct Personas {
    admin: String,
    global_mod: String,
    board_mod_general: String,
    board_mod_tech: String,
    member: String,
    general: String,
    tech: String,
}

async fn setup_personas(pool: &DatabasePool) -> Personas {
    seed_builtin_roles(pool).await.unwrap();
    let admin = insert_user(pool, "admin").await;
    let global_mod = insert_user(pool, "gmod").await;
    let board_mod_general = insert_user(pool, "bga").await;
    let board_mod_tech = insert_user(pool, "bte").await;
    let member = insert_user(pool, "mem").await;
    let general = board_id_by_slug(pool, "general").await;
    let tech = board_id_by_slug(pool, "tech").await;

    assign_global_role(pool, &admin, "administrator").await;
    assign_global_role(pool, &global_mod, "global_moderator").await;
    assign_board_role(pool, &board_mod_general, &general).await;
    assign_board_role(pool, &board_mod_tech, &tech).await;
    // M02-MFA-05：administrator/版主属强制启用——未完成 TOTP enrollment 不得
    // 持有高权限（member 保持可选，无需 enrollment）。
    for elevated in [&admin, &global_mod, &board_mod_general, &board_mod_tech] {
        common::enroll_totp(pool, elevated).await;
    }

    Personas {
        admin,
        global_mod,
        board_mod_general,
        board_mod_tech,
        member,
        general,
        tech,
    }
}

async fn allowed(
    pool: &DatabasePool,
    user_id: &str,
    permission: &str,
    board_id: Option<&str>,
) -> bool {
    authorize_action(pool, user_id, permission, board_id, AUTHZ_POLICY_VERSION)
        .await
        .expect("authorize_action 必须成功")
        .is_allowed()
}

/// post.moderate / moderation.review：板块版主仅本板块、全局版主全局、
/// 管理员全量、member 全拒（正负边界）。
#[tokio::test]
async fn moderation_boundary_across_personas() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let p = setup_personas(&pool).await;

    for permission in ["post.moderate", "moderation.review"] {
        // 管理员：本板块/其他板块/无板块全允许
        assert!(allowed(&pool, &p.admin, permission, Some(&p.general)).await);
        assert!(allowed(&pool, &p.admin, permission, Some(&p.tech)).await);
        assert!(allowed(&pool, &p.admin, permission, None).await);

        // 全局版主：任意板块/无板块全允许
        assert!(allowed(&pool, &p.global_mod, permission, Some(&p.general)).await);
        assert!(allowed(&pool, &p.global_mod, permission, Some(&p.tech)).await);
        assert!(allowed(&pool, &p.global_mod, permission, None).await);

        // 当前板块版主：本板块允许，其他板块/无板块拒绝
        assert!(allowed(&pool, &p.board_mod_general, permission, Some(&p.general)).await);
        assert!(!allowed(&pool, &p.board_mod_general, permission, Some(&p.tech)).await);
        assert!(!allowed(&pool, &p.board_mod_general, permission, None).await);

        // 其他板块版主：其板块允许，本板块拒绝
        assert!(!allowed(&pool, &p.board_mod_tech, permission, Some(&p.general)).await);
        assert!(allowed(&pool, &p.board_mod_tech, permission, Some(&p.tech)).await);

        // member：全拒
        assert!(!allowed(&pool, &p.member, permission, Some(&p.general)).await);
        assert!(!allowed(&pool, &p.member, permission, None).await);
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// admin.manage / user.manage / role.manage：仅管理员；其余 persona 全拒。
#[tokio::test]
async fn system_permissions_only_for_administrator() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let p = setup_personas(&pool).await;

    for permission in ["admin.manage", "user.manage", "role.manage"] {
        assert!(allowed(&pool, &p.admin, permission, None).await);
        assert!(!allowed(&pool, &p.global_mod, permission, None).await);
        assert!(!allowed(&pool, &p.board_mod_general, permission, None).await);
        assert!(!allowed(&pool, &p.member, permission, None).await);
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// member 基线（post.read/reaction.create）所有 persona 均允许（读不受角色限制）。
#[tokio::test]
async fn member_baseline_applies_to_all_personas() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let p = setup_personas(&pool).await;

    for permission in ["post.read", "reaction.create"] {
        for (name, id) in [
            ("admin", &p.admin),
            ("global_mod", &p.global_mod),
            ("board_mod_general", &p.board_mod_general),
            ("board_mod_tech", &p.board_mod_tech),
            ("member", &p.member),
        ] {
            assert!(
                allowed(&pool, id, permission, Some(&p.general)).await,
                "{name} 的 {permission} 必须放行"
            );
        }
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 聚合视角：板块版主只在所属板块聚合出板块角色（board_roles 精确）。
#[tokio::test]
async fn board_roles_aggregate_only_for_own_board() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let p = setup_personas(&pool).await;

    let in_board = aggregate_permissions(&pool, &p.board_mod_general, Some(&p.general))
        .await
        .expect("聚合必须成功");
    assert_eq!(in_board.board_roles, vec!["board_moderator".to_string()]);

    let other_board = aggregate_permissions(&pool, &p.board_mod_general, Some(&p.tech))
        .await
        .expect("聚合必须成功");
    assert!(
        other_board.board_roles.is_empty(),
        "其他板块不得出现板块角色"
    );
    assert!(!other_board.has("post.moderate"));

    // 全局版主/管理员无板块角色
    let global = aggregate_permissions(&pool, &p.global_mod, Some(&p.general))
        .await
        .expect("聚合必须成功");
    assert!(global.board_roles.is_empty());
    assert_eq!(global.global_roles, vec!["global_moderator".to_string()]);
    let admin = aggregate_permissions(&pool, &p.admin, Some(&p.tech))
        .await
        .expect("聚合必须成功");
    assert_eq!(admin.global_roles, vec!["administrator".to_string()]);

    // 决策结果一致性：板块版主在非本板块 post.moderate → Deny
    let decision = authorize_action(
        &pool,
        &p.board_mod_general,
        "post.moderate",
        Some(&p.tech),
        AUTHZ_POLICY_VERSION,
    )
    .await
    .expect("authorize_action 必须成功");
    assert!(matches!(decision, Decision::Deny { .. }));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 账号状态叠加：banned 板块版主即使有板块角色也全拒（状态门优先）。
#[tokio::test]
async fn banned_board_moderator_denied_everything() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let p = setup_personas(&pool).await;

    match &pool {
        Either::Left(db) => {
            sqlx::query("UPDATE users SET status = 'banned' WHERE id = ?")
                .bind(&p.board_mod_general)
                .execute(db)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    // 即使在本板块也拒绝（banned 状态门优先于权限）
    assert!(
        !allowed(
            &pool,
            &p.board_mod_general,
            "post.moderate",
            Some(&p.general)
        )
        .await
    );
    assert!(!allowed(&pool, &p.board_mod_general, "post.read", Some(&p.general)).await);

    close_pool(&pool).await;
    cleanup(&dir);
}
