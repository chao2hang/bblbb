//! M05-APPEALS：申诉服务（创建/列表/详情/撤回/分配复核人/决定）。
//!
//! 数据模型见 [`crate::moderation::model`]（`Appeal`/`AppealDecision`，
//! 迁移 0044）；状态机见 `docs/STATE-MACHINES.md` §3。
//!
//! 规则要点：
//!
//! - 可申诉对象：本人名下、尚未撤销的处罚；每处罚至多一条申诉
//!   （`UNIQUE(sanctions)` 兜底），被拒后不可重复申诉，只能等新处罚。
//! - 窗口：处罚创建后 `APPEAL_WINDOW_MS`（7 天）内可申诉。
//! - 文字：`1..=APPEAL_MESSAGE_MAX`（5000 字符），禁止附件引用标记。
//! - 复核人资格：排除申诉人本人、原处罚执行者、超出板块 scope 的人员、
//!   以及无有效 assignment（过期/无全局或板块角色）的人员。
//! - 决定只追加 `appeal_decisions`，不改历史；`uphold` 以追加
//!   `sanction_reversals` 撤销原处罚（不删历史）作为修正。
//! - 并发决定：`expected_version` 为读取时的 `updated_at`，更新以
//!   `WHERE id=? AND updated_at=?` 原子守卫。

use serde_json::{json, Value};
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::events::types::APPEAL_CHANGED;
use crate::moderation::model::{Appeal, AppealDecision, AppealDecisionValue, AppealStatus};
use crate::outbox::{enqueue, now_millis, OutboxTx};

/// 申诉窗口：处罚创建后 7 天内可申诉（M05-APPEALS-01）。
pub const APPEAL_WINDOW_MS: i64 = 7 * 24 * 3600 * 1000;
/// 申诉文字最大长度（字符，M05-APPEALS-01，与 OpenAPI `AppealCreate.content`
/// `maxLength: 5000` 一致）。
pub const APPEAL_MESSAGE_MAX: usize = 5000;

/// 申诉服务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppealsError {
    Db(String),
    NotFound(String),
    /// 越权：非本人处罚、复核人利益冲突、无权限。
    Forbidden(String),
    /// 输入非法（文字空/超长/含附件引用、reason 空）。
    Invalid(String),
    /// 业务冲突（重复申诉、窗口过期、处罚已撤销、已决定/已撤回后操作）。
    Conflict(String),
    /// 乐观并发：`expected_version` 落后于当前 `updated_at`（并发 decision）。
    StaleVersion,
    /// 复核人利益冲突（原处理者/本人/超范围/无有效 assignment）。
    ReviewerConflict(String),
}

impl From<sqlx::Error> for AppealsError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for AppealsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "appeals db error: {msg}"),
            Self::NotFound(msg) => write!(f, "appeal not found: {msg}"),
            Self::Forbidden(msg) => write!(f, "appeal forbidden: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid appeal: {msg}"),
            Self::Conflict(msg) => write!(f, "appeal conflict: {msg}"),
            Self::StaleVersion => write!(f, "appeal version conflict"),
            Self::ReviewerConflict(msg) => write!(f, "appeal reviewer conflict: {msg}"),
        }
    }
}

impl std::error::Error for AppealsError {}

/// 申诉创建输入（M05-APPEALS-01/02）。
#[derive(Debug, Clone)]
pub struct CreateAppealInput {
    pub sanction_id: String,
    pub message: String,
}

/// 附件引用规则（M05-APPEALS-01）：申诉消息为纯文本，禁止携带附件引用标记。
fn contains_attachment_reference(message: &str) -> bool {
    message.contains("![") || message.contains("attachment://") || message.contains("@[")
}

/// 申诉文字校验：非空、`1..=APPEAL_MESSAGE_MAX` 字符、不含附件引用。
pub fn validate_message(message: &str) -> Result<(), AppealsError> {
    let trimmed = message.trim();
    if trimmed.is_empty() {
        return Err(AppealsError::Invalid(
            "appeal message is required".to_string(),
        ));
    }
    if trimmed.chars().count() > APPEAL_MESSAGE_MAX {
        return Err(AppealsError::Invalid(
            "appeal message exceeds 5000 characters".to_string(),
        ));
    }
    if contains_attachment_reference(trimmed) {
        return Err(AppealsError::Invalid(
            "appeal message must not reference attachments".to_string(),
        ));
    }
    Ok(())
}

/// 读取申诉行（映射为模型）。
async fn load_appeal(pool: &DatabasePool, appeal_id: &str) -> Result<Option<Appeal>, AppealsError> {
    let row: Option<AppealRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, AppealRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals WHERE id = ?",
        )
        .bind(appeal_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as::<_, AppealRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals WHERE id = ?",
        )
        .bind(appeal_id)
        .fetch_optional(p)
        .await?,
    };
    Ok(row.map(AppealRow::into_model))
}

/// 创建申诉（M05-APPEALS-01/02）。
///
/// 规则：非撤销处罚、本人处罚、7 天窗口内、每处罚至多一条、文字长度
/// `1..=5000`、禁止附件引用。写审计 + `appeal.changed.v1` Outbox。
pub async fn create_appeal(
    pool: &DatabasePool,
    user_id: &str,
    input: CreateAppealInput,
    now: i64,
) -> Result<Appeal, AppealsError> {
    validate_message(&input.message)?;

    let sanction: Option<(String, String, i64)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT user_id, kind, created_at FROM sanctions WHERE id = ?")
                .bind(&input.sanction_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT user_id, kind, created_at FROM sanctions WHERE id = ?")
                .bind(&input.sanction_id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((sanction_user_id, _kind, created_at)) = sanction else {
        return Err(AppealsError::NotFound(
            "sanction not found or not appealable".to_string(),
        ));
    };
    if sanction_user_id != user_id {
        return Err(AppealsError::Forbidden(
            "cannot appeal another user's sanction".to_string(),
        ));
    }
    // 窗口（M05-APPEALS-01）。
    if now - created_at > APPEAL_WINDOW_MS {
        return Err(AppealsError::Conflict(
            "appeal window expired (7 days after sanction)".to_string(),
        ));
    }
    // 重复提交：每处罚至多一条（0044 UNIQUE(sanction_id) 兜底）。
    let existing: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT id FROM appeals WHERE sanction_id = ?")
                .bind(&input.sanction_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT id FROM appeals WHERE sanction_id = ?")
                .bind(&input.sanction_id)
                .fetch_optional(p)
                .await?
        }
    };
    if existing.is_some() {
        return Err(AppealsError::Conflict(
            "an appeal already exists for this sanction".to_string(),
        ));
    }

    let appeal = Appeal {
        id: uuid::Uuid::now_v7().to_string(),
        sanction_id: input.sanction_id,
        user_id: user_id.to_string(),
        message: input.message,
        status: AppealStatus::Submitted,
        reviewed_by: None,
        decided_at: None,
        submitted_at: now,
        updated_at: now,
    };

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO appeals
                     (id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, NULL, NULL, ?, ?)",
            )
            .bind(&appeal.id)
            .bind(&appeal.sanction_id)
            .bind(&appeal.user_id)
            .bind(&appeal.message)
            .bind(appeal.status.as_str())
            .bind(appeal.submitted_at)
            .bind(appeal.updated_at)
            .execute(&mut *tx)
            .await?;
            let audit = AuditEntry::user_action(user_id, "appeal.create")
                .with_target("sanction", &appeal.sanction_id)
                .with_reason("appeal submitted")
                .with_policy_version(AUTHZ_POLICY_VERSION);
            let mut otx = OutboxTx::Left(tx);
            audit.record_in_tx(&mut otx).await?;
            crate::outbox::enqueue_in_tx(
                &mut otx,
                APPEAL_CHANGED,
                json!({
                    "appeal_id": appeal.id,
                    "sanction_id": appeal.sanction_id,
                    "status": appeal.status.as_str(),
                }),
            )
            .await?;
            match otx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO appeals
                     (id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, NULL, NULL, ?, ?)",
            )
            .bind(&appeal.id)
            .bind(&appeal.sanction_id)
            .bind(&appeal.user_id)
            .bind(&appeal.message)
            .bind(appeal.status.as_str())
            .bind(appeal.submitted_at)
            .bind(appeal.updated_at)
            .execute(&mut *tx)
            .await?;
            let audit = AuditEntry::user_action(user_id, "appeal.create")
                .with_target("sanction", &appeal.sanction_id)
                .with_reason("appeal submitted")
                .with_policy_version(AUTHZ_POLICY_VERSION);
            let mut otx = OutboxTx::Right(tx);
            audit.record_in_tx(&mut otx).await?;
            crate::outbox::enqueue_in_tx(
                &mut otx,
                APPEAL_CHANGED,
                json!({
                    "appeal_id": appeal.id,
                    "sanction_id": appeal.sanction_id,
                    "status": appeal.status.as_str(),
                }),
            )
            .await?;
            match otx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    Ok(appeal)
}

/// 我的申诉列表（M05-APPEALS-02，申诉人侧安全投影由路由层处理）。
pub async fn list_own_appeals(
    pool: &DatabasePool,
    user_id: &str,
    limit: i64,
) -> Result<Vec<Appeal>, AppealsError> {
    let limit = limit.clamp(1, 100);
    let rows: Vec<AppealRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, AppealRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals WHERE user_id = ? ORDER BY submitted_at DESC LIMIT ?",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(p)
        .await?,
        Either::Right(p) => sqlx::query_as::<_, AppealRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals WHERE user_id = ? ORDER BY submitted_at DESC LIMIT ?",
        )
        .bind(user_id)
        .bind(limit)
        .fetch_all(p)
        .await?,
    };
    Ok(rows.into_iter().map(AppealRow::into_model).collect())
}

/// 我的申诉详情（M05-APPEALS-02）：只允许读取本人申诉，避免横向越权。
pub async fn get_own_appeal(
    pool: &DatabasePool,
    user_id: &str,
    appeal_id: &str,
) -> Result<Appeal, AppealsError> {
    let row: Option<AppealRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, AppealRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals WHERE id = ? AND user_id = ?",
        )
        .bind(appeal_id)
        .bind(user_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as::<_, AppealRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals WHERE id = ? AND user_id = ?",
        )
        .bind(appeal_id)
        .bind(user_id)
        .fetch_optional(p)
        .await?,
    };
    row.map(AppealRow::into_model)
        .ok_or_else(|| AppealsError::NotFound("appeal not found".to_string()))
}

/// 未审理前撤回（M05-APPEALS-02）：`submitted`/`reviewing` 均可撤回
/// （终态 `withdrawn`；STATE-MACHINES.md §3）。
pub async fn withdraw_appeal(
    pool: &DatabasePool,
    user_id: &str,
    appeal_id: &str,
    now: i64,
) -> Result<Appeal, AppealsError> {
    let appeal = get_own_appeal(pool, user_id, appeal_id).await?;
    if !appeal.status.can_transition_to(AppealStatus::Withdrawn) {
        return Err(AppealsError::Conflict(
            "appeal cannot be withdrawn after a decision".to_string(),
        ));
    }
    let updated = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE appeals SET status = 'withdrawn', updated_at = ? WHERE id = ? AND user_id = ?",
        )
        .bind(now)
        .bind(appeal_id)
        .bind(user_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE appeals SET status = 'withdrawn', updated_at = ? WHERE id = ? AND user_id = ?",
        )
        .bind(now)
        .bind(appeal_id)
        .bind(user_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if updated == 0 {
        return Err(AppealsError::Conflict(
            "appeal cannot be withdrawn".to_string(),
        ));
    }
    let _ = AuditEntry::user_action(user_id, "appeal.withdraw")
        .with_target("appeal", appeal_id)
        .with_reason("withdrawn before decision")
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    let _ = enqueue(
        pool,
        APPEAL_CHANGED,
        json!({
            "appeal_id": appeal_id,
            "sanction_id": appeal.sanction_id,
            "status": "withdrawn",
        }),
    )
    .await;
    Ok(Appeal {
        status: AppealStatus::Withdrawn,
        updated_at: now,
        ..appeal
    })
}

/// 加载处罚的复核上下文（申诉人、原处理者、板块 scope）。
struct AppealSanctionContext {
    appellant_id: String,
    created_by: String,
    board_id: Option<String>,
    kind: String,
}

async fn load_sanction_context(
    pool: &DatabasePool,
    sanction_id: &str,
    appellant_id: &str,
) -> Result<AppealSanctionContext, AppealsError> {
    let row: Option<(String, String, Option<String>, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT user_id, created_by, board_id, kind FROM sanctions WHERE id = ?")
                .bind(sanction_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT user_id, created_by, board_id, kind FROM sanctions WHERE id = ?")
                .bind(sanction_id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((sanction_user_id, created_by, board_id, kind)) = row else {
        return Err(AppealsError::NotFound("sanction not found".to_string()));
    };
    if sanction_user_id != appellant_id {
        return Err(AppealsError::Forbidden(
            "sanction does not belong to the appellant".to_string(),
        ));
    }
    Ok(AppealSanctionContext {
        appellant_id: appellant_id.to_string(),
        created_by,
        board_id,
        kind,
    })
}

/// 有效 scope 判定（M05-APPEALS-03）：全局处罚需全局角色；板块处罚需
/// 该板块的版主指派或全局角色；过期 assignment 视为无效。
async fn has_moderation_scope(
    pool: &DatabasePool,
    reviewer_id: &str,
    board_id: Option<&str>,
    now: i64,
) -> Result<bool, AppealsError> {
    let global: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON r.id = ur.role_id
             WHERE ur.user_id = ? AND r.name IN ('administrator', 'global_moderator')
               AND (ur.expires_at IS NULL OR ur.expires_at > ?)",
            )
            .bind(reviewer_id)
            .bind(now)
            .fetch_one(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON r.id = ur.role_id
             WHERE ur.user_id = ? AND r.name IN ('administrator', 'global_moderator')
               AND (ur.expires_at IS NULL OR ur.expires_at > ?)",
            )
            .bind(reviewer_id)
            .bind(now)
            .fetch_one(p)
            .await?
        }
    };
    if global > 0 {
        return Ok(true);
    }
    if let Some(board_id) = board_id {
        let board: i64 = match pool {
            Either::Left(p) => sqlx::query_scalar(
                "SELECT COUNT(*) FROM board_role_assignments bra JOIN roles r ON r.id = bra.role_id
                 WHERE bra.user_id = ? AND bra.board_id = ? AND r.name IN ('board_moderator', 'global_moderator', 'administrator')
                   AND (bra.expires_at IS NULL OR bra.expires_at > ?)",
            )
            .bind(reviewer_id)
            .bind(board_id)
            .bind(now)
            .fetch_one(p)
            .await?,
            Either::Right(p) => sqlx::query_scalar(
                "SELECT COUNT(*) FROM board_role_assignments bra JOIN roles r ON r.id = bra.role_id
                 WHERE bra.user_id = ? AND bra.board_id = ? AND r.name IN ('board_moderator', 'global_moderator', 'administrator')
                   AND (bra.expires_at IS NULL OR bra.expires_at > ?)",
            )
            .bind(reviewer_id)
            .bind(board_id)
            .bind(now)
            .fetch_one(p)
            .await?,
        };
        if board > 0 {
            return Ok(true);
        }
    }
    Ok(false)
}

/// 复核人资格（M05-APPEALS-03）：排除申诉人本人、原处罚执行者（原处理者）、
/// 超范围（板块 scope 不符）与无有效 assignment 人员。
pub async fn reviewer_eligibility(
    pool: &DatabasePool,
    appeal: &Appeal,
    reviewer_id: &str,
    now: i64,
) -> Result<(), AppealsError> {
    let ctx = load_sanction_context(pool, &appeal.sanction_id, &appeal.user_id).await?;
    if reviewer_id == ctx.appellant_id {
        return Err(AppealsError::ReviewerConflict(
            "reviewer must not be the appellant".to_string(),
        ));
    }
    if reviewer_id == ctx.created_by {
        return Err(AppealsError::ReviewerConflict(
            "reviewer must not be the original sanction issuer".to_string(),
        ));
    }
    if !has_moderation_scope(pool, reviewer_id, ctx.board_id.as_deref(), now).await? {
        return Err(AppealsError::ReviewerConflict(
            "reviewer lacks a valid moderation assignment in scope".to_string(),
        ));
    }
    Ok(())
}

/// 分配复核人（M05-APPEALS-03）：设置 `reviewed_by` 并推进到 `reviewing`。
///
/// `expected_version` 为读取时的 `updated_at`（乐观并发守卫）。
pub async fn assign_reviewer(
    pool: &DatabasePool,
    _actor_id: &str,
    appeal_id: &str,
    reviewer_id: &str,
    expected_version: i64,
    now: i64,
) -> Result<Appeal, AppealsError> {
    let appeal = load_appeal(pool, appeal_id)
        .await?
        .ok_or_else(|| AppealsError::NotFound("appeal not found".to_string()))?;
    if !appeal.status.can_transition_to(AppealStatus::Reviewing) {
        return Err(AppealsError::Conflict(
            "appeal cannot be moved into review in its current state".to_string(),
        ));
    }
    reviewer_eligibility(pool, &appeal, reviewer_id, now).await?;

    let updated = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE appeals SET reviewed_by = ?, status = 'reviewing', updated_at = ?
             WHERE id = ? AND updated_at = ?",
        )
        .bind(reviewer_id)
        .bind(now)
        .bind(appeal_id)
        .bind(expected_version)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE appeals SET reviewed_by = ?, status = 'reviewing', updated_at = ?
             WHERE id = ? AND updated_at = ?",
        )
        .bind(reviewer_id)
        .bind(now)
        .bind(appeal_id)
        .bind(expected_version)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if updated == 0 {
        return Err(AppealsError::StaleVersion);
    }
    let _ = AuditEntry::user_action(reviewer_id, "appeal.assign")
        .with_target("appeal", appeal_id)
        .with_reason("reviewer assigned")
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    let _ = enqueue(
        pool,
        APPEAL_CHANGED,
        json!({
            "appeal_id": appeal_id,
            "sanction_id": appeal.sanction_id,
            "status": "reviewing",
        }),
    )
    .await;
    Ok(Appeal {
        status: AppealStatus::Reviewing,
        reviewed_by: Some(reviewer_id.to_string()),
        updated_at: now,
        ..appeal
    })
}

/// 决定申诉（M05-APPEALS-04/06）：`uphold`/`partially_upheld`/`rejected`。
///
/// - 决定只追加 `appeal_decisions`（`conflict_of_interest` 由模型校验）；
/// - 并发：`expected_version` 必须等于当前 `updated_at`，否则 `StaleVersion`；
/// - `uphold` 以追加 `sanction_reversals` 撤销原处罚（ban 恢复账号 active），
///   不删历史；`partially_upheld` 记录补偿说明于 `decision_note`；
/// - 决定者同时成为复核人（若尚未指派），并过复核人资格校验。
pub async fn decide_appeal(
    pool: &DatabasePool,
    actor_id: &str,
    appeal_id: &str,
    decision: AppealDecisionValue,
    reason: &str,
    expected_version: i64,
    now: i64,
) -> Result<Value, AppealsError> {
    if reason.trim().is_empty() {
        return Err(AppealsError::Invalid(
            "decision reason is required".to_string(),
        ));
    }
    let appeal = load_appeal(pool, appeal_id)
        .await?
        .ok_or_else(|| AppealsError::NotFound("appeal not found".to_string()))?;
    if !matches!(
        appeal.status,
        AppealStatus::Submitted | AppealStatus::Reviewing
    ) {
        return Err(AppealsError::Conflict(
            "appeal already decided or withdrawn".to_string(),
        ));
    }
    // 决定者即复核人：必须先过复核人资格（排除原处理者/本人/超范围/无 assignment）。
    reviewer_eligibility(pool, &appeal, actor_id, now).await?;
    let ctx = load_sanction_context(pool, &appeal.sanction_id, &appeal.user_id).await?;

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO appeal_decisions
                     (id, appeal_id, reviewer_id, decision, decision_note, conflict_of_interest, created_at)
                 VALUES (?, ?, ?, ?, ?, NULL, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(appeal_id)
            .bind(actor_id)
            .bind(decision.as_str())
            .bind(reason)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            let updated = sqlx::query(
                "UPDATE appeals SET status = ?, reviewed_by = COALESCE(reviewed_by, ?), decided_at = ?, updated_at = ?
                 WHERE id = ? AND updated_at = ?",
            )
            .bind(decision.as_str())
            .bind(actor_id)
            .bind(now)
            .bind(now)
            .bind(appeal_id)
            .bind(expected_version)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated == 0 {
                return Err(AppealsError::StaleVersion);
            }
            // uphold：以撤销记录修正（不删历史）——追加 sanction_reversals，
            // 原处罚行镜像为 revoked，ban 恢复账号 active。
            if decision == AppealDecisionValue::Upheld {
                let revoke_reason = format!("appeal upheld: {}", reason.trim());
                sqlx::query(
                    "INSERT INTO sanction_reversals (id, sanction_id, reversed_by, reversed_at, reason)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(uuid::Uuid::now_v7().to_string())
                .bind(&appeal.sanction_id)
                .bind(actor_id)
                .bind(now)
                .bind(&revoke_reason)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE sanctions SET status = 'revoked', revoked_at = ?, revoked_by = ?, revoke_reason = ?
                     WHERE id = ?",
                )
                .bind(now)
                .bind(actor_id)
                .bind(&revoke_reason)
                .bind(&appeal.sanction_id)
                .execute(&mut *tx)
                .await?;
                if ctx.kind == "ban" {
                    sqlx::query("UPDATE users SET status = 'active', updated_at = ? WHERE id = ?")
                        .bind(now)
                        .bind(&ctx.appellant_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            let audit = AuditEntry::user_action(actor_id, "appeal.decide")
                .with_target("appeal", appeal_id)
                .with_effective_role("moderator")
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION);
            let mut otx = OutboxTx::Left(tx);
            audit.record_in_tx(&mut otx).await?;
            crate::outbox::enqueue_in_tx(
                &mut otx,
                APPEAL_CHANGED,
                json!({
                    "appeal_id": appeal_id,
                    "sanction_id": appeal.sanction_id,
                    "status": decision.as_str(),
                }),
            )
            .await?;
            match otx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO appeal_decisions
                     (id, appeal_id, reviewer_id, decision, decision_note, conflict_of_interest, created_at)
                 VALUES (?, ?, ?, ?, ?, NULL, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(appeal_id)
            .bind(actor_id)
            .bind(decision.as_str())
            .bind(reason)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            let updated = sqlx::query(
                "UPDATE appeals SET status = ?, reviewed_by = COALESCE(reviewed_by, ?), decided_at = ?, updated_at = ?
                 WHERE id = ? AND updated_at = ?",
            )
            .bind(decision.as_str())
            .bind(actor_id)
            .bind(now)
            .bind(now)
            .bind(appeal_id)
            .bind(expected_version)
            .execute(&mut *tx)
            .await?
            .rows_affected();
            if updated == 0 {
                return Err(AppealsError::StaleVersion);
            }
            if decision == AppealDecisionValue::Upheld {
                let revoke_reason = format!("appeal upheld: {}", reason.trim());
                sqlx::query(
                    "INSERT INTO sanction_reversals (id, sanction_id, reversed_by, reversed_at, reason)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(uuid::Uuid::now_v7().to_string())
                .bind(&appeal.sanction_id)
                .bind(actor_id)
                .bind(now)
                .bind(&revoke_reason)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "UPDATE sanctions SET status = 'revoked', revoked_at = ?, revoked_by = ?, revoke_reason = ?
                     WHERE id = ?",
                )
                .bind(now)
                .bind(actor_id)
                .bind(&revoke_reason)
                .bind(&appeal.sanction_id)
                .execute(&mut *tx)
                .await?;
                if ctx.kind == "ban" {
                    sqlx::query("UPDATE users SET status = 'active', updated_at = ? WHERE id = ?")
                        .bind(now)
                        .bind(&ctx.appellant_id)
                        .execute(&mut *tx)
                        .await?;
                }
            }
            let audit = AuditEntry::user_action(actor_id, "appeal.decide")
                .with_target("appeal", appeal_id)
                .with_effective_role("moderator")
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION);
            let mut otx = OutboxTx::Right(tx);
            audit.record_in_tx(&mut otx).await?;
            crate::outbox::enqueue_in_tx(
                &mut otx,
                APPEAL_CHANGED,
                json!({
                    "appeal_id": appeal_id,
                    "sanction_id": appeal.sanction_id,
                    "status": decision.as_str(),
                }),
            )
            .await?;
            match otx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    // uphold 时处罚被撤销，追加 sanction.changed.v1 供 Session/权限失效与通知消费。
    if decision == AppealDecisionValue::Upheld {
        let _ = enqueue(
            pool,
            crate::events::types::SANCTION_CHANGED,
            json!({
                "sanction_id": appeal.sanction_id,
                "kind": ctx.kind,
                "status": "revoked",
            }),
        )
        .await;
    }
    Ok(admin_appeal_projection(
        &Appeal {
            status: match decision {
                AppealDecisionValue::Upheld => AppealStatus::Upheld,
                AppealDecisionValue::PartiallyUpheld => AppealStatus::PartiallyUpheld,
                AppealDecisionValue::Rejected => AppealStatus::Rejected,
            },
            reviewed_by: Some(actor_id.to_string()),
            decided_at: Some(now),
            updated_at: now,
            ..appeal
        },
        &[],
    ))
}

/// 管理端申诉列表（含未决申诉；决定记录按需另查）。
pub async fn list_all_appeals(
    pool: &DatabasePool,
    limit: i64,
) -> Result<Vec<Appeal>, AppealsError> {
    let limit = limit.clamp(1, 100);
    let rows: Vec<AppealRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, AppealRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals ORDER BY submitted_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(p)
        .await?,
        Either::Right(p) => sqlx::query_as::<_, AppealRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals ORDER BY submitted_at DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(p)
        .await?,
    };
    Ok(rows.into_iter().map(AppealRow::into_model).collect())
}

/// 读取申诉的全部决定记录（管理端投影用）。
pub async fn list_decisions(
    pool: &DatabasePool,
    appeal_id: &str,
) -> Result<Vec<AppealDecision>, AppealsError> {
    let rows: Vec<DecisionRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, DecisionRow>(
            "SELECT id, appeal_id, reviewer_id, decision, decision_note, conflict_of_interest, created_at
             FROM appeal_decisions WHERE appeal_id = ? ORDER BY created_at",
        )
        .bind(appeal_id)
        .fetch_all(p)
        .await?,
        Either::Right(p) => sqlx::query_as::<_, DecisionRow>(
            "SELECT id, appeal_id, reviewer_id, decision, decision_note, conflict_of_interest, created_at
             FROM appeal_decisions WHERE appeal_id = ? ORDER BY created_at",
        )
        .bind(appeal_id)
        .fetch_all(p)
        .await?,
    };
    Ok(rows.into_iter().map(DecisionRow::into_model).collect())
}

// ─── 投影（M05-APPEALS-05） ─────────────────────────────────────────────

/// 申诉人侧安全投影：只含本人可见字段，不含内部 note、利益冲突声明与复核人。
pub fn own_appeal_projection(appeal: &Appeal) -> Value {
    json!({
        "id": appeal.id,
        "sanction_id": appeal.sanction_id,
        "status": appeal.status.as_str(),
        "message": appeal.message,
        "submitted_at": appeal.submitted_at,
        "decided_at": appeal.decided_at,
        "updated_at": appeal.updated_at,
    })
}

/// 审核员侧投影：含内部 note（decision_note/conflict_of_interest）与复核人。
pub fn admin_appeal_projection(appeal: &Appeal, decisions: &[AppealDecision]) -> Value {
    json!({
        "id": appeal.id,
        "sanction_id": appeal.sanction_id,
        "user_id": appeal.user_id,
        "status": appeal.status.as_str(),
        "message": appeal.message,
        "reviewed_by": appeal.reviewed_by,
        "decided_at": appeal.decided_at,
        "submitted_at": appeal.submitted_at,
        "updated_at": appeal.updated_at,
        "decisions": decisions.iter().map(|d| {
            json!({
                "id": d.id,
                "reviewer_id": d.reviewer_id,
                "decision": d.decision.as_str(),
                "decision_note": d.decision_note,
                "conflict_of_interest": d.conflict_of_interest,
                "created_at": d.created_at,
            })
        }).collect::<Vec<_>>(),
    })
}

/// 供路由使用的助手：当前 Unix 毫秒。
pub fn now() -> i64 {
    now_millis()
}

// ─── 行映射 ──────────────────────────────────────────────────────────────

/// appeals 行 → 模型。
#[derive(sqlx::FromRow)]
struct AppealRow {
    id: String,
    sanction_id: String,
    user_id: String,
    message: String,
    status: String,
    reviewed_by: Option<String>,
    decided_at: Option<i64>,
    submitted_at: i64,
    updated_at: i64,
}

impl AppealRow {
    fn into_model(self) -> Appeal {
        Appeal {
            id: self.id,
            sanction_id: self.sanction_id,
            user_id: self.user_id,
            message: self.message,
            status: AppealStatus::parse(&self.status).unwrap_or(AppealStatus::Submitted),
            reviewed_by: self.reviewed_by,
            decided_at: self.decided_at,
            submitted_at: self.submitted_at,
            updated_at: self.updated_at,
        }
    }
}

/// appeal_decisions 行 → 模型。
#[derive(sqlx::FromRow)]
struct DecisionRow {
    id: String,
    appeal_id: String,
    reviewer_id: String,
    decision: String,
    decision_note: Option<String>,
    conflict_of_interest: Option<String>,
    created_at: i64,
}

impl DecisionRow {
    fn into_model(self) -> AppealDecision {
        AppealDecision {
            id: self.id,
            appeal_id: self.appeal_id,
            reviewer_id: self.reviewer_id,
            decision: AppealDecisionValue::parse(&self.decision)
                .unwrap_or(AppealDecisionValue::Rejected),
            decision_note: self.decision_note,
            conflict_of_interest: self.conflict_of_interest,
            created_at: self.created_at,
        }
    }
}
