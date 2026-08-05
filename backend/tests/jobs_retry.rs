//! M01-JOBS-05：分类重试、指数退避 + jitter、最大次数、dead-letter 与人工重放。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::retry::{fail_job, replay_job, FailOutcome, RetryClass, RetryPolicy};
use bblbb_backend::jobs::worker;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

/// 建库并应用全部真实迁移；返回 (pool, sqlite 文件路径)。
async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-retry-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 插入一个 queued 任务（默认 max_attempts=3）。
async fn insert_queued_job(pool: &DatabasePool, id: &str, available_at: i64) {
    let base = now_ms();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, created_at, updated_at)
                 VALUES (?, 'default', 'mail', '{}', 1, 'queued', 0, 3, ?, ?, ?)",
            )
            .bind(id)
            .bind(available_at)
            .bind(base)
            .bind(base)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 把 retry_wait 任务的下次可用时间改到过去，使其可再次领取。
async fn make_available_now(pool: &DatabasePool, id: &str) {
    let base = now_ms();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE jobs SET available_at = ? WHERE id = ?")
                .bind(base - 1_000)
                .bind(id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 读取任务的 (status, attempts, locked_by, available_at, last_error, completed_at)。
type JobSnapshot = (
    String,
    i64,
    Option<String>,
    i64,
    Option<String>,
    Option<i64>,
);

async fn job_row(pool: &DatabasePool, id: &str) -> JobSnapshot {
    match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT status, attempts, locked_by, available_at, last_error, completed_at
                 FROM jobs WHERE id = ?",
        )
        .bind(id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn policy() -> RetryPolicy {
    RetryPolicy {
        base_delay_ms: 1_000,
        max_delay_ms: 8_000,
        jitter_ms: 500,
    }
}

/// 临时性失败 → retry_wait，按退避写下次 available_at，释放租约并记录错误摘要。
#[tokio::test]
async fn transient_failure_goes_to_retry_wait_with_backoff() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;
    let claimed = worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    let before = now_ms();

    let outcome = fail_job(
        &pool,
        "worker-a",
        "j1",
        "smtp connect timeout",
        RetryClass::Transient,
        &policy(),
    )
    .await
    .unwrap();
    let FailOutcome::Retry { next_available_at } = outcome else {
        panic!("临时性错误应转入 retry_wait，得到 {outcome:?}");
    };
    let after = now_ms();
    assert!(
        next_available_at >= before + 1_000 && next_available_at <= after + 1_500,
        "退避应在 [base, base+jitter] 内：{next_available_at}"
    );

    let (status, attempts, locked_by, available_at, last_error, _) = job_row(&pool, "j1").await;
    assert_eq!(status, "retry_wait");
    assert_eq!(attempts, 1, "attempts 在领取时计数，失败不改动");
    assert_eq!(locked_by, None, "失败后必须释放租约");
    assert_eq!(available_at, next_available_at);
    assert_eq!(last_error.as_deref(), Some("smtp connect timeout"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 永久性错误 → 直接 dead-letter，不重试。
#[tokio::test]
async fn permanent_failure_goes_straight_to_dead() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();

    let outcome = fail_job(
        &pool,
        "worker-a",
        "j1",
        "unsupported attachment format",
        RetryClass::Permanent,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(outcome, FailOutcome::Dead);

    let (status, _, locked_by, _, last_error, completed_at) = job_row(&pool, "j1").await;
    assert_eq!(status, "dead");
    assert_eq!(locked_by, None);
    assert_eq!(last_error.as_deref(), Some("unsupported attachment format"));
    assert!(completed_at.is_some(), "dead-letter 也应记录完成时间");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 达到最大次数 → dead-letter：临时性错误在 max_attempts 次后仍失败。
#[tokio::test]
async fn failure_exceeding_max_attempts_dead_letters() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;

    // 第一次执行失败 → retry_wait
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    let outcome = fail_job(
        &pool,
        "worker-a",
        "j1",
        "timeout #1",
        RetryClass::Transient,
        &policy(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, FailOutcome::Retry { .. }));

    // 第二次执行失败 → retry_wait
    make_available_now(&pool, "j1").await;
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    let outcome = fail_job(
        &pool,
        "worker-a",
        "j1",
        "timeout #2",
        RetryClass::Transient,
        &policy(),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, FailOutcome::Retry { .. }));

    // 第三次执行（attempts=3 = max_attempts）失败 → dead
    make_available_now(&pool, "j1").await;
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    let outcome = fail_job(
        &pool,
        "worker-a",
        "j1",
        "timeout #3",
        RetryClass::Transient,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(
        outcome,
        FailOutcome::Dead,
        "达到 max_attempts 必须 dead-letter"
    );

    let (status, attempts, _, _, last_error, _) = job_row(&pool, "j1").await;
    assert_eq!(status, "dead");
    assert_eq!(attempts, 3);
    assert_eq!(last_error.as_deref(), Some("timeout #3"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 退避随尝试次数指数增长（第 2 次是第 1 次的 2 倍；jitter=0 时确定）。
#[tokio::test]
async fn backoff_doubles_with_each_attempt() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;
    // jitter=0 → 退避确定，便于精确断言
    let p = RetryPolicy {
        base_delay_ms: 1_000,
        max_delay_ms: 8_000,
        jitter_ms: 0,
    };

    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    let t1 = now_ms();
    let first = fail_job(&pool, "worker-a", "j1", "e1", RetryClass::Transient, &p)
        .await
        .unwrap();
    let FailOutcome::Retry {
        next_available_at: first_at,
    } = first
    else {
        panic!("expected retry")
    };
    let delay1 = first_at - t1;
    assert_eq!(delay1, 1_000, "第 1 次失败退避 1s");

    make_available_now(&pool, "j1").await;
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    let t2 = now_ms();
    let second = fail_job(&pool, "worker-a", "j1", "e2", RetryClass::Transient, &p)
        .await
        .unwrap();
    let FailOutcome::Retry {
        next_available_at: second_at,
    } = second
    else {
        panic!("expected retry")
    };
    let delay2 = second_at - t2;
    assert_eq!(delay2, 2_000, "第 2 次失败退避 2s（指数增长）");
    assert!(delay2 > delay1, "退避必须随尝试次数增长");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 非 owner 或已失去租约的任务：fail_job 返回 LostLease，不做任何修改。
#[tokio::test]
async fn fail_job_requires_owner_and_returns_lost_lease() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();

    // 非 owner 失败 → LostLease，状态不变
    let outcome = fail_job(
        &pool,
        "worker-b",
        "j1",
        "nope",
        RetryClass::Transient,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(outcome, FailOutcome::LostLease);
    let (status, _, locked_by, _, last_error, _) = job_row(&pool, "j1").await;
    assert_eq!(status, "running");
    assert_eq!(locked_by.as_deref(), Some("worker-a"));
    assert_eq!(last_error, None, "LostLease 不得修改任务");

    // 不存在的任务 → LostLease
    let outcome = fail_job(
        &pool,
        "worker-a",
        "missing",
        "x",
        RetryClass::Transient,
        &policy(),
    )
    .await
    .unwrap();
    assert_eq!(outcome, FailOutcome::LostLease);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 成功完成：complete_job 仅 owner 有效，写 succeeded + completed_at，释放租约。
#[tokio::test]
async fn complete_job_marks_succeeded_and_clears_lock() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();

    assert!(
        worker::complete_job(&pool, "worker-a", "j1").await.unwrap(),
        "owner 应能标记成功"
    );
    let (status, attempts, locked_by, _, _, completed_at) = job_row(&pool, "j1").await;
    assert_eq!(status, "succeeded");
    assert_eq!(attempts, 1);
    assert_eq!(locked_by, None, "完成后释放租约");
    assert!(completed_at.is_some(), "完成必须写 completed_at");

    // 已 succeeded 的任务再次 complete → false；非 owner 也 false
    assert!(!worker::complete_job(&pool, "worker-a", "j1").await.unwrap());

    insert_queued_job(&pool, "j2", now - 10_000).await;
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    assert!(
        !worker::complete_job(&pool, "worker-b", "j2").await.unwrap(),
        "非 owner 不能标记成功"
    );
    let (status, _, _, _, _, _) = job_row(&pool, "j2").await;
    assert_eq!(status, "running");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 人工重放：dead → queued，重置 attempts/last_error/租约并立即可领取。
#[tokio::test]
async fn replay_dead_job_requeues_and_resets() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;
    worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    fail_job(
        &pool,
        "worker-a",
        "j1",
        "permanent failure",
        RetryClass::Permanent,
        &policy(),
    )
    .await
    .unwrap();
    let (status, _, _, _, _, _) = job_row(&pool, "j1").await;
    assert_eq!(status, "dead");

    assert!(replay_job(&pool, "j1").await.unwrap(), "dead 任务应可重放");
    let (status, attempts, locked_by, available_at, last_error, completed_at) =
        job_row(&pool, "j1").await;
    assert_eq!(status, "queued");
    assert_eq!(attempts, 0, "重放重置 attempts");
    assert_eq!(locked_by, None);
    assert!(available_at <= now_ms() + 1_000, "重放后立即可领取");
    assert_eq!(last_error, None, "重放清空 last_error");
    assert_eq!(completed_at, None, "重放清空 completed_at");

    // 重放后可以再次领取执行
    let claimed = worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].id, "j1");
    assert_eq!(claimed[0].attempts, 1);

    // 非 dead 任务不可重放
    assert!(!replay_job(&pool, "j1").await.unwrap());
    assert!(!replay_job(&pool, "missing").await.unwrap());

    close_pool(&pool).await;
    cleanup(&dir);
}
