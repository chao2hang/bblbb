//! M03-AUTHZ-05/06：Handler 统一授权调用模式——require-action + require-object-scope，
//! 账号状态实时参与授权（未验证/冷静期/restricted/mute/board_mute/banned）。
//!
//! Handler 模式（默认拒绝：任何未命中规则一律 Deny，绝无隐式 Allow）：
//! ```text
//! let decision = authorize_action(pool, user_id, "post.create", board_id, POLICY).await?;
//! if !decision.is_allowed() { return Err(deny_to_error(reason, request_id)); }
//! require_object_scope(user_id, resource, expected_owner, &[Published])?;  // 对象级
//! ```
//! - [`authorize_action`]：加载聚合角色（AUTHZ-02/03）→ [`load_account_gates`]
//!   （AUTHZ-06 状态门）→ [`authorize_with`] 组合判定；
//! - [`authorize_with`]：先 [`account_gate`]（未验证/冷静期/restricted/mute/
//!   board_mute/banned 实时门），再 [`decide_action`]（策略版本 + 权限存在）；
//! - [`require_object_scope`]：对象级 owner + resource state 判定；
//! - [`deny_to_error`]：DenyReason → AppError（未认证 401 / 其余 403）。

use sqlx::Either;

use crate::authz::decision::{
    AccountStatus, Decision, DenyReason, ResourceInfo, ResourceState, AUTHZ_POLICY_VERSION,
};
use crate::authz::roles::{aggregate_permissions, RoleAggregation};
use crate::db::DatabasePool;
use crate::error::AppError;
use crate::outbox::now_millis;

/// 新账户内容写入冷静期（邮箱验证后；RETENTION/注册策略默认 24 小时）。
pub const ACCOUNT_COOLDOWN_MS: i64 = 24 * 60 * 60 * 1000;

/// 账号状态门输入（M03-AUTHZ-06）。
///
/// - `status`/`email_verified` 来自 `users`；`cooldown_until` 由
///   `email_verified_at + ACCOUNT_COOLDOWN_MS` 推导；
/// - `mute_until`/`board_mute_until` 来自 sanction（M5 落地前由调用方注入，
///   `load_account_gates` 当前返回 None）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountGates {
    pub status: AccountStatus,
    /// `email_verified_at` 非空。
    pub email_verified: bool,
    /// 冷静期结束时间（`None` = 无冷静期门）。
    pub cooldown_until: Option<i64>,
    /// 全局 mute 结束时间（`None` = 未 mute）。
    pub mute_until: Option<i64>,
    /// 本板块 board_mute 结束时间（`None` = 未 board_mute）。
    pub board_mute_until: Option<i64>,
}

/// 内容写入权限集合（mute/冷静期/未验证/restricted 影响的动作）。
pub fn is_content_write_permission(permission: &str) -> bool {
    matches!(
        permission,
        "post.create" | "comment.create" | "attachment.upload" | "reaction.create" | "video.embed"
    )
}

/// 账号状态实时门（AUTHZ-06）：按权限类别与状态判定（默认拒绝）。
///
/// 规则：
/// 1. 终态/处罚态（banned / pending_delete / deleted）→ 一律拒绝
///    `AccountNotAllowed`（读与写都不放）；
/// 2. 未验证（`status == pending` 或 `email_verified == false`）：内容写入
///    → `EmailUnverified`；读/own/验证相关放行；
/// 3. 内容写入类权限附加门（按序）：
///    - 冷静期未过 → `InCooldown`；
///    - `restricted` → `AccountNotAllowed`；
///    - 全局 mute 未过 → `Muted`；
///    - 本板块 board_mute 未过（请求携带板块）→ `BoardMuted`；
/// 4. 其余 → `Ok(())`（权限存在性由 `decide_action` 判定）。
pub fn account_gate(
    gates: &AccountGates,
    permission: &str,
    board_id: Option<&str>,
    now: i64,
) -> Result<(), DenyReason> {
    match gates.status {
        AccountStatus::Banned | AccountStatus::PendingDelete | AccountStatus::Deleted => {
            return Err(DenyReason::AccountNotAllowed);
        }
        AccountStatus::Pending => {
            return if is_content_write_permission(permission) {
                Err(DenyReason::EmailUnverified)
            } else {
                Ok(())
            };
        }
        _ => {}
    }

    if !gates.email_verified && is_content_write_permission(permission) {
        return Err(DenyReason::EmailUnverified);
    }

    if is_content_write_permission(permission) {
        if let Some(cooldown) = gates.cooldown_until {
            if now < cooldown {
                return Err(DenyReason::InCooldown);
            }
        }
        if gates.status == AccountStatus::Restricted {
            return Err(DenyReason::AccountNotAllowed);
        }
        if let Some(mute) = gates.mute_until {
            if now < mute {
                return Err(DenyReason::Muted);
            }
        }
        if let Some(board_mute) = gates.board_mute_until {
            if board_id.is_some() && now < board_mute {
                return Err(DenyReason::BoardMuted);
            }
        }
    }
    Ok(())
}

/// 从 `users` 行加载账号状态门（AUTHZ-06）。
///
/// 冷静期 = `email_verified_at + ACCOUNT_COOLDOWN_MS`；mute/board_mute 来自
/// sanction（M5-MODERATION 落地前返回 None，由调用方在 sanction 存在时注入）。
pub async fn load_account_gates(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<AccountGates, String> {
    let (status, email_verified_at): (String, Option<i64>) = match pool {
        Either::Left(db) => {
            sqlx::query_as("SELECT status, email_verified_at FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_one(db)
                .await
                .map_err(|e| e.to_string())?
        }
        Either::Right(db) => {
            sqlx::query_as("SELECT status, email_verified_at FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_one(db)
                .await
                .map_err(|e| e.to_string())?
        }
    };
    let status =
        AccountStatus::parse(&status).ok_or_else(|| format!("unknown account status: {status}"))?;
    Ok(AccountGates {
        status,
        email_verified: email_verified_at.is_some(),
        cooldown_until: email_verified_at.map(|verified| verified + ACCOUNT_COOLDOWN_MS),
        mute_until: None,
        board_mute_until: None,
    })
}

/// require-action 纯函数：仅凭聚合权限 + 账号状态 + 策略版本判定。
///
/// 规则（顺序）：
/// 1. `policy_version != AUTHZ_POLICY_VERSION` → `PolicyVersionMismatch`；
/// 2. 账号状态门槛：仅 `Active`/`Restricted` 可执行（AUTHZ-06 细化）→
///    其余 `AccountNotAllowed`；
/// 3. `permission ∈ roles.permissions` → `Allow`；
/// 4. 否则 → `MissingPermission`（默认拒绝）。
pub fn decide_action(
    roles: &RoleAggregation,
    permission: &str,
    status: AccountStatus,
    policy_version: &str,
) -> Decision {
    if policy_version != AUTHZ_POLICY_VERSION {
        return Decision::Deny {
            reason: DenyReason::PolicyVersionMismatch,
        };
    }
    match status {
        AccountStatus::Active | AccountStatus::Restricted => {}
        _ => {
            return Decision::Deny {
                reason: DenyReason::AccountNotAllowed,
            }
        }
    }
    if roles.has(permission) {
        Decision::Allow
    } else {
        Decision::Deny {
            reason: DenyReason::MissingPermission,
        }
    }
}

/// require-action：加载聚合角色（AUTHZ-02/03；`board_id` 携带板块范围）并判定。
pub async fn require_action(
    pool: &DatabasePool,
    user_id: &str,
    status: AccountStatus,
    permission: &str,
    board_id: Option<&str>,
    policy_version: &str,
) -> Result<Decision, String> {
    let roles = aggregate_permissions(pool, user_id, board_id).await?;
    Ok(decide_action(&roles, permission, status, policy_version))
}

/// 组合判定（AUTHZ-06）：账号状态门 → 动作门（默认拒绝，顺序即优先级）。
pub fn authorize_with(
    roles: &RoleAggregation,
    gates: &AccountGates,
    permission: &str,
    board_id: Option<&str>,
    policy_version: &str,
) -> Decision {
    if let Err(reason) = account_gate(gates, permission, board_id, now_millis()) {
        return Decision::Deny { reason };
    }
    decide_action(roles, permission, gates.status, policy_version)
}

/// Handler 统一入口：加载角色 + 账号状态门 → 组合判定。
pub async fn authorize_action(
    pool: &DatabasePool,
    user_id: &str,
    permission: &str,
    board_id: Option<&str>,
    policy_version: &str,
) -> Result<Decision, String> {
    let roles = aggregate_permissions(pool, user_id, board_id).await?;
    let gates = load_account_gates(pool, user_id).await?;
    Ok(authorize_with(
        &roles,
        &gates,
        permission,
        board_id,
        policy_version,
    ))
}

/// require-object-scope：对象级 owner + resource state 判定（默认拒绝）。
///
/// - `resource` 存在时：owner 必须为 actor（`expected_owner` 可显式指定，
///   否则取 `resource.owner_id`），否则 `NotResourceOwner`；
///   `resource.state` 必须在 `allowed_states` 内，否则 `ResourceStateNotAllowed`；
/// - 无资源（非对象动作）→ `Ok(())`。
pub fn require_object_scope(
    actor_id: &str,
    resource: Option<&ResourceInfo>,
    expected_owner: Option<&str>,
    allowed_states: &[ResourceState],
) -> Result<(), DenyReason> {
    let Some(resource) = resource else {
        return Ok(());
    };
    let owner = expected_owner.unwrap_or(resource.owner_id);
    if !crate::authz::decision::is_resource_owner(actor_id, owner) {
        return Err(DenyReason::NotResourceOwner);
    }
    if !allowed_states.contains(&resource.state) {
        return Err(DenyReason::ResourceStateNotAllowed);
    }
    Ok(())
}

/// 提取拒绝原因（Allow → None）。
pub fn denied_reason(decision: &Decision) -> Option<DenyReason> {
    match decision {
        Decision::Allow => None,
        Decision::Deny { reason } => Some(*reason),
    }
}

/// DenyReason → AppError：NotAuthenticated → 401，其余 → 403。
/// 拒绝原因不含内部信息，可直接进入响应与审计。
pub fn deny_to_error(reason: DenyReason, request_id: &str) -> AppError {
    match reason {
        DenyReason::NotAuthenticated => AppError::unauthorized(reason.to_string(), request_id),
        _ => AppError::forbidden(reason.to_string(), request_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member_roles() -> RoleAggregation {
        let mut permissions = std::collections::BTreeSet::new();
        for name in [
            "post.read",
            "post.create",
            "reaction.create",
            "user.edit_own",
        ] {
            permissions.insert(name.to_string());
        }
        RoleAggregation {
            permissions,
            global_roles: vec!["member".to_string()],
            board_roles: Vec::new(),
        }
    }

    #[test]
    fn action_allow_when_permission_present() {
        let decision = decide_action(
            &member_roles(),
            "post.read",
            AccountStatus::Active,
            AUTHZ_POLICY_VERSION,
        );
        assert!(decision.is_allowed());
    }

    #[test]
    fn action_default_deny_missing_permission() {
        let decision = decide_action(
            &member_roles(),
            "post.moderate",
            AccountStatus::Active,
            AUTHZ_POLICY_VERSION,
        );
        assert_eq!(
            denied_reason(&decision),
            Some(DenyReason::MissingPermission)
        );
    }

    #[test]
    fn action_denies_non_active_statuses() {
        for status in [
            AccountStatus::Pending,
            AccountStatus::Banned,
            AccountStatus::PendingDelete,
            AccountStatus::Deleted,
        ] {
            let decision =
                decide_action(&member_roles(), "post.read", status, AUTHZ_POLICY_VERSION);
            assert_eq!(
                denied_reason(&decision),
                Some(DenyReason::AccountNotAllowed),
                "{status} 必须拒绝"
            );
        }
        // Restricted 放行（AUTHZ-06 细化）
        assert!(decide_action(
            &member_roles(),
            "post.read",
            AccountStatus::Restricted,
            AUTHZ_POLICY_VERSION
        )
        .is_allowed());
    }

    #[test]
    fn action_rejects_stale_policy_version() {
        let decision = decide_action(&member_roles(), "post.read", AccountStatus::Active, "0.9.0");
        assert_eq!(
            denied_reason(&decision),
            Some(DenyReason::PolicyVersionMismatch)
        );
    }

    #[test]
    fn object_scope_owner_and_state() {
        let own = ResourceInfo {
            owner_id: "u1",
            state: ResourceState::Published,
        };
        // owner 匹配 + 状态允许
        assert!(require_object_scope("u1", Some(&own), None, &[ResourceState::Published]).is_ok());
        // owner 不匹配 → NotResourceOwner
        assert_eq!(
            require_object_scope("u2", Some(&own), None, &[ResourceState::Published]),
            Err(DenyReason::NotResourceOwner)
        );
        // 状态不允许 → ResourceStateNotAllowed
        assert_eq!(
            require_object_scope("u1", Some(&own), None, &[ResourceState::Draft]),
            Err(DenyReason::ResourceStateNotAllowed)
        );
        // 无资源（非对象动作）→ Ok
        assert!(require_object_scope("u1", None, None, &[]).is_ok());
    }

    #[test]
    fn deny_maps_to_error() {
        use axum::http::StatusCode;
        use axum::response::IntoResponse;
        let unauthorized = deny_to_error(DenyReason::NotAuthenticated, "req-1").into_response();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);
        let forbidden = deny_to_error(DenyReason::MissingPermission, "req-2").into_response();
        assert_eq!(forbidden.status(), StatusCode::FORBIDDEN);
        let default = deny_to_error(DenyReason::DefaultDeny, "req-3").into_response();
        assert_eq!(default.status(), StatusCode::FORBIDDEN);
    }

    // ── M03-AUTHZ-06：账号状态门 ──────────────────────────────────────────

    fn gates(status: AccountStatus, email_verified: bool) -> AccountGates {
        AccountGates {
            status,
            email_verified,
            cooldown_until: None,
            mute_until: None,
            board_mute_until: None,
        }
    }

    #[test]
    fn terminal_and_banned_statuses_deny_everything() {
        let now = 1_700_000_000_000;
        for status in [
            AccountStatus::Banned,
            AccountStatus::PendingDelete,
            AccountStatus::Deleted,
        ] {
            for permission in [
                "post.read",
                "post.create",
                "user.read_own",
                "appeal.create_own",
            ] {
                assert_eq!(
                    account_gate(&gates(status, true), permission, None, now),
                    Err(DenyReason::AccountNotAllowed),
                    "{status} 的 {permission} 必须拒绝"
                );
            }
        }
    }

    #[test]
    fn unverified_blocks_content_write_but_allows_reads() {
        let now = 1_700_000_000_000;
        let pending = gates(AccountStatus::Pending, false);
        assert_eq!(
            account_gate(&pending, "post.create", None, now),
            Err(DenyReason::EmailUnverified)
        );
        assert!(account_gate(&pending, "user.read_own", None, now).is_ok());

        // active 但 email_verified=false（历史账户）同样挡内容写入
        let unverified = gates(AccountStatus::Active, false);
        assert_eq!(
            account_gate(&unverified, "comment.create", None, now),
            Err(DenyReason::EmailUnverified)
        );
        assert!(account_gate(&unverified, "post.read", None, now).is_ok());
    }

    #[test]
    fn cooldown_and_restricted_gate_content_write() {
        let now = 1_700_000_000_000;
        // 冷静期未过
        let mut cooling = gates(AccountStatus::Active, true);
        cooling.cooldown_until = Some(now + 3_600_000);
        assert_eq!(
            account_gate(&cooling, "post.create", None, now),
            Err(DenyReason::InCooldown)
        );
        assert!(account_gate(&cooling, "post.read", None, now).is_ok());
        // 冷静期已过
        cooling.cooldown_until = Some(now - 1);
        assert!(account_gate(&cooling, "post.create", None, now).is_ok());

        // restricted：挡内容写入，放行读取
        let restricted = gates(AccountStatus::Restricted, true);
        assert_eq!(
            account_gate(&restricted, "post.create", None, now),
            Err(DenyReason::AccountNotAllowed)
        );
        assert!(account_gate(&restricted, "post.read", None, now).is_ok());
    }

    #[test]
    fn mute_and_board_mute_gate_content_write() {
        let now = 1_700_000_000_000;
        // 全局 mute 未过
        let mut muted = gates(AccountStatus::Active, true);
        muted.mute_until = Some(now + 60_000);
        assert_eq!(
            account_gate(&muted, "post.create", None, now),
            Err(DenyReason::Muted)
        );
        assert!(account_gate(&muted, "post.read", None, now).is_ok());
        // mute 已过
        muted.mute_until = Some(now - 1);
        assert!(account_gate(&muted, "post.create", None, now).is_ok());

        // board_mute：仅携带板块时挡内容写入
        let mut board_muted = gates(AccountStatus::Active, true);
        board_muted.board_mute_until = Some(now + 60_000);
        assert_eq!(
            account_gate(&board_muted, "post.create", Some("b-1"), now),
            Err(DenyReason::BoardMuted)
        );
        // 无板块（全局动作）不受 board_mute 影响
        assert!(account_gate(&board_muted, "post.create", None, now).is_ok());
        // board_mute 已过
        board_muted.board_mute_until = Some(now - 1);
        assert!(account_gate(&board_muted, "post.create", Some("b-1"), now).is_ok());
    }

    #[test]
    fn authorize_with_composes_gate_then_action() {
        let mut roles = RoleAggregation {
            permissions: Default::default(),
            global_roles: Vec::new(),
            board_roles: Vec::new(),
        };
        roles.permissions.insert("post.create".to_string());
        roles.permissions.insert("post.read".to_string());

        // 未验证：即使有权限也拒（EmailUnverified 优先于权限判定）
        let unverified = gates(AccountStatus::Active, false);
        assert_eq!(
            authorize_with(
                &roles,
                &unverified,
                "post.create",
                None,
                AUTHZ_POLICY_VERSION
            ),
            Decision::Deny {
                reason: DenyReason::EmailUnverified
            }
        );
        // 验证且冷静期过后：权限在 → Allow
        let ready = gates(AccountStatus::Active, true);
        assert!(
            authorize_with(&roles, &ready, "post.create", None, AUTHZ_POLICY_VERSION).is_allowed()
        );
        // 权限不在 → MissingPermission
        assert_eq!(
            authorize_with(&roles, &ready, "post.moderate", None, AUTHZ_POLICY_VERSION),
            Decision::Deny {
                reason: DenyReason::MissingPermission
            }
        );
    }
}
