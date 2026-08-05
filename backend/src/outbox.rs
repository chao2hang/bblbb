//! 事务性发件箱模块 (Transactional Outbox Pattern)
//!
//! 确保领域事件在数据库事务中原子写入，然后由后台工作器异步处理。
//! 这避免了"数据库提交了但消息没发出"的常见问题。
//!
//! 工作流程：
//! 1. 在业务事务中调用 `enqueue_in_tx` 写入 outbox_events
//! 2. 消费者对每个事件开启事务：`consume_in_tx` 写入去重标记
//!    （outbox_consumed，M01-JOBS-06）→ 执行业务副作用 → `mark_sent_in_tx`
//!    标记 delivered → 提交
//! 3. 失败重试直到 max_attempts，之后进入 `failed`
//!
//! 去重保证（M01-JOBS-06）：去重标记与业务副作用在同一事务提交。即使
//! "至少一次投递"导致同一事件被再次取回（崩溃重试/多消费者竞争），
//! 唯一约束 `(event_id, consumer)` 也会让重复消费返回 `false`，业务副作用
//! 不会重复提交。消费者崩溃则整个事务回滚，标记与副作用一起消失。

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
    let now = now_millis();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
                 VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
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
                "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
                 VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
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

/// 业务事务内的发件箱事务类型。
pub type OutboxTx<'e> =
    Either<sqlx::Transaction<'e, sqlx::Sqlite>, sqlx::Transaction<'e, sqlx::MySql>>;

/// 在业务事务内写 Outbox（M01-JOBS-02）。
///
/// 与业务表变更在同一事务：事务提交事件才持久化，事务回滚事件同步消失。
/// 调用方必须先 `begin` 拿到事务，再在提交前调用本函数。
pub async fn enqueue_in_tx<'e>(
    tx: &mut OutboxTx<'e>,
    event_type: &str,
    payload: Value,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();

    match tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
                 VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
            )
            .bind(&id)
            .bind(event_type)
            .bind(&payload_str)
            .bind(now)
            .bind(now)
            .execute(&mut **t)
            .await?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
                 VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
            )
            .bind(&id)
            .bind(event_type)
            .bind(&payload_str)
            .bind(now)
            .bind(now)
            .execute(&mut **t)
            .await?;
        }
    }

    tracing::debug!(event_id = %id, event_type = %event_type, "outbox event enqueued in transaction");
    Ok(id)
}

/// 当前 Unix 毫秒（跨库时间戳约定 M01-DB-08）。
pub fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// 获取待处理事件（后台工作器调用）
pub async fn fetch_pending(
    pool: &DatabasePool,
    limit: i64,
) -> Result<Vec<OutboxEvent>, sqlx::Error> {
    let limit = limit.clamp(1, 50);
    let now = now_millis();

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

/// 在业务事务内登记消费者去重标记（M01-JOBS-06）。
///
/// 返回 `true` 表示该消费者第一次处理此事件（应执行业务副作用）；
/// 返回 `false` 表示已处理过（重复投递），必须跳过副作用。
/// 与业务副作用、[`mark_sent_in_tx`] 在同一事务提交，保证至少一次投递
/// 不产生重复的业务副作用。
pub async fn consume_in_tx<'e>(
    tx: &mut OutboxTx<'e>,
    event_id: &str,
    consumer: &str,
) -> Result<bool, sqlx::Error> {
    let now = now_millis();
    let rows = match tx {
        Either::Left(t) => sqlx::query(
            "INSERT OR IGNORE INTO outbox_consumed (event_id, consumer, consumed_at)
                 VALUES (?, ?, ?)",
        )
        .bind(event_id)
        .bind(consumer)
        .bind(now)
        .execute(&mut **t)
        .await?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "INSERT IGNORE INTO outbox_consumed (event_id, consumer, consumed_at)
                 VALUES (?, ?, ?)",
        )
        .bind(event_id)
        .bind(consumer)
        .bind(now)
        .execute(&mut **t)
        .await?
        .rows_affected(),
    };
    Ok(rows == 1)
}

/// 在业务事务内把事件标记为已投递（`pending/processing → sent`）。
///
/// 幂等：事件已 `sent` 时返回 `false`。与业务副作用、[`consume_in_tx`]
/// 在同一事务提交；消费者崩溃则一并回滚，事件保持 `pending` 可重投。
pub async fn mark_sent_in_tx<'e>(
    tx: &mut OutboxTx<'e>,
    event_id: &str,
) -> Result<bool, sqlx::Error> {
    let now = now_millis();
    let rows = match tx {
        Either::Left(t) => sqlx::query(
            "UPDATE outbox_events
                 SET status = 'sent', processed_at = ?
                 WHERE id = ? AND status IN ('pending', 'processing')",
        )
        .bind(now)
        .bind(event_id)
        .execute(&mut **t)
        .await?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE outbox_events
                 SET status = 'sent', processed_at = ?
                 WHERE id = ? AND status IN ('pending', 'processing')",
        )
        .bind(now)
        .bind(event_id)
        .execute(&mut **t)
        .await?
        .rows_affected(),
    };
    Ok(rows == 1)
}

/// 标记事件为已处理（幂等；非事务路径）
pub async fn mark_sent(pool: &DatabasePool, event_id: &str) -> Result<(), sqlx::Error> {
    let now = now_millis();

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE outbox_events SET status = 'sent', processed_at = ? WHERE id = ? AND status IN ('pending', 'processing')",
            )
            .bind(now)
            .bind(event_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE outbox_events SET status = 'sent', processed_at = ? WHERE id = ? AND status IN ('pending', 'processing')",
            )
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
    let now = now_millis();
    let retry_delay = 60_000; // 1 分钟后重试（Unix 毫秒）

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
