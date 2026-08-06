//! M03-AUTHZ-02/03：内置角色定义、幂等种子与角色聚合。
//!
//! - [`BUILTIN_ROLES`]：administrator / global_moderator / board_moderator /
//!   member 四个内置角色定义（`is_system=1`，不可删除/改名，SCHEMA-06），
//!   以及自定义角色共享的聚合路径（自定义角色与内置角色一样通过
//!   `roles` + `role_permissions` 表参与聚合）；
//! - [`seed_builtin_roles`]：幂等种子（INSERT OR IGNORE / INSERT IGNORE），
//!   写入全部 68 项权限（来自 [`super::PERMISSION_REGISTRY`]，单一事实来源）
//!   + 内置角色 + `role_permissions` 映射；服务启动与测试均可调用；
//! - [`aggregate_permissions`]：给定用户（可选板块）聚合有效权限 =
//!   **member 基线**（已登录用户默认角色，无 assignment 也生效）∪
//!   全局角色（`user_roles`）∪ 板块角色（`board_role_assignments`，板块范围）；
//! - **生效/到期实时判断（M03-AUTHZ-03）**：[`assignment_effective_at`]——
//!   assignment 生效当且仅当 `granted_at <= now` 且 `expires_at` 为空
//!   （永久）或 `expires_at > now`；未来授权与已到期均视为未生效，但
//!   **过期/未来行保留**供审计与恢复（不删除，STATE-MACHINES §Authorization）。

use std::collections::BTreeSet;

use sqlx::Either;

use crate::db::DatabasePool;
use crate::outbox::now_millis;

use super::{Permission, RiskLevel, PERMISSION_REGISTRY};

/// 角色作用域：全局（`user_roles`）或板块（`board_role_assignments`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleScope {
    Global,
    Board,
}

/// 角色权限集合：`All` = 注册表全部权限（administrator）；`List` = 显式清单。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RolePermissions {
    All,
    List(&'static [&'static str]),
}

/// 内置角色定义（种子与文档的事实来源）。
#[derive(Debug, Clone, Copy)]
pub struct BuiltinRole {
    pub name: &'static str,
    pub display_name: &'static str,
    pub description: &'static str,
    pub is_system: bool,
    pub scope: RoleScope,
    pub permissions: RolePermissions,
}

/// 内置角色（v1）。`member` 是已登录用户的默认基线角色；aggregation 中
/// 无需 assignment 即生效。`board_moderator` 通过 `board_role_assignments`
/// 按板块生效；`global_moderator` / `administrator` 通过 `user_roles` 全局生效。
pub static BUILTIN_ROLES: &[BuiltinRole] = &[
    BuiltinRole {
        name: "member",
        display_name: "成员",
        description: "已登录用户的默认基线角色（无 assignment 也生效）",
        is_system: true,
        scope: RoleScope::Global,
        permissions: RolePermissions::List(&[
            "board.read",
            "post.read",
            "post.read_own",
            "post.read_revision",
            "post.create",
            "post.edit_own",
            "comment.read",
            "comment.create",
            "comment.edit_own",
            "attachment.read",
            "attachment.upload",
            "reaction.create",
            "session.revoke_own",
            "session.read_own",
            "user.read_public",
            "user.read_own",
            "user.edit_own",
            "mfa.enroll",
            "mfa.recovery_codes",
            "mfa.disable",
            "mfa.reauth",
            "appeal.create_own",
            "appeal.read_own",
            "download.read",
            "download.create",
            "download.read_own",
            "marketplace_offer.manage_own",
            "marketplace_purchase.create",
            "marketplace_purchase.confirm_own",
            "marketplace_purchase.read_own",
            "marketplace_refund.create_own",
            "marketplace_secret.rotate",
            "marketplace_webhook.replay",
            "ai.format",
            "ai.seo",
            "ai.consent_own",
            "shop.read",
            "shop.purchase",
            "shop.entitlement.manage_own",
            "activity.claim_own",
            "video.embed",
            "oauth.token",
            "oauth.revoke",
            "oauth.interaction",
            "openid",
            "openid.logout",
        ]),
    },
    BuiltinRole {
        name: "board_moderator",
        display_name: "板块版主",
        description: "板块范围内容审核（board_role_assignments 生效）",
        is_system: true,
        scope: RoleScope::Board,
        permissions: RolePermissions::List(&[
            "post.moderate",
            "moderation.review",
            "moderation.sanction",
        ]),
    },
    BuiltinRole {
        name: "global_moderator",
        display_name: "全局版主",
        description: "全局内容审核（user_roles 生效）",
        is_system: true,
        scope: RoleScope::Global,
        permissions: RolePermissions::List(&[
            "post.moderate",
            "moderation.review",
            "moderation.sanction",
        ]),
    },
    BuiltinRole {
        name: "administrator",
        display_name: "管理员",
        description: "全部权限（注册表 68 项，system 级）",
        is_system: true,
        scope: RoleScope::Global,
        permissions: RolePermissions::All,
    },
];

/// 聚合结果（权限名集合 + 生效角色名）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleAggregation {
    /// 有效权限名（已排序去重）：member 基线 ∪ 全局角色 ∪ 板块角色。
    pub permissions: BTreeSet<String>,
    /// 生效的全局角色名（已排序去重）。
    pub global_roles: Vec<String>,
    /// 生效的板块角色名（已排序去重）。
    pub board_roles: Vec<String>,
}

impl RoleAggregation {
    pub fn has(&self, permission: &str) -> bool {
        self.permissions.contains(permission)
    }
}

/// 注册表风险等级查询（权限名 → risk_level；未知权限名返回 `None`）。
pub fn permission_risk_level(name: &str) -> Option<RiskLevel> {
    PERMISSION_REGISTRY
        .iter()
        .find(|p| p.name == name)
        .map(|p| p.risk_level)
}

/// M02-MFA-05：强制启用判定——聚合含 **elevated** 内容即必须完成 TOTP
/// enrollment：
/// - 任意全局角色 ≠ `member`（administrator/global_moderator/自定义角色）；
/// - 任意板块角色（board_moderator）；
/// - 任何 `sensitive`/`system` 风险权限（高风险账务账号，如
///   `user.manage`/`role.manage`/`admin.manage`/`points.adjust`/
///   `marketplace.refund_admin`/`download_billing.manage`/`storage.manage`）。
///
/// 纯 member 基线（全部 `normal` 权限，无额外角色）为**可选** TOTP——
/// member 基线权限已验证均为 normal（PERMISSION_REGISTRY）。
pub fn aggregation_requires_totp(agg: &RoleAggregation) -> bool {
    let has_elevated_role =
        !agg.global_roles.iter().all(|r| r == "member") || !agg.board_roles.is_empty();
    let has_sensitive_permission = agg
        .permissions
        .iter()
        .any(|p| permission_risk_level(p).is_some_and(|r| r != RiskLevel::Normal));
    has_elevated_role || has_sensitive_permission
}

/// assignment 生效/到期实时判断（M03-AUTHZ-03）。
///
/// 生效当且仅当 `granted_at <= now`（含未来授权未生效）且 `expires_at`
/// 为空（永久）或 `expires_at > now`。边界：`granted_at == now` 生效、
/// `expires_at == now` 已到期。行本身保留供审计/恢复，判断不删除。
pub fn assignment_effective_at(granted_at: i64, expires_at: Option<i64>, now: i64) -> bool {
    granted_at <= now && expires_at.is_none_or(|expires| expires > now)
}

// ────────────────────────── 幂等种子 ───────────────────────────────────────

/// 写入（或忽略已存在）单个权限行。
async fn upsert_permission(pool: &DatabasePool, p: &Permission) -> Result<(), String> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let risk = p.risk_level.as_str();
    let sys = p.is_system as i64;
    match pool {
        Either::Left(db) => {
            sqlx::query(
                "INSERT OR IGNORE INTO permissions (id, name, description, risk_level, is_system, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(p.name)
            .bind(p.description)
            .bind(risk)
            .bind(sys)
            .bind(now)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(db) => {
            sqlx::query(
                "INSERT IGNORE INTO permissions (id, name, description, risk_level, is_system, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(p.name)
            .bind(p.description)
            .bind(risk)
            .bind(sys)
            .bind(now)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 写入（或忽略已存在）内置角色行。
async fn upsert_role(pool: &DatabasePool, role: &BuiltinRole) -> Result<(), String> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let sys = role.is_system as i64;
    match pool {
        Either::Left(db) => {
            sqlx::query(
                "INSERT OR IGNORE INTO roles (id, name, display_name, description, is_system, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(role.name)
            .bind(role.display_name)
            .bind(role.description)
            .bind(sys)
            .bind(now)
            .bind(now)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(db) => {
            sqlx::query(
                "INSERT IGNORE INTO roles (id, name, display_name, description, is_system, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(role.name)
            .bind(role.display_name)
            .bind(role.description)
            .bind(sys)
            .bind(now)
            .bind(now)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn role_id_by_name(pool: &DatabasePool, name: &str) -> Result<String, String> {
    match pool {
        Either::Left(db) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(name)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string()),
        Either::Right(db) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(name)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string()),
    }
}

async fn permission_id_by_name(pool: &DatabasePool, name: &str) -> Result<String, String> {
    match pool {
        Either::Left(db) => sqlx::query_scalar("SELECT id FROM permissions WHERE name = ?")
            .bind(name)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string()),
        Either::Right(db) => sqlx::query_scalar("SELECT id FROM permissions WHERE name = ?")
            .bind(name)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string()),
    }
}

/// 写入（或忽略已存在）角色-权限映射。
async fn upsert_role_permission(
    pool: &DatabasePool,
    role_id: &str,
    permission_id: &str,
) -> Result<(), String> {
    match pool {
        Either::Left(db) => {
            sqlx::query(
                "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
            )
            .bind(role_id)
            .bind(permission_id)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(db) => {
            sqlx::query(
                "INSERT IGNORE INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
            )
            .bind(role_id)
            .bind(permission_id)
            .execute(db)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 幂等种子：注册表全部权限 + 内置角色 + `role_permissions` 映射。
///
/// 可重复调用（INSERT OR IGNORE / INSERT IGNORE）；已存在的行保留其既有
/// risk/is_system 值（不覆盖），权限与注册表不一致由
/// [`super::verify_db_permissions`] 单独校验。
pub async fn seed_builtin_roles(pool: &DatabasePool) -> Result<(), String> {
    for p in PERMISSION_REGISTRY {
        upsert_permission(pool, p).await?;
    }
    for role in BUILTIN_ROLES {
        upsert_role(pool, role).await?;
    }
    for role in BUILTIN_ROLES {
        let role_id = role_id_by_name(pool, role.name).await?;
        let names: Vec<&str> = match role.permissions {
            RolePermissions::All => PERMISSION_REGISTRY.iter().map(|p| p.name).collect(),
            RolePermissions::List(names) => names.to_vec(),
        };
        for name in names {
            let permission_id = permission_id_by_name(pool, name).await?;
            upsert_role_permission(pool, &role_id, &permission_id).await?;
        }
    }
    Ok(())
}

// ────────────────────────── 角色聚合 ───────────────────────────────────────

/// 某角色的全部权限名（按角色名）。
async fn role_permissions_by_role_name(
    pool: &DatabasePool,
    role_name: &str,
) -> Result<Vec<String>, String> {
    match pool {
        Either::Left(db) => sqlx::query_scalar(
            "SELECT p.name FROM roles r
             JOIN role_permissions rp ON rp.role_id = r.id
             JOIN permissions p ON p.id = rp.permission_id
             WHERE r.name = ?
             ORDER BY p.name",
        )
        .bind(role_name)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string()),
        Either::Right(db) => sqlx::query_scalar(
            "SELECT p.name FROM roles r
             JOIN role_permissions rp ON rp.role_id = r.id
             JOIN permissions p ON p.id = rp.permission_id
             WHERE r.name = ?
             ORDER BY p.name",
        )
        .bind(role_name)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string()),
    }
}

/// 全局角色（user_roles，生效中）→ (role_name, permission_name)。
///
/// 生效 = `granted_at <= now` 且 `expires_at` 为空或 `> now`（M03-AUTHZ-03）；
/// 过期/未来行保留不删除。
async fn global_role_permissions(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<Vec<(String, String)>, String> {
    match pool {
        Either::Left(db) => sqlx::query_as(
            "SELECT r.name, p.name FROM user_roles ur
             JOIN roles r ON r.id = ur.role_id
             JOIN role_permissions rp ON rp.role_id = r.id
             JOIN permissions p ON p.id = rp.permission_id
             WHERE ur.user_id = ? AND ur.granted_at <= ?
               AND (ur.expires_at IS NULL OR ur.expires_at > ?)
             ORDER BY r.name, p.name",
        )
        .bind(user_id)
        .bind(now)
        .bind(now)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string()),
        Either::Right(db) => sqlx::query_as(
            "SELECT r.name, p.name FROM user_roles ur
             JOIN roles r ON r.id = ur.role_id
             JOIN role_permissions rp ON rp.role_id = r.id
             JOIN permissions p ON p.id = rp.permission_id
             WHERE ur.user_id = ? AND ur.granted_at <= ?
               AND (ur.expires_at IS NULL OR ur.expires_at > ?)
             ORDER BY r.name, p.name",
        )
        .bind(user_id)
        .bind(now)
        .bind(now)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string()),
    }
}

/// 板块角色（board_role_assignments，生效中）→ (role_name, permission_name)。
///
/// 生效语义同上（M03-AUTHZ-03）；过期/未来行保留不删除。
async fn board_role_permissions(
    pool: &DatabasePool,
    user_id: &str,
    board_id: &str,
    now: i64,
) -> Result<Vec<(String, String)>, String> {
    match pool {
        Either::Left(db) => sqlx::query_as(
            "SELECT r.name, p.name FROM board_role_assignments bra
             JOIN roles r ON r.id = bra.role_id
             JOIN role_permissions rp ON rp.role_id = r.id
             JOIN permissions p ON p.id = rp.permission_id
             WHERE bra.user_id = ? AND bra.board_id = ?
               AND bra.granted_at <= ?
               AND (bra.expires_at IS NULL OR bra.expires_at > ?)
             ORDER BY r.name, p.name",
        )
        .bind(user_id)
        .bind(board_id)
        .bind(now)
        .bind(now)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string()),
        Either::Right(db) => sqlx::query_as(
            "SELECT r.name, p.name FROM board_role_assignments bra
             JOIN roles r ON r.id = bra.role_id
             JOIN role_permissions rp ON rp.role_id = r.id
             JOIN permissions p ON p.id = rp.permission_id
             WHERE bra.user_id = ? AND bra.board_id = ?
               AND bra.granted_at <= ?
               AND (bra.expires_at IS NULL OR bra.expires_at > ?)
             ORDER BY r.name, p.name",
        )
        .bind(user_id)
        .bind(board_id)
        .bind(now)
        .bind(now)
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string()),
    }
}

/// 聚合用户有效权限：member 基线 ∪ 全局角色 ∪（可选）板块角色。
///
/// - `board_id` 为 `None` 时只聚合全局作用域（member + user_roles）；
/// - 生效判断：`granted_at <= now` 且 `expires_at` 为空或 `> now`
///   （[`assignment_effective_at`]，M03-AUTHZ-03）；过期/未来行保留；
/// - 结果权限与角色名排序，便于确定性断言。
pub async fn aggregate_permissions(
    pool: &DatabasePool,
    user_id: &str,
    board_id: Option<&str>,
) -> Result<RoleAggregation, String> {
    let now = now_millis();

    let mut permissions: BTreeSet<String> = BTreeSet::new();
    let mut global_role_set: BTreeSet<String> = BTreeSet::new();
    let mut board_role_set: BTreeSet<String> = BTreeSet::new();

    // 1) member 基线：已登录用户默认角色（无 assignment 也生效）
    let mut member_permissions: BTreeSet<String> = BTreeSet::new();
    for name in role_permissions_by_role_name(pool, "member").await? {
        member_permissions.insert(name.clone());
        permissions.insert(name);
    }

    // 2) 全局角色
    for (role, permission) in global_role_permissions(pool, user_id, now).await? {
        global_role_set.insert(role);
        permissions.insert(permission);
    }

    // 3) 板块角色（板块范围）
    if let Some(board_id) = board_id {
        for (role, permission) in board_role_permissions(pool, user_id, board_id, now).await? {
            board_role_set.insert(role);
            permissions.insert(permission);
        }
    }

    let full = RoleAggregation {
        permissions,
        global_roles: global_role_set.into_iter().collect(),
        board_roles: board_role_set.into_iter().collect(),
    };

    // 4) M02-MFA-05/06：强制启用——聚合含 elevated 角色/权限但未完成 TOTP
    //    enrollment 的账号，聚合降级为 member 基线（fail-closed：未完成强制
    //    enrollment 不得取得高权限 Session 或执行高风险操作）。纯 member 基线
    //    不触发 TOTP 查询（TOTP 对普通 member 保持可选）。
    if aggregation_requires_totp(&full)
        && !crate::auth::mfa::has_confirmed_totp(pool, user_id)
            .await
            .map_err(|e| e.to_string())?
    {
        return Ok(RoleAggregation {
            permissions: member_permissions,
            global_roles: vec!["member".to_string()],
            board_roles: Vec::new(),
        });
    }

    Ok(full)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authz::is_registered;

    #[test]
    fn builtin_roles_are_system_and_names_valid() {
        assert!(
            BUILTIN_ROLES.iter().all(|r| r.is_system),
            "内置角色必须 is_system=1"
        );
        let mut names: Vec<&str> = BUILTIN_ROLES.iter().map(|r| r.name).collect();
        names.sort();
        assert_eq!(
            names,
            vec![
                "administrator",
                "board_moderator",
                "global_moderator",
                "member"
            ]
        );
    }

    #[test]
    fn builtin_role_permissions_are_registered() {
        // List 清单中的每个权限名都必须在注册表中；administrator = All
        let registry: BTreeSet<&str> = PERMISSION_REGISTRY.iter().map(|p| p.name).collect();
        for role in BUILTIN_ROLES {
            if let RolePermissions::List(names) = role.permissions {
                for name in names {
                    assert!(
                        registry.contains(name),
                        "角色 {} 的权限 {name} 不在注册表中",
                        role.name
                    );
                    assert!(is_registered(name));
                }
            }
        }
    }

    #[test]
    fn member_baseline_excludes_moderation_and_admin() {
        let member = BUILTIN_ROLES
            .iter()
            .find(|r| r.name == "member")
            .expect("member 必须存在");
        let RolePermissions::List(names) = member.permissions else {
            panic!("member 必须用 List");
        };
        assert!(names.contains(&"post.read"));
        assert!(names.contains(&"post.create"));
        assert!(names.contains(&"reaction.create"));
        assert!(!names.contains(&"post.moderate"));
        assert!(!names.contains(&"moderation.review"));
        assert!(!names.contains(&"admin.manage"));
        assert!(!names.contains(&"user.manage"));
    }

    #[test]
    fn assignment_effective_window_semantics() {
        let now = 1_700_000_000_000;
        // 生效：granted_at 在过去 + 永久（expires_at=None）
        assert!(assignment_effective_at(now - 1, None, now));
        // 生效：granted_at 在过去 + 未到期
        assert!(assignment_effective_at(now - 1, Some(now + 1), now));
        // 边界：granted_at == now 生效
        assert!(assignment_effective_at(now, None, now));
        // 边界：expires_at == now 已到期（不生效）
        assert!(!assignment_effective_at(now - 1, Some(now), now));
        // 已到期：expires_at < now 不生效
        assert!(!assignment_effective_at(now - 1, Some(now - 1), now));
        // 未来授权：granted_at > now 不生效（即使 expires_at 为空）
        assert!(!assignment_effective_at(now + 1, None, now));
        assert!(!assignment_effective_at(now + 1, Some(now + 10), now));
    }
}
