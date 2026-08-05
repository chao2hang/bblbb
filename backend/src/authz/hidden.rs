//! M03-AUTHZ-07：管理员/版主读取隐藏内容的显式管理投影路径。
//!
//! 契约（SECURITY.md §6「隐藏正文不包含在未授权 API 响应、DOM、日志、异常
//! 或遥测中」；MODERATION.md §11）：
//! - 隐藏内容（hidden 可见性 / deleted 内容 / 封禁相关内容）**绝不**出现在
//!   普通公开投影中；
//! - 管理员/版主必须**显式**走本路径读取：具备 `moderation.review` 或
//!   `post.moderate` 权限 + **非空理由** + **审计**（不可删除）；
//! - 任何缺失（权限/理由）一律默认拒绝，不返回内容；
//! - 本路径是 M4 隐藏内容投影 handler 的统一前置；投影本身只暴露受控字段。

use crate::audit::AuditEntry;
use crate::authz::decision::{Decision, DenyReason, AUTHZ_POLICY_VERSION};
use crate::authz::enforce::authorize_action;
use crate::db::DatabasePool;
use crate::error::AppError;

/// 理由最大长度（抑制日志/审计膨胀）。
pub const HIDDEN_READ_REASON_MAX: usize = 200;

/// 隐藏内容读取错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HiddenReadError {
    /// 理由为空或全空白。
    MissingReason,
    /// 理由超长（> [`HIDDEN_READ_REASON_MAX`]）。
    ReasonTooLong,
    /// 理由含控制字符。
    ReasonInvalid,
    /// 授权拒绝（携带原因）。
    Denied(DenyReason),
    Database(String),
}

impl std::fmt::Display for HiddenReadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HiddenReadError::MissingReason => {
                write!(f, "hidden content read requires an explicit reason")
            }
            HiddenReadError::ReasonTooLong => {
                write!(f, "reason exceeds {} characters", HIDDEN_READ_REASON_MAX)
            }
            HiddenReadError::ReasonInvalid => write!(f, "reason contains control characters"),
            HiddenReadError::Denied(reason) => write!(f, "hidden content read denied: {reason}"),
            HiddenReadError::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for HiddenReadError {}

/// 校验读取理由：非空、≤200 字符、无控制字符。
pub fn validate_hidden_read_reason(reason: &str) -> Result<(), HiddenReadError> {
    if reason.trim().is_empty() {
        return Err(HiddenReadError::MissingReason);
    }
    if reason.chars().count() > HIDDEN_READ_REASON_MAX {
        return Err(HiddenReadError::ReasonTooLong);
    }
    if reason.chars().any(|c| c.is_control()) {
        return Err(HiddenReadError::ReasonInvalid);
    }
    Ok(())
}

/// 显式读取隐藏内容：权限（默认拒绝）→ 理由校验 → 审计。
///
/// `permission` 必须为 `moderation.review` 或 `post.moderate`（M4 隐藏内容
/// 投影 handler 据此裁决）；审计写入 `moderation.read_hidden`（actor +
/// target + reason + policy_version + request_id），append-only 不可删除。
pub async fn require_hidden_read(
    pool: &DatabasePool,
    operator_id: &str,
    permission: &str,
    target_type: &str,
    target_id: &str,
    reason: &str,
    request_id: &str,
) -> Result<(), HiddenReadError> {
    // 1) 理由必须显式（先于权限，避免未授权者探测内容存在性）
    validate_hidden_read_reason(reason)?;

    // 2) 权限判定（默认拒绝；板块范围经聚合实时生效）
    let decision = authorize_action(pool, operator_id, permission, None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(HiddenReadError::Database)?;
    if !decision.is_allowed() {
        let denied = match decision {
            Decision::Deny { reason } => reason,
            Decision::Allow => DenyReason::DefaultDeny,
        };
        return Err(HiddenReadError::Denied(denied));
    }

    // 3) 审计（不可删除审计；M01-AUDIT-06 moderation_action）
    AuditEntry::moderation_action(
        operator_id,
        target_type,
        target_id,
        "moderation.read_hidden",
        reason,
        AUTHZ_POLICY_VERSION,
    )
    .with_request_id(request_id)
    .record(pool)
    .await
    .map_err(|e| HiddenReadError::Database(e.to_string()))?;

    Ok(())
}

/// HiddenReadError → AppError（理由类 400 / 拒绝 401-403 / DB 500）。
pub fn hidden_read_to_error(err: HiddenReadError, request_id: &str) -> AppError {
    match err {
        HiddenReadError::MissingReason
        | HiddenReadError::ReasonTooLong
        | HiddenReadError::ReasonInvalid => {
            AppError::bad_request(err.to_string(), request_id, None)
        }
        HiddenReadError::Denied(reason) => crate::authz::enforce::deny_to_error(reason, request_id),
        HiddenReadError::Database(e) => AppError::internal(e, request_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reason_validation() {
        assert_eq!(
            validate_hidden_read_reason(""),
            Err(HiddenReadError::MissingReason)
        );
        assert_eq!(
            validate_hidden_read_reason("   "),
            Err(HiddenReadError::MissingReason)
        );
        let long = "x".repeat(HIDDEN_READ_REASON_MAX + 1);
        assert_eq!(
            validate_hidden_read_reason(&long),
            Err(HiddenReadError::ReasonTooLong)
        );
        assert_eq!(
            validate_hidden_read_reason("含\x00控制字符"),
            Err(HiddenReadError::ReasonInvalid)
        );
        assert!(validate_hidden_read_reason("用户申诉复核，隐藏内容存在性核验").is_ok());
        // 恰好上限长度
        let exact = "y".repeat(HIDDEN_READ_REASON_MAX);
        assert!(validate_hidden_read_reason(&exact).is_ok());
    }
}
