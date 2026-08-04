//! 事务性发件箱模块 (Transactional Outbox Pattern)
//!
//! 确保领域事件在数据库事务中原子写入，然后由后台工作器异步处理。
//! 这避免了"数据库提交了但消息没发出"的常见问题。
//!
//! 工作流程：
//! 1. 在业务事务中调用 `enqueue` 写入 outbox_events
//! 2. 后台工作器定期调用 `process_pending` 处理待发送事件
//! 3. 处理成功标记为 'sent'，失败重试直到 max_attempts

use chrono::Utc;
use serde_json::Value;
use sqlx::Either;

use crate::db::pool::DatabasePool;

/// 发件箱事件类型
#[derive(Debug, Clone)]
pub struct OutboxEvent {
    pub id: String,
    pub event_type: String,
    pub payload: Value,
    pub status: String,
    pub attempts: i64,
    pub max_attempts: i64,
    pub created_at: i64,
    pub processed_at: Option<i64>,
    pub error: Option<String>,
}

/// 将事件入队（在业务事务中调用）
pub async fn enqueue(
    pool: &DatabasePool,
    event_type: &str,
    payload: Value,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = Utc::now().timestamp();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO outbox_events (id, event_type, payload, status, attempts, max_attempts, next_attempt_at, created_at)
                 VALUES (?, ?, ?, 'pending', 0, 5, ?, ?)",
            )
            .bind(&id)
            .bind(event_type)
            .bind(&payload_str)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO outbox_events (id, event_type, payload, status, attempts, max_attempts, next_attempt_at, created_at)
                 VALUES (?, ?, ?, 'pending', 0, 5, ?, ?)",
            )
            .bind(&id)
            .bind(event_type)
            .bind(&payload_str)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?;
        }
    }

    tracing::debug!(event_id = %id, event_type = %event_type, "outbox event enqueued");
    Ok(id)
}

/// 获取待处理事件（后台工作器调用）
pub async fn fetch_pending(
    pool: &DatabasePool,
    limit: i64,
) -> Result<Vec<OutboxEvent>, sqlx::Error> {
    let limit = limit.clamp(1, 50);
    let now = Utc::now().timestamp();

    match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, OutboxEventRow>(
                "SELECT id, event_type, payload, status, attempts, max_attempts, created_at, processed_at, error
                 FROM outbox_events
                 WHERE status = 'pending' AND next_attempt_at <= ?
                 ORDER BY created_at ASC LIMIT ?",
            )
            .bind(now)
            .bind(limit)
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, OutboxEventRow>(
                "SELECT id, event_type, payload, status, attempts, max_attempts, created_at, processed_at, error
                 FROM outbox_events
                 WHERE status = 'pending' AND next_attempt_at <= ?
                 ORDER BY created_at ASC LIMIT ?",
            )
            .bind(now)
            .bind(limit)
            .fetch_all(p)
            .await
        }
    }
    .map(|rows| {
        rows.into_iter()
            .map(|r| OutboxEvent {
                id: r.id,
                event_type: r.event_type,
                payload: serde_json::from_str(&r.payload).unwrap_or(Value::Null),
                status: r.status,
                attempts: r.attempts,
                max_attempts: r.max_attempts,
                created_at: r.created_at,
                processed_at: r.processed_at,
                error: r.error,
            })
            .collect()
    })
}

/// 标记事件为已处理
pub async fn mark_sent(pool: &DatabasePool, event_id: &str) -> Result<(), sqlx::Error> {
    let now = Utc::now().timestamp();

    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE outbox_events SET status = 'sent', processed_at = ? WHERE id = ?")
                .bind(now)
                .bind(event_id)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE outbox_events SET status = 'sent', processed_at = ? WHERE id = ?")
                .bind(now)
                .bind(event_id)
                .execute(p)
                .await?;
        }
    }

    Ok(())
}

/// 标记事件处理失败，安排重试或标记为最终失败
pub async fn mark_failed(
    pool: &DatabasePool,
    event_id: &str,
    error: &str,
) -> Result<(), sqlx::Error> {
    let now = Utc::now().timestamp();
    let retry_delay = 60; // 1 分钟后重试

    match pool {
        Either::Left(p) => {
            // 增加尝试次数，如果超过最大次数则标记为 failed，否则安排重试
            sqlx::query(
                "UPDATE outbox_events
                 SET attempts = attempts + 1,
                     error = ?,
                     status = CASE WHEN attempts + 1 >= max_attempts THEN 'failed' ELSE 'pending' END,
                     next_attempt_at = CASE WHEN attempts + 1 >= max_attempts THEN next_attempt_at ELSE ? END
                 WHERE id = ?",
            )
            .bind(error)
            .bind(now + retry_delay)
            .bind(event_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE outbox_events
                 SET attempts = attempts + 1,
                     error = ?,
                     status = CASE WHEN attempts + 1 >= max_attempts THEN 'failed' ELSE 'pending' END,
                     next_attempt_at = CASE WHEN attempts + 1 >= max_attempts THEN next_attempt_at ELSE ? END
                 WHERE id = ?",
            )
            .bind(error)
            .bind(now + retry_delay)
            .bind(event_id)
            .execute(p)
            .await?;
        }
    }

    Ok(())
}

/// 统计待处理事件数量（监控用）
pub async fn pending_count(pool: &DatabasePool) -> Result<i64, sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE status = 'pending'")
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE status = 'pending'")
                .fetch_one(p)
                .await
        }
    }
}

#[derive(sqlx::FromRow)]
struct OutboxEventRow {
    id: String,
    event_type: String,
    payload: String,
    status: String,
    attempts: i64,
    max_attempts: i64,
    created_at: i64,
    processed_at: Option<i64>,
    error: Option<String>,
}

#[cfg(test)]
mod tests {
    use serde_json::{json, Value};

    #[test]
    fn outbox_payload_serialization() {
        let payload = json!({
            "user_id": "123",
            "action": "registered",
            "timestamp": 1722816000,
        });
        let serialized = serde_json::to_string(&payload).unwrap();
        let deserialized: Value = serde_json::from_str(&serialized).unwrap();
        assert_eq!(deserialized["user_id"], "123");
        assert_eq!(deserialized["action"], "registered");
    }
}
