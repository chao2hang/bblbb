//! M05-SANCTIONS：处罚、实时生效与撤销。
//!
//! 构建在 M05-SCHEMA 的 `sanctions`/`sanction_reversals` 之上：
//!
//! - [`create_sanction`]（M05-SANCTIONS-02/04/06）：权限 + 板块范围 + 越权防护
//!   （低权限版主不能处罚更高权限账号、超出板块或时长上限）+ reason + 期限校验；
//!   ban 同流程撤销全部 Session 并投递 `sanction.changed.v1`（OIDC Refresh family
//!   撤销事件由 M11 消费）。
//! - [`revoke_sanction`]（M05-SANCTIONS-05）：只追加 `sanction_reversals`，不改原
//!   处罚；ban 撤销恢复账号状态。
//! - [`effective_sanctions`]（M05-SANCTIONS-03）：请求时实时计算（不依赖 worker
//!   到期任务），供授权门注入。
//! - [`user_sanction_status`]（M05-SANCTIONS-08）：用户安全状态投影，只含
//!   kind/expiry，不含内部依据或举报人。

use serde_json::{json, Value};
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::db::DatabasePool;
use crate::events::types::SANCTION_CHANGED;
use crate::moderation::model::{Sanction, SanctionKind, SanctionStatus};
use crate::outbox::{enqueue, now_millis, OutboxTx};

/// 处罚服务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SanctionsError {
    Db(String),
    NotFound(String),
    /// 权限/板块范围/越权/自处罚等阻断。
    Forbidden(String),
    /// 输入非法（reason 空、期限非法等）。
    Invalid(String),
    /// 越权：目标账号权限不低于操作者。
    Escalation(String),
}

impl From<sqlx::Error> for SanctionsError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for SanctionsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "sanctions db error: {msg}"),
            Self::NotFound(msg) => write!(f, "sanction not found: {msg}"),
            Self::Forbidden(msg) => write!(f, "sanction forbidden: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid sanction: {msg}"),
            Self::Escalation(msg) => write!(f, "sanction escalation denied: {msg}"),
        }
    }
}

impl std::error::Error for SanctionsError {}

/// 处罚创建输入。
#[derive(Debug, Clone)]
pub struct CreateSanctionInput {
    pub target_user_id: String,
    pub board_id: Option<String>,
    pub kind: SanctionKind,
    pub reason: String,
    pub starts_at: i64,
    pub ends_at: Option<i64>,
}

/// 角色等级排名（越权防护用）：administrator > global_moderator >
/// board_moderator > member。
fn role_rank(role_name: &str) -> u8 {
    match role_name {
        "administrator" => 3,
        "global_moderator" => 2,
        "board_moderator" => 1,
        _ => 0,
    }
}

/// 最高角色排名（user_roles + board_role_assignments 聚合；板块版主经
/// 板块指派授衔，越权防护须一并计入）。
async fn max_role_rank(pool: &DatabasePool, user_id: &str) -> Result<u8, SanctionsError> {
    let rows: Vec<(String,)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT r.name FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = ?
                 UNION ALL
                 SELECT r.name FROM board_role_assignments bra JOIN roles r ON r.id = bra.role_id WHERE bra.user_id = ?",
            )
            .bind(user_id)
            .bind(user_id)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT r.name FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = ?
                 UNION ALL
                 SELECT r.name FROM board_role_assignments bra JOIN roles r ON r.id = bra.role_id WHERE bra.user_id = ?",
            )
            .bind(user_id)
            .bind(user_id)
            .fetch_all(p)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(|(name,)| role_rank(&name))
        .max()
        .unwrap_or(0))
}

/// 时长上限（**毫秒**，与 starts_at/ends_at 同单位）按操作者角色：
/// board_moderator 30 天、global_moderator 365 天、administrator 不限。
fn max_duration_ms(actor_rank: u8) -> Option<i64> {
    match actor_rank {
        1 => Some(30 * 86_400 * 1000),
        2 => Some(365 * 86_400 * 1000),
        _ => None,
    }
}

/// 创建处罚（M05-SANCTIONS-02/04/06）。
///
/// - reason 必填、板块范围校验（model `validate_board_scope`）；
/// - 权限：`moderation.sanction`（板块范围按 board_id；非 board_mute 需全局）；
/// - 越权防护：目标最高角色排名必须低于操作者（同级别/更高 → 阻断）；
/// - 时长上限：按操作者角色封顶（`max_duration_secs`）；
/// - 近期认证（recent-auth）由路由层 step-up 中间件强制（M02-SESSION）；
/// - ban：同流程撤销目标全部 Session + 置 `users.status='banned'`。
pub async fn create_sanction(
    pool: &DatabasePool,
    actor_id: &str,
    input: CreateSanctionInput,
    now: i64,
) -> Result<Sanction, SanctionsError> {
    if input.reason.trim().is_empty() {
        return Err(SanctionsError::Invalid("reason 必填".into()));
    }
    if input.target_user_id == actor_id {
        return Err(SanctionsError::Forbidden("cannot sanction yourself".into()));
    }
    Sanction::validate_board_scope(input.kind, input.board_id.as_deref())
        .map_err(SanctionsError::Invalid)?;
    Sanction::validate_timeline(input.starts_at, input.ends_at).map_err(SanctionsError::Invalid)?;

    let actor_rank = max_role_rank(pool, actor_id).await?;
    let target_rank = max_role_rank(pool, &input.target_user_id).await?;
    if target_rank >= actor_rank {
        let _ = AuditEntry::user_action(actor_id, "sanction.create_blocked_escalation")
            .with_target("user", &input.target_user_id)
            .with_effective_role("moderator")
            .with_reason("target account privilege is not lower than actor")
            .with_policy_version(AUTHZ_POLICY_VERSION)
            .record(pool)
            .await;
        return Err(SanctionsError::Escalation(
            "cannot sanction an account with equal or higher privilege".into(),
        ));
    }
    if let Some(end) = input.ends_at {
        if let Some(cap) = max_duration_ms(actor_rank) {
            if end - input.starts_at > cap {
                return Err(SanctionsError::Invalid(
                    "sanction duration exceeds actor limit".into(),
                ));
            }
        }
    }

    // 权限：板块内 board_mute 需板块 scope；其余需全局 moderation.sanction。
    let decision = authorize_action(
        pool,
        actor_id,
        "moderation.sanction",
        input.board_id.as_deref(),
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(SanctionsError::Db)?;
    if !decision.is_allowed() {
        return Err(SanctionsError::Forbidden(
            "moderation.sanction permission required".into(),
        ));
    }

    let sanction = Sanction {
        id: uuid::Uuid::now_v7().to_string(),
        user_id: input.target_user_id.clone(),
        board_id: input.board_id,
        kind: input.kind,
        status: if now >= input.starts_at {
            SanctionStatus::Active
        } else {
            SanctionStatus::Scheduled
        },
        reason: Some(input.reason.clone()),
        starts_at: input.starts_at,
        ends_at: input.ends_at,
        created_by: actor_id.to_string(),
        created_at: now,
        revoked_at: None,
        revoked_by: None,
        revoke_reason: None,
    };
    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO sanctions
                     (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&sanction.id)
            .bind(&sanction.user_id)
            .bind(&sanction.board_id)
            .bind(sanction.kind.as_str())
            .bind(sanction.status.as_str())
            .bind(&sanction.reason)
            .bind(sanction.starts_at)
            .bind(sanction.ends_at)
            .bind(&sanction.created_by)
            .bind(sanction.created_at)
            .execute(&mut *tx)
            .await?;
            // ban：立即生效时置 banned + 撤销全部 Session（SANCTIONS-04）；
            // 预约 ban（starts_at 未来）由实时检查在生效窗口内强制。
            if sanction.kind == SanctionKind::Ban && now >= sanction.starts_at {
                sqlx::query("UPDATE users SET status = 'banned', updated_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(&sanction.user_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "UPDATE user_sessions SET revoked_at = ?, revoke_reason = ? WHERE user_id = ? AND revoked_at IS NULL",
                )
                .bind(now)
                .bind(&sanction.reason)
                .bind(&sanction.user_id)
                .execute(&mut *tx)
                .await?;
            }
            let audit = AuditEntry::user_action(actor_id, "sanction.create")
                .with_target("user", &sanction.user_id)
                .with_effective_role("moderator")
                .with_reason(&sanction.reason.clone().unwrap_or_default())
                .with_policy_version(AUTHZ_POLICY_VERSION);
            let mut otx = OutboxTx::Left(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO sanctions
                     (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&sanction.id)
            .bind(&sanction.user_id)
            .bind(&sanction.board_id)
            .bind(sanction.kind.as_str())
            .bind(sanction.status.as_str())
            .bind(&sanction.reason)
            .bind(sanction.starts_at)
            .bind(sanction.ends_at)
            .bind(&sanction.created_by)
            .bind(sanction.created_at)
            .execute(&mut *tx)
            .await?;
            if sanction.kind == SanctionKind::Ban && now >= sanction.starts_at {
                sqlx::query("UPDATE users SET status = 'banned', updated_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(&sanction.user_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "UPDATE user_sessions SET revoked_at = ?, revoke_reason = ? WHERE user_id = ? AND revoked_at IS NULL",
                )
                .bind(now)
                .bind(&sanction.reason)
                .bind(&sanction.user_id)
                .execute(&mut *tx)
                .await?;
            }
            let audit = AuditEntry::user_action(actor_id, "sanction.create")
                .with_target("user", &sanction.user_id)
                .with_effective_role("moderator")
                .with_reason(&sanction.reason.clone().unwrap_or_default())
                .with_policy_version(AUTHZ_POLICY_VERSION);
            let mut otx = OutboxTx::Right(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    let _ = enqueue(
        pool,
        SANCTION_CHANGED,
        json!({
            "sanction_id": sanction.id,
            "kind": sanction.kind.as_str(),
            "status": "active",
            "user_id": sanction.user_id,
        }),
    )
    .await;
    Ok(sanction)
}

/// 撤销处罚（M05-SANCTIONS-05）：只追加 `sanction_reversals`，不改原处罚；
/// ban 撤销恢复账号为 active（历史证据链保留）。
pub async fn revoke_sanction(
    pool: &DatabasePool,
    actor_id: &str,
    sanction_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), SanctionsError> {
    if reason.trim().is_empty() {
        return Err(SanctionsError::Invalid("reason 必填".into()));
    }
    let row: Option<(String, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT user_id, kind FROM sanctions WHERE id = ?")
                .bind(sanction_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT user_id, kind FROM sanctions WHERE id = ?")
                .bind(sanction_id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((target_user_id, kind)) = row else {
        return Err(SanctionsError::NotFound("sanction not found".into()));
    };

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            // 撤销证据链只追加（UNIQUE(sanction_id) 兜底至多一条）
            sqlx::query(
                "INSERT INTO sanction_reversals (id, sanction_id, reversed_by, reversed_at, reason)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(sanction_id)
            .bind(actor_id)
            .bind(now)
            .bind(reason)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE sanctions SET status = 'revoked', revoked_at = ?, revoked_by = ?, revoke_reason = ? WHERE id = ?",
            )
            .bind(now)
            .bind(actor_id)
            .bind(reason)
            .bind(sanction_id)
            .execute(&mut *tx)
            .await?;
            if kind == "ban" {
                sqlx::query("UPDATE users SET status = 'active', updated_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(&target_user_id)
                    .execute(&mut *tx)
                    .await?;
            }
            let audit = AuditEntry::user_action(actor_id, "sanction.revoke")
                .with_target("sanction", sanction_id)
                .with_effective_role("moderator")
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION);
            let mut otx = OutboxTx::Left(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO sanction_reversals (id, sanction_id, reversed_by, reversed_at, reason)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(sanction_id)
            .bind(actor_id)
            .bind(now)
            .bind(reason)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE sanctions SET status = 'revoked', revoked_at = ?, revoked_by = ?, revoke_reason = ? WHERE id = ?",
            )
            .bind(now)
            .bind(actor_id)
            .bind(reason)
            .bind(sanction_id)
            .execute(&mut *tx)
            .await?;
            if kind == "ban" {
                sqlx::query("UPDATE users SET status = 'active', updated_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(&target_user_id)
                    .execute(&mut *tx)
                    .await?;
            }
            let audit = AuditEntry::user_action(actor_id, "sanction.revoke")
                .with_target("sanction", sanction_id)
                .with_effective_role("moderator")
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION);
            let mut otx = OutboxTx::Right(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    let _ = enqueue(
        pool,
        SANCTION_CHANGED,
        json!({ "sanction_id": sanction_id, "kind": kind, "status": "revoked" }),
    )
    .await;
    Ok(())
}

/// 请求时实时计算目标用户在当前板块的有效处罚（M05-SANCTIONS-03）。
///
/// 不把 worker 到期任务作为正确性边界：状态按 `starts_at <= now < ends_at`
/// 实时判定（半开边界，`ends_at` 空 = 永久）。
pub async fn effective_sanctions(
    pool: &DatabasePool,
    user_id: &str,
    board_id: Option<&str>,
    now: i64,
) -> Result<Vec<Sanction>, SanctionsError> {
    let rows: Vec<SanctionRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, SanctionRow>(
                "SELECT id, user_id, board_id, kind, status, reason, starts_at, ends_at,
                        created_by, created_at, revoked_at, revoked_by, revoke_reason
                 FROM sanctions
                 WHERE user_id = ? AND status != 'revoked'
                   AND (board_id IS NULL OR board_id = ?)
                 ORDER BY created_at DESC",
            )
            .bind(user_id)
            .bind(board_id)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, SanctionRow>(
                "SELECT id, user_id, board_id, kind, status, reason, starts_at, ends_at,
                        created_by, created_at, revoked_at, revoked_by, revoke_reason
                 FROM sanctions
                 WHERE user_id = ? AND status != 'revoked'
                   AND (board_id IS NULL OR board_id = ?)
                 ORDER BY created_at DESC",
            )
            .bind(user_id)
            .bind(board_id)
            .fetch_all(p)
            .await?
        }
    };
    Ok(rows
        .into_iter()
        .map(SanctionRow::into_model)
        .filter(|s| s.is_active_at(now))
        .collect())
}

/// 全局 mute 截止时间（请求时实时，供账号门注入）。
pub async fn global_mute_until(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<Option<i64>, SanctionsError> {
    let sanctions = effective_sanctions(pool, user_id, None, now).await?;
    Ok(sanctions
        .into_iter()
        .filter(|s| s.kind == SanctionKind::Mute)
        .filter_map(|s| s.ends_at)
        .max())
}

/// 用户安全处罚状态投影（M05-SANCTIONS-08）：只含 kind/expiry，不含
/// 内部依据、举报人或规则细节。
pub async fn user_sanction_status(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<Value, SanctionsError> {
    let sanctions = effective_sanctions(pool, user_id, None, now).await?;
    let items: Vec<Value> = sanctions
        .into_iter()
        .map(|s| {
            json!({
                "kind": s.kind.as_str(),
                "status": "active",
                "expires_at": s.ends_at,
            })
        })
        .collect();
    Ok(json!({ "items": items, "next_cursor": null, "has_more": false }))
}

/// 供路由使用的助手：当前 Unix 毫秒。
pub fn now() -> i64 {
    now_millis()
}

/// Sanction 行 → 模型。
#[derive(sqlx::FromRow)]
struct SanctionRow {
    id: String,
    user_id: String,
    board_id: Option<String>,
    kind: String,
    status: String,
    reason: Option<String>,
    starts_at: i64,
    ends_at: Option<i64>,
    created_by: String,
    created_at: i64,
    revoked_at: Option<i64>,
    revoked_by: Option<String>,
    revoke_reason: Option<String>,
}

impl SanctionRow {
    fn into_model(self) -> Sanction {
        Sanction {
            id: self.id,
            user_id: self.user_id,
            board_id: self.board_id,
            kind: SanctionKind::parse(&self.kind).unwrap_or(SanctionKind::Warning),
            status: SanctionStatus::parse(&self.status).unwrap_or(SanctionStatus::Scheduled),
            reason: self.reason,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            created_by: self.created_by,
            created_at: self.created_at,
            revoked_at: self.revoked_at,
            revoked_by: self.revoked_by,
            revoke_reason: self.revoke_reason,
        }
    }
}
