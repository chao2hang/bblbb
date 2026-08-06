//! M04-POSTS-05：发布前预检（P0）。
//!
//! 发布是"最后防线"：必须在写事务前**重新读取**全部服务端权威状态，绝不信任
//! 草稿/客户端缓存的任何值：
//! 1. **账号状态**：实时聚合权限 + `post.create` 门（邮箱验证/冷静期/mute/
//!    账号状态，M03-AUTHZ-06）——发布时若账号已处罚/停用则拒绝；
//! 2. **作者当前等级**：`users.level` 重读，`visibility_level ≤ 作者等级`
//!    （防升级前缓存的高隐藏级别）；
//! 3. **板块规则**：板块存在/未删除/`is_active`/`posting_mode ∈ {normal,
//!    approval}`（readonly/closed 拒发）；
//! 4. **附件状态**：引用的附件必须存在且属于作者（`attachments` 表 M6 落地；
//!    当前发布输入尚无附件引用，无引用即通过——有引用时校验生效）；
//! 5. **access policy 结构**：level 需 min_level、paid 需 currency_id+amount
//!    且为正（复用 [`ContentAccessPolicy::validate`]）。
//!
//! 预检通过只代表"此输入可发布"；发布写路径（M04-POSTS-06）仍在同一事务内
//! 再次原子写入并处理失败回滚。

use sqlx::Either;

use crate::authz::decision::{Decision, DenyReason, AUTHZ_POLICY_VERSION};
use crate::authz::enforce::authorize_action;
use crate::content::model::ContentAccessPolicy;
use crate::db::DatabasePool;
use crate::domain::posts::AccessPolicy;

/// 发布预检输入（由草稿 + 解析后的访问策略细节构成）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPreflightInput {
    pub author_id: String,
    pub board_id: String,
    pub visibility_level: Option<u32>,
    pub access_policy: String,
    /// access_policy=level 时的门槛等级。
    pub min_level: Option<i64>,
    /// access_policy=paid 时的币种。
    pub currency_id: Option<String>,
    /// access_policy=paid 时的金额（必须为正）。
    pub amount: Option<i64>,
    /// 发布引用的附件 id（无引用时为空）。
    pub attachment_ids: Vec<String>,
}

/// 预检被阻断的原因（稳定 Display，不含原始输入回显）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishBlocked {
    /// 账号状态不允许发布（原因见内文）。
    AccountUnavailable(String),
    /// visibility_level 超过作者当前等级。
    VisibilityExceedsLevel { requested: u32, author_level: u32 },
    /// 板块不存在/已删除/停用/不允许发帖（原因见内文）。
    BoardNotAcceptingPosts(String),
    /// access policy 未知或结构非法。
    InvalidAccessPolicy(String),
    /// 附件不存在或不属于作者。
    AttachmentNotAllowed(String),
    /// 内部错误（数据库等）。
    Internal(String),
}

impl std::fmt::Display for PublishBlocked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AccountUnavailable(msg) => write!(f, "account cannot publish: {msg}"),
            Self::VisibilityExceedsLevel {
                requested,
                author_level,
            } => write!(
                f,
                "visibility_level {requested} exceeds author level {author_level}"
            ),
            Self::BoardNotAcceptingPosts(msg) => write!(f, "board does not accept posts: {msg}"),
            Self::InvalidAccessPolicy(msg) => write!(f, "access policy invalid: {msg}"),
            Self::AttachmentNotAllowed(msg) => write!(f, "attachment not allowed: {msg}"),
            Self::Internal(msg) => write!(f, "publish preflight internal: {msg}"),
        }
    }
}

impl std::error::Error for PublishBlocked {}

/// 发布前预检：全部状态**重新读取**，任一不符即阻断。
pub async fn publish_preflight(
    pool: &DatabasePool,
    input: &PublishPreflightInput,
) -> Result<(), PublishBlocked> {
    // 1) 账号状态实时门 + post.create 权限（含邮箱验证/冷静期/mute/账号状态）
    recheck_account_gate(pool, &input.author_id, &input.board_id).await?;
    // 2) 作者当前等级（visibility_level 必须 ≤ 等级）
    recheck_author_level(pool, &input.author_id, input.visibility_level).await?;
    // 3) 板块规则
    recheck_board(pool, &input.board_id).await?;
    // 4) 附件状态（引用存在 + 属于作者）
    recheck_attachments(pool, &input.attachment_ids, &input.author_id).await?;
    // 5) access policy 结构
    check_access_policy(input)
}

/// 账号状态实时门：聚合权限 + post.create（M03-AUTHZ-06 全部门）。
async fn recheck_account_gate(
    pool: &DatabasePool,
    author_id: &str,
    board_id: &str,
) -> Result<(), PublishBlocked> {
    let decision = authorize_action(
        pool,
        author_id,
        "post.create",
        Some(board_id),
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(PublishBlocked::Internal)?;
    if decision.is_allowed() {
        return Ok(());
    }
    let reason = match decision {
        Decision::Deny { reason } => reason,
        Decision::Allow => unreachable!("已在上方判定"),
    };
    let msg = match reason {
        DenyReason::AccountNotAllowed => "account status does not allow posting",
        DenyReason::EmailUnverified => "email not verified",
        DenyReason::InCooldown => "account is in the posting cooldown",
        DenyReason::Muted => "account is muted",
        DenyReason::BoardMuted => "muted in this board",
        DenyReason::MissingPermission => "post.create permission required",
        _ => "account gate denied",
    };
    Err(PublishBlocked::AccountUnavailable(msg.to_string()))
}

/// 重读作者当前等级；visibility_level（缺省 1）不得超过等级。
async fn recheck_author_level(
    pool: &DatabasePool,
    author_id: &str,
    visibility_level: Option<u32>,
) -> Result<(), PublishBlocked> {
    let level: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
            .bind(author_id)
            .fetch_optional(p)
            .await
            .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
        Either::Right(p) => sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
            .bind(author_id)
            .fetch_optional(p)
            .await
            .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
    };
    let author_level = level
        .ok_or_else(|| PublishBlocked::AccountUnavailable("author not found".to_string()))?
        .clamp(1, i64::from(u32::MAX)) as u32;
    let requested = visibility_level.unwrap_or(1);
    if requested > author_level {
        return Err(PublishBlocked::VisibilityExceedsLevel {
            requested,
            author_level,
        });
    }
    Ok(())
}

/// 重读板块规则：存在/未删除/is_active/posting_mode 允许发帖。
async fn recheck_board(pool: &DatabasePool, board_id: &str) -> Result<(), PublishBlocked> {
    let row: Option<(i64, String, Option<i64>)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT is_active, posting_mode, deleted_at FROM boards WHERE id = ?")
                .bind(board_id)
                .fetch_optional(p)
                .await
                .map_err(|e| PublishBlocked::Internal(e.to_string()))?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT is_active, posting_mode, deleted_at FROM boards WHERE id = ?")
                .bind(board_id)
                .fetch_optional(p)
                .await
                .map_err(|e| PublishBlocked::Internal(e.to_string()))?
        }
    };
    let Some((is_active, posting_mode, deleted_at)) = row else {
        return Err(PublishBlocked::BoardNotAcceptingPosts(
            "board not found".to_string(),
        ));
    };
    if deleted_at.is_some() {
        return Err(PublishBlocked::BoardNotAcceptingPosts(
            "board deleted".to_string(),
        ));
    }
    if is_active == 0 {
        return Err(PublishBlocked::BoardNotAcceptingPosts(
            "board is not active".to_string(),
        ));
    }
    match posting_mode.as_str() {
        "normal" | "approval" => Ok(()),
        "readonly" => Err(PublishBlocked::BoardNotAcceptingPosts(
            "board is read-only".to_string(),
        )),
        "closed" => Err(PublishBlocked::BoardNotAcceptingPosts(
            "board is closed".to_string(),
        )),
        other => Err(PublishBlocked::BoardNotAcceptingPosts(format!(
            "board posting_mode unknown: {other}"
        ))),
    }
}

/// 附件状态：所有引用必须存在且属于作者。
///
/// `attachments` 表随 M6 落地；当前发布输入无附件引用（空数组）即通过，
/// 表存在后本校验自动生效。
async fn recheck_attachments(
    pool: &DatabasePool,
    attachment_ids: &[String],
    author_id: &str,
) -> Result<(), PublishBlocked> {
    if attachment_ids.is_empty() {
        return Ok(());
    }
    // attachments 表 M6 才落地：当前不存在则按"无附件语义"放行（引用均为空）
    let table_exists: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'attachments'",
        )
        .fetch_one(p)
        .await
        .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM information_schema.tables WHERE table_schema = DATABASE() AND table_name = 'attachments'",
            )
            .fetch_one(p)
            .await
            .map_err(|e| PublishBlocked::Internal(e.to_string()))?
        }
    };
    if table_exists == 0 {
        return Err(PublishBlocked::AttachmentNotAllowed(
            "attachments not supported yet".to_string(),
        ));
    }
    // 存在表：校验每个引用属于作者（M6 后启用，这里保持契约可测）
    for id in attachment_ids {
        let owner: Option<String> = match pool {
            Either::Left(p) => sqlx::query_scalar("SELECT owner_id FROM attachments WHERE id = ?")
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
            Either::Right(p) => sqlx::query_scalar("SELECT owner_id FROM attachments WHERE id = ?")
                .bind(id)
                .fetch_optional(p)
                .await
                .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
        };
        match owner {
            None => {
                return Err(PublishBlocked::AttachmentNotAllowed(format!(
                    "attachment {id} not found"
                )))
            }
            Some(o) if o != author_id => {
                return Err(PublishBlocked::AttachmentNotAllowed(format!(
                    "attachment {id} belongs to another user"
                )))
            }
            _ => {}
        }
    }
    Ok(())
}

/// access policy 结构校验（复用 [`ContentAccessPolicy::validate`]）。
fn check_access_policy(input: &PublishPreflightInput) -> Result<(), PublishBlocked> {
    let kind = AccessPolicy::parse(&input.access_policy)
        .ok_or_else(|| PublishBlocked::InvalidAccessPolicy("unknown policy".to_string()))?;
    let policy = ContentAccessPolicy {
        id: String::new(),
        kind,
        min_level: input.min_level,
        currency_id: input.currency_id.clone(),
        amount: input.amount,
        reply_grant_persists: false,
        policy_version: 1,
        created_by: String::new(),
        created_at: 0,
    };
    policy
        .validate()
        .map_err(|msg| PublishBlocked::InvalidAccessPolicy(msg.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(over: impl FnOnce(&mut PublishPreflightInput)) -> PublishPreflightInput {
        let mut i = PublishPreflightInput {
            author_id: "u1".to_string(),
            board_id: "b1".to_string(),
            visibility_level: Some(1),
            access_policy: "public".to_string(),
            min_level: None,
            currency_id: None,
            amount: None,
            attachment_ids: Vec::new(),
        };
        over(&mut i);
        i
    }

    #[test]
    fn public_policy_passes() {
        assert!(check_access_policy(&input(|_| {})).is_ok());
    }

    #[test]
    fn level_policy_requires_min_level() {
        let i = input(|i| i.access_policy = "level".into());
        assert_eq!(
            check_access_policy(&i).unwrap_err(),
            PublishBlocked::InvalidAccessPolicy("level 策略必须指定 min_level".into())
        );
        let i = input(|i| {
            i.access_policy = "level".into();
            i.min_level = Some(3);
        });
        assert!(check_access_policy(&i).is_ok());
    }

    #[test]
    fn paid_policy_requires_currency_and_positive_amount() {
        let i = input(|i| {
            i.access_policy = "paid".into();
            i.currency_id = Some("usd".into());
        });
        assert_eq!(
            check_access_policy(&i).unwrap_err(),
            PublishBlocked::InvalidAccessPolicy("paid 策略必须指定 currency_id 与 amount".into())
        );
        let i = input(|i| {
            i.access_policy = "paid".into();
            i.currency_id = Some("usd".into());
            i.amount = Some(0);
        });
        assert_eq!(
            check_access_policy(&i).unwrap_err(),
            PublishBlocked::InvalidAccessPolicy("amount 必须为正".into())
        );
        let i = input(|i| {
            i.access_policy = "paid".into();
            i.currency_id = Some("usd".into());
            i.amount = Some(100);
        });
        assert!(check_access_policy(&i).is_ok());
    }

    #[test]
    fn unknown_policy_rejected() {
        let i = input(|i| i.access_policy = "private".into());
        assert_eq!(
            check_access_policy(&i).unwrap_err(),
            PublishBlocked::InvalidAccessPolicy("unknown policy".into())
        );
    }
}
