//! M03-AUTHZ-04：动作授权输入与判定结果类型。
//!
//! 一次授权判定的完整输入：
//! - **actor**：请求者（user_id + 账号状态 + 已聚合角色/权限，AUTHZ-02/03）；
//! - **permission**：所需 `resource.action` 权限名（注册表，AUTHZ-01）；
//! - **board**：板块范围（board_id + visibility + posting_mode + 软删），
//!   可选——全局动作无板块；
//! - **resource**：对象级输入（owner_id + resource state），可选——非对象
//!   动作无资源；
//! - **policy_version**：本次判定依据的权限策略版本（审计与失效检测）。
//!
//! 判定结果：[`Decision`]（Allow / Deny{reason}），reason 供审计与错误映射；
//! 默认拒绝语义（AUTHZ-05）：未命中任何 Allow 规则即 Deny::DefaultDeny。
//!
//! 账号状态的具体门槛（未验证/冷静期/restricted/mute/board_mute/banned）
//! 由 M03-AUTHZ-06 落地；本模块定义枚举与输入组装。

use std::fmt;

use super::roles::RoleAggregation;

/// 权限策略版本（v1 基线）。授权判定必须携带，审计与失效检测据此核对；
/// 后续策略变更递增该版本并使旧缓存判定失效。
pub const AUTHZ_POLICY_VERSION: &str = "1.0.0";

/// 账号状态（users.status 稳定枚举，与迁移 0001 CHECK 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    Pending,
    Active,
    Restricted,
    Banned,
    PendingDelete,
    Deleted,
}

impl AccountStatus {
    pub const ALL: [AccountStatus; 6] = [
        AccountStatus::Pending,
        AccountStatus::Active,
        AccountStatus::Restricted,
        AccountStatus::Banned,
        AccountStatus::PendingDelete,
        AccountStatus::Deleted,
    ];

    /// 数据库表示（与 users.status CHECK 一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            AccountStatus::Pending => "pending",
            AccountStatus::Active => "active",
            AccountStatus::Restricted => "restricted",
            AccountStatus::Banned => "banned",
            AccountStatus::PendingDelete => "pending_delete",
            AccountStatus::Deleted => "deleted",
        }
    }

    pub fn parse(value: &str) -> Option<AccountStatus> {
        Self::ALL.iter().find(|s| s.as_str() == value).copied()
    }
}

impl fmt::Display for AccountStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 资源状态（对象级判定输入；Post/Comment 共用稳定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResourceState {
    Draft,
    Published,
    Hidden,
    Deleted,
}

impl ResourceState {
    pub const ALL: [ResourceState; 4] = [
        ResourceState::Draft,
        ResourceState::Published,
        ResourceState::Hidden,
        ResourceState::Deleted,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            ResourceState::Draft => "draft",
            ResourceState::Published => "published",
            ResourceState::Hidden => "hidden",
            ResourceState::Deleted => "deleted",
        }
    }

    pub fn parse(value: &str) -> Option<ResourceState> {
        Self::ALL.iter().find(|s| s.as_str() == value).copied()
    }
}

impl fmt::Display for ResourceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 板块可见性（boards.visibility 稳定枚举，迁移 0022 CHECK）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardVisibility {
    Public,
    Members,
    Restricted,
    Hidden,
}

impl BoardVisibility {
    pub const ALL: [BoardVisibility; 4] = [
        BoardVisibility::Public,
        BoardVisibility::Members,
        BoardVisibility::Restricted,
        BoardVisibility::Hidden,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            BoardVisibility::Public => "public",
            BoardVisibility::Members => "members",
            BoardVisibility::Restricted => "restricted",
            BoardVisibility::Hidden => "hidden",
        }
    }

    pub fn parse(value: &str) -> Option<BoardVisibility> {
        Self::ALL.iter().find(|v| v.as_str() == value).copied()
    }
}

/// 板块发帖模式（boards.posting_mode 稳定枚举，迁移 0022 CHECK）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BoardPostingMode {
    Normal,
    Approval,
    Readonly,
    Closed,
}

impl BoardPostingMode {
    pub const ALL: [BoardPostingMode; 4] = [
        BoardPostingMode::Normal,
        BoardPostingMode::Approval,
        BoardPostingMode::Readonly,
        BoardPostingMode::Closed,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            BoardPostingMode::Normal => "normal",
            BoardPostingMode::Approval => "approval",
            BoardPostingMode::Readonly => "readonly",
            BoardPostingMode::Closed => "closed",
        }
    }

    pub fn parse(value: &str) -> Option<BoardPostingMode> {
        Self::ALL.iter().find(|m| m.as_str() == value).copied()
    }

    /// 该板块是否允许新增帖子（STATE-MACHINES §Board：`readonly`/`closed`
    /// 禁止新增帖子，服务层强制，M03-BOARDS-03 落地）。
    pub fn allows_content_write(&self) -> bool {
        matches!(self, BoardPostingMode::Normal | BoardPostingMode::Approval)
    }
}

/// 请求者上下文（actor）。
#[derive(Debug, Clone)]
pub struct ActorContext<'a> {
    pub user_id: &'a str,
    pub status: AccountStatus,
    /// 已聚合的有效权限与角色（AUTHZ-02/03；由调用方在判定前加载）。
    pub roles: &'a RoleAggregation,
}

/// 对象级输入（可选）。
#[derive(Debug, Clone, Copy)]
pub struct ResourceInfo<'a> {
    /// 资源所有者/作者（owner 判定：`owner_id == actor.user_id`）。
    pub owner_id: &'a str,
    /// 资源状态（draft/published/hidden/deleted）。
    pub state: ResourceState,
}

/// 板块范围输入（可选；全局动作无板块）。
#[derive(Debug, Clone, Copy)]
pub struct BoardContext<'a> {
    pub board_id: &'a str,
    pub visibility: BoardVisibility,
    pub posting_mode: BoardPostingMode,
    /// `deleted_at` 非空（软删除板块）。
    pub deleted: bool,
}

/// 动作授权输入（handler 组装后交给决策器，AUTHZ-05）。
#[derive(Debug, Clone)]
pub struct AuthzInput<'a> {
    pub actor: ActorContext<'a>,
    /// 所需 `resource.action` 权限名（必须在注册表内）。
    pub permission: &'a str,
    pub board: Option<BoardContext<'a>>,
    pub resource: Option<ResourceInfo<'a>>,
    /// 本次判定依据的权限策略版本（应等于 [`AUTHZ_POLICY_VERSION`]）。
    pub policy_version: &'a str,
}

impl<'a> AuthzInput<'a> {
    /// 构造授权输入；`permission` 必须为已注册权限名，否则返回 `None`。
    pub fn new(
        actor: ActorContext<'a>,
        permission: &'a str,
        board: Option<BoardContext<'a>>,
        resource: Option<ResourceInfo<'a>>,
        policy_version: &'a str,
    ) -> Option<AuthzInput<'a>> {
        if !super::is_registered(permission) {
            return None;
        }
        Some(AuthzInput {
            actor,
            permission,
            board,
            resource,
            policy_version,
        })
    }
}

/// 对象级 owner 判定：资源所有者 == actor。
pub fn is_resource_owner(actor_id: &str, owner_id: &str) -> bool {
    actor_id == owner_id
}

/// 判定结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Deny { reason: DenyReason },
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// 拒绝原因（供审计与错误映射；默认拒绝优先）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DenyReason {
    /// 未认证/无 actor。
    NotAuthenticated,
    /// 账号状态不允许该动作（封禁/注销中/已删除等）。
    AccountNotAllowed,
    /// 邮箱未验证（内容写入受限，M03-AUTHZ-06）。
    EmailUnverified,
    /// 新账户冷静期内（内容写入受限，M03-AUTHZ-06）。
    InCooldown,
    /// 全局 mute 生效中（内容写入受限，M03-AUTHZ-06）。
    Muted,
    /// 本板块 board_mute 生效中（板块内容写入受限，M03-AUTHZ-06）。
    BoardMuted,
    /// 聚合权限中缺少所需 permission。
    MissingPermission,
    /// 对象级 owner 不匹配（edit_own 等）。
    NotResourceOwner,
    /// 资源状态不允许（如已删除内容不可编辑）。
    ResourceStateNotAllowed,
    /// 板块范围不符（板块版主只在其板块生效）。
    BoardScopeMismatch,
    /// 策略版本与当前版本不一致（判定作废）。
    PolicyVersionMismatch,
    /// 默认拒绝：未命中任何 Allow 规则。
    DefaultDeny,
}

impl fmt::Display for DenyReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            DenyReason::NotAuthenticated => "not authenticated",
            DenyReason::AccountNotAllowed => "account status not allowed",
            DenyReason::EmailUnverified => "email not verified",
            DenyReason::InCooldown => "account in cooldown",
            DenyReason::Muted => "account muted",
            DenyReason::BoardMuted => "account board-muted",
            DenyReason::MissingPermission => "missing permission",
            DenyReason::NotResourceOwner => "not resource owner",
            DenyReason::ResourceStateNotAllowed => "resource state not allowed",
            DenyReason::BoardScopeMismatch => "board scope mismatch",
            DenyReason::PolicyVersionMismatch => "policy version mismatch",
            DenyReason::DefaultDeny => "default deny",
        };
        f.write_str(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_status_round_trips() {
        for status in AccountStatus::ALL {
            assert_eq!(AccountStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(AccountStatus::parse("unknown"), None);
    }

    #[test]
    fn resource_state_and_board_enums_round_trip() {
        for state in ResourceState::ALL {
            assert_eq!(ResourceState::parse(state.as_str()), Some(state));
        }
        for visibility in BoardVisibility::ALL {
            assert_eq!(
                BoardVisibility::parse(visibility.as_str()),
                Some(visibility)
            );
        }
        for mode in BoardPostingMode::ALL {
            assert_eq!(BoardPostingMode::parse(mode.as_str()), Some(mode));
        }
        assert_eq!(BoardVisibility::parse("bogus"), None);
        assert_eq!(BoardPostingMode::parse("bogus"), None);
    }

    #[test]
    fn locked_boards_do_not_allow_content_write() {
        // 锁定板块（readonly/closed）禁止新增帖子；normal/approval 允许
        assert!(!BoardPostingMode::Readonly.allows_content_write());
        assert!(!BoardPostingMode::Closed.allows_content_write());
        assert!(BoardPostingMode::Normal.allows_content_write());
        assert!(BoardPostingMode::Approval.allows_content_write());
    }

    #[test]
    fn policy_version_is_set() {
        assert!(!AUTHZ_POLICY_VERSION.is_empty());
        assert_eq!(AUTHZ_POLICY_VERSION.split('.').count(), 3, "语义化版本");
    }

    #[test]
    fn authz_input_rejects_unregistered_permission() {
        let roles = RoleAggregation {
            permissions: Default::default(),
            global_roles: Vec::new(),
            board_roles: Vec::new(),
        };
        let actor = ActorContext {
            user_id: "u1",
            status: AccountStatus::Active,
            roles: &roles,
        };
        assert!(AuthzInput::new(
            actor.clone(),
            "not.a.real.permission",
            None,
            None,
            AUTHZ_POLICY_VERSION
        )
        .is_none());
        let input = AuthzInput::new(actor, "post.read", None, None, AUTHZ_POLICY_VERSION);
        assert!(input.is_some());
        assert!(input.unwrap().policy_version == AUTHZ_POLICY_VERSION);
    }

    #[test]
    fn owner_and_decision_predicates() {
        assert!(is_resource_owner("u1", "u1"));
        assert!(!is_resource_owner("u1", "u2"));
        assert!(Decision::Allow.is_allowed());
        assert!(!Decision::Deny {
            reason: DenyReason::DefaultDeny
        }
        .is_allowed());
        assert_eq!(
            DenyReason::MissingPermission.to_string(),
            "missing permission"
        );
    }
}
