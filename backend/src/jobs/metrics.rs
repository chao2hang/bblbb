//! 队列健康与处理延迟指标（M01-JOBS-13）。
//!
//! - [`snapshot`]：按 queue 读取队列深度（queued/running/retry_wait/dead）、
//!   最老待处理任务年龄、平均尝试次数与租约过期（overdue）running 数。
//! - [`LatencyTracker`]：进程内处理延迟（领取→完成）计数/均值/最大值，
//!   worker 每个任务完成后更新（M15 接入指标时转出）。
//!
//! 观测语义（docs/JOBS.md §10）：安全/邮件队列最老任务超过 5 分钟告警、
//! dead-letter 新增告警、Outbox 堆积告警等都以这些快照为输入。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use sqlx::Either;

use crate::db::pool::DatabasePool;
use crate::jobs::now_millis;

/// 单个 queue 的健康指标快照。
#[derive(Debug, Clone, PartialEq)]
pub struct QueueSnapshot {
    pub queue: String,
    pub queued: i64,
    pub running: i64,
    pub retry_wait: i64,
    pub dead: i64,
    /// 最老待处理任务年龄（毫秒）；无待处理任务时为 `None`。
    pub oldest_pending_age_ms: Option<i64>,
    /// 待处理任务的平均尝试次数（queued/running/retry_wait）。
    pub avg_attempts: f64,
    /// 租约已过期（`locked_until <= now`）的 running 任务数（崩溃未回收）。
    pub lease_overdue: i64,
}

/// 读取一个 queue 的健康指标快照（M01-JOBS-13）。
pub async fn snapshot(pool: &DatabasePool, queue: &str) -> Result<QueueSnapshot, sqlx::Error> {
    let now = now_millis();
    let queued = count_by_status(pool, queue, "queued").await?;
    let running = count_by_status(pool, queue, "running").await?;
    let retry_wait = count_by_status(pool, queue, "retry_wait").await?;
    let dead = count_by_status(pool, queue, "dead").await?;

    // 租约过期的 running 任务（locked_until 已过）
    let lease_overdue: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs
                 WHERE queue = ? AND status = 'running'
                   AND locked_until IS NOT NULL AND locked_until <= ?",
            )
            .bind(queue)
            .bind(now)
            .fetch_one(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs
                 WHERE queue = ? AND status = 'running'
                   AND locked_until IS NOT NULL AND locked_until <= ?",
            )
            .bind(queue)
            .bind(now)
            .fetch_one(p)
            .await?
        }
    };

    // 最老待处理任务年龄
    let oldest_available_at: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT MIN(available_at) FROM jobs
                 WHERE queue = ? AND status IN ('queued', 'retry_wait') AND available_at <= ?",
            )
            .bind(queue)
            .bind(now)
            .fetch_one(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT MIN(available_at) FROM jobs
                 WHERE queue = ? AND status IN ('queued', 'retry_wait') AND available_at <= ?",
            )
            .bind(queue)
            .bind(now)
            .fetch_one(p)
            .await?
        }
    };
    let oldest_pending_age_ms = oldest_available_at.map(|at| (now - at).max(0));

    // 平均尝试次数
    let avg_attempts: f64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT AVG(attempts) FROM jobs
                 WHERE queue = ? AND status IN ('queued', 'running', 'retry_wait')",
            )
            .bind(queue)
            .fetch_one(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT AVG(attempts) FROM jobs
                 WHERE queue = ? AND status IN ('queued', 'running', 'retry_wait')",
            )
            .bind(queue)
            .fetch_one(p)
            .await?
        }
    };

    Ok(QueueSnapshot {
        queue: queue.to_owned(),
        queued,
        running,
        retry_wait,
        dead,
        oldest_pending_age_ms,
        avg_attempts,
        lease_overdue,
    })
}

/// 某个状态的任务数量。
async fn count_by_status(
    pool: &DatabasePool,
    queue: &str,
    status: &str,
) -> Result<i64, sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE queue = ? AND status = ?")
                .bind(queue)
                .bind(status)
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM jobs WHERE queue = ? AND status = ?")
                .bind(queue)
                .bind(status)
                .fetch_one(p)
                .await
        }
    }
}

/// 处理延迟统计（领取→完成）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LatencyStats {
    pub count: u64,
    pub total_ms: u64,
    pub max_ms: u64,
}

impl LatencyStats {
    /// 平均耗时（毫秒）；无样本时为 `None`。
    pub fn average_ms(&self) -> Option<u64> {
        self.total_ms.checked_div(self.count)
    }
}

/// 进程内处理延迟追踪器（原子更新；M15 转指标时读取快照）。
#[derive(Clone, Default, Debug)]
pub struct LatencyTracker {
    count: Arc<AtomicU64>,
    total_ms: Arc<AtomicU64>,
    max_ms: Arc<AtomicU64>,
}

impl LatencyTracker {
    /// 记录一次领取→完成耗时（毫秒）。
    pub fn record(&self, elapsed_ms: u64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.total_ms.fetch_add(elapsed_ms, Ordering::Relaxed);
        self.max_ms.fetch_max(elapsed_ms, Ordering::Relaxed);
    }

    /// 读取当前统计快照。
    pub fn snapshot(&self) -> LatencyStats {
        LatencyStats {
            count: self.count.load(Ordering::Relaxed),
            total_ms: self.total_ms.load(Ordering::Relaxed),
            max_ms: self.max_ms.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn latency_tracker_records_count_total_max() {
        let tracker = LatencyTracker::default();
        assert_eq!(tracker.snapshot().count, 0);
        assert_eq!(tracker.snapshot().average_ms(), None, "空统计无均值");

        tracker.record(10);
        tracker.record(30);
        tracker.record(20);

        let stats = tracker.snapshot();
        assert_eq!(stats.count, 3);
        assert_eq!(stats.total_ms, 60);
        assert_eq!(stats.max_ms, 30);
        assert_eq!(stats.average_ms(), Some(20));
    }
}
