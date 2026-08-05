//! M03-PROFILE-07：账户注销匿名化服务。
//!
//! 语义（RETENTION-PRIVACY.md §矩阵）：
//! - 帖子/评论等公开讨论**保留**（内容按策略处理），作者标识保留 `author_id`
//!   指向已匿名化的 users 行；公开投影对 `deleted` 用户返回 404（PROFILE-06），
//!   前端以"已注销用户"降级展示，等同替换作者标识；
//! - users 行就地匿名化：username/email 替换为不可识别且唯一的派生值，
//!   display_name/bio/signature/头像/Cover/last_login_at/delete_requested_at
//!   清空，status → `deleted`（终止态），version +1；
//! - 断开可识别资料关系：删除 user_preferences/user_privacy；
//! - 立即撤销全部 Session（revoked_at + revoke_reason='account_deleted'）；
//! - 审计/账务记录不删除（不可删除审计，MODERATION.md §11）；profile_revisions
//!   保留。
//!
//! 注销请求/冷却/取消/执行 Job/法律保留属 M03-PROFILE-08；本模块是执行器。

//! M03-PROFILE-08：注销请求 / 冷却 / 取消 / 执行 Job / 法律保留 / 不可删除审计。
//!
//! 状态机（STATE-MACHINES.md §2 User）：
//! ```text
//! active ─request→ pending_delete ─执行(冷却到期)─→ deleted（匿名化，终态）
//!   ↑──── cancel（冷却期内本人撤销，恢复 active）────┘
//! ```
//! - 冷却期 = [`DELETION_COOLDOWN_MS`]（默认 30 天，RETENTION-PRIVACY.md
//!   §矩阵"注销延迟 30 天"）；请求即入队 `account_deletion` Job，
//!   `available_at = 请求时间 + 冷却期`，由 worker 到点领取执行；
//! - 冷却期内本人可取消：恢复 active、清空 delete_requested_at、把排队中的
//!   执行 Job 置为 cancelled；
//! - 法律保留（`legal_hold_at` 非空，RETENTION-PRIVACY.md §1 最高优先级）：
//!   禁止发起请求；若保留在冷却期内被设置，到期 Job 跳过执行并写审计
//!   `user.deletion_deferred_legal_hold`（账户保持 pending_delete，保留解除
//!   后由管理端重新触发，M13）；
//! - 全部生命周期迁移在业务事务内写 audit_logs（append-only、无删除 API，
//!   M01-AUDIT-01/08），注销本身不删除审计——即"不可删除审计"。

use serde_json::json;
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::db::DatabasePool;
use crate::jobs::retry::RetryClass;
use crate::jobs::worker::ClaimedJob;
use crate::jobs::worker_loop::JobOutcome;
use crate::outbox::now_millis;

/// 匿名化用户的派生用户名前缀（不可识别、全局唯一）。
const DELETED_USERNAME_PREFIX: &str = "deleted_user_";
/// 匿名化邮箱域名（不可路由，RFC 2606 `.invalid`）。
const DELETED_EMAIL_DOMAIN: &str = "@deleted.invalid";

/// 注销冷却期（RETENTION-PRIVACY.md §矩阵：注销延迟 30 天）。
pub const DELETION_COOLDOWN_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// `account_deletion` 执行 Job 的 kind。
pub const ACCOUNT_DELETION_JOB_KIND: &str = "account_deletion";
/// 执行 Job 队列（与 worker 默认 queue 一致）。
const ACCOUNT_DELETION_QUEUE: &str = "default";
/// 执行 Job 去重键前缀（每用户至多一个有效注销 Job，去重键唯一约束兜底）。
const ACCOUNT_DELETION_DEDUP_PREFIX: &str = "account_deletion:";

/// 执行注销匿名化（单事务，幂等：已 deleted 的行再次调用无副作用）。
pub async fn anonymize_user(pool: &DatabasePool, user_id: &str) -> Result<(), String> {
    let now = now_millis();
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin().await.map_err(|e| e.to_string())?),
        Either::Right(p) => Either::Right(p.begin().await.map_err(|e| e.to_string())?),
    };
    anonymize_user_in_tx(&mut tx, user_id, now).await?;
    match tx {
        Either::Left(t) => t.commit().await.map_err(|e| e.to_string()),
        Either::Right(t) => t.commit().await.map_err(|e| e.to_string()),
    }
}

/// 注销匿名化的事务内执行体（供公共包装与 Job 执行共享）。
///
/// 返回是否真正匿名化（`false` = 行不存在或已是 deleted，幂等无操作）。
/// 调用方负责在同一事务内写审计并提交（M01-AUDIT-08：高风险操作必须先写
/// 审计再提交业务事务）。
async fn anonymize_user_in_tx(
    tx: &mut crate::outbox::OutboxTx<'_>,
    user_id: &str,
    now: i64,
) -> Result<bool, String> {
    let short_id = &user_id[..user_id.len().min(12)];
    let anonymous_username = format!("{DELETED_USERNAME_PREFIX}{short_id}");
    let anonymous_email = format!("{short_id}{DELETED_EMAIL_DOMAIN}");

    // 1. users 就地匿名化
    let affected = match tx {
        Either::Left(t) => sqlx::query(
            "UPDATE users
             SET username_normalized = ?,
                 email_normalized = ?,
                 display_name = NULL,
                 bio = NULL,
                 signature = NULL,
                 avatar_attachment_id = NULL,
                 cover_attachment_id = NULL,
                 last_login_at = NULL,
                 delete_requested_at = NULL,
                 deleted_at = ?,
                 status = 'deleted',
                 level = 1,
                 version = version + 1
             WHERE id = ? AND status != 'deleted'",
        )
        .bind(&anonymous_username)
        .bind(&anonymous_email)
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE users
             SET username_normalized = ?,
                 email_normalized = ?,
                 display_name = NULL,
                 bio = NULL,
                 signature = NULL,
                 avatar_attachment_id = NULL,
                 cover_attachment_id = NULL,
                 last_login_at = NULL,
                 delete_requested_at = NULL,
                 deleted_at = ?,
                 status = 'deleted',
                 level = 1,
                 version = version + 1
             WHERE id = ? AND status != 'deleted'",
        )
        .bind(&anonymous_username)
        .bind(&anonymous_email)
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
    };
    if affected == 0 {
        // 用户不存在或已匿名化（幂等）
        return Ok(false);
    }

    // 2. 断开可识别资料关系：删除私有偏好/隐私行
    match tx {
        Either::Left(t) => {
            sqlx::query("DELETE FROM user_preferences WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM user_privacy WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
        }
        Either::Right(t) => {
            sqlx::query("DELETE FROM user_preferences WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query("DELETE FROM user_privacy WHERE user_id = ?")
                .bind(user_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
        }
    }

    // 3. 立即撤销全部 Session（含设备）
    match tx {
        Either::Left(t) => {
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = ?, revoke_reason = 'account_deleted'
                 WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(t) => {
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = ?, revoke_reason = 'account_deleted'
                 WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    Ok(true)
}

// ────────────────────────── 注销生命周期（M03-PROFILE-08）────────────────────

/// 注销请求结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeletionRequest {
    /// 请求时间（Unix 毫秒）。
    pub requested_at: i64,
    /// 预计执行时间 = requested_at + 冷却期。
    pub executes_at: i64,
}

/// 注销请求错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionRequestError {
    /// 已注销（终态，不可再请求）。
    AlreadyDeleted,
    /// 已在冷却期且有活跃执行 Job（幂等冲突：不再重复入队；由
    /// `enqueue_account_deletion_job` 去重键兜底返回）。
    AlreadyPending,
    /// 法律保留/调查冻结中（RETENTION-PRIVACY.md §1 最高优先级）。
    LegalHold,
    /// 未验证账户（状态机 pending_verification 尚未进入 active）。
    Unverified,
    /// 处罚中（banned）：不得通过自助注销绕过 sanction/案件链路
    /// （STATE-MACHINES.md §2；MODERATION.md §11）。
    Banned,
    Database(String),
}

impl std::fmt::Display for DeletionRequestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyDeleted => write!(f, "account already deleted"),
            Self::AlreadyPending => write!(f, "deletion already requested"),
            Self::LegalHold => write!(f, "account under legal hold"),
            Self::Unverified => write!(f, "unverified account cannot request deletion"),
            Self::Banned => write!(f, "banned account cannot self-serve deletion"),
            Self::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for DeletionRequestError {}

/// 取消注销错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CancelDeletionError {
    /// 不在冷却期（未请求过或已注销）。
    NotPending,
    Database(String),
}

impl std::fmt::Display for CancelDeletionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotPending => write!(f, "no pending deletion request"),
            Self::Database(e) => write!(f, "database error: {e}"),
        }
    }
}

impl std::error::Error for CancelDeletionError {}

/// 到期执行注销的结果（供 worker 映射 [`JobOutcome`]）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeletionExecution {
    /// 已匿名化（终态）。
    Executed,
    /// 已是 deleted（幂等，无需执行）。
    AlreadyDeleted,
    /// 已不在冷却期（期间被取消/从未请求）。
    NotPending,
    /// 法律保留例外：跳过执行，账户保持 pending_delete（审计已记录）。
    DeferredByLegalHold,
    /// 冷却期未到（调度器兜底，worker 应重试）。
    NotYetDue,
}

/// 注销状态行（查询辅助）。
#[derive(sqlx::FromRow)]
struct DeletionStateRow {
    status: String,
    legal_hold_at: Option<i64>,
    delete_requested_at: Option<i64>,
}

async fn read_deletion_state(
    tx: &mut crate::outbox::OutboxTx<'_>,
    user_id: &str,
) -> Result<Option<DeletionStateRow>, DeletionRequestError> {
    let row = match tx {
        Either::Left(t) => sqlx::query_as::<_, DeletionStateRow>(
            "SELECT status, legal_hold_at, delete_requested_at FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&mut **t)
        .await
        .map_err(|e| DeletionRequestError::Database(e.to_string()))?,
        Either::Right(t) => sqlx::query_as::<_, DeletionStateRow>(
            "SELECT status, legal_hold_at, delete_requested_at FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(&mut **t)
        .await
        .map_err(|e| DeletionRequestError::Database(e.to_string()))?,
    };
    Ok(row)
}

/// 发起注销请求：`active`/`restricted` → `pending_delete`，写冷却时间并入队
/// 到期执行 Job（单事务：状态变更 + Job 入队 + 审计原子提交，M01-AUDIT-08）。
///
/// 幂等：已在 `pending_delete` 时返回 `Ok`（保留原请求时间与截止时间，不重复
/// 入队）；去重键唯一约束兜底保证每用户至多一个有效执行 Job。
pub async fn request_deletion(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<DeletionRequest, DeletionRequestError> {
    let now = now_millis();
    let deadline = now + DELETION_COOLDOWN_MS;

    let mut tx = match pool {
        Either::Left(p) => Either::Left(
            p.begin()
                .await
                .map_err(|e| DeletionRequestError::Database(e.to_string()))?,
        ),
        Either::Right(p) => Either::Right(
            p.begin()
                .await
                .map_err(|e| DeletionRequestError::Database(e.to_string()))?,
        ),
    };

    // 1) 读取并裁决状态
    let state = read_deletion_state(&mut tx, user_id).await?;
    let Some(state) = state else {
        return Err(DeletionRequestError::AlreadyDeleted);
    };
    match state.status.as_str() {
        "deleted" => return Err(DeletionRequestError::AlreadyDeleted),
        "pending_delete" => {
            let requested = state.delete_requested_at.unwrap_or(now);
            return Ok(DeletionRequest {
                requested_at: requested,
                executes_at: requested + DELETION_COOLDOWN_MS,
            });
        }
        _ => {}
    }
    if state.legal_hold_at.is_some() {
        return Err(DeletionRequestError::LegalHold);
    }
    match state.status.as_str() {
        "pending" => return Err(DeletionRequestError::Unverified),
        "banned" => return Err(DeletionRequestError::Banned),
        "active" | "restricted" => {}
        other => {
            return Err(DeletionRequestError::Database(format!(
                "unknown status: {other}"
            )))
        }
    }

    // 2) 状态迁移（带并发守卫）
    let affected = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE users
             SET status = 'pending_delete', delete_requested_at = ?,
                 updated_at = ?, version = version + 1
             WHERE id = ? AND status IN ('active', 'restricted') AND legal_hold_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map_err(|e| DeletionRequestError::Database(e.to_string()))?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE users
             SET status = 'pending_delete', delete_requested_at = ?,
                 updated_at = ?, version = version + 1
             WHERE id = ? AND status IN ('active', 'restricted') AND legal_hold_at IS NULL",
        )
        .bind(now)
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map_err(|e| DeletionRequestError::Database(e.to_string()))?
        .rows_affected(),
    };
    if affected == 0 {
        // 并发变更：重新裁决一次
        let state = read_deletion_state(&mut tx, user_id).await?;
        let Some(state) = state else {
            return Err(DeletionRequestError::AlreadyDeleted);
        };
        match state.status.as_str() {
            "deleted" => return Err(DeletionRequestError::AlreadyDeleted),
            "pending_delete" => {
                let requested = state.delete_requested_at.unwrap_or(now);
                return Ok(DeletionRequest {
                    requested_at: requested,
                    executes_at: requested + DELETION_COOLDOWN_MS,
                });
            }
            _ => {}
        }
        if state.legal_hold_at.is_some() {
            return Err(DeletionRequestError::LegalHold);
        }
        return Err(DeletionRequestError::Database(
            "并发状态变更，请重试".to_string(),
        ));
    }

    // 3) 入队到期执行 Job（available_at = 冷却结束；去重键 = 用户）
    enqueue_account_deletion_job(&mut tx, user_id, deadline).await?;

    // 4) 审计（事务内，不可删除）
    AuditEntry::user_action(user_id, "user.deletion_requested")
        .with_target("user", user_id)
        .with_metadata(json!({ "status": "pending_delete", "executes_at": deadline }))
        .record_in_tx(&mut tx)
        .await
        .map_err(|e| DeletionRequestError::Database(e.to_string()))?;

    match tx {
        Either::Left(t) => t
            .commit()
            .await
            .map_err(|e| DeletionRequestError::Database(e.to_string()))?,
        Either::Right(t) => t
            .commit()
            .await
            .map_err(|e| DeletionRequestError::Database(e.to_string()))?,
    }
    Ok(DeletionRequest {
        requested_at: now,
        executes_at: deadline,
    })
}

/// 在业务事务内入队 `account_deletion` Job。
///
/// 去重键冲突（此前取消/死信的历史 Job 仍占用唯一键）时重武装该行
/// （`cancelled`/`dead` → `queued`，重置 attempts、更新 available_at 与 payload），
/// 使"取消后再请求"正确生效；活跃 Job 冲突返回 `AlreadyPending`。
async fn enqueue_account_deletion_job<'e>(
    tx: &mut crate::outbox::OutboxTx<'e>,
    user_id: &str,
    available_at: i64,
) -> Result<(), DeletionRequestError> {
    let id = uuid::Uuid::now_v7().to_string();
    let payload = json!({ "user_id": user_id });
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let dedup = format!("{ACCOUNT_DELETION_DEDUP_PREFIX}{user_id}");
    let now = now_millis();

    let result = match tx {
        Either::Left(t) => sqlx::query(
            "INSERT INTO jobs
                 (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                  available_at, deduplication_key, created_at, updated_at)
             VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(ACCOUNT_DELETION_QUEUE)
        .bind(ACCOUNT_DELETION_JOB_KIND)
        .bind(&payload_str)
        .bind(available_at)
        .bind(&dedup)
        .bind(now)
        .bind(now)
        .execute(&mut **t)
        .await
        .map(|_| ()),
        Either::Right(t) => sqlx::query(
            "INSERT INTO jobs
                 (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                  available_at, deduplication_key, created_at, updated_at)
             VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(ACCOUNT_DELETION_QUEUE)
        .bind(ACCOUNT_DELETION_JOB_KIND)
        .bind(&payload_str)
        .bind(available_at)
        .bind(&dedup)
        .bind(now)
        .bind(now)
        .execute(&mut **t)
        .await
        .map(|_| ()),
    };

    match result {
        Ok(()) => Ok(()),
        Err(sqlx::Error::Database(ref db)) if db.is_unique_violation() => {
            // 历史 Job 占用去重键：重武装 cancelled/dead 行，活跃 Job 视为幂等。
            let rearmed = match tx {
                Either::Left(t) => sqlx::query(
                    "UPDATE jobs
                     SET status = 'queued', available_at = ?, attempts = 0,
                         last_error = NULL, completed_at = NULL, payload = ?,
                         updated_at = ?
                     WHERE deduplication_key = ? AND status IN ('cancelled', 'dead')",
                )
                .bind(available_at)
                .bind(&payload_str)
                .bind(now)
                .bind(&dedup)
                .execute(&mut **t)
                .await
                .map_err(|e| DeletionRequestError::Database(e.to_string()))?
                .rows_affected(),
                Either::Right(t) => sqlx::query(
                    "UPDATE jobs
                     SET status = 'queued', available_at = ?, attempts = 0,
                         last_error = NULL, completed_at = NULL, payload = ?,
                         updated_at = ?
                     WHERE deduplication_key = ? AND status IN ('cancelled', 'dead')",
                )
                .bind(available_at)
                .bind(&payload_str)
                .bind(now)
                .bind(&dedup)
                .execute(&mut **t)
                .await
                .map_err(|e| DeletionRequestError::Database(e.to_string()))?
                .rows_affected(),
            };
            if rearmed == 1 {
                Ok(())
            } else {
                Err(DeletionRequestError::AlreadyPending)
            }
        }
        Err(e) => Err(DeletionRequestError::Database(e.to_string())),
    }
}

/// 取消注销（冷却期内本人撤销）：`pending_delete` → `active`，清空
/// `delete_requested_at`，取消排队/重试中的执行 Job，事务内写审计。
pub async fn cancel_deletion(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<(), CancelDeletionError> {
    let now = now_millis();

    let mut tx = match pool {
        Either::Left(p) => Either::Left(
            p.begin()
                .await
                .map_err(|e| CancelDeletionError::Database(e.to_string()))?,
        ),
        Either::Right(p) => Either::Right(
            p.begin()
                .await
                .map_err(|e| CancelDeletionError::Database(e.to_string()))?,
        ),
    };

    // 1) 恢复 active（仅冷却期有效；已注销/未请求 → 0 行）
    let affected = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE users
             SET status = 'active', delete_requested_at = NULL,
                 updated_at = ?, version = version + 1
             WHERE id = ? AND status = 'pending_delete'",
        )
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map_err(|e| CancelDeletionError::Database(e.to_string()))?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE users
             SET status = 'active', delete_requested_at = NULL,
                 updated_at = ?, version = version + 1
             WHERE id = ? AND status = 'pending_delete'",
        )
        .bind(now)
        .bind(user_id)
        .execute(&mut **t)
        .await
        .map_err(|e| CancelDeletionError::Database(e.to_string()))?
        .rows_affected(),
    };
    if affected == 0 {
        return Err(CancelDeletionError::NotPending);
    }

    // 2) 取消排队/重试中的执行 Job（状态机 queued/retry_wait → cancelled）
    let dedup = format!("{ACCOUNT_DELETION_DEDUP_PREFIX}{user_id}");
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "UPDATE jobs
                 SET status = 'cancelled', updated_at = ?
                 WHERE kind = ? AND deduplication_key = ? AND status IN ('queued', 'retry_wait')",
            )
            .bind(now)
            .bind(ACCOUNT_DELETION_JOB_KIND)
            .bind(&dedup)
            .execute(&mut **t)
            .await
            .map_err(|e| CancelDeletionError::Database(e.to_string()))?;
        }
        Either::Right(t) => {
            sqlx::query(
                "UPDATE jobs
                 SET status = 'cancelled', updated_at = ?
                 WHERE kind = ? AND deduplication_key = ? AND status IN ('queued', 'retry_wait')",
            )
            .bind(now)
            .bind(ACCOUNT_DELETION_JOB_KIND)
            .bind(&dedup)
            .execute(&mut **t)
            .await
            .map_err(|e| CancelDeletionError::Database(e.to_string()))?;
        }
    }

    // 3) 审计（事务内，不可删除）
    AuditEntry::user_action(user_id, "user.deletion_cancelled")
        .with_target("user", user_id)
        .record_in_tx(&mut tx)
        .await
        .map_err(|e| CancelDeletionError::Database(e.to_string()))?;

    match tx {
        Either::Left(t) => t
            .commit()
            .await
            .map_err(|e| CancelDeletionError::Database(e.to_string()))?,
        Either::Right(t) => t
            .commit()
            .await
            .map_err(|e| CancelDeletionError::Database(e.to_string()))?,
    }
    Ok(())
}

/// 执行到期注销（Job handler 主体）。幂等；法律保留例外跳过并写审计。
pub async fn execute_account_deletion(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<DeletionExecution, String> {
    let now = now_millis();

    let state: Option<DeletionStateRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, DeletionStateRow>(
            "SELECT status, legal_hold_at, delete_requested_at FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, DeletionStateRow>(
            "SELECT status, legal_hold_at, delete_requested_at FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    let Some(state) = state else {
        return Ok(DeletionExecution::AlreadyDeleted);
    };

    if state.status == "deleted" {
        return Ok(DeletionExecution::AlreadyDeleted);
    }
    if state.status != "pending_delete" {
        return Ok(DeletionExecution::NotPending);
    }
    if state.legal_hold_at.is_some() {
        // 法律保留例外：跳过执行并写审计（账户保持 pending_delete，保留解除后
        // 由管理端重新触发，M13）。审计不可删除（M01-AUDIT-01）。
        AuditEntry::user_action(user_id, "user.deletion_deferred_legal_hold")
            .with_target("user", user_id)
            .with_reason("legal hold")
            .record(pool)
            .await
            .map_err(|e| e.to_string())?;
        return Ok(DeletionExecution::DeferredByLegalHold);
    }
    let due_at = state.delete_requested_at.unwrap_or(0) + DELETION_COOLDOWN_MS;
    if due_at > now {
        // 冷却未到（调度器兜底：正常路径 available_at 已按 deadline 入队）
        return Ok(DeletionExecution::NotYetDue);
    }

    // 执行匿名化 + 审计在同一事务提交（M01-AUDIT-08：无审计不得提交）
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin().await.map_err(|e| e.to_string())?),
        Either::Right(p) => Either::Right(p.begin().await.map_err(|e| e.to_string())?),
    };
    anonymize_user_in_tx(&mut tx, user_id, now).await?;
    AuditEntry::user_action(user_id, "user.deletion_executed")
        .with_target("user", user_id)
        .with_metadata(json!({ "status": "deleted" }))
        .record_in_tx(&mut tx)
        .await
        .map_err(|e| e.to_string())?;
    match tx {
        Either::Left(t) => t.commit().await.map_err(|e| e.to_string())?,
        Either::Right(t) => t.commit().await.map_err(|e| e.to_string())?,
    }
    Ok(DeletionExecution::Executed)
}

/// Worker 集成入口：解析 `account_deletion` Job payload 并映射为
/// [`JobOutcome`]。终态/跳过 → `Succeeded`；冷却未到/数据库错误 → 重试；
/// 无效 payload → 永久死信。
pub async fn handle_account_deletion(pool: &DatabasePool, job: &ClaimedJob) -> JobOutcome {
    let user_id = match job.payload.get("user_id").and_then(|v| v.as_str()) {
        Some(id) if !id.is_empty() => id,
        _ => {
            return JobOutcome::Failed {
                class: RetryClass::Permanent,
                error: "account_deletion: invalid payload: missing user_id".to_owned(),
            }
        }
    };
    match execute_account_deletion(pool, user_id).await {
        Ok(
            DeletionExecution::Executed
            | DeletionExecution::AlreadyDeleted
            | DeletionExecution::NotPending
            | DeletionExecution::DeferredByLegalHold,
        ) => JobOutcome::Succeeded,
        Ok(DeletionExecution::NotYetDue) => JobOutcome::Failed {
            class: RetryClass::Transient,
            error: "account_deletion: not yet due".to_owned(),
        },
        Err(e) => JobOutcome::Failed {
            class: RetryClass::Transient,
            error: format!("account_deletion: {e}"),
        },
    }
}
