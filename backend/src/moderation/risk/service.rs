//! 风险评估服务（M05-RISK-03/07/08/09）。
//!
//! - [`evaluate_risk`]：加载**当前**策略版本执行规则；AI 建议有截止时间，
//!   关闭/失败/迟到时按规则结果兜底（不阻塞发布）；返回 [`RiskVerdict`]
//!   （`allow` 直接发布 / `pending_review` 进人工队列）。
//! - [`update_risk_policy`]：管理员版本化更新（M05-RISK-08）——必须提供
//!   reason、写审计、并发版本控制（`UNIQUE(id, version)`，期望版本不匹配
//!   返回 [`RiskError::PolicyConflict`]）。
//! - 指标（M05-RISK-09）：`record_evaluation` 只记 verdict/category/延迟/
//!   策略版本（**不记录正文**）；`record_review_outcome` 记录队列时长与
//!   误判反馈。

use std::time::Duration;

use sqlx::Either;

use crate::audit::AuditEntry;
use crate::db::DatabasePool;
use crate::outbox::OutboxTx;

use super::policy::{
    RiskInput, RiskPolicy, RiskVerdict, Thresholds, BUILTIN_POLICY_VERSION, DEFAULT_RISK_POLICY_ID,
};
use super::provider::{AiModerationProvider, AiSuggestion, NullAiModerationProvider};
use super::rules;

/// 风险评估错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskError {
    Db(String),
    /// 规则/策略加载超时或异常（fail-closed：调用方按 pending_review 处理）。
    Timeout,
    /// 管理员更新期望版本与当前版本不一致（409 语义）。
    PolicyConflict {
        expected: i64,
        current: i64,
    },
    /// 策略载荷非法。
    InvalidPolicy(String),
}

impl From<sqlx::Error> for RiskError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for RiskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "risk db error: {msg}"),
            Self::Timeout => write!(f, "risk evaluation timed out"),
            Self::PolicyConflict { expected, current } => {
                write!(
                    f,
                    "risk policy conflict: expected version {expected}, current {current}"
                )
            }
            Self::InvalidPolicy(msg) => write!(f, "invalid risk policy: {msg}"),
        }
    }
}

/// AI 建议默认截止时间（超过即忽略建议，不阻塞）。
pub const AI_SUGGEST_DEADLINE: Duration = Duration::from_secs(2);

/// 加载当前生效的策略（最新版本行；无行 → 内置默认 version 0）。
pub async fn load_policy(pool: &DatabasePool) -> Result<RiskPolicy, RiskError> {
    let row: Option<(i64, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT version, thresholds_json FROM risk_policies
             WHERE id = ? ORDER BY version DESC LIMIT 1",
            )
            .bind(DEFAULT_RISK_POLICY_ID)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT version, thresholds_json FROM risk_policies
             WHERE id = ? ORDER BY version DESC LIMIT 1",
            )
            .bind(DEFAULT_RISK_POLICY_ID)
            .fetch_optional(p)
            .await?
        }
    };
    match row {
        Some((version, json)) => RiskPolicy::parse(version, &json),
        None => Ok(RiskPolicy::builtin()),
    }
}

/// 执行风险评估。
///
/// - 规则命中 → `PendingReview`（最高优先级）；
/// - 规则未命中且提供了 Provider → 在 `deadline` 内取 AI 建议：
///   `Flag` → `PendingReview`，`NoAction`/超时/迟到 → `Allow`（不阻塞）；
/// - 规则未命中且无 Provider → `Allow`（Null Adapter 语义）。
///
/// 不写任何指标（指标由发布事务内 [`record_evaluation`] 写入，需要 post_id）。
pub async fn evaluate_risk(
    pool: &DatabasePool,
    input: &RiskInput,
    provider: Option<&dyn AiModerationProvider>,
    deadline: Duration,
    now: i64,
) -> Result<RiskVerdict, RiskError> {
    let policy = load_policy(pool).await?;
    let version = policy.version;

    if let Some(category) = rules::run_rules(pool, &policy.thresholds, input).await? {
        return Ok(RiskVerdict::PendingReview {
            reason: category,
            policy_version: version,
        });
    }

    let provider = provider.unwrap_or(&NullAiModerationProvider);
    match tokio::time::timeout(deadline, provider.suggest(input, now)).await {
        Ok(suggestion) => match suggestion {
            AiSuggestion::NoAction => Ok(RiskVerdict::Allow {
                policy_version: version,
            }),
            AiSuggestion::Flag(reason) => Ok(RiskVerdict::PendingReview {
                reason,
                policy_version: version,
            }),
        },
        Err(_) => {
            // AI 超时/迟到：按规则结果放行，不阻塞发布（M05-RISK-07）。
            tracing::debug!(author_id = %input.author_id, "risk AI suggestion timed out; using rule verdict");
            Ok(RiskVerdict::Allow {
                policy_version: version,
            })
        }
    }
}

/// 记录评估指标（M05-RISK-09；发布事务内调用，保证与 post 同生共死）。
/// `latency_ms` 为评估耗时；`reviewed_at`/`false_positive` 由
/// [`record_review_outcome`] 回填。
pub async fn record_evaluation(
    pool: &DatabasePool,
    post_id: &str,
    author_id: &str,
    verdict: &RiskVerdict,
    latency_ms: i64,
    now: i64,
) -> Result<(), sqlx::Error> {
    let id = uuid::Uuid::now_v7().to_string();
    let (kind, category) = match verdict {
        RiskVerdict::Allow { .. } => ("allow", None),
        RiskVerdict::PendingReview { reason, .. } => ("pending_review", Some(reason.as_str())),
    };
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO risk_evaluations
                     (id, post_id, author_id, verdict, reason_category, policy_version, latency_ms, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(post_id)
            .bind(author_id)
            .bind(kind)
            .bind(category)
            .bind(verdict.policy_version())
            .bind(latency_ms)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO risk_evaluations
                     (id, post_id, author_id, verdict, reason_category, policy_version, latency_ms, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(post_id)
            .bind(author_id)
            .bind(kind)
            .bind(category)
            .bind(verdict.policy_version())
            .bind(latency_ms)
            .bind(now)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

/// 记录审核结果（队列时长 = `reviewed_at - created_at`；误判反馈）。
/// 不更新/删除评估历史，只回填 outcome 字段。
pub async fn record_review_outcome(
    pool: &DatabasePool,
    post_id: &str,
    reviewed_at: i64,
    false_positive: bool,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE risk_evaluations
                 SET reviewed_at = ?, false_positive = ?
                 WHERE post_id = ? AND reviewed_at IS NULL",
            )
            .bind(reviewed_at)
            .bind(false_positive as i64)
            .bind(post_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE risk_evaluations
                 SET reviewed_at = ?, false_positive = ?
                 WHERE post_id = ? AND reviewed_at IS NULL",
            )
            .bind(reviewed_at)
            .bind(false_positive as i64)
            .bind(post_id)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

/// 管理员版本化更新风险策略（M05-RISK-08）。
///
/// 并发版本控制：事务内读取当前最大版本，与 `expected_version` 不一致 →
/// [`RiskError::PolicyConflict`]；一致则写入 `version+1`（`UNIQUE(id, version)`
/// 二次兜底）。reason 必填；审计随事务提交（`record_in_tx`）。
pub async fn update_risk_policy(
    pool: &DatabasePool,
    updated_by: &str,
    thresholds: Thresholds,
    reason: &str,
    expected_version: i64,
    now: i64,
) -> Result<RiskPolicy, RiskError> {
    if reason.trim().is_empty() {
        return Err(RiskError::InvalidPolicy("reason 必填".into()));
    }
    let new_version = expected_version + 1;
    let thresholds_json = serde_json::to_string(&thresholds)
        .map_err(|e| RiskError::InvalidPolicy(format!("serialize: {e}")))?;
    let policy = RiskPolicy {
        version: new_version,
        thresholds,
    };

    match pool {
        Either::Left(p) => {
            let mut tx = OutboxTx::Left(p.begin().await?);
            let current: Option<i64> = match &mut tx {
                Either::Left(t) => {
                    sqlx::query_scalar("SELECT MAX(version) FROM risk_policies WHERE id = ?")
                        .bind(DEFAULT_RISK_POLICY_ID)
                        .fetch_one(&mut **t)
                        .await?
                }
                Either::Right(_) => unreachable!(),
            };
            let current = current.unwrap_or(BUILTIN_POLICY_VERSION);
            if current != expected_version {
                return Err(RiskError::PolicyConflict {
                    expected: expected_version,
                    current,
                });
            }
            let insert = "INSERT INTO risk_policies (id, version, thresholds_json, reason, updated_by, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)";
            match &mut tx {
                Either::Left(t) => {
                    sqlx::query(insert)
                        .bind(DEFAULT_RISK_POLICY_ID)
                        .bind(new_version)
                        .bind(&thresholds_json)
                        .bind(reason)
                        .bind(updated_by)
                        .bind(now)
                        .execute(&mut **t)
                        .await?;
                }
                Either::Right(_) => unreachable!(),
            }
            AuditEntry::user_action(updated_by, "risk_policy.update")
                .with_effective_role("administrator")
                .with_target("risk_policy", DEFAULT_RISK_POLICY_ID)
                .with_reason(reason)
                .with_policy_version(&new_version.to_string())
                .record_in_tx(&mut tx)
                .await?;
            match tx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
            Ok(policy)
        }
        Either::Right(p) => {
            let mut tx = OutboxTx::Right(p.begin().await?);
            let current: Option<i64> = match &mut tx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => {
                    sqlx::query_scalar("SELECT MAX(version) FROM risk_policies WHERE id = ?")
                        .bind(DEFAULT_RISK_POLICY_ID)
                        .fetch_one(&mut **t)
                        .await?
                }
            };
            let current = current.unwrap_or(BUILTIN_POLICY_VERSION);
            if current != expected_version {
                return Err(RiskError::PolicyConflict {
                    expected: expected_version,
                    current,
                });
            }
            let insert = "INSERT INTO risk_policies (id, version, thresholds_json, reason, updated_by, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)";
            match &mut tx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => {
                    sqlx::query(insert)
                        .bind(DEFAULT_RISK_POLICY_ID)
                        .bind(new_version)
                        .bind(&thresholds_json)
                        .bind(reason)
                        .bind(updated_by)
                        .bind(now)
                        .execute(&mut **t)
                        .await?;
                }
            }
            AuditEntry::user_action(updated_by, "risk_policy.update")
                .with_effective_role("administrator")
                .with_target("risk_policy", DEFAULT_RISK_POLICY_ID)
                .with_reason(reason)
                .with_policy_version(&new_version.to_string())
                .record_in_tx(&mut tx)
                .await?;
            match tx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
            Ok(policy)
        }
    }
}
