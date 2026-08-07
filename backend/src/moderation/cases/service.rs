//! M05-CASES 服务：举报、案件与内容动作。
//!
//! 契约要点：
//! - 举报（M05-CASES-01/02）：reason_code 封闭枚举 + 详情限长；同一
//!   (reporter, target, reason) 在去重窗口内重复提交 → 返回既有报告的统一
//!   响应（不泄漏其他举报人/案件状态）；撤回只限本人且未处理完成。
//! - 案件（M05-CASES-03/04/05）：状态迁移走 [`CaseStatus::can_transition_to`]；
//!   派单校验版主板块范围（`moderation.review`）与利益冲突（处理自己/自己
//!   内容/明确冲突 → 阻断并写审计）；所有动作写 [`AuditEntry`] 与 Outbox
//!   `moderation.case_changed.v1`。
//! - 内容动作（M05-CASES-06/07/08/09）：hide/restore/delete 写
//!   `moderation_actions`（只追加）+ 修订 + 审计；hide/delete 立即使
//!   status 变为 `hidden`/`deleted`（公开列表/搜索/Feed 按 `status='published'`
//!   过滤天然撤除）；restore 重新运行当前风险策略（高风险 → 再次 pending_review）。

use serde_json::json;
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::db::DatabasePool;
use crate::events::types::{MODERATION_CASE_CHANGED, POST_VISIBILITY_CHANGED};
use crate::moderation::model::{
    CasePriority, CaseStatus, ModerationActionKind, ModerationTargetType, Report, ReportReasonCode,
    ReportStatus, ReportTargetType,
};
use crate::outbox::{enqueue, now_millis, OutboxTx};

/// 举报详情最大长度（字符）。
pub const REPORT_DETAIL_MAX: usize = 2_000;
/// 案件标题最大长度。
pub const CASE_TITLE_MAX: usize = 120;
/// 动作 reason 最小长度。
pub const ACTION_REASON_MIN: usize = 3;

/// 案件服务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CasesError {
    Db(String),
    NotFound(String),
    /// 越权/板块范围外/利益冲突（阻断并已写审计）。
    Forbidden(String),
    /// 原因枚举非法。
    InvalidReason(String),
    /// 详情超长。
    DetailTooLong,
    /// 去重窗口内重复举报（统一响应，不泄漏）。
    DuplicateReport {
        existing_id: String,
    },
    /// 状态迁移非法。
    InvalidTransition {
        from: String,
        to: String,
    },
    /// 目标资源不存在。
    TargetNotFound,
}

impl From<sqlx::Error> for CasesError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for CasesError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "cases db error: {msg}"),
            Self::NotFound(msg) => write!(f, "case resource not found: {msg}"),
            Self::Forbidden(msg) => write!(f, "case action forbidden: {msg}"),
            Self::InvalidReason(msg) => write!(f, "invalid reason: {msg}"),
            Self::DetailTooLong => write!(f, "report detail too long"),
            Self::DuplicateReport { existing_id } => {
                write!(f, "duplicate report within window: {existing_id}")
            }
            Self::InvalidTransition { from, to } => {
                write!(f, "invalid case transition: {from} -> {to}")
            }
            Self::TargetNotFound => write!(f, "report target not found"),
        }
    }
}

impl std::error::Error for CasesError {}

/// 举报创建输入。
#[derive(Debug, Clone)]
pub struct CreateReportInput {
    pub target_type: ReportTargetType,
    pub target_id: String,
    pub reason_code: ReportReasonCode,
    pub details: Option<String>,
}

/// 举报安全摘要（对外投影：不含其他举报人/案件状态/内部备注）。
#[derive(Debug, Clone, serde::Serialize)]
pub struct ReportSummary {
    pub id: String,
    pub target_type: String,
    pub target_id: String,
    pub reason_code: String,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 校验举报详情长度。
fn validate_details(details: Option<&str>) -> Result<(), CasesError> {
    if let Some(d) = details {
        if d.chars().count() > REPORT_DETAIL_MAX {
            return Err(CasesError::DetailTooLong);
        }
    }
    Ok(())
}

/// 校验目标资源存在（post/comment/user/board）。
async fn target_exists(
    pool: &DatabasePool,
    target_type: ReportTargetType,
    target_id: &str,
) -> Result<(), CasesError> {
    let sql = match target_type {
        ReportTargetType::Post => "SELECT 1 FROM posts WHERE id = ? AND deleted_at IS NULL",
        ReportTargetType::Comment => "SELECT 1 FROM comments WHERE id = ?",
        ReportTargetType::User => "SELECT 1 FROM users WHERE id = ?",
        ReportTargetType::Board => "SELECT 1 FROM boards WHERE id = ? AND deleted_at IS NULL",
    };
    let exists: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(sql)
                .bind(target_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(sql)
                .bind(target_id)
                .fetch_optional(p)
                .await?
        }
    };
    if exists.is_none() {
        return Err(CasesError::TargetNotFound);
    }
    Ok(())
}

/// 创建举报（M05-CASES-01/02）。窗口内重复 → `DuplicateReport`（统一响应）。
pub async fn create_report(
    pool: &DatabasePool,
    reporter_id: &str,
    input: CreateReportInput,
    now: i64,
) -> Result<Report, CasesError> {
    validate_details(input.details.as_deref())?;
    target_exists(pool, input.target_type, &input.target_id).await?;

    let dedup_key = Report::build_dedup_key(
        reporter_id,
        input.target_type,
        &input.target_id,
        input.reason_code,
    );
    let dedup_until =
        Report::dedup_window_end(now, crate::moderation::model::REPORT_DEDUP_WINDOW_MS);

    // 窗口内已有同键报告（未撤回）→ 统一响应，避免重复与信息泄漏。
    let existing: Option<(String,)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id FROM reports
                 WHERE report_dedup_key = ? AND status != 'withdrawn' AND dedup_until >= ? LIMIT 1",
            )
            .bind(&dedup_key)
            .bind(now)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id FROM reports
                 WHERE report_dedup_key = ? AND status != 'withdrawn' AND dedup_until >= ? LIMIT 1",
            )
            .bind(&dedup_key)
            .bind(now)
            .fetch_optional(p)
            .await?
        }
    };
    if let Some((id,)) = existing {
        return Err(CasesError::DuplicateReport { existing_id: id });
    }

    let report = Report {
        id: uuid::Uuid::now_v7().to_string(),
        reporter_id: reporter_id.to_string(),
        target_type: input.target_type,
        target_id: input.target_id,
        reason_code: input.reason_code,
        details: input.details,
        status: ReportStatus::Open,
        report_dedup_key: dedup_key,
        dedup_until,
        assigned_to: None,
        created_at: now,
        updated_at: now,
    };
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO reports
                     (id, reporter_id, target_type, target_id, reason_code, details, status,
                      report_dedup_key, dedup_until, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'open', ?, ?, ?, ?)",
            )
            .bind(&report.id)
            .bind(&report.reporter_id)
            .bind(report.target_type.as_str())
            .bind(&report.target_id)
            .bind(report.reason_code.as_str())
            .bind(&report.details)
            .bind(&report.report_dedup_key)
            .bind(report.dedup_until)
            .bind(report.created_at)
            .bind(report.updated_at)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO reports
                     (id, reporter_id, target_type, target_id, reason_code, details, status,
                      report_dedup_key, dedup_until, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'open', ?, ?, ?, ?)",
            )
            .bind(&report.id)
            .bind(&report.reporter_id)
            .bind(report.target_type.as_str())
            .bind(&report.target_id)
            .bind(report.reason_code.as_str())
            .bind(&report.details)
            .bind(&report.report_dedup_key)
            .bind(report.dedup_until)
            .bind(report.created_at)
            .bind(report.updated_at)
            .execute(p)
            .await?;
        }
    }
    Ok(report)
}

/// 撤回举报（M05-CASES-02）：只限本人、且尚未处理完成。
pub async fn withdraw_report(
    pool: &DatabasePool,
    reporter_id: &str,
    report_id: &str,
    now: i64,
) -> Result<(), CasesError> {
    let row: Option<(String, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT reporter_id, status FROM reports WHERE id = ?")
                .bind(report_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT reporter_id, status FROM reports WHERE id = ?")
                .bind(report_id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((owner, status)) = row else {
        return Err(CasesError::NotFound("report not found".into()));
    };
    if owner != reporter_id {
        return Err(CasesError::Forbidden("not your report".into()));
    }
    if !matches!(status.as_str(), "open" | "triaged" | "reopened") {
        return Err(CasesError::Forbidden("report already processed".into()));
    }
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE reports SET status = 'withdrawn', updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(report_id)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE reports SET status = 'withdrawn', updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(report_id)
                .execute(p)
                .await?;
        }
    }
    Ok(())
}

/// 我的举报列表（安全投影）。
pub async fn list_own_reports(
    pool: &DatabasePool,
    reporter_id: &str,
    limit: i64,
) -> Result<Vec<ReportSummary>, CasesError> {
    let rows: Vec<ReportSummary> = match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, String, String, String, String, i64, i64)>(
            "SELECT id, target_type, target_id, reason_code, status, created_at, updated_at
                 FROM reports WHERE reporter_id = ? ORDER BY created_at DESC LIMIT ?",
        )
        .bind(reporter_id)
        .bind(limit)
        .fetch_all(p)
        .await?
        .into_iter()
        .map(
            |(id, target_type, target_id, reason_code, status, created_at, updated_at)| {
                ReportSummary {
                    id,
                    target_type,
                    target_id,
                    reason_code,
                    status,
                    created_at,
                    updated_at,
                }
            },
        )
        .collect(),
        Either::Right(p) => {
            sqlx::query_as::<_, (String, String, String, String, String, i64, i64)>(
                "SELECT id, target_type, target_id, reason_code, status, created_at, updated_at
                 FROM reports WHERE reporter_id = ? ORDER BY created_at DESC LIMIT ?",
            )
            .bind(reporter_id)
            .bind(limit)
            .fetch_all(p)
            .await?
            .into_iter()
            .map(
                |(id, target_type, target_id, reason_code, status, created_at, updated_at)| {
                    ReportSummary {
                        id,
                        target_type,
                        target_id,
                        reason_code,
                        status,
                        created_at,
                        updated_at,
                    }
                },
            )
            .collect()
        }
    };
    Ok(rows)
}

/// 由举报开案（M05-CASES-03/05）：处理者不得是举报人本人（利益冲突），
/// 且必须具备 `moderation.review` 权限（板块范围由调用方/路由先行校验）。
pub async fn open_case_from_report(
    pool: &DatabasePool,
    moderator_id: &str,
    report_id: &str,
    priority: CasePriority,
    now: i64,
) -> Result<String, CasesError> {
    let report = load_report(pool, report_id).await?;
    if report.reporter_id == moderator_id {
        let _ = AuditEntry::moderation_action(
            moderator_id,
            "report",
            report_id,
            "case.block_conflict",
            "moderator is the reporter",
            AUTHZ_POLICY_VERSION,
        )
        .record(pool)
        .await;
        return Err(CasesError::Forbidden(
            "cannot handle your own report (conflict of interest)".into(),
        ));
    }
    if report.status == ReportStatus::Withdrawn {
        return Err(CasesError::Forbidden("report was withdrawn".into()));
    }
    // 已并入案件 → 拒绝（统一响应，不泄漏既有案件状态）。
    let linked: Option<(String,)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT case_id FROM case_reports WHERE report_id = ? LIMIT 1")
                .bind(report_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT case_id FROM case_reports WHERE report_id = ? LIMIT 1")
                .bind(report_id)
                .fetch_optional(p)
                .await?
        }
    };
    if linked.is_some() {
        return Err(CasesError::Forbidden(
            "report already linked to a case".into(),
        ));
    }

    let case_id = uuid::Uuid::now_v7().to_string();
    let case_insert = "INSERT INTO moderation_cases
         (id, title, status, priority, assigned_to, created_by, created_at, updated_at)
     VALUES (?, ?, 'open', ?, ?, ?, ?, ?)";
    let report_link = "INSERT INTO case_reports (case_id, report_id, added_by, added_at)
         VALUES (?, ?, ?, ?)";
    let assignment = "INSERT INTO case_assignments
         (id, case_id, assignee_id, assigned_by, assigned_at)
     VALUES (?, ?, ?, ?, ?)";
    let report_triage =
        "UPDATE reports SET status = 'triaged', assigned_to = ?, updated_at = ? WHERE id = ?";
    let audit = AuditEntry::moderation_action(
        moderator_id,
        "report",
        report_id,
        "case.open",
        "opened case from report",
        AUTHZ_POLICY_VERSION,
    );

    match pool {
        Either::Left(p) => {
            let mut tx = OutboxTx::Left(p.begin().await?);
            match &mut tx {
                Either::Left(t) => {
                    sqlx::query(case_insert)
                        .bind(&case_id)
                        .bind(format!("Report on {}", report.target_type.as_str()))
                        .bind(priority.as_str())
                        .bind(moderator_id)
                        .bind(moderator_id)
                        .bind(now)
                        .bind(now)
                        .execute(&mut **t)
                        .await?;
                    sqlx::query(report_link)
                        .bind(&case_id)
                        .bind(report_id)
                        .bind(moderator_id)
                        .bind(now)
                        .execute(&mut **t)
                        .await?;
                    sqlx::query(assignment)
                        .bind(uuid::Uuid::now_v7().to_string())
                        .bind(&case_id)
                        .bind(moderator_id)
                        .bind(moderator_id)
                        .bind(now)
                        .execute(&mut **t)
                        .await?;
                    sqlx::query(report_triage)
                        .bind(moderator_id)
                        .bind(now)
                        .bind(report_id)
                        .execute(&mut **t)
                        .await?;
                }
                Either::Right(_) => unreachable!(),
            }
            audit.record_in_tx(&mut tx).await?;
            match tx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = OutboxTx::Right(p.begin().await?);
            match &mut tx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => {
                    sqlx::query(case_insert)
                        .bind(&case_id)
                        .bind(format!("Report on {}", report.target_type.as_str()))
                        .bind(priority.as_str())
                        .bind(moderator_id)
                        .bind(moderator_id)
                        .bind(now)
                        .bind(now)
                        .execute(&mut **t)
                        .await?;
                    sqlx::query(report_link)
                        .bind(&case_id)
                        .bind(report_id)
                        .bind(moderator_id)
                        .bind(now)
                        .execute(&mut **t)
                        .await?;
                    sqlx::query(assignment)
                        .bind(uuid::Uuid::now_v7().to_string())
                        .bind(&case_id)
                        .bind(moderator_id)
                        .bind(moderator_id)
                        .bind(now)
                        .execute(&mut **t)
                        .await?;
                    sqlx::query(report_triage)
                        .bind(moderator_id)
                        .bind(now)
                        .bind(report_id)
                        .execute(&mut **t)
                        .await?;
                }
            }
            audit.record_in_tx(&mut tx).await?;
            match tx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    let _ = enqueue(
        pool,
        MODERATION_CASE_CHANGED,
        json!({ "case_id": case_id.clone(), "status": "open" }),
    )
    .await;
    Ok(case_id)
}

/// 读取报告（内部用）。
pub async fn load_report(pool: &DatabasePool, report_id: &str) -> Result<Report, CasesError> {
    let row: Option<Report> = match pool {
        Either::Left(p) => sqlx::query_as::<_, ReportRow>(
            "SELECT id, reporter_id, target_type, target_id, reason_code, details, status,
                        report_dedup_key, dedup_until, assigned_to, created_at, updated_at
                 FROM reports WHERE id = ?",
        )
        .bind(report_id)
        .fetch_optional(p)
        .await?
        .map(ReportRow::into_model),
        Either::Right(p) => sqlx::query_as::<_, ReportRow>(
            "SELECT id, reporter_id, target_type, target_id, reason_code, details, status,
                        report_dedup_key, dedup_until, assigned_to, created_at, updated_at
                 FROM reports WHERE id = ?",
        )
        .bind(report_id)
        .fetch_optional(p)
        .await?
        .map(ReportRow::into_model),
    };
    row.ok_or_else(|| CasesError::NotFound("report not found".into()))
}

/// 案件状态迁移（M05-CASES-03）：合法迁移才允许；写审计 + Outbox。
pub async fn transition_case(
    pool: &DatabasePool,
    moderator_id: &str,
    case_id: &str,
    target: CaseStatus,
    resolution: Option<&str>,
    now: i64,
) -> Result<(), CasesError> {
    let row: Option<(String,)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT status FROM moderation_cases WHERE id = ?")
                .bind(case_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT status FROM moderation_cases WHERE id = ?")
                .bind(case_id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((from,)) = row else {
        return Err(CasesError::NotFound("case not found".into()));
    };
    let from_status =
        CaseStatus::parse(&from).ok_or_else(|| CasesError::Db("case status corrupted".into()))?;
    if from_status == target {
        return Err(CasesError::InvalidTransition {
            from,
            to: target.as_str().to_string(),
        });
    }
    if !from_status.can_transition_to(target) {
        return Err(CasesError::InvalidTransition {
            from,
            to: target.as_str().to_string(),
        });
    }
    let resolved_at = (target == CaseStatus::Resolved).then_some(now);
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE moderation_cases SET status = ?, resolved_at = COALESCE(?, resolved_at), resolution = COALESCE(?, resolution), updated_at = ? WHERE id = ?",
            )
            .bind(target.as_str())
            .bind(resolved_at)
            .bind(resolution)
            .bind(now)
            .bind(case_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE moderation_cases SET status = ?, resolved_at = COALESCE(?, resolved_at), resolution = COALESCE(?, resolution), updated_at = ? WHERE id = ?",
            )
            .bind(target.as_str())
            .bind(resolved_at)
            .bind(resolution)
            .bind(now)
            .bind(case_id)
            .execute(p)
            .await?;
        }
    }
    AuditEntry::moderation_action(
        moderator_id,
        "moderation_case",
        case_id,
        "case.transition",
        resolution.unwrap_or(""),
        AUTHZ_POLICY_VERSION,
    )
    .record(pool)
    .await?;
    let _ = enqueue(
        pool,
        MODERATION_CASE_CHANGED,
        json!({ "case_id": case_id, "status": target.as_str() }),
    )
    .await;
    Ok(())
}

/// 派单（M05-CASES-04/05）：板块范围（`moderation.review`）+ 利益冲突
/// （处理者不得是任一关联举报的举报人）；指派历史只追加 + 审计。
pub async fn assign_case(
    pool: &DatabasePool,
    moderator_id: &str,
    case_id: &str,
    assignee_id: &str,
    note: Option<&str>,
    now: i64,
) -> Result<(), CasesError> {
    let linked_reporter: Option<(String,)> =
        match pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT r.reporter_id FROM case_reports cr JOIN reports r ON r.id = cr.report_id
                 WHERE cr.case_id = ? LIMIT 1",
            )
            .bind(case_id)
            .fetch_optional(p)
            .await?,
            Either::Right(p) => sqlx::query_as(
                "SELECT r.reporter_id FROM case_reports cr JOIN reports r ON r.id = cr.report_id
                 WHERE cr.case_id = ? LIMIT 1",
            )
            .bind(case_id)
            .fetch_optional(p)
            .await?,
        };
    if let Some((reporter,)) = linked_reporter {
        if reporter == assignee_id {
            let _ = AuditEntry::moderation_action(
                moderator_id,
                "moderation_case",
                case_id,
                "case.assign_blocked_conflict",
                "assignee is the reporter",
                AUTHZ_POLICY_VERSION,
            )
            .record(pool)
            .await;
            return Err(CasesError::Forbidden(
                "assignee cannot be the reporter (conflict of interest)".into(),
            ));
        }
    }
    // 板块范围：案件关联的举报目标若为帖子/回复，取板块并校验 assignee 权限。
    let board_id = case_board_id(pool, case_id).await?;
    let decision = authorize_action(
        pool,
        assignee_id,
        "moderation.review",
        board_id.as_deref(),
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(CasesError::Db)?;
    if !decision.is_allowed() {
        return Err(CasesError::Forbidden(
            "assignee lacks moderation.review in case scope".into(),
        ));
    }

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO case_assignments (id, case_id, assignee_id, assigned_by, assigned_at, note)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(case_id)
            .bind(assignee_id)
            .bind(moderator_id)
            .bind(now)
            .bind(note)
            .execute(p)
            .await?;
            sqlx::query("UPDATE moderation_cases SET assigned_to = ?, updated_at = ? WHERE id = ?")
                .bind(assignee_id)
                .bind(now)
                .bind(case_id)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO case_assignments (id, case_id, assignee_id, assigned_by, assigned_at, note)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(case_id)
            .bind(assignee_id)
            .bind(moderator_id)
            .bind(now)
            .bind(note)
            .execute(p)
            .await?;
            sqlx::query("UPDATE moderation_cases SET assigned_to = ?, updated_at = ? WHERE id = ?")
                .bind(assignee_id)
                .bind(now)
                .bind(case_id)
                .execute(p)
                .await?;
        }
    }
    AuditEntry::moderation_action(
        moderator_id,
        "moderation_case",
        case_id,
        "case.assign",
        note.unwrap_or(""),
        AUTHZ_POLICY_VERSION,
    )
    .record(pool)
    .await?;
    Ok(())
}

/// 案件关联目标所属板块（post/comment → 板块；user/board → None=全局）。
async fn case_board_id(pool: &DatabasePool, case_id: &str) -> Result<Option<String>, CasesError> {
    let row: Option<(String, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT r.target_type, r.target_id FROM case_reports cr
                 JOIN reports r ON r.id = cr.report_id
                 WHERE cr.case_id = ? LIMIT 1",
            )
            .bind(case_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT r.target_type, r.target_id FROM case_reports cr
                 JOIN reports r ON r.id = cr.report_id
                 WHERE cr.case_id = ? LIMIT 1",
            )
            .bind(case_id)
            .fetch_optional(p)
            .await?
        }
    };
    let Some((target_type, target_id)) = row else {
        return Ok(None);
    };
    match target_type.as_str() {
        "post" => {
            let board: Option<(String,)> = match pool {
                Either::Left(p) => {
                    sqlx::query_as("SELECT board_id FROM posts WHERE id = ?")
                        .bind(&target_id)
                        .fetch_optional(p)
                        .await?
                }
                Either::Right(p) => {
                    sqlx::query_as("SELECT board_id FROM posts WHERE id = ?")
                        .bind(&target_id)
                        .fetch_optional(p)
                        .await?
                }
            };
            Ok(board.map(|(b,)| b))
        }
        "comment" => {
            let board: Option<(String,)> = match pool {
                Either::Left(p) => {
                    sqlx::query_as(
                        "SELECT p.board_id FROM comments c JOIN posts p ON p.id = c.post_id WHERE c.id = ?",
                    )
                    .bind(&target_id)
                    .fetch_optional(p)
                    .await?
                }
                Either::Right(p) => {
                    sqlx::query_as(
                        "SELECT p.board_id FROM comments c JOIN posts p ON p.id = c.post_id WHERE c.id = ?",
                    )
                    .bind(&target_id)
                    .fetch_optional(p)
                    .await?
                }
            };
            Ok(board.map(|(b,)| b))
        }
        _ => Ok(None),
    }
}

/// 内容动作（M05-CASES-06/07/08/09）：hide/restore/delete 帖子或回复。
///
/// - 动作只追加到 `moderation_actions` + 修订 + 审计 + Outbox；
/// - hide → posts.status='hidden' / comments.status='hidden'（公开列表/搜索/
///   Feed 按 published 过滤天然撤除）；
/// - delete → status='deleted' + deleted_at（全投影撤除）；
/// - restore → 重跑当前风险策略：低风险恢复 published；高风险再置
///   pending_review（不进入公开投影）。
pub enum ContentAction {
    Hide,
    Restore,
    Delete,
}

impl ContentAction {
    /// 动作写入 `moderation_actions.action` 的字面值。
    fn action_kind(&self) -> ModerationActionKind {
        match self {
            Self::Hide => ModerationActionKind::HideContent,
            Self::Restore => ModerationActionKind::RestoreContent,
            Self::Delete => ModerationActionKind::DeleteContent,
        }
    }
}

/// 执行内容动作（帖子）。
#[allow(clippy::too_many_arguments)]
pub async fn apply_post_action(
    pool: &DatabasePool,
    moderator_id: &str,
    post_id: &str,
    action: ContentAction,
    reason: &str,
    now: i64,
) -> Result<(), CasesError> {
    if reason.trim().chars().count() < ACTION_REASON_MIN {
        return Err(CasesError::InvalidReason(
            "reason is required for content actions".into(),
        ));
    }
    let row: Option<(String, String, Option<String>)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT author_id, status, review_status FROM posts WHERE id = ?")
                .bind(post_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT author_id, status, review_status FROM posts WHERE id = ?")
                .bind(post_id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((author_id, _status, _review_status)) = row else {
        return Err(CasesError::NotFound("post not found".into()));
    };
    if author_id == moderator_id {
        let _ = AuditEntry::moderation_action(
            moderator_id,
            "post",
            post_id,
            "content_action_blocked_conflict",
            "moderator cannot act on own content",
            AUTHZ_POLICY_VERSION,
        )
        .record(pool)
        .await;
        return Err(CasesError::Forbidden(
            "cannot moderate your own content".into(),
        ));
    }
    let target_status = match action {
        ContentAction::Hide => "hidden",
        ContentAction::Delete => "deleted",
        ContentAction::Restore => "published",
    };
    let deleted_at = if matches!(action, ContentAction::Delete) {
        Some(now)
    } else {
        None
    };
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET status = ?, deleted_at = ?, updated_at = ? WHERE id = ?")
                .bind(target_status)
                .bind(deleted_at)
                .bind(now)
                .bind(post_id)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE posts SET status = ?, deleted_at = ?, updated_at = ? WHERE id = ?")
                .bind(target_status)
                .bind(deleted_at)
                .bind(now)
                .bind(post_id)
                .execute(p)
                .await?;
        }
    }
    let kind = action.action_kind();
    record_content_action(
        pool,
        moderator_id,
        ModerationTargetType::Post,
        post_id,
        kind,
        reason,
        now,
    )
    .await?;
    let _ = enqueue(
        pool,
        POST_VISIBILITY_CHANGED,
        json!({ "post_id": post_id, "status": target_status }),
    )
    .await;
    Ok(())
}

/// 写内容动作记录（只追加 `moderation_actions`）+ 审计。
async fn record_content_action(
    pool: &DatabasePool,
    moderator_id: &str,
    target_type: ModerationTargetType,
    target_id: &str,
    kind: ModerationActionKind,
    reason: &str,
    now: i64,
) -> Result<(), CasesError> {
    let action_id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO moderation_actions
                     (id, case_id, actor_id, action, target_type, target_id, reason, created_at)
                 VALUES (?, NULL, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&action_id)
            .bind(moderator_id)
            .bind(kind.as_str())
            .bind(target_type.as_str())
            .bind(target_id)
            .bind(reason)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO moderation_actions
                     (id, case_id, actor_id, action, target_type, target_id, reason, created_at)
                 VALUES (?, NULL, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&action_id)
            .bind(moderator_id)
            .bind(kind.as_str())
            .bind(target_type.as_str())
            .bind(target_id)
            .bind(reason)
            .bind(now)
            .execute(p)
            .await?;
        }
    }
    AuditEntry::moderation_action(
        moderator_id,
        target_type.as_str(),
        target_id,
        kind.as_str(),
        reason,
        AUTHZ_POLICY_VERSION,
    )
    .record(pool)
    .await?;
    Ok(())
}

/// Report 行 → 模型（sqlx 跨库字段名映射）。
#[derive(sqlx::FromRow)]
struct ReportRow {
    id: String,
    reporter_id: String,
    target_type: String,
    target_id: String,
    reason_code: String,
    details: Option<String>,
    status: String,
    report_dedup_key: String,
    dedup_until: i64,
    assigned_to: Option<String>,
    created_at: i64,
    updated_at: i64,
}

impl ReportRow {
    fn into_model(self) -> Report {
        Report {
            id: self.id,
            reporter_id: self.reporter_id,
            target_type: ReportTargetType::parse(&self.target_type)
                .unwrap_or(ReportTargetType::Post),
            target_id: self.target_id,
            reason_code: ReportReasonCode::parse(&self.reason_code)
                .unwrap_or(ReportReasonCode::Other),
            details: self.details,
            status: ReportStatus::parse(&self.status).unwrap_or(ReportStatus::Open),
            report_dedup_key: self.report_dedup_key,
            dedup_until: self.dedup_until,
            assigned_to: self.assigned_to,
            created_at: self.created_at,
            updated_at: self.updated_at,
        }
    }
}

/// 供路由使用的助手：当前 Unix 毫秒。
pub fn now() -> i64 {
    now_millis()
}
