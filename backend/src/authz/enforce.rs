//! M03-AUTHZ-05：Handler 统一授权调用模式——require-action + require-object-scope。
//!
//! Handler 模式（默认拒绝：任何未命中规则一律 Deny，绝无隐式 Allow）：
//! ```text
//! let decision = require_action(pool, user_id, status, "post.edit_own", board_id, POLICY).await?;
//! if !decision.is_allowed() { return Err(deny_to_error(decision.reason(), request_id)); }
//! require_object_scope(user_id, resource, expected_owner, &[Published])?;  // 对象级
//! ```
//! - [`require_action`]：加载聚合角色（AUTHZ-02/03，含板块范围）→
//!   [`decide_action`] 纯函数判定（权限存在 + 账号状态门槛 + 策略版本）；
//! - [`require_object_scope`]：对象级 owner + resource state 判定；
//! - [`deny_to_error`]：DenyReason → AppError（未认证 401 / 其余 403），
//!   拒绝原因可审计（audit policy_version + reason）。
//!
//! 账号状态门槛为基本门（Active/Restricted 放行，其余拒绝）；未验证/冷静期/
//! mute/board_mute/banned 的精细参与由 M03-AUTHZ-06 落地。

use crate::authz::decision::{
    AccountStatus, Decision, DenyReason, ResourceInfo, ResourceState, AUTHZ_POLICY_VERSION,
};
use crate::authz::roles::{aggregate_permissions, RoleAggregation};
use crate::db::DatabasePool;
use crate::error::AppError;

/// require-action 纯函数：仅凭聚合权限 + 账号状态 + 策略版本判定。
///
/// 规则（顺序）：
/// 1. `policy_version != AUTHZ_POLICY_VERSION` → `PolicyVersionMismatch`；
/// 2. 账号状态门槛：仅 `Active`/`Restricted` 可执行（基本门，AUTHZ-06 细化）→
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
}
