//! M03-BOARDS-03：板块可见性（public/members/restricted/hidden）服务层门，
//! 统一套用授权服务（AUTHZ-02/03/05/06）。
//!
//! AUTHORIZATION.md §查看限制板块（v1 注册表命名）：
//! - **public**：匿名可见（不查授权）；
//! - **members**：有效已登录用户可见（`authorize_action` `board.read`，含账号
//!   状态门 AUTHZ-06：banned/pending_delete/deleted 一律拒绝）；
//! - **restricted**：有效已登录用户 + 本板块生效角色（`board_role_assignments`
//!   生效窗口内，`aggregate_permissions` 板块作用域 `board_roles` 非空），
//!   即板块成员；
//! - **hidden**：不进入公开列表/搜索；仅管理权限可读
//!   （`board.manage` / `post.moderate` / `moderation.review` 任一，
//!   `authorize_action`）。
//!
//! 错误映射（`VisibilityDeny::to_error`）：匿名 → 401 `authentication_required`；
//! restricted 非成员 / 账号状态 → 403 `forbidden`；hidden 无管理权限 → 404
//! （不泄漏隐藏板块存在性，与 AUTHZ-07 隐藏内容策略一致）。

use crate::authz::decision::{BoardVisibility, DenyReason, AUTHZ_POLICY_VERSION};
use crate::authz::enforce::authorize_action;
use crate::authz::roles::aggregate_permissions;
use crate::db::DatabasePool;
use crate::error::AppError;

/// 隐藏板块可读所需的管理权限（任一命中即放行）。
pub const HIDDEN_READ_PERMISSIONS: [&str; 3] =
    ["board.manage", "post.moderate", "moderation.review"];

/// 可见性拒绝原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisibilityDeny {
    /// 匿名访问 members/restricted/hidden → 401
    Unauthenticated,
    /// restricted 板块非本板块成员 → 403
    NotBoardMember,
    /// hidden 板块无管理权限 → 404（不泄漏存在性）
    MissingPermission,
    /// 账号状态不允许（banned/pending_delete/deleted）→ 403
    AccountNotAllowed,
}

impl VisibilityDeny {
    /// 拒绝 → AppError（隐藏板块用 404 防存在性探测）。
    pub fn to_error(self, request_id: &str) -> AppError {
        match self {
            VisibilityDeny::Unauthenticated => {
                AppError::unauthorized("authentication required", request_id)
            }
            VisibilityDeny::MissingPermission => AppError::not_found("board not found", request_id),
            VisibilityDeny::NotBoardMember | VisibilityDeny::AccountNotAllowed => {
                AppError::forbidden("board is not accessible to you", request_id)
            }
        }
    }
}

/// 单板块访问判定结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BoardAccess {
    pub visible: bool,
    pub deny: Option<VisibilityDeny>,
}

impl BoardAccess {
    fn visible() -> Self {
        Self {
            visible: true,
            deny: None,
        }
    }

    fn denied(deny: VisibilityDeny) -> Self {
        Self {
            visible: false,
            deny: Some(deny),
        }
    }
}

/// 单板块读取门（getBoard 等 handler 统一前置）。
///
/// `actor_id = None` 表示匿名请求；`visibility` 来自 `boards.visibility`
/// （迁移 0022/0025 CHECK 稳定枚举）。
pub async fn board_read_gate(
    pool: &DatabasePool,
    board_id: &str,
    visibility: BoardVisibility,
    actor_id: Option<&str>,
) -> Result<BoardAccess, String> {
    match visibility {
        BoardVisibility::Public => Ok(BoardAccess::visible()),
        BoardVisibility::Members => member_gate(pool, board_id, actor_id).await,
        BoardVisibility::Restricted => restricted_gate(pool, board_id, actor_id).await,
        BoardVisibility::Hidden => hidden_gate(pool, board_id, actor_id).await,
    }
}

/// members：有效已登录用户（board.read 经授权服务，含账号状态门）。
async fn member_gate(
    pool: &DatabasePool,
    board_id: &str,
    actor_id: Option<&str>,
) -> Result<BoardAccess, String> {
    let Some(actor) = actor_id else {
        return Ok(BoardAccess::denied(VisibilityDeny::Unauthenticated));
    };
    let decision = authorize_action(
        pool,
        actor,
        "board.read",
        Some(board_id),
        AUTHZ_POLICY_VERSION,
    )
    .await?;
    match decision {
        crate::authz::decision::Decision::Allow => Ok(BoardAccess::visible()),
        crate::authz::decision::Decision::Deny {
            reason: DenyReason::AccountNotAllowed,
        } => Ok(BoardAccess::denied(VisibilityDeny::AccountNotAllowed)),
        crate::authz::decision::Decision::Deny { .. } => {
            Ok(BoardAccess::denied(VisibilityDeny::MissingPermission))
        }
    }
}

/// restricted：有效已登录用户 + 本板块生效角色（板块成员），或全局/管理通道
/// （`post.moderate` 命中的管理员/全局版主——他们无板块角色但必须能读受限板块）。
async fn restricted_gate(
    pool: &DatabasePool,
    board_id: &str,
    actor_id: Option<&str>,
) -> Result<BoardAccess, String> {
    let Some(actor) = actor_id else {
        return Ok(BoardAccess::denied(VisibilityDeny::Unauthenticated));
    };
    let decision = authorize_action(
        pool,
        actor,
        "board.read",
        Some(board_id),
        AUTHZ_POLICY_VERSION,
    )
    .await?;
    if let crate::authz::decision::Decision::Deny { reason } = decision {
        return Ok(match reason {
            DenyReason::AccountNotAllowed => BoardAccess::denied(VisibilityDeny::AccountNotAllowed),
            _ => BoardAccess::denied(VisibilityDeny::MissingPermission),
        });
    }
    let roles = aggregate_permissions(pool, actor, Some(board_id)).await?;
    if !roles.board_roles.is_empty() {
        return Ok(BoardAccess::visible());
    }
    // 全局/管理通道：管理员与全局版主（post.moderate 在板块范围外仍放行）
    let moderate = authorize_action(
        pool,
        actor,
        "post.moderate",
        Some(board_id),
        AUTHZ_POLICY_VERSION,
    )
    .await?;
    if moderate.is_allowed() {
        Ok(BoardAccess::visible())
    } else {
        Ok(BoardAccess::denied(VisibilityDeny::NotBoardMember))
    }
}

/// hidden：仅管理权限可读（board.manage / post.moderate / moderation.review）。
async fn hidden_gate(
    pool: &DatabasePool,
    board_id: &str,
    actor_id: Option<&str>,
) -> Result<BoardAccess, String> {
    let Some(actor) = actor_id else {
        return Ok(BoardAccess::denied(VisibilityDeny::Unauthenticated));
    };
    let mut account_denied = false;
    for permission in HIDDEN_READ_PERMISSIONS {
        match authorize_action(
            pool,
            actor,
            permission,
            Some(board_id),
            AUTHZ_POLICY_VERSION,
        )
        .await?
        {
            crate::authz::decision::Decision::Allow => return Ok(BoardAccess::visible()),
            crate::authz::decision::Decision::Deny {
                reason: DenyReason::AccountNotAllowed,
            } => account_denied = true,
            crate::authz::decision::Decision::Deny { .. } => {}
        }
    }
    Ok(BoardAccess::denied(if account_denied {
        VisibilityDeny::AccountNotAllowed
    } else {
        VisibilityDeny::MissingPermission
    }))
}

/// 列表投影过滤：返回对 `actor_id` 可见的 board_id（保持输入顺序）。
///
/// - `public` 对所有人可见；`members`/`restricted` 对匿名隐藏；
/// - `hidden` 仅对可读 hidden 的 actor 出现（不进入公开列表/搜索）。
pub async fn filter_visible_board_ids(
    pool: &DatabasePool,
    boards: &[(String, BoardVisibility)],
    actor_id: Option<&str>,
) -> Result<Vec<String>, String> {
    let mut out = Vec::with_capacity(boards.len());
    for (id, visibility) in boards {
        if board_read_gate(pool, id, *visibility, actor_id)
            .await?
            .visible
        {
            out.push(id.clone());
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    #[test]
    fn deny_maps_to_status_codes() {
        let unauth = VisibilityDeny::Unauthenticated
            .to_error("r")
            .into_response();
        assert_eq!(unauth.status(), StatusCode::UNAUTHORIZED);

        let hidden = VisibilityDeny::MissingPermission
            .to_error("r")
            .into_response();
        assert_eq!(
            hidden.status(),
            StatusCode::NOT_FOUND,
            "隐藏板块 404 防存在性泄漏"
        );

        let not_member = VisibilityDeny::NotBoardMember.to_error("r").into_response();
        assert_eq!(not_member.status(), StatusCode::FORBIDDEN);

        let account = VisibilityDeny::AccountNotAllowed
            .to_error("r")
            .into_response();
        assert_eq!(account.status(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn hidden_read_permissions_are_registered() {
        for permission in HIDDEN_READ_PERMISSIONS {
            assert!(
                crate::authz::is_registered(permission),
                "{permission} 必须在权限注册表"
            );
        }
    }
}
