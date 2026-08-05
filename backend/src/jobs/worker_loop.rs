//! Worker 运行循环与优雅停机（M01-JOBS-08）。
//!
//! 停机语义：
//! - 收到停机信号后立即停止领取新任务；
//! - 正在执行的任务继续完成（成功 `complete_job` / 失败按策略 `fail_job`），
//!   整个收尾受 [`WorkerConfig::drain_timeout`] 总超时约束；
//! - 超过总超时仍未完成的任务保持 `running`，其租约到期后由其他 worker
//!   安全重领（M01-JOBS-04），不阻塞停机。
//!
//! 长时间任务执行期间，worker 周期性续租（`renew_lease`）；失去租约
//! （已被其他 worker 重领）即停止续租并放弃，避免双跑。
//!
//! 领取遇到 SQLite busy 时指数退避并计数（M01-JOBS-09），不无延迟自旋。

use std::future::Future;
use std::time::Duration;

use tokio::sync::watch;

use crate::db::busy::{retry_on_busy, BusyCounter, BusyPolicy};
use crate::db::pool::DatabasePool;
use crate::jobs::metrics::LatencyTracker;
use crate::jobs::retry::{fail_job, RetryClass, RetryPolicy};
use crate::jobs::worker::{claim_batch, complete_job, renew_lease, ClaimedJob};

/// Worker 配置（M01-JOBS-08）。
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub worker_id: String,
    pub queue: String,
    /// 领取轮询间隔。
    pub poll_interval: Duration,
    /// 单批最大领取数。
    pub batch_limit: u32,
    /// 每次领取的租约时长（毫秒）。
    pub lease_ms: i64,
    /// 停机后的收尾总超时：超过即放弃运行中任务，交由租约恢复。
    pub drain_timeout: Duration,
    /// 失败重试策略（传给 `fail_job`）。
    pub retry_policy: RetryPolicy,
    /// SQLite busy 指数退避策略（M01-JOBS-09）。
    pub busy_policy: BusyPolicy,
    /// SQLite busy 累计计数（观测用，M15 接入指标）。
    pub busy_counter: BusyCounter,
    /// 处理延迟追踪（领取→完成，M01-JOBS-13）。
    pub latency: LatencyTracker,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: "worker-default".to_owned(),
            queue: "default".to_owned(),
            poll_interval: Duration::from_secs(1),
            batch_limit: 8,
            lease_ms: 30_000,
            drain_timeout: Duration::from_secs(10),
            retry_policy: RetryPolicy {
                base_delay_ms: 10_000,
                max_delay_ms: 600_000,
                jitter_ms: 1_000,
            },
            busy_policy: BusyPolicy::default(),
            busy_counter: BusyCounter::default(),
            latency: LatencyTracker::default(),
        }
    }
}

/// 任务处理结果。
#[derive(Debug, Clone)]
pub enum JobOutcome {
    Succeeded,
    Failed { class: RetryClass, error: String },
}

/// 运行一个 queue 的 worker，直到停机信号到达并完成收尾。
///
/// `shutdown` 置 `true` 后：不再领取新任务；正在处理的批次继续完成
/// （受 `drain_timeout` 总超时约束），随后退出。
pub async fn run_worker<F, Fut>(
    pool: &DatabasePool,
    config: WorkerConfig,
    mut shutdown: watch::Receiver<bool>,
    handler: F,
) where
    F: Fn(ClaimedJob) -> Fut,
    Fut: Future<Output = JobOutcome>,
{
    let mut poll = tokio::time::interval(config.poll_interval);
    poll.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        // 1) 未停机时领取一批；停机后不再领取。
        let batch = if *shutdown.borrow() {
            Vec::new()
        } else {
            tokio::select! {
                _ = shutdown.changed() => Vec::new(),
                _ = poll.tick() => {
                    // SQLite busy 时指数退避并计数，不无延迟自旋（M01-JOBS-09）。
                    let claim = retry_on_busy(&config.busy_policy, &config.busy_counter, || {
                        claim_batch(
                            pool,
                            &config.worker_id,
                            &config.queue,
                            config.batch_limit,
                            config.lease_ms,
                        )
                    })
                    .await;
                    match claim {
                        Ok(batch) => batch,
                        Err(error) => {
                            tracing::warn!(%error, worker_id = %config.worker_id, queue = %config.queue, "claim failed");
                            Vec::new()
                        }
                    }
                }
            }
        };

        if batch.is_empty() {
            if *shutdown.borrow() {
                tracing::info!(worker_id = %config.worker_id, queue = %config.queue, "worker drained, stopping");
                break;
            }
            continue;
        }

        // 2) 处理本批：整个批次（含停机后仍在途的任务）受总超时约束。
        let drain = tokio::time::timeout(config.drain_timeout, async {
            for job in batch {
                process_job(pool, &config, &job, &handler).await;
            }
        });
        if drain.await.is_err() {
            tracing::warn!(
                worker_id = %config.worker_id,
                queue = %config.queue,
                "worker drain exceeded timeout, leaving remaining jobs to lease recovery"
            );
        }

        if *shutdown.borrow() {
            break;
        }
    }
}

/// 执行单个任务：期间周期续租；完成后 `complete_job`，失败按策略 `fail_job`。
///
/// 续租任务随本 future 一起停止：任务完成即 abort；若本 future 因总超时被
/// 丢弃，guard 的 Drop 也会 abort，不会泄漏后台续租。
async fn process_job<F, Fut>(
    pool: &DatabasePool,
    config: &WorkerConfig,
    job: &ClaimedJob,
    handler: &F,
) where
    F: Fn(ClaimedJob) -> Fut,
    Fut: Future<Output = JobOutcome>,
{
    let renewer = RenewerGuard::new(pool, config, &job.id);

    let started = std::time::Instant::now();
    let outcome = handler(job.clone()).await;
    drop(renewer);
    config.latency.record(started.elapsed().as_millis() as u64);

    match outcome {
        JobOutcome::Succeeded => {
            if complete_job(pool, &config.worker_id, &job.id)
                .await
                .unwrap_or(false)
            {
                tracing::info!(job_id = %job.id, kind = %job.kind, "job succeeded");
            } else {
                tracing::warn!(job_id = %job.id, "lost lease before completion");
            }
        }
        JobOutcome::Failed { class, error } => {
            match fail_job(
                pool,
                &config.worker_id,
                &job.id,
                &error,
                class,
                &config.retry_policy,
            )
            .await
            {
                Ok(outcome) => {
                    tracing::info!(job_id = %job.id, ?outcome, "job failure recorded")
                }
                Err(e) => tracing::error!(job_id = %job.id, %e, "failed to record job failure"),
            }
        }
    }
}

/// 周期续租任务；Drop 时 abort（任务完成或 future 被丢弃都停止续租）。
struct RenewerGuard {
    handle: tokio::task::JoinHandle<()>,
}

impl RenewerGuard {
    fn new(pool: &DatabasePool, config: &WorkerConfig, job_id: &str) -> Self {
        let renew_pool = pool.clone();
        let worker_id = config.worker_id.clone();
        let job_id = job_id.to_owned();
        let lease_ms = config.lease_ms;
        let handle = tokio::spawn(async move {
            let interval = Duration::from_millis((lease_ms / 2).max(1) as u64);
            let mut tick = tokio::time::interval(interval);
            tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
            loop {
                tick.tick().await;
                match renew_lease(&renew_pool, &worker_id, &job_id, lease_ms).await {
                    Ok(true) => continue,
                    _ => break, // 失去租约或出错：停止续租
                }
            }
        });
        Self { handle }
    }
}

impl Drop for RenewerGuard {
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// 把 SIGTERM/SIGINT（非 Unix 为 Ctrl-C）映射为 worker 停机 watch。
///
/// main.rs 集成点：与 HTTP 服务器的优雅停机共享同一信号来源。
pub async fn worker_shutdown_signal() -> watch::Receiver<bool> {
    let (tx, rx) = watch::channel(false);
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        let mut sigint = signal(SignalKind::interrupt()).expect("install SIGINT handler");
        tokio::spawn(async move {
            tokio::select! {
                _ = sigterm.recv() => tracing::info!("worker received SIGTERM, stopping"),
                _ = sigint.recv() => tracing::info!("worker received SIGINT, stopping"),
            }
            let _ = tx.send(true);
        });
    }
    #[cfg(not(unix))]
    {
        tokio::spawn(async move {
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("worker received Ctrl-C, stopping");
            let _ = tx.send(true);
        });
    }
    rx
}
