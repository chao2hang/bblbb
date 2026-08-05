//! M03-AUTHZ-02：内置角色种子与角色聚合集成测试——
//! administrator / global moderator / board moderator / member / 自定义角色，
//! 全局与板块作用域、过期 assignment 实时排除、幂等种子。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::roles::{aggregate_permissions, seed_builtin_roles, BUILTIN_ROLES};
use bblbb_backend::authz::{verify_db_permissions, PERMISSION_REGISTRY};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authz-{}", uuid::Uuid::now_v7()));
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

async fn permission_id_by_name(pool: &DatabasePool, name: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM permissions WHERE name = ?")
            .bind(name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 全局角色 assignment（user_roles）。
async fn assign_global_role(
    pool: &DatabasePool,
    user_id: &str,
    role_name: &str,
    expires_at: Option<i64>,
) {
    let role_id = role_id_by_name(pool, role_name).await;
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, ?)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now)
            .bind(expires_at)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 板块角色 assignment（board_role_assignments）。
async fn assign_board_role(
    pool: &DatabasePool,
    user_id: &str,
    board_id: &str,
    role_name: &str,
    expires_at: Option<i64>,
) {
    let role_id = role_id_by_name(pool, role_name).await;
    let now = now_millis();
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
            .bind(now)
            .bind(expires_at)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 创建自定义角色并授予权限（全局）。
async fn create_custom_role(pool: &DatabasePool, name: &str, permissions: &[&str]) -> String {
    let role_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO roles (id, name, display_name, description, is_system, created_at, updated_at)
                 VALUES (?, ?, ?, '自定义角色', 0, ?, ?)",
            )
            .bind(&role_id)
            .bind(name)
            .bind(name)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            for perm in permissions {
                let permission_id = permission_id_by_name(pool, perm).await;
                sqlx::query("INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)")
                    .bind(&role_id)
                    .bind(&permission_id)
                    .execute(p)
                    .await
                    .unwrap();
            }
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    role_id
}

/// 全局角色 assignment（user_roles），指定 granted_at 与 expires_at。
async fn assign_global_role_at(
    pool: &DatabasePool,
    user_id: &str,
    role_name: &str,
    granted_at: i64,
    expires_at: Option<i64>,
) {
    let role_id = role_id_by_name(pool, role_name).await;
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, ?)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(granted_at)
            .bind(expires_at)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 种子幂等 + 完整性：两次调用后权限 68 项全注册、4 个内置角色、映射就绪。
#[tokio::test]
async fn seed_is_idempotent_and_complete() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    seed_builtin_roles(&pool).await.expect("首次种子必须成功");
    seed_builtin_roles(&pool)
        .await
        .expect("重复种子必须幂等成功");

    let check = verify_db_permissions(&pool)
        .await
        .expect("种子后权限必须全部注册");
    assert_eq!(check.known_in_db, PERMISSION_REGISTRY.len());
    assert!(check.missing_from_db.is_empty(), "种子后不得缺权限");

    let roles: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM roles")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(roles, BUILTIN_ROLES.len() as i64, "必须恰好 4 个内置角色");

    let member_perms: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM role_permissions rp
             JOIN roles r ON r.id = rp.role_id WHERE r.name = 'member'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(member_perms > 20, "member 基线必须有权限映射");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// member 基线：无任何 assignment 也具备基线权限；不含审核/管理权限。
#[tokio::test]
async fn member_baseline_applies_without_assignment() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "mem").await;

    let agg = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(agg.has("post.read"), "member 基线含 post.read");
    assert!(agg.has("reaction.create"), "member 基线含 reaction.create");
    assert!(agg.has("user.edit_own"), "member 基线含 user.edit_own");
    assert!(!agg.has("post.moderate"), "member 基线不得含 post.moderate");
    assert!(!agg.has("admin.manage"), "member 基线不得含 admin.manage");
    assert!(!agg.has("role.manage"), "member 基线不得含 role.manage");
    assert!(
        agg.global_roles.is_empty(),
        "无 assignment 时不得有全局角色"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// administrator：全局 assignment 后聚合到注册表全部 68 项。
#[tokio::test]
async fn administrator_has_all_registry_permissions() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "adm").await;

    assign_global_role(&pool, &user_id, "administrator", None).await;

    let agg = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert_eq!(
        agg.permissions.len(),
        PERMISSION_REGISTRY.len(),
        "administrator 必须聚合到全部注册权限"
    );
    assert!(agg.has("admin.manage"));
    assert!(agg.has("role.manage"));
    assert!(agg.has("user.manage"));
    assert!(agg.has("storage.manage"));
    assert_eq!(agg.global_roles, vec!["administrator".to_string()]);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// board moderator：仅在所属板块聚合到审核权限；其他板块/全局作用域不含。
#[tokio::test]
async fn board_moderator_is_scoped_to_board() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "bmod").await;
    let general = board_id_by_slug(&pool, "general").await;
    let tech = board_id_by_slug(&pool, "tech").await;

    assign_board_role(&pool, &user_id, &general, "board_moderator", None).await;

    let in_board = aggregate_permissions(&pool, &user_id, Some(&general))
        .await
        .expect("聚合必须成功");
    assert!(
        in_board.has("post.moderate"),
        "所属板块必须聚合 post.moderate"
    );
    assert!(in_board.has("moderation.review"));
    assert_eq!(in_board.board_roles, vec!["board_moderator".to_string()]);

    let other_board = aggregate_permissions(&pool, &user_id, Some(&tech))
        .await
        .expect("聚合必须成功");
    assert!(
        !other_board.has("post.moderate"),
        "其他板块不得聚合板块版主权限"
    );
    assert!(other_board.board_roles.is_empty());

    let global = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(
        !global.has("post.moderate"),
        "全局作用域不得聚合板块版主权限"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// global moderator：全局 assignment 后在任意板块作用域生效。
#[tokio::test]
async fn global_moderator_applies_across_boards() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "gmod").await;
    let general = board_id_by_slug(&pool, "general").await;

    assign_global_role(&pool, &user_id, "global_moderator", None).await;

    let global = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(
        global.has("post.moderate"),
        "全局版主全局作用域含 post.moderate"
    );
    assert_eq!(global.global_roles, vec!["global_moderator".to_string()]);

    let in_board = aggregate_permissions(&pool, &user_id, Some(&general))
        .await
        .expect("聚合必须成功");
    assert!(in_board.has("post.moderate"), "全局版主在任意板块也生效");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 自定义角色：与内置角色同路径聚合。
#[tokio::test]
async fn custom_role_aggregates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "cust").await;

    create_custom_role(&pool, "shop_operator", &["shop.manage", "shop.refund"]).await;
    assign_global_role(&pool, &user_id, "shop_operator", None).await;

    let agg = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(agg.has("shop.manage"), "自定义角色权限必须聚合");
    assert!(agg.has("shop.refund"));
    assert!(agg.has("post.read"), "自定义角色之上仍保留 member 基线");
    assert_eq!(agg.global_roles, vec!["shop_operator".to_string()]);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 过期 assignment 实时排除；未过期保留（M03-AUTHZ-03 形式化完整语义）。
#[tokio::test]
async fn expired_assignment_is_excluded() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "exp").await;

    let now = now_millis();
    // 已过期（过去 1 小时）
    assign_global_role(&pool, &user_id, "global_moderator", Some(now - 3_600_000)).await;
    let agg = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(!agg.has("post.moderate"), "过期 assignment 不得聚合");
    assert!(agg.global_roles.is_empty());

    // 未过期（未来 1 小时）
    assign_global_role(&pool, &user_id, "board_moderator", Some(now + 3_600_000)).await;
    // 通过 user_roles 赋予板块角色（语义上允许），断言实时生效
    let agg = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(agg.has("post.moderate"), "未过期 assignment 必须聚合");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 生效/到期实时判断（M03-AUTHZ-03）：未来授权不生效、行保留供审计恢复。
#[tokio::test]
async fn future_grant_is_not_effective_and_rows_are_retained() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let user_id = insert_user(&pool, "fut").await;

    let now = now_millis();
    // 未来授权（granted_at 在 1 小时后）：即使 expires_at 为空也不生效
    assign_global_role_at(&pool, &user_id, "global_moderator", now + 3_600_000, None).await;
    let agg = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(!agg.has("post.moderate"), "未来授权不得生效");
    assert!(agg.global_roles.is_empty());

    // 已到期（expires_at 在 1 小时前）：不生效
    assign_global_role_at(
        &pool,
        &user_id,
        "board_moderator",
        now - 3_600_000,
        Some(now - 3_600_000),
    )
    .await;
    let agg = aggregate_permissions(&pool, &user_id, None)
        .await
        .expect("聚合必须成功");
    assert!(!agg.has("post.moderate"), "已到期 assignment 不得生效");

    // 行保留：未来/过期行仍在表中（供审计与恢复，不删除）
    let rows: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM user_roles WHERE user_id = ?")
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(rows, 2, "未来/过期 assignment 行必须保留");

    close_pool(&pool).await;
    cleanup(&dir);
}
