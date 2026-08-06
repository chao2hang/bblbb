//! M03-AUTHZ-10：Feature Flag / 前端字段 / 请求体不能授予任何额外权限。
//!
//! 授权判定的唯一输入是**服务端**数据（AUTHZ-02/03/06）：
//! - 聚合角色/权限只来自 DB（member 基线 + `user_roles` + `board_role_assignments`，
//!   含生效窗口），`aggregate_permissions` 没有请求体/客户端输入参数；
//! - 账号状态门只来自 `users` 行（`load_account_gates`）；
//! - Feature Flag（M01-CONFIG-06）只控制**可选能力路由**的可用性：关闭时
//!   feature_gate 中间件返回 409 `feature_disabled`（拒绝），启用时请求照常
//!   走授权层；**核心论坛路由不在 `feature_for_path` 映射内**，Flag 系统也
//!   没有任何“授予权限”的入口（只有 `set` / `emergency_off`）。
//!
//! 因此：即使攻击者把所有 Flag 打开、在请求体里伪造
//! `{roles:[administrator], permissions:[admin.manage], ...}`，授权判定分毫
//! 不变（member 仍被拒，管理员仍放行）。Flag 只能让能力路由更严格（409），
//! 绝无“跳过授权”或“授予权限”的通道。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::decision::{Decision, AUTHZ_POLICY_VERSION};
use bblbb_backend::authz::enforce::{authorize_action, authorize_with, load_account_gates};
use bblbb_backend::authz::roles::{aggregate_permissions, seed_builtin_roles};
use bblbb_backend::authz::PERMISSION_REGISTRY;
use bblbb_backend::config::flags::{feature_for_path, FeatureFlags, FeatureName};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

mod common;

// ────────────────────────── 测试基础设施 ───────────────────────────────────

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authz-nocl-{}", uuid::Uuid::now_v7()));
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

/// 插入一条 user_roles（生效窗口由服务端判定，AUTHZ-03）。
async fn insert_user_role(
    pool: &DatabasePool,
    user_id: &str,
    role_id: &str,
    granted_at: i64,
    expires_at: Option<i64>,
) {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, ?)",
            )
            .bind(user_id)
            .bind(role_id)
            .bind(granted_at)
            .bind(expires_at)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 改写生效窗口（user_roles 主键 = (user_id, role_id)，单行）。
async fn update_user_role(
    pool: &DatabasePool,
    user_id: &str,
    role_id: &str,
    granted_at: i64,
    expires_at: Option<i64>,
) {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE user_roles SET granted_at = ?, expires_at = ? WHERE user_id = ? AND role_id = ?",
            )
            .bind(granted_at)
            .bind(expires_at)
            .bind(user_id)
            .bind(role_id)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 构造 persona：admin（administrator）+ member（无任何授权），board=general。
struct Personas {
    admin: String,
    member: String,
    general: String,
}

async fn setup(pool: &DatabasePool) -> Personas {
    seed_builtin_roles(pool).await.unwrap();
    let admin = insert_user(pool, "admin").await;
    let member = insert_user(pool, "mem").await;
    let general = board_id_by_slug(pool, "general").await;
    assign_global_role(pool, &admin, "administrator").await;
    common::enroll_totp(pool, &admin).await; // M02-MFA-05：管理员必须完成 TOTP 才能持有高权限
    Personas {
        admin,
        member,
        general,
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

// ────────────────────────── M03-AUTHZ-10 测试 ──────────────────────────────

/// Flag 开启/紧急关闭都不得改变授权判定；Flag 全开也不会给 member 加任何权限。
#[tokio::test]
async fn feature_flags_never_change_authz_decisions() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let p = setup(&pool).await;

    let snapshot = async |who: &str| -> Vec<Decision> {
        let mut out = Vec::new();
        for perm in [
            "post.read",
            "post.moderate",
            "moderation.review",
            "admin.manage",
            "user.manage",
            "role.manage",
        ] {
            out.push(
                authorize_action(&pool, who, perm, None, AUTHZ_POLICY_VERSION)
                    .await
                    .expect("authorize_action 必须成功"),
            );
        }
        out
    };

    let before = snapshot(&p.member).await;

    // 全部 Flag 打开（立即生效）→ 判定不变
    let now = bblbb_backend::outbox::now_millis();
    let mut flags_on = FeatureFlags::all_default();
    for name in FeatureName::ALL {
        flags_on
            .set(name, true, 1, 0, "admin", "test: all on", now)
            .unwrap();
        assert!(flags_on.is_enabled(name, now), "{name} 必须已启用");
    }
    assert_eq!(
        snapshot(&p.member).await,
        before,
        "开启全部 Flag 不得改变授权判定"
    );

    // 紧急关闭 → 判定仍不变（Flag 只拦路由，不参与权限）
    let mut flags_off = FeatureFlags::all_default();
    for name in FeatureName::ALL {
        flags_off
            .set(name, true, 1, 0, "admin", "test", now)
            .unwrap();
    }
    flags_off.emergency_off("oncall", "test incident", now);
    for name in FeatureName::ALL {
        assert!(!flags_off.is_enabled(name, now), "紧急关闭必须覆盖 {name}");
    }
    assert_eq!(
        snapshot(&p.member).await,
        before,
        "紧急关闭也不得改变授权判定"
    );

    // 核心断言：Flag 全开时 member 仍拿不到任何管理权限
    for perm in [
        "post.moderate",
        "moderation.review",
        "admin.manage",
        "user.manage",
        "role.manage",
    ] {
        assert!(
            !allowed(&pool, &p.member, perm, None).await,
            "Flag 全开时 member 的 {perm} 仍必须拒绝"
        );
    }
    // 管理员不受 Flag 影响，仍全量放行
    assert!(allowed(&pool, &p.admin, "post.moderate", None).await);
    assert!(allowed(&pool, &p.admin, "admin.manage", None).await);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 请求体可携带任意角色/权限/策略版本声明，但授权子系统没有任何消费它的入口。
const FORGED_BODY: &str = r#"{
  "roles": ["administrator", "global_moderator"],
  "permissions": ["admin.manage", "user.manage", "role.manage", "post.moderate", "moderation.review"],
  "policy_version": "1.0.0",
  "board_id": "general"
}"#;

/// 模拟一个恶意 handler 的请求体结构（字段可被 serde 解析——攻击者确实能发出）。
#[derive(serde::Deserialize, Debug)]
struct ForgedBody {
    roles: Vec<String>,
    permissions: Vec<String>,
    policy_version: String,
    board_id: Option<String>,
}

#[tokio::test]
async fn adversarial_request_body_roles_and_permissions_are_ignored() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let p = setup(&pool).await;

    // 恶意请求体可正常解析（说明这些字段确实会被“接收”），但授权路径不读它
    let forged: ForgedBody = serde_json::from_str(FORGED_BODY).expect("恶意请求体必须可解析");
    assert!(forged.roles.iter().any(|r| r == "administrator"));
    assert!(forged.permissions.iter().any(|pr| pr == "admin.manage"));
    assert_eq!(forged.policy_version, AUTHZ_POLICY_VERSION);
    assert_eq!(forged.board_id.as_deref(), Some("general"));

    // 带板块（攻击者声明自己是“general”板块版主）：仍全拒
    for perm in [
        "admin.manage",
        "user.manage",
        "role.manage",
        "post.moderate",
    ] {
        let decision = authorize_action(
            &pool,
            &p.member,
            perm,
            Some(&p.general),
            AUTHZ_POLICY_VERSION,
        )
        .await
        .expect("authorize_action 必须成功");
        assert!(
            matches!(decision, Decision::Deny { .. }),
            "member 的 {perm} 必须拒绝"
        );
    }
    // 无板块：同样全拒
    assert!(!allowed(&pool, &p.member, "admin.manage", None).await);

    // 结构性证明：显式组装 AuthzInput（仅服务端聚合 + 状态门）与 handler 入口
    // 判定一致——除服务端输入外没有任何可影响结果的输入源。
    let roles = aggregate_permissions(&pool, &p.member, Some(&p.general))
        .await
        .expect("聚合必须成功");
    let gates = load_account_gates(&pool, &p.member)
        .await
        .expect("状态门必须成功");
    for perm in ["admin.manage", "post.moderate", "post.read"] {
        let explicit = authorize_with(&roles, &gates, perm, Some(&p.general), AUTHZ_POLICY_VERSION);
        let via_handler = authorize_action(
            &pool,
            &p.member,
            perm,
            Some(&p.general),
            AUTHZ_POLICY_VERSION,
        )
        .await
        .expect("authorize_action 必须成功");
        assert_eq!(explicit, via_handler, "{perm} 的服务端输入判定必须一致");
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 生效窗口由服务端时间判定（AUTHZ-03/09）：过期/未来的管理员授权都无效，
/// 客户端无法伪造“now”或把 DB 行推入生效窗口。
#[tokio::test]
async fn assignment_window_is_server_governed() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let p = setup(&pool).await;
    let admin_role = role_id_by_name(&pool, "administrator").await;
    let now = bblbb_backend::outbox::now_millis();

    // 过期授权（expires_at 在过去）→ 不生效
    insert_user_role(
        &pool,
        &p.member,
        &admin_role,
        now - 100 * 86_400_000,
        Some(now - 1),
    )
    .await;
    assert!(!allowed(&pool, &p.member, "admin.manage", None).await);
    assert!(!allowed(&pool, &p.member, "user.manage", None).await);

    // 未来授权（granted_at 在未来，即使永久）→ 不生效
    update_user_role(&pool, &p.member, &admin_role, now + 100 * 86_400_000, None).await;
    assert!(!allowed(&pool, &p.member, "admin.manage", None).await);

    // 永久生效授权（granted_at 在过去 + expires_at NULL）→ 生效
    // （M02-MFA-05：member 先完成 TOTP，持有 administrator 后才能获得高权限）
    common::enroll_totp(&pool, &p.member).await;
    update_user_role(&pool, &p.member, &admin_role, now - 1, None).await;
    assert!(allowed(&pool, &p.member, "admin.manage", None).await);
    assert!(allowed(&pool, &p.member, "role.manage", None).await);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 核心论坛路由不在 Flag 映射内（feature_gate 直接放行到授权层）；Flag 只可能
/// 把可选能力路由拦成 409 `feature_disabled`（拒绝），绝无“放行跳过授权”的通道。
#[tokio::test]
async fn core_routes_are_not_feature_gated() {
    for path in [
        "/api/v1/posts",
        "/api/v1/posts/x/comments",
        "/api/v1/boards",
        "/api/v1/boards/x",
        "/api/v1/users/x",
        "/api/v1/me",
        "/api/v1/reactions",
        "/api/v1/attachments", // 上传不受 Download Billing Flag 控制
        "/api/v1/conversations",
        "/api/v1/notifications",
        "/api/v1/moderations",
        "/api/v1/roles",
        "/healthz",
        "/api/v1/openapi.json",
    ] {
        assert_eq!(
            feature_for_path(path),
            None,
            "{path} 不得被 Flag 门控（核心路由只走授权层）"
        );
    }
    // 可选能力前缀确实命中：关闭即 409，启用后才继续走授权层
    assert_eq!(
        feature_for_path("/api/v1/ai/capabilities"),
        Some(FeatureName::Ai)
    );
    assert_eq!(
        feature_for_path("/api/v1/video-embeds/resolve"),
        Some(FeatureName::Video)
    );
    assert_eq!(
        feature_for_path("/api/v1/attachments/x/download"),
        Some(FeatureName::DownloadBilling)
    );
}

/// 结构性不变量：能力名 ≠ 权限名；Flag 系统没有“授予权限”的入口。
#[tokio::test]
async fn flag_names_are_not_permissions() {
    let registry_names: std::collections::BTreeSet<&str> =
        PERMISSION_REGISTRY.iter().map(|p| p.name).collect();
    let flag_names: std::collections::BTreeSet<&str> =
        FeatureName::ALL.iter().map(|n| n.as_str()).collect();
    // 注册表与能力名完全不相交（能力开关不可能被当成权限授予）
    assert!(
        registry_names.is_disjoint(&flag_names),
        "Flag 能力名不得出现在权限注册表中"
    );
    for name in FeatureName::ALL {
        assert!(
            !bblbb_backend::authz::is_registered(name.as_str()),
            "能力名 {} 不得是已注册权限",
            name
        );
    }
}
