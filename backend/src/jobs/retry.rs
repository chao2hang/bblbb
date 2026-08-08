//! Job 重试策略（M01-JOBS-05）：分类重试、指数退避 + jitter、最大次数、
//! dead-letter 与人工重放。
//!
//! - [`RetryClass`] 区分临时性错误（重试）与永久性错误（直接死信）。
//! - [`RetryPolicy`] 定义指数退避的底数/上限/jitter；`attempts` 在领取时 +1
//!   （M01-JOBS-04），第 N 次失败按第 N 次尝试计算退避。
//! - [`fail_job`] 由 worker 在失败后调用：达到 `max_attempts`（行级配置）或
//!   永久错误 → dead-letter；否则 → retry_wait 并写下次 `available_at`。
//! - [`replay_job`] 是人工重放：管理员在审计下把 dead 任务重新入队
//!   （状态机 `dead → queued`），重置 attempts/last_error。
//!
//! 错误文本必须为安全摘要（docs/JOBS.md §6）：不得写入邮件正文、Token、
//! 隐藏内容等敏感信息。

use rand::Rng;
use sqlx::Either;

use crate::db::pool::DatabasePool;
use crate::jobs::now_millis;

/// 重试分类：决定任务失败后是重试还是直接死信。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    /// 临时性错误（网络、SMTP 4xx、S3 超时、数据库暂不可用）：按退避重试。
    Transient,
    /// 永久性错误（输入无效、模板缺失、附件格式不支持）：直接 dead-letter。
    Permanent,
}

/// 指数退避策略（底数 + 上限 + jitter）。
///
/// 第 `attempt` 次失败的延迟 = `min(base_delay_ms * 2^(attempt-1), max_delay_ms)
/// + [0, jitter_ms]`。`jitter_ms = 0` 时退避确定，便于测试与观测。
#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// 首次重试的等待时间（毫秒）。
    pub base_delay_ms: i64,
    /// 单次重试等待时间的硬上限（毫秒），防止无限指数增长。
    pub max_delay_ms: i64,
    /// 每次重试在底数之上叠加的随机抖动上限（毫秒）。
    pub jitter_ms: i64,
}

impl RetryPolicy {
    /// 确定性退避底数（不含 jitter）：`min(base * 2^(attempt-1), max_delay)`。
    ///
    /// `attempt = 0` 按第一次尝试处理。指数用饱和运算，避免大次数时溢出。
    pub fn backoff(&self, attempt: u32) -> i64 {
        let attempt = attempt.max(1);
        let exponent = attempt - 1;
        let doubled = self
            .base_delay_ms
            .saturating_mul(2i64.saturating_pow(exponent));
        doubled.min(self.max_delay_ms)
    }

    /// 指数退避 + jitter：在 `[backoff(attempt), backoff(attempt) + jitter_ms]`
    /// 内随机取值，避免同批失败任务同时重试（惊群）。
    pub fn backoff_with_jitter(&self, attempt: u32) -> i64 {
        let base = self.backoff(attempt);
        if self.jitter_ms <= 0 {
            return base;
        }
        let jitter = rand::thread_rng().gen_range(0..=self.jitter_ms);
        base.saturating_add(jitter)
    }
}

/// `fail_job` 的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailOutcome {
    /// 已转入 `retry_wait`，`next_available_at` 为下次可领取时间。
    Retry { next_available_at: i64 },
    /// 已进入 dead-letter（永久错误或达到最大次数）。
    Dead,
    /// 任务已不在本 worker 手中（lease 失效/已被重领），未做任何修改。
    LostLease,
}

/// 任务失败处理（M01-JOBS-05）。
///
/// 由 owner 调用；错误文本必须是安全摘要。达到行级 `max_attempts` 或
/// [`RetryClass::Permanent`] 时转入 dead-letter，否则按退避写入
/// `retry_wait` 的 `available_at`。
pub async fn fail_job(
    pool: &DatabasePool,
    worker_id: &str,
    job_id: &str,
    error: &str,
    class: RetryClass,
    policy: &RetryPolicy,
) -> Result<FailOutcome, sqlx::Error> {
    let now = now_millis();

    // 读取当前行并校验 owner/状态（与执行路径一致：只允许 running 的 owner）。
    let Some(row) = fetch_job_row(pool, job_id).await? else {
        return Ok(FailOutcome::LostLease);
    };
    if row.status != "running" || row.locked_by.as_deref() != Some(worker_id) {
        return Ok(FailOutcome::LostLease);
    }

    // 决策：永久错误或已达最大次数 → dead；否则按退避重试。
    let dead = class == RetryClass::Permanent || row.attempts >= row.max_attempts;
    if dead {
        // M15-OBSERVE-06：dead-letter 累计指标
        crate::observability::metrics::registry().counter_inc("bblbb_jobs_dead_total", 1);
    }
    let outcome = if dead {
        FailOutcome::Dead
    } else {
        FailOutcome::Retry {
            next_available_at: now + policy.backoff_with_jitter(row.attempts as u32),
        }
    };

    let affected = match (&outcome, pool) {
        (FailOutcome::Dead, Either::Left(p)) => sqlx::query(
            "UPDATE jobs
                 SET status = 'dead', last_error = ?, completed_at = ?,
                     locked_by = NULL, locked_until = NULL, updated_at = ?
                 WHERE id = ? AND status = 'running' AND locked_by = ?",
        )
        .bind(error)
        .bind(now)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(p)
        .await?
        .rows_affected(),
        (FailOutcome::Dead, Either::Right(p)) => sqlx::query(
            "UPDATE jobs
                 SET status = 'dead', last_error = ?, completed_at = ?,
                     locked_by = NULL, locked_until = NULL, updated_at = ?
                 WHERE id = ? AND status = 'running' AND locked_by = ?",
        )
        .bind(error)
        .bind(now)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(p)
        .await?
        .rows_affected(),
        (FailOutcome::Retry { next_available_at }, Either::Left(p)) => sqlx::query(
            "UPDATE jobs
                 SET status = 'retry_wait', last_error = ?, available_at = ?,
                     locked_by = NULL, locked_until = NULL, updated_at = ?
                 WHERE id = ? AND status = 'running' AND locked_by = ?",
        )
        .bind(error)
        .bind(next_available_at)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(p)
        .await?
        .rows_affected(),
        (FailOutcome::Retry { next_available_at }, Either::Right(p)) => sqlx::query(
            "UPDATE jobs
                 SET status = 'retry_wait', last_error = ?, available_at = ?,
                     locked_by = NULL, locked_until = NULL, updated_at = ?
                 WHERE id = ? AND status = 'running' AND locked_by = ?",
        )
        .bind(error)
        .bind(next_available_at)
        .bind(now)
        .bind(job_id)
        .bind(worker_id)
        .execute(p)
        .await?
        .rows_affected(),
        // LostLease 是读取校验失败/竞态，不写库。
        (FailOutcome::LostLease, _) => 0,
    };

    if affected == 1 {
        Ok(outcome)
    } else {
        Ok(FailOutcome::LostLease)
    }
}

/// 人工重放（M01-JOBS-05）：把 dead-letter 任务重新入队（`dead → queued`）。
///
/// 重置 `attempts`/`last_error`/`completed_at`/租约并立即可领取。管理操作，
/// 调用方必须写审计（docs/JOBS.md §11）。非 `dead` 任务返回 `false`。
pub async fn replay_job(pool: &DatabasePool, job_id: &str) -> Result<bool, sqlx::Error> {
    let now = now_millis();
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE jobs
                 SET status = 'queued', attempts = 0, last_error = NULL,
                     available_at = ?, locked_by = NULL, locked_until = NULL,
                     completed_at = NULL, updated_at = ?
                 WHERE id = ? AND status = 'dead'",
        )
        .bind(now)
        .bind(now)
        .bind(job_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE jobs
                 SET status = 'queued', attempts = 0, last_error = NULL,
                     available_at = ?, locked_by = NULL, locked_until = NULL,
                     completed_at = NULL, updated_at = ?
                 WHERE id = ? AND status = 'dead'",
        )
        .bind(now)
        .bind(now)
        .bind(job_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(rows == 1)
}

/// 失败决策需要的最小行视图。
#[derive(sqlx::FromRow)]
struct JobRow {
    status: String,
    attempts: i64,
    max_attempts: i64,
    locked_by: Option<String>,
}

async fn fetch_job_row(pool: &DatabasePool, job_id: &str) -> Result<Option<JobRow>, sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, JobRow>(
                "SELECT status, attempts, max_attempts, locked_by FROM jobs WHERE id = ?",
            )
            .bind(job_id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, JobRow>(
                "SELECT status, attempts, max_attempts, locked_by FROM jobs WHERE id = ?",
            )
            .bind(job_id)
            .fetch_optional(p)
            .await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> RetryPolicy {
        RetryPolicy {
            base_delay_ms: 1_000,
            max_delay_ms: 8_000,
            jitter_ms: 0,
        }
    }

    #[test]
    fn backoff_is_exponential_then_capped() {
        let p = policy();
        assert_eq!(p.backoff(0), 1_000, "第 0 次按第 1 次处理");
        assert_eq!(p.backoff(1), 1_000);
        assert_eq!(p.backoff(2), 2_000);
        assert_eq!(p.backoff(3), 4_000);
        assert_eq!(p.backoff(4), 8_000, "达到上限");
        assert_eq!(p.backoff(10), 8_000, "上限封顶，不无限增长");
    }

    #[test]
    fn backoff_does_not_overflow_on_huge_attempt() {
        let p = RetryPolicy {
            base_delay_ms: i64::MAX / 2,
            max_delay_ms: i64::MAX,
            jitter_ms: 0,
        };
        let delay = p.backoff(u32::MAX);
        assert_eq!(delay, i64::MAX, "饱和到上限，不得溢出为负数");
    }

    #[test]
    fn backoff_with_jitter_stays_in_range() {
        let p = RetryPolicy {
            base_delay_ms: 1_000,
            max_delay_ms: 8_000,
            jitter_ms: 500,
        };
        for _ in 0..200 {
            let delay = p.backoff_with_jitter(3);
            assert!(
                (4_000..=4_500).contains(&delay),
                "jitter 必须在 [base, base+jitter] 内，得到 {delay}"
            );
        }
    }

    #[test]
    fn backoff_with_jitter_is_deterministic_when_jitter_is_zero() {
        let p = policy();
        for _ in 0..200 {
            assert_eq!(p.backoff_with_jitter(2), 2_000);
        }
    }

    #[test]
    fn retry_class_and_outcome_are_plain_equality() {
        assert_eq!(RetryClass::Transient, RetryClass::Transient);
        assert_ne!(RetryClass::Transient, RetryClass::Permanent);
        assert_eq!(
            FailOutcome::Retry {
                next_available_at: 42
            },
            FailOutcome::Retry {
                next_available_at: 42
            }
        );
        assert_eq!(FailOutcome::Dead, FailOutcome::Dead);
        assert_eq!(FailOutcome::LostLease, FailOutcome::LostLease);
    }
}
