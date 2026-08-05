//! SQLite busy 退避与计数（M01-JOBS-09）。
//!
//! SQLite 连接池已配置 `busy_timeout`（M01-DB-03，默认 5s）；超过后 sqlx
//! 返回 `SQLITE_BUSY`/`SQLITE_LOCKED`。此模块对这类错误做**指数退避重试并
//! 计数**，禁止无延迟高频自旋：每次 busy 至少等待 `base_delay_ms`，累计计入
//! [`BusyCounter`]（M15 观测接入点）。
//!
//! MySQL/MariaDB 的行锁超时（1205）与死锁（1213）不在此重试，由
//! `transaction_concurrency` 测试与业务层处理。

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use rand::Rng;
use sqlx::Error;

/// SQLite busy 指数退避策略。
#[derive(Debug, Clone, Copy)]
pub struct BusyPolicy {
    /// 首次 busy 后的等待时间（毫秒）。
    pub base_delay_ms: i64,
    /// 单次等待上限（毫秒），防止无限指数增长。
    pub max_delay_ms: i64,
    /// 最大重试次数；超过后把最后一个 busy 错误返回给调用方。
    pub max_attempts: u32,
    /// 每次等待叠加的随机抖动上限（毫秒）。
    pub jitter_ms: i64,
}

impl Default for BusyPolicy {
    fn default() -> Self {
        Self {
            base_delay_ms: 50,
            max_delay_ms: 2_000,
            max_attempts: 8,
            jitter_ms: 50,
        }
    }
}

impl BusyPolicy {
    /// 第 `attempt` 次重试的等待时长：`min(base * 2^(attempt-1), max) + [0, jitter]`。
    /// `attempt = 0` 按第一次处理；指数饱和不溢出。
    pub fn backoff(&self, attempt: u32) -> Duration {
        let attempt = attempt.max(1);
        let doubled = self
            .base_delay_ms
            .saturating_mul(2i64.saturating_pow(attempt - 1));
        let base = doubled.min(self.max_delay_ms);
        let jitter = if self.jitter_ms > 0 {
            rand::thread_rng().gen_range(0..=self.jitter_ms)
        } else {
            0
        };
        Duration::from_millis(base.saturating_add(jitter) as u64)
    }
}

/// busy 计数（原子，观测用；M15 接入指标时读取）。
#[derive(Clone, Default, Debug)]
pub struct BusyCounter {
    inner: Arc<AtomicU64>,
}

impl BusyCounter {
    /// 记录一次 SQLite busy。
    pub fn increment(&self) {
        self.inner.fetch_add(1, Ordering::Relaxed);
    }

    /// 累计 busy 次数。
    pub fn count(&self) -> u64 {
        self.inner.load(Ordering::Relaxed)
    }

    /// 清零（重连/重置观测基线）。
    pub fn reset(&self) {
        self.inner.store(0, Ordering::Relaxed);
    }
}

/// 判断是否为 SQLite busy/locked 错误。
///
/// 依据主错误码：`SQLITE_BUSY = 5`、`SQLITE_LOCKED = 6`；并用消息兜底
/// （`database is locked` / `database table is locked`）。MySQL 的 1205/1213
/// 消息不含 `locked`，不会被误判。
pub fn is_busy_error(err: &Error) -> bool {
    match err {
        Error::Database(db) => {
            let code_busy = match db.code() {
                Some(code) => {
                    let code = code.trim();
                    code == "5" || code == "6"
                }
                None => false,
            };
            code_busy || db.message().to_lowercase().contains("locked")
        }
        _ => false,
    }
}

/// 对 SQLite busy 做指数退避重试；非 busy 错误原样返回，不做任何等待。
///
/// 每次 busy 至少等待 `base_delay_ms` 并累计计数，杜绝无延迟高频自旋。
/// 达到 `max_attempts` 后返回最后一个 busy 错误。
pub async fn retry_on_busy<T, F, Fut>(
    policy: &BusyPolicy,
    counter: &BusyCounter,
    operation: F,
) -> Result<T, Error>
where
    F: Fn() -> Fut,
    Fut: Future<Output = Result<T, Error>>,
{
    let mut attempt = 0u32;
    loop {
        match operation().await {
            Ok(value) => return Ok(value),
            Err(err) if is_busy_error(&err) => {
                attempt += 1;
                counter.increment();
                if attempt >= policy.max_attempts {
                    tracing::warn!(attempt, "sqlite busy, giving up after max_attempts");
                    return Err(err);
                }
                let delay = policy.backoff(attempt);
                tracing::warn!(
                    attempt,
                    delay_ms = delay.as_millis() as u64,
                    "sqlite busy, backing off exponentially"
                );
                tokio::time::sleep(delay).await;
            }
            Err(err) => return Err(err),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> BusyPolicy {
        BusyPolicy {
            base_delay_ms: 100,
            max_delay_ms: 800,
            max_attempts: 8,
            jitter_ms: 0,
        }
    }

    #[test]
    fn backoff_is_exponential_then_capped() {
        let p = policy();
        assert_eq!(
            p.backoff(0),
            Duration::from_millis(100),
            "第 0 次按第 1 次处理"
        );
        assert_eq!(p.backoff(1), Duration::from_millis(100));
        assert_eq!(p.backoff(2), Duration::from_millis(200));
        assert_eq!(p.backoff(3), Duration::from_millis(400));
        assert_eq!(p.backoff(4), Duration::from_millis(800), "达到上限");
        assert_eq!(p.backoff(20), Duration::from_millis(800), "上限封顶");
    }

    #[test]
    fn backoff_does_not_overflow() {
        let p = BusyPolicy {
            base_delay_ms: i64::MAX / 2,
            max_delay_ms: i64::MAX,
            max_attempts: 8,
            jitter_ms: 0,
        };
        assert_eq!(p.backoff(u32::MAX), Duration::from_millis(i64::MAX as u64));
    }

    #[test]
    fn backoff_with_jitter_stays_in_range() {
        let p = BusyPolicy {
            base_delay_ms: 100,
            max_delay_ms: 800,
            max_attempts: 8,
            jitter_ms: 50,
        };
        for _ in 0..200 {
            let d = p.backoff(2);
            let ms = d.as_millis() as i64;
            assert!((200..=250).contains(&ms), "jitter 必须在区间内，得到 {ms}");
        }
    }

    #[test]
    fn busy_counter_counts_and_resets() {
        let c = BusyCounter::default();
        assert_eq!(c.count(), 0);
        c.increment();
        c.increment();
        c.increment();
        assert_eq!(c.count(), 3);
        c.reset();
        assert_eq!(c.count(), 0);
    }
}
