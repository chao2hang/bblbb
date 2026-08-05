//! Job worker 租约：批量领取、owner 续租、成功完成与 lease 过期后的安全重领
//! （M01-JOBS-04 / M01-JOBS-05）。
//!
//! 契约：
//! - [`claim_batch`]：先把本 queue 中 lease 已过期的 `running` 任务重新入队
//!   （状态机 `running → queued`，崩溃恢复），再按 `available_at` 升序领取
//!   最多 `limit` 个可领取任务；每个任务通过 CAS UPDATE 抢占，只有赢得
//!   UPDATE 的行才算领取成功，因此多 worker 并发不会重复领取。
//! - 领取成功后 `attempts + 1`（一次领取 = 一次执行尝试），`locked_by` 记录
//!   owner，`locked_until = now + lease`。
//! - [`renew_lease`] 只允许 owner 在 lease 未过期时续租；[`complete_job`]
//!   只允许 owner 标记成功（`running → succeeded`）。lease 已过期、owner 不符
//!   时两者都返回 `false`，worker 必须立即放弃该任务（它可能已被重领）。
//! - 失败路径与重试策略见 [`super::retry`]。
//!
//! SQLite 与 MySQL/MariaDB 使用同一套 CAS 语句；批量领取数量小、快速提交
//! 租约，不在锁事务内执行任务（docs/JOBS.md §4）。

use serde_json::Value;
use sqlx::Either;

use crate::db::pool::DatabasePool;
use crate::jobs::now_millis;

/// 领取到的任务及其租约信息，供 worker 执行与续租。
#[derive(Debug, Clone)]
pub struct ClaimedJob {
    pub id: String,
    pub queue: String,
    pub kind: String,
    pub payload: Value,
    pub payload_version: i64,
    /// 本次执行是第几次尝试（领取时 `attempts + 1`）。
    pub attempts: i64,
    pub max_attempts: i64,
    /// 本次租约截止时间（Unix 毫秒）。
    pub locked_until: i64,
}

/// 批量领取任务（M01-JOBS-04）。
///
/// 返回本次实际抢占成功的任务；调用方逐个执行，并在执行期间周期性调用
/// [`renew_lease`] 续租。`limit` 与 `lease_ms` 会被钳制到安全范围。
pub async fn claim_batch(
    pool: &DatabasePool,
    worker_id: &str,
    queue: &str,
    limit: u32,
    lease_ms: i64,
) -> Result<Vec<ClaimedJob>, sqlx::Error> {
    let limit = limit.clamp(1, 100);
    let lease_ms = lease_ms.max(1_000);
    let now = now_millis();

    // 1) 崩溃恢复：lease 已过期的 running 任务重新入队（running → queued）。
    requeue_expired_leases(pool, queue, now).await?;

    // 2) 选出候选（本 queue、可领取、按 available_at 升序）。
    let candidates = select_claimable(pool, queue, now, limit).await?;

    // 3) CAS 领取：只有赢得 UPDATE 的行才算领取成功，避免多 worker 重复领取。
    let mut claimed = Vec::with_capacity(candidates.len());
    for cand in candidates {
        let locked_until = now + lease_ms;
        if try_claim(pool, worker_id, queue, &cand.id, now, locked_until).await? {
            claimed.push(ClaimedJob {
                id: cand.id,
                queue: cand.queue,
                kind: cand.kind,
                payload: serde_json::from_str(&cand.payload).unwrap_or(Value::Null),
                payload_version: cand.payload_version,
                attempts: cand.attempts + 1,
                max_attempts: cand.max_attempts,
                locked_until,
            });
        }
    }
    Ok(claimed)
}

/// 由 owner 在 lease 未过期时续租（M01-JOBS-04）。
///
/// 返回 `false` 表示续租失败：任务不是 `running`、owner 不符或 lease 已过期。
/// worker 收到 `false` 必须立即停止处理该任务（它可能已被其他 worker 重领）。
pub async fn renew_lease(
    pool: &DatabasePool,
    worker_id: &str,
    job_id: &str,
    lease_ms: i64,
) -> Result<bool, sqlx::Error> {
    let now = now_millis();
    let lease_ms = lease_ms.max(1_000);
    let locked_until = now + lease_ms;

    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE jobs
                 SET locked_until = ?, updated_at = ?
                 WHERE id = ? AND status = 'running' AND locked_by = ? AND locked_until >= ?",
        )
        .bind(locked_until)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE jobs
                 SET locked_until = ?, updated_at = ?
                 WHERE id = ? AND status = 'running' AND locked_by = ? AND locked_until >= ?",
        )
        .bind(locked_until)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(rows == 1)
}

/// 成功完成任务：`running → succeeded`，写 `completed_at` 并释放租约（M01-JOBS-05）。
///
/// 仅 owner 有效；lease 已失效、任务非 `running` 或 owner 不符时返回 `false`，
/// worker 必须停止处理该任务（它可能已被重领）。
pub async fn complete_job(
    pool: &DatabasePool,
    worker_id: &str,
    job_id: &str,
) -> Result<bool, sqlx::Error> {
    let now = now_millis();
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE jobs
                 SET status = 'succeeded', completed_at = ?, locked_by = NULL,
                     locked_until = NULL, updated_at = ?
                 WHERE id = ? AND status = 'running' AND locked_by = ?",
        )
        .bind(now)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE jobs
                 SET status = 'succeeded', completed_at = ?, locked_by = NULL,
                     locked_until = NULL, updated_at = ?
                 WHERE id = ? AND status = 'running' AND locked_by = ?",
        )
        .bind(now)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(rows == 1)
}

#[derive(sqlx::FromRow)]
struct JobCandidate {
    id: String,
    queue: String,
    kind: String,
    payload: String,
    payload_version: i64,
    attempts: i64,
    max_attempts: i64,
}

/// 把本 queue 中 lease 已过期的 running 任务重新入队（状态机 running → queued）。
async fn requeue_expired_leases(
    pool: &DatabasePool,
    queue: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE jobs
                 SET status = 'queued', locked_by = NULL, locked_until = NULL,
                     available_at = ?, updated_at = ?
                 WHERE queue = ? AND status = 'running'
                   AND locked_until IS NOT NULL AND locked_until <= ?",
            )
            .bind(now)
            .bind(now)
            .bind(queue)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE jobs
                 SET status = 'queued', locked_by = NULL, locked_until = NULL,
                     available_at = ?, updated_at = ?
                 WHERE queue = ? AND status = 'running'
                   AND locked_until IS NOT NULL AND locked_until <= ?",
            )
            .bind(now)
            .bind(now)
            .bind(queue)
            .bind(now)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

/// 选出当前可领取的候选任务：本 queue、`queued`/`retry_wait`、`available_at`
/// 已到且无未过期锁。按 `available_at` 升序（最老优先）。
async fn select_claimable(
    pool: &DatabasePool,
    queue: &str,
    now: i64,
    limit: u32,
) -> Result<Vec<JobCandidate>, sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, JobCandidate>(
                "SELECT id, queue, kind, payload, payload_version, attempts, max_attempts
                 FROM jobs
                 WHERE queue = ?
                   AND status IN ('queued', 'retry_wait')
                   AND available_at <= ?
                   AND (locked_by IS NULL OR locked_until IS NULL OR locked_until <= ?)
                 ORDER BY available_at ASC, created_at ASC
                 LIMIT ?",
            )
            .bind(queue)
            .bind(now)
            .bind(now)
            .bind(limit)
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, JobCandidate>(
                "SELECT id, queue, kind, payload, payload_version, attempts, max_attempts
                 FROM jobs
                 WHERE queue = ?
                   AND status IN ('queued', 'retry_wait')
                   AND available_at <= ?
                   AND (locked_by IS NULL OR locked_until IS NULL OR locked_until <= ?)
                 ORDER BY available_at ASC, created_at ASC
                 LIMIT ?",
            )
            .bind(queue)
            .bind(now)
            .bind(now)
            .bind(limit)
            .fetch_all(p)
            .await
        }
    }
}

/// CAS 领取单个任务；返回是否赢得了该任务（`rows_affected == 1`）。
async fn try_claim(
    pool: &DatabasePool,
    worker_id: &str,
    queue: &str,
    job_id: &str,
    now: i64,
    locked_until: i64,
) -> Result<bool, sqlx::Error> {
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE jobs
                 SET status = 'running', locked_by = ?, locked_until = ?,
                     attempts = attempts + 1, updated_at = ?
                 WHERE id = ? AND queue = ?
                   AND status IN ('queued', 'retry_wait')
                   AND available_at <= ?
                   AND (locked_by IS NULL OR locked_until IS NULL OR locked_until <= ?)",
        )
        .bind(worker_id)
        .bind(locked_until)
        .bind(now)
        .bind(job_id)
        .bind(queue)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE jobs
                 SET status = 'running', locked_by = ?, locked_until = ?,
                     attempts = attempts + 1, updated_at = ?
                 WHERE id = ? AND queue = ?
                   AND status IN ('queued', 'retry_wait')
                   AND available_at <= ?
                   AND (locked_by IS NULL OR locked_until IS NULL OR locked_until <= ?)",
        )
        .bind(worker_id)
        .bind(locked_until)
        .bind(now)
        .bind(job_id)
        .bind(queue)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(rows == 1)
}
