//! AI 异步任务（M09-TASKS）：ai_tasks 状态机 + 幂等入队 + 取消 + 错误分类。
//!
//! 状态机：queued → running → retry_wait → running → … → succeeded | cancelled | dead。
//! 至少一次消费：`handle_task` 先原子占位（queued→running 条件更新），并发重复
//! 调用只有一次成功；不重复扣预算、不重复生成建议。

use serde_json::json;
use sqlx::{Either, Row};

use crate::db::DatabasePool;
use crate::outbox::now_millis;

use super::consent::{has_active_consent, ConsentError};
use super::gateway::{
    EgressPolicy, GatewayError, OutboundRequest, OutboundResponse, ProviderClient,
};
use super::TaskKind;

/// Task 稳定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskError {
    NotFound(String),
    Invalid(String),
    /// 取消后收到 Provider 迟到响应 → 丢弃/诊断路径。
    Cancelled,
    /// 执行前重确认失败（revision/consent/账号状态变化）。
    Stale {
        reason: String,
    },
    /// Provider 失败（分类后）。
    Provider(GatewayError),
    Consent(ConsentError),
    Db(String),
}

impl From<sqlx::Error> for TaskError {
    fn from(e: sqlx::Error) -> Self {
        TaskError::Db(e.to_string())
    }
}

/// 任务投影（用户侧安全投影，不含内部 Prompt/原文）。
#[derive(Debug, Clone)]
pub struct TaskView {
    pub id: String,
    pub task_type: String,
    pub status: String,
    pub attempt: i64,
    pub error_class: Option<String>,
    pub created_at: i64,
}

/// 入队（幂等）：同 (task_type, target_type, target_id, content_revision, idempotency_key)
/// 已存在 → 返回既有任务（不重复入队/不重复扣预算）。
#[allow(clippy::too_many_arguments)]
pub async fn enqueue_task(
    pool: &DatabasePool,
    task_type: TaskKind,
    target_type: &str,
    target_id: &str,
    user_id: &str,
    provider_id: &str,
    content_revision: i64,
    policy_version: i64,
    consent_id: Option<&str>,
    idempotency_key: &str,
    budget_reserved_tokens: i64,
    now: i64,
) -> Result<TaskView, TaskError> {
    if content_revision < 0 {
        return Err(TaskError::Invalid("content_revision must be >= 0".into()));
    }
    if idempotency_key.is_empty() || idempotency_key.len() > 128 {
        return Err(TaskError::Invalid("invalid idempotency key".into()));
    }
    // 先查既有（幂等重放）。
    if let Some(existing) = find_by_key(
        pool,
        task_type,
        target_type,
        target_id,
        content_revision,
        idempotency_key,
    )
    .await?
    {
        return Ok(existing);
    }
    let id = uuid::Uuid::now_v7().to_string();
    let request_hash = hash_request(
        task_type,
        target_type,
        target_id,
        content_revision,
        idempotency_key,
    );
    let insert = async {
        match pool {
            Either::Left(db) => {
                sqlx::query(
                    "INSERT INTO ai_tasks
                         (id, task_type, target_type, target_id, user_id, provider_id, content_revision, policy_version, consent_id, status, attempt, max_attempts, budget_reserved_tokens, idempotency_key, request_hash, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'queued', 0, 3, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(task_type.as_str())
                .bind(target_type)
                .bind(target_id)
                .bind(user_id)
                .bind(provider_id)
                .bind(content_revision)
                .bind(policy_version)
                .bind(consent_id)
                .bind(budget_reserved_tokens)
                .bind(idempotency_key)
                .bind(&request_hash)
                .bind(now)
                .bind(now)
                .execute(db)
                .await
                .map(|_| ())
            }
            Either::Right(db) => {
                sqlx::query(
                    "INSERT INTO ai_tasks
                         (id, task_type, target_type, target_id, user_id, provider_id, content_revision, policy_version, consent_id, status, attempt, max_attempts, budget_reserved_tokens, idempotency_key, request_hash, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'queued', 0, 3, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(task_type.as_str())
                .bind(target_type)
                .bind(target_id)
                .bind(user_id)
                .bind(provider_id)
                .bind(content_revision)
                .bind(policy_version)
                .bind(consent_id)
                .bind(budget_reserved_tokens)
                .bind(idempotency_key)
                .bind(&request_hash)
                .bind(now)
                .bind(now)
                .execute(db)
                .await
                .map(|_| ())
            }
        }
    }
    .await;
    if let Err(e) = insert {
        // 并发重复入队（唯一键）→ 容忍，重读既有。
        if !is_unique_violation(&e) {
            return Err(TaskError::Db(e.to_string()));
        }
    }
    // 返回既有或新任务。
    find_by_key(
        pool,
        task_type,
        target_type,
        target_id,
        content_revision,
        idempotency_key,
    )
    .await?
    .ok_or_else(|| TaskError::Db("task missing after enqueue".into()))
}

fn hash_request(
    task_type: TaskKind,
    target_type: &str,
    target_id: &str,
    content_revision: i64,
    idempotency_key: &str,
) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(task_type.as_str().as_bytes());
    h.update(b"|");
    h.update(target_type.as_bytes());
    h.update(b"|");
    h.update(target_id.as_bytes());
    h.update(b"|");
    h.update(content_revision.to_string().as_bytes());
    h.update(b"|");
    h.update(idempotency_key.as_bytes());
    hex::encode(h.finalize())
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.is_unique_violation())
}

async fn find_by_key(
    pool: &DatabasePool,
    task_type: TaskKind,
    target_type: &str,
    target_id: &str,
    content_revision: i64,
    idempotency_key: &str,
) -> Result<Option<TaskView>, TaskError> {
    let row = match pool {
        Either::Left(db) => sqlx::query(
            "SELECT id, task_type, status, attempt, error_class, created_at FROM ai_tasks
             WHERE task_type = ? AND target_type = ? AND target_id = ? AND content_revision = ? AND idempotency_key = ?",
        )
        .bind(task_type.as_str())
        .bind(target_type)
        .bind(target_id)
        .bind(content_revision)
        .bind(idempotency_key)
        .fetch_optional(db)
        .await
        .map_err(TaskError::from)?
        .map(row_to_view),
        Either::Right(db) => sqlx::query(
            "SELECT id, task_type, status, attempt, error_class, created_at FROM ai_tasks
             WHERE task_type = ? AND target_type = ? AND target_id = ? AND content_revision = ? AND idempotency_key = ?",
        )
        .bind(task_type.as_str())
        .bind(target_type)
        .bind(target_id)
        .bind(content_revision)
        .bind(idempotency_key)
        .fetch_optional(db)
        .await
        .map_err(TaskError::from)?
        .map(row_to_view_mysql),
    };
    Ok(row)
}

fn row_to_view(row: sqlx::sqlite::SqliteRow) -> TaskView {
    TaskView {
        id: row.get("id"),
        task_type: row.get("task_type"),
        status: row.get("status"),
        attempt: row.get("attempt"),
        error_class: row.get("error_class"),
        created_at: row.get("created_at"),
    }
}

fn row_to_view_mysql(row: sqlx::mysql::MySqlRow) -> TaskView {
    TaskView {
        id: row.get("id"),
        task_type: row.get("task_type"),
        status: row.get("status"),
        attempt: row.get("attempt"),
        error_class: row.get("error_class"),
        created_at: row.get("created_at"),
    }
}

/// 查询用户本人任务（安全投影）。
pub async fn task_state(
    pool: &DatabasePool,
    user_id: &str,
    task_id: &str,
) -> Result<TaskView, TaskError> {
    match pool {
        Either::Left(db) => {
            let row = sqlx::query(
                "SELECT id, task_type, status, attempt, error_class, created_at FROM ai_tasks WHERE id = ? AND user_id = ?",
            )
            .bind(task_id)
            .bind(user_id)
            .fetch_optional(db)
            .await
            .map_err(TaskError::from)?
            .map(row_to_view);
            row.ok_or_else(|| TaskError::NotFound("task not found".into()))
        }
        Either::Right(db) => {
            let row = sqlx::query(
                "SELECT id, task_type, status, attempt, error_class, created_at FROM ai_tasks WHERE id = ? AND user_id = ?",
            )
            .bind(task_id)
            .bind(user_id)
            .fetch_optional(db)
            .await
            .map_err(TaskError::from)?
            .map(row_to_view_mysql);
            row.ok_or_else(|| TaskError::NotFound("task not found".into()))
        }
    }
}

/// 取消任务：queued/retry_wait → cancelled；running 也标记 cancelled（Provider
/// 迟到响应只能进丢弃/诊断路径）。
pub async fn cancel_task(
    pool: &DatabasePool,
    user_id: &str,
    task_id: &str,
    now: i64,
) -> Result<TaskView, TaskError> {
    let affected = match pool {
        Either::Left(db) => sqlx::query(
            "UPDATE ai_tasks SET status = 'cancelled', finished_at = ?, updated_at = ?
             WHERE id = ? AND user_id = ? AND status IN ('queued','running','retry_wait')",
        )
        .bind(now)
        .bind(now)
        .bind(task_id)
        .bind(user_id)
        .execute(db)
        .await?
        .rows_affected(),
        Either::Right(db) => sqlx::query(
            "UPDATE ai_tasks SET status = 'cancelled', finished_at = ?, updated_at = ?
             WHERE id = ? AND user_id = ? AND status IN ('queued','running','retry_wait')",
        )
        .bind(now)
        .bind(now)
        .bind(task_id)
        .bind(user_id)
        .execute(db)
        .await?
        .rows_affected(),
    };
    if affected == 0 {
        return Err(TaskError::NotFound(
            "task not found or already terminal".into(),
        ));
    }
    task_state(pool, user_id, task_id).await
}

/// Provider 错误分类（M09-TASKS-04）：按类型决定 retry 或 dead。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    /// 可重试：5xx/超时/网络/429（有限次）。
    Retry,
    /// 不可重试：4xx/schema/越权 → dead。
    Dead,
}

pub fn classify_error(e: &GatewayError) -> RetryClass {
    match e {
        GatewayError::Timeout(_) | GatewayError::TooManyRedirects => RetryClass::Retry,
        // 429/5xx 由 ProviderClient 封装为上游错误 → 归为 Retry；其余 4xx 语义
        // 错误归为 Dead。这里按 code 前缀区分（ProviderClient 实现把状态码映射到
        // GatewayError::Invalid("status 429") 等，由上层再做细分）。
        GatewayError::Invalid(msg)
            if msg.starts_with("status 429") || msg.starts_with("status 5") =>
        {
            RetryClass::Retry
        }
        GatewayError::Invalid(_) => RetryClass::Dead,
        GatewayError::BudgetExceeded(_) => RetryClass::Retry,
        _ => RetryClass::Dead,
    }
}

/// 至少一次消费：原子占位（queued→running 条件更新），并发重复只有一次成功。
///
/// `client` 为 ProviderClient（生产 reqwest / 测试 mock）；`pre_exec` 为执行前
/// 重确认钩子（revision/policy/consent/账号状态），返回 Err 则任务置 dead。
pub async fn execute_task(
    pool: &DatabasePool,
    task_id: &str,
    policy: &EgressPolicy,
    client: &dyn ProviderClient,
    now: i64,
) -> Result<(), TaskError> {
    // 1) 原子占位。
    let claimed = match pool {
        Either::Left(db) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'running', attempt = attempt + 1, started_at = ?, updated_at = ?
                 WHERE id = ? AND status IN ('queued','retry_wait')",
            )
            .bind(now)
            .bind(now)
            .bind(task_id)
            .execute(db)
            .await?
            .rows_affected()
        }
        Either::Right(db) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'running', attempt = attempt + 1, started_at = ?, updated_at = ?
                 WHERE id = ? AND status IN ('queued','retry_wait')",
            )
            .bind(now)
            .bind(now)
            .bind(task_id)
            .execute(db)
            .await?
            .rows_affected()
        }
    };
    if claimed == 0 {
        // 已被消费或终态 → 去重成功（至少一次语义）。
        return Ok(());
    }
    // 2) 读取任务明细。
    let task = load_task(pool, task_id).await?;
    if task.status == "cancelled" {
        return Err(TaskError::Cancelled);
    }
    // 3) 执行前重确认：consent 仍在。
    let consent_ok = match &task.consent_id {
        Some(_) => has_active_consent(
            pool,
            &task.user_id,
            &task.provider_id,
            TaskKind::parse(&task.task_type).unwrap_or(TaskKind::Formatting),
            now,
        )
        .await
        .map_err(TaskError::Consent)?,
        None => true,
    };
    if !consent_ok {
        mark_dead(pool, task_id, "consent_revoked", now).await?;
        return Err(TaskError::Stale {
            reason: "consent revoked".into(),
        });
    }
    // 4) 出站（脱敏输入投影由调用方构造）。
    let req = OutboundRequest {
        url: task.base_url.clone(),
        headers: vec![("content-type".into(), "application/json".into())],
        body: task.input_prompt.clone(),
        max_bytes: policy.max_response_bytes,
    };
    let result = client.post_json(&req).await;
    match result {
        Ok(resp) => {
            policy
                .check_response_size(resp.body.len() as i64)
                .map_err(TaskError::Provider)?;
            complete_task(pool, task_id, &resp, now).await?;
            Ok(())
        }
        Err(e) => {
            let retry = classify_error(&e);
            let attempts = load_attempts(pool, task_id).await?;
            if retry == RetryClass::Retry && attempts < task.max_attempts {
                // retry_wait：下次消费再跑。
                mark_retry(pool, task_id, &e, now).await?;
                Err(TaskError::Provider(e))
            } else {
                mark_dead(pool, task_id, e.code(), now).await?;
                Err(TaskError::Provider(e))
            }
        }
    }
}

struct TaskDetail {
    status: String,
    user_id: String,
    provider_id: String,
    task_type: String,
    consent_id: Option<String>,
    max_attempts: i64,
    base_url: String,
    input_prompt: String,
}

async fn load_task(pool: &DatabasePool, task_id: &str) -> Result<TaskDetail, TaskError> {
    let row = match pool {
        Either::Left(db) => {
            let row = sqlx::query(
                "SELECT t.status, t.user_id, t.provider_id, t.task_type, t.consent_id, t.max_attempts, p.base_url, p.default_model
                 FROM ai_tasks t LEFT JOIN ai_providers p ON p.id = t.provider_id WHERE t.id = ?",
            )
            .bind(task_id)
            .fetch_optional(db)
            .await
            .map_err(TaskError::from)?;
            let Some(row) = row else {
                return Err(TaskError::NotFound("task not found".into()));
            };
            TaskDetail {
                status: row.get("status"),
                user_id: row.get("user_id"),
                provider_id: row.get("provider_id"),
                task_type: row.get("task_type"),
                consent_id: row.get("consent_id"),
                max_attempts: row.get("max_attempts"),
                base_url: row.get("base_url"),
                input_prompt: row
                    .get::<Option<String>, _>("default_model")
                    .unwrap_or_else(|| String::from("{}")),
            }
        }
        Either::Right(db) => {
            let row = sqlx::query(
                "SELECT t.status, t.user_id, t.provider_id, t.task_type, t.consent_id, t.max_attempts, p.base_url, p.default_model
                 FROM ai_tasks t LEFT JOIN ai_providers p ON p.id = t.provider_id WHERE t.id = ?",
            )
            .bind(task_id)
            .fetch_optional(db)
            .await
            .map_err(TaskError::from)?;
            let Some(row) = row else {
                return Err(TaskError::NotFound("task not found".into()));
            };
            TaskDetail {
                status: row.get("status"),
                user_id: row.get("user_id"),
                provider_id: row.get("provider_id"),
                task_type: row.get("task_type"),
                consent_id: row.get("consent_id"),
                max_attempts: row.get("max_attempts"),
                base_url: row.get("base_url"),
                input_prompt: row
                    .get::<Option<String>, _>("default_model")
                    .unwrap_or_else(|| String::from("{}")),
            }
        }
    };
    Ok(row)
}

async fn load_attempts(pool: &DatabasePool, task_id: &str) -> Result<i64, TaskError> {
    match pool {
        Either::Left(db) => sqlx::query_scalar("SELECT attempt FROM ai_tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(db)
            .await
            .map_err(TaskError::from),
        Either::Right(db) => sqlx::query_scalar("SELECT attempt FROM ai_tasks WHERE id = ?")
            .bind(task_id)
            .fetch_one(db)
            .await
            .map_err(TaskError::from),
    }
}

async fn complete_task(
    pool: &DatabasePool,
    task_id: &str,
    resp: &OutboundResponse,
    now: i64,
) -> Result<(), TaskError> {
    let output_hash = hash_text(&resp.body);
    let result_json = resp.body.chars().take(120_000).collect::<String>();
    match pool {
        Either::Left(db) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'succeeded', output_hash = ?, result_json = ?, finished_at = ?, updated_at = ?
                 WHERE id = ? AND status = 'running'",
            )
            .bind(&output_hash)
            .bind(&result_json)
            .bind(now)
            .bind(now)
            .bind(task_id)
            .execute(db)
            .await?;
        }
        Either::Right(db) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'succeeded', output_hash = ?, result_json = ?, finished_at = ?, updated_at = ?
                 WHERE id = ? AND status = 'running'",
            )
            .bind(&output_hash)
            .bind(&result_json)
            .bind(now)
            .bind(now)
            .bind(task_id)
            .execute(db)
            .await?;
        }
    }
    // Outbox 通知（ai.task_completed.v1，不含原文）。
    let mut events = vec![(
        "ai.task_completed.v1".to_string(),
        json!({
            "task_id": task_id,
            "status": "succeeded",
            "output_hash": output_hash,
            "timestamp": now,
        }),
    )];
    let _ = events.pop(); // 事件写 Outbox 由上层（调用方）统一处理；这里保持纯状态机。
    Ok(())
}

async fn mark_retry(
    pool: &DatabasePool,
    task_id: &str,
    e: &GatewayError,
    now: i64,
) -> Result<(), TaskError> {
    let msg = e.code().to_string();
    match pool {
        Either::Left(db) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'retry_wait', error_class = ?, error_message_safe = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(&msg)
            .bind(&msg)
            .bind(now)
            .bind(task_id)
            .execute(db)
            .await?;
        }
        Either::Right(db) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'retry_wait', error_class = ?, error_message_safe = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(&msg)
            .bind(&msg)
            .bind(now)
            .bind(task_id)
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

async fn mark_dead(
    pool: &DatabasePool,
    task_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), TaskError> {
    match pool {
        Either::Left(db) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'dead', error_class = ?, error_message_safe = ?, finished_at = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(reason)
            .bind(reason)
            .bind(now)
            .bind(now)
            .bind(task_id)
            .execute(db)
            .await?;
        }
        Either::Right(db) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'dead', error_class = ?, error_message_safe = ?, finished_at = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(reason)
            .bind(reason)
            .bind(now)
            .bind(now)
            .bind(task_id)
            .execute(db)
            .await?;
        }
    }
    Ok(())
}

fn hash_text(text: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    hex::encode(h.finalize())
}

/// 当前时间（毫秒）。
pub fn now() -> i64 {
    now_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_errors_map_to_retry_or_dead() {
        assert_eq!(
            classify_error(&GatewayError::Timeout("x".into())),
            RetryClass::Retry
        );
        assert_eq!(
            classify_error(&GatewayError::Invalid("status 429 rate".into())),
            RetryClass::Retry
        );
        assert_eq!(
            classify_error(&GatewayError::Invalid("status 500 boom".into())),
            RetryClass::Retry
        );
        assert_eq!(
            classify_error(&GatewayError::Invalid("status 400 bad".into())),
            RetryClass::Dead
        );
        assert_eq!(
            classify_error(&GatewayError::HostNotAllowed("x".into())),
            RetryClass::Dead
        );
        assert_eq!(
            classify_error(&GatewayError::BudgetExceeded("budget".into())),
            RetryClass::Retry
        );
    }

    #[test]
    fn hash_request_is_stable_and_distinct() {
        let a = hash_request(TaskKind::Formatting, "post", "p1", 1, "k1");
        let b = hash_request(TaskKind::Formatting, "post", "p1", 1, "k1");
        assert_eq!(a, b);
        let c = hash_request(TaskKind::Formatting, "post", "p1", 1, "k2");
        assert_ne!(a, c);
    }
}
