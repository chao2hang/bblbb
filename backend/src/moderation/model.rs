//! M05-SCHEMA：moderation 数据模型与约束校验（纯数据/约束层，无路由）。
//!
//! 覆盖迁移 0041-0044 对应的行结构、枚举与规则：
//! - 举报去重键构造与锚定去重窗口（0041/0042... 见 0041 report_dedup_key）；
//! - 非法状态迁移校验（ReportStatus/CaseStatus/AppealStatus 的
//!   `can_transition_to`，与 STATE-MACHINES.md §3 一致）；
//! - 到期判断与板块范围/期限/撤销一致性校验（sanctions，0043）；
//! - 只追加修订校验（moderation_action_revisions，0042）；
//! - 利益冲突 reviewer 校验（appeal_decisions，0044）。
//!
//! DB 层以 CHECK/UNIQUE 约束兜底（三库一致），本模块提供应用层语义。

/// 举报去重窗口（毫秒）：同一 (reporter, target, reason) 在锚定窗口内至多一条。
pub const REPORT_DEDUP_WINDOW_MS: i64 = 7 * 24 * 60 * 60 * 1000;

/// 举报目标类型（reports.target_type，0041）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportTargetType {
    Post,
    Comment,
    User,
    Board,
}

impl ReportTargetType {
    /// 全部合法取值（与 0041 CHECK 一致）。
    pub const ALL: [ReportTargetType; 4] = [
        ReportTargetType::Post,
        ReportTargetType::Comment,
        ReportTargetType::User,
        ReportTargetType::Board,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            ReportTargetType::Post => "post",
            ReportTargetType::Comment => "comment",
            ReportTargetType::User => "user",
            ReportTargetType::Board => "board",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<ReportTargetType> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// 举报原因码（reports.reason_code，0041）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportReasonCode {
    Spam,
    Harassment,
    Illegal,
    Nsfw,
    Misinformation,
    Impersonation,
    Other,
}

impl ReportReasonCode {
    /// 全部合法取值（与 0041 CHECK 一致）。
    pub const ALL: [ReportReasonCode; 7] = [
        ReportReasonCode::Spam,
        ReportReasonCode::Harassment,
        ReportReasonCode::Illegal,
        ReportReasonCode::Nsfw,
        ReportReasonCode::Misinformation,
        ReportReasonCode::Impersonation,
        ReportReasonCode::Other,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            ReportReasonCode::Spam => "spam",
            ReportReasonCode::Harassment => "harassment",
            ReportReasonCode::Illegal => "illegal",
            ReportReasonCode::Nsfw => "nsfw",
            ReportReasonCode::Misinformation => "misinformation",
            ReportReasonCode::Impersonation => "impersonation",
            ReportReasonCode::Other => "other",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<ReportReasonCode> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// 举报状态（reports.status，0041）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReportStatus {
    Open,
    Triaged,
    Investigating,
    Resolved,
    Rejected,
    Reopened,
    Withdrawn,
}

impl ReportStatus {
    /// 全部合法取值（与 0041 CHECK 一致）。
    pub const ALL: [ReportStatus; 7] = [
        ReportStatus::Open,
        ReportStatus::Triaged,
        ReportStatus::Investigating,
        ReportStatus::Resolved,
        ReportStatus::Rejected,
        ReportStatus::Reopened,
        ReportStatus::Withdrawn,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            ReportStatus::Open => "open",
            ReportStatus::Triaged => "triaged",
            ReportStatus::Investigating => "investigating",
            ReportStatus::Resolved => "resolved",
            ReportStatus::Rejected => "rejected",
            ReportStatus::Reopened => "reopened",
            ReportStatus::Withdrawn => "withdrawn",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<ReportStatus> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// 合法状态迁移（STATE-MACHINES.md §3）；withdrawn 为终态。
    pub fn can_transition_to(self, next: ReportStatus) -> bool {
        use ReportStatus as S;
        matches!(
            (self, next),
            (
                S::Open,
                S::Triaged | S::Investigating | S::Resolved | S::Rejected | S::Withdrawn
            ) | (S::Triaged, S::Investigating | S::Resolved | S::Rejected)
                | (S::Investigating, S::Resolved | S::Rejected)
                | (S::Resolved, S::Reopened)
                | (S::Rejected, S::Reopened | S::Withdrawn)
                | (
                    S::Reopened,
                    S::Triaged | S::Investigating | S::Resolved | S::Rejected | S::Withdrawn
                )
        )
    }
}

/// 举报（reports 表，0041）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub id: String,
    pub reporter_id: String,
    pub target_type: ReportTargetType,
    pub target_id: String,
    pub reason_code: ReportReasonCode,
    pub details: Option<String>,
    pub status: ReportStatus,
    pub report_dedup_key: String,
    pub dedup_until: i64,
    pub assigned_to: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Report {
    /// 归一化去重键 `(reporter, target_type, target_id, reason_code)`。
    ///
    /// 参考 0040 grant_target_key 手法：把复合组合折叠为单列，
    /// 避免跨库 NULL 唯一性语义差异（本表字段均 NOT NULL，
    /// 折叠后亦可直接参与 UNIQUE 约束）。
    pub fn build_dedup_key(
        reporter_id: &str,
        target_type: ReportTargetType,
        target_id: &str,
        reason_code: ReportReasonCode,
    ) -> String {
        format!(
            "{}|{}|{}|{}",
            reporter_id,
            target_type.as_str(),
            target_id,
            reason_code.as_str()
        )
    }

    /// 锚定去重窗口终点：`created_at` 所在窗口的终点（半开区间）。
    ///
    /// 同一锚定窗口内提交的所有举报共享同一 `dedup_until`，故
    /// `UNIQUE(report_dedup_key, dedup_until)` 在 DB 层即可拒绝窗口内重复；
    /// 下一窗口（`dedup_until` 之后）允许重新举报。
    pub fn dedup_window_end(created_at: i64, window_ms: i64) -> i64 {
        created_at
            .div_euclid(window_ms)
            .saturating_add(1)
            .saturating_mul(window_ms)
    }

    /// `now` 是否仍处于去重窗口内（`dedup_until` 半开：`now < dedup_until`）。
    pub fn is_within_dedup_window(dedup_until: i64, now: i64) -> bool {
        now < dedup_until
    }
}

/// 案件状态（moderation_cases.status，0041）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaseStatus {
    Open,
    Triaged,
    Investigating,
    Resolved,
    Rejected,
    Reopened,
}

impl CaseStatus {
    /// 全部合法取值（与 0041 CHECK 一致；案件不含 withdrawn）。
    pub const ALL: [CaseStatus; 6] = [
        CaseStatus::Open,
        CaseStatus::Triaged,
        CaseStatus::Investigating,
        CaseStatus::Resolved,
        CaseStatus::Rejected,
        CaseStatus::Reopened,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            CaseStatus::Open => "open",
            CaseStatus::Triaged => "triaged",
            CaseStatus::Investigating => "investigating",
            CaseStatus::Resolved => "resolved",
            CaseStatus::Rejected => "rejected",
            CaseStatus::Reopened => "reopened",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<CaseStatus> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// 合法状态迁移（STATE-MACHINES.md §3）。
    pub fn can_transition_to(self, next: CaseStatus) -> bool {
        use CaseStatus as S;
        matches!(
            (self, next),
            (
                S::Open,
                S::Triaged | S::Investigating | S::Resolved | S::Rejected
            ) | (S::Triaged, S::Investigating | S::Resolved | S::Rejected)
                | (S::Investigating, S::Resolved | S::Rejected)
                | (S::Resolved, S::Reopened)
                | (S::Rejected, S::Reopened)
                | (
                    S::Reopened,
                    S::Triaged | S::Investigating | S::Resolved | S::Rejected
                )
        )
    }
}

/// 案件优先级（moderation_cases.priority，0041）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CasePriority {
    Low,
    Normal,
    High,
    Urgent,
}

impl CasePriority {
    /// 全部合法取值（与 0041 CHECK 一致）。
    pub const ALL: [CasePriority; 4] = [
        CasePriority::Low,
        CasePriority::Normal,
        CasePriority::High,
        CasePriority::Urgent,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            CasePriority::Low => "low",
            CasePriority::Normal => "normal",
            CasePriority::High => "high",
            CasePriority::Urgent => "urgent",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<CasePriority> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// 审核案件（moderation_cases 表，0041）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModerationCase {
    pub id: String,
    pub title: String,
    pub status: CaseStatus,
    pub priority: CasePriority,
    pub assigned_to: Option<String>,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub resolved_at: Option<i64>,
    pub resolution: Option<String>,
}

/// 审核动作类型（moderation_actions.action，0042）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModerationActionKind {
    Escalate,
    Assign,
    Resolve,
    Reject,
    Reopen,
    HideContent,
    RestoreContent,
    DeleteContent,
    IssueSanction,
    RevokeSanction,
    MergeCases,
    RemoveReport,
}

impl ModerationActionKind {
    /// 全部合法取值（与 0042 CHECK 一致）。
    pub const ALL: [ModerationActionKind; 12] = [
        ModerationActionKind::Escalate,
        ModerationActionKind::Assign,
        ModerationActionKind::Resolve,
        ModerationActionKind::Reject,
        ModerationActionKind::Reopen,
        ModerationActionKind::HideContent,
        ModerationActionKind::RestoreContent,
        ModerationActionKind::DeleteContent,
        ModerationActionKind::IssueSanction,
        ModerationActionKind::RevokeSanction,
        ModerationActionKind::MergeCases,
        ModerationActionKind::RemoveReport,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            ModerationActionKind::Escalate => "escalate",
            ModerationActionKind::Assign => "assign",
            ModerationActionKind::Resolve => "resolve",
            ModerationActionKind::Reject => "reject",
            ModerationActionKind::Reopen => "reopen",
            ModerationActionKind::HideContent => "hide_content",
            ModerationActionKind::RestoreContent => "restore_content",
            ModerationActionKind::DeleteContent => "delete_content",
            ModerationActionKind::IssueSanction => "issue_sanction",
            ModerationActionKind::RevokeSanction => "revoke_sanction",
            ModerationActionKind::MergeCases => "merge_cases",
            ModerationActionKind::RemoveReport => "remove_report",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<ModerationActionKind> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// 审核动作目标类型（moderation_actions.target_type，0042）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ModerationTargetType {
    Post,
    Comment,
    User,
    Report,
    Case,
    Sanction,
}

impl ModerationTargetType {
    /// 全部合法取值（与 0042 CHECK 一致）。
    pub const ALL: [ModerationTargetType; 6] = [
        ModerationTargetType::Post,
        ModerationTargetType::Comment,
        ModerationTargetType::User,
        ModerationTargetType::Report,
        ModerationTargetType::Case,
        ModerationTargetType::Sanction,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            ModerationTargetType::Post => "post",
            ModerationTargetType::Comment => "comment",
            ModerationTargetType::User => "user",
            ModerationTargetType::Report => "report",
            ModerationTargetType::Case => "case",
            ModerationTargetType::Sanction => "sanction",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<ModerationTargetType> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// 审核动作（moderation_actions 表，0042）。只追加不覆盖：行不可变，
/// 修正一律写入 `ModerationActionRevision`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModerationAction {
    pub id: String,
    pub case_id: Option<String>,
    pub actor_id: String,
    pub action: ModerationActionKind,
    pub target_type: Option<ModerationTargetType>,
    pub target_id: Option<String>,
    pub reason: Option<String>,
    pub metadata_json: Option<String>,
    pub created_at: i64,
}

/// 审核动作修订快照（moderation_action_revisions 表，0042）。只追加：
/// `(action_id, revision)` 唯一且 revision 严格递增。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModerationActionRevision {
    pub id: String,
    pub action_id: String,
    pub revision: i64,
    pub snapshot_json: String,
    pub change_reason: Option<String>,
    pub created_by: String,
    pub created_at: i64,
}

impl ModerationActionRevision {
    /// 只追加校验：新修订号必须严格大于当前最大修订号（历史不可覆盖）。
    ///
    /// `current_max` 取该 action 现有最大 revision（无修订时为 0）。
    pub fn validate_revision(current_max: i64, next: i64) -> Result<(), String> {
        if next > current_max {
            Ok(())
        } else {
            Err(format!(
                "修订必须严格递增（只追加）：当前最大 {current_max}，收到 {next}"
            ))
        }
    }
}

/// 处罚种类（sanctions.kind，0043）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SanctionKind {
    Warning,
    RateLimit,
    Mute,
    BoardMute,
    Ban,
}

impl SanctionKind {
    /// 全部合法取值（与 0043 CHECK 一致）。
    pub const ALL: [SanctionKind; 5] = [
        SanctionKind::Warning,
        SanctionKind::RateLimit,
        SanctionKind::Mute,
        SanctionKind::BoardMute,
        SanctionKind::Ban,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            SanctionKind::Warning => "warning",
            SanctionKind::RateLimit => "rate_limit",
            SanctionKind::Mute => "mute",
            SanctionKind::BoardMute => "board_mute",
            SanctionKind::Ban => "ban",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<SanctionKind> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// 处罚状态（sanctions.status，0043）。时间推移由模型层推进。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SanctionStatus {
    Scheduled,
    Active,
    Expired,
    Revoked,
}

impl SanctionStatus {
    /// 全部合法取值（与 0043 CHECK 一致）。
    pub const ALL: [SanctionStatus; 4] = [
        SanctionStatus::Scheduled,
        SanctionStatus::Active,
        SanctionStatus::Expired,
        SanctionStatus::Revoked,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            SanctionStatus::Scheduled => "scheduled",
            SanctionStatus::Active => "active",
            SanctionStatus::Expired => "expired",
            SanctionStatus::Revoked => "revoked",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<SanctionStatus> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// 处罚（sanctions 表，0043）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sanction {
    pub id: String,
    pub user_id: String,
    pub board_id: Option<String>,
    pub kind: SanctionKind,
    pub status: SanctionStatus,
    pub reason: Option<String>,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
    pub created_by: String,
    pub created_at: i64,
    pub revoked_at: Option<i64>,
    pub revoked_by: Option<String>,
    pub revoke_reason: Option<String>,
}

impl Sanction {
    /// 板块范围校验（0043 CHECK 的应用层镜像）：board_mute 必须带
    /// board_id；其他 kind 拒绝携带板块范围。
    pub fn validate_board_scope(kind: SanctionKind, board_id: Option<&str>) -> Result<(), String> {
        match kind {
            SanctionKind::BoardMute if board_id.is_none() => {
                Err("board_mute 必须带 board_id".to_string())
            }
            SanctionKind::BoardMute => Ok(()),
            _ if board_id.is_some() => Err(format!(
                "{} 拒绝携带板块范围（仅 board_mute 支持板块限定）",
                kind.as_str()
            )),
            _ => Ok(()),
        }
    }

    /// 期限校验（0043 CHECK 的应用层镜像）：ends_at 可空（永久），
    /// 非空时须晚于 starts_at。
    pub fn validate_timeline(starts_at: i64, ends_at: Option<i64>) -> Result<(), String> {
        if let Some(end) = ends_at {
            if end <= starts_at {
                return Err("ends_at 必须晚于 starts_at".to_string());
            }
        }
        Ok(())
    }

    /// 撤销一致性校验（0043 CHECK 的应用层镜像）：status='revoked'
    /// 必须带 revoked_at 与 revoked_by。
    pub fn validate_revoked(
        status: SanctionStatus,
        revoked_at: Option<i64>,
        revoked_by: Option<&str>,
    ) -> Result<(), String> {
        if status == SanctionStatus::Revoked && (revoked_at.is_none() || revoked_by.is_none()) {
            return Err("revoked 状态必须带 revoked_at 与 revoked_by".to_string());
        }
        Ok(())
    }

    /// 指定时刻是否生效：status=active 且 `starts_at <= now < ends_at`
    /// （ends_at 可空 = 永久；边界半开，ends_at 恰等于 now 即已到期）。
    pub fn is_active_at(&self, now: i64) -> bool {
        self.status == SanctionStatus::Active
            && self.starts_at <= now
            && self.ends_at.is_none_or(|end| now < end)
    }

    /// 指定时刻是否已到期：ends_at 非空且 `now >= ends_at`。
    pub fn is_expired_at(&self, now: i64) -> bool {
        self.ends_at.is_some_and(|end| now >= end)
    }
}

/// 处罚撤销记录（sanction_reversals 表，0043）。只追加不可变；
/// 每处罚至多一条撤销（UNIQUE(sanction_id)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SanctionReversal {
    pub id: String,
    pub sanction_id: String,
    pub reversed_by: String,
    pub reason: String,
    pub reversed_at: i64,
}

/// 申诉状态（appeals.status，0044）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppealStatus {
    Submitted,
    Reviewing,
    Upheld,
    PartiallyUpheld,
    Rejected,
    Withdrawn,
}

impl AppealStatus {
    /// 全部合法取值（与 0044 CHECK 一致）。
    pub const ALL: [AppealStatus; 6] = [
        AppealStatus::Submitted,
        AppealStatus::Reviewing,
        AppealStatus::Upheld,
        AppealStatus::PartiallyUpheld,
        AppealStatus::Rejected,
        AppealStatus::Withdrawn,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            AppealStatus::Submitted => "submitted",
            AppealStatus::Reviewing => "reviewing",
            AppealStatus::Upheld => "upheld",
            AppealStatus::PartiallyUpheld => "partially_upheld",
            AppealStatus::Rejected => "rejected",
            AppealStatus::Withdrawn => "withdrawn",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<AppealStatus> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// 合法状态迁移（STATE-MACHINES.md §3）：
    /// `submitted → reviewing → upheld | partially_upheld | rejected`；
    /// submitted/reviewing 均可撤回（withdrawn，终态）。
    pub fn can_transition_to(self, next: AppealStatus) -> bool {
        use AppealStatus as S;
        matches!(
            (self, next),
            (S::Submitted, S::Reviewing | S::Withdrawn)
                | (
                    S::Reviewing,
                    S::Upheld | S::PartiallyUpheld | S::Rejected | S::Withdrawn
                )
        )
    }
}

/// 申诉（appeals 表，0044）。每处罚至多一条（UNIQUE(sanction_id)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appeal {
    pub id: String,
    pub sanction_id: String,
    pub user_id: String,
    pub message: String,
    pub status: AppealStatus,
    pub reviewed_by: Option<String>,
    pub decided_at: Option<i64>,
    pub submitted_at: i64,
    pub updated_at: i64,
}

/// 申诉决定取值（appeal_decisions.decision，0044）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppealDecisionValue {
    Upheld,
    PartiallyUpheld,
    Rejected,
}

impl AppealDecisionValue {
    /// 全部合法取值（与 0044 CHECK 一致）。
    pub const ALL: [AppealDecisionValue; 3] = [
        AppealDecisionValue::Upheld,
        AppealDecisionValue::PartiallyUpheld,
        AppealDecisionValue::Rejected,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            AppealDecisionValue::Upheld => "upheld",
            AppealDecisionValue::PartiallyUpheld => "partially_upheld",
            AppealDecisionValue::Rejected => "rejected",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<AppealDecisionValue> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }
}

/// 申诉决定记录（appeal_decisions 表，0044）。只追加，不覆盖历史决定。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppealDecision {
    pub id: String,
    pub appeal_id: String,
    pub reviewer_id: String,
    pub decision: AppealDecisionValue,
    pub decision_note: Option<String>,
    pub conflict_of_interest: Option<String>,
    pub created_at: i64,
}

impl AppealDecision {
    /// 利益冲突 reviewer 校验：审查者不得是申诉人本人；声明利益冲突时
    /// 必须填写理由（conflict_of_interest 非空且非空白）。
    pub fn validate_reviewer(
        appellant_id: &str,
        reviewer_id: &str,
        conflict_of_interest: Option<&str>,
    ) -> Result<(), String> {
        if appellant_id == reviewer_id {
            return Err("审查者不得是申诉人本人（利益冲突）".to_string());
        }
        if let Some(reason) = conflict_of_interest {
            if reason.trim().is_empty() {
                return Err("声明利益冲突时必须填写理由".to_string());
            }
        }
        Ok(())
    }
}
