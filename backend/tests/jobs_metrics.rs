//! M01-JOBS-13：暴露 queue depth、age、attempt、lease timeout、dead count
//! 与处理延迟指标。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::metrics::{snapshot, LatencyTracker};
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-metrics-{}", uuid::Uuid::now_v7()));
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

/// 插入指定状态的 job。
async fn insert_job(
    pool: &DatabasePool,
    id: &str,
    queue: &str,
    status: &str,
    attempts: i64,
    available_at: i64,
    locked_until: Option<i64>,
) {
    let base = now_ms();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, locked_by, locked_until, created_at, updated_at)
                 VALUES (?, ?, 'mail', '{}', 1, ?, ?, 5, ?, 'w', ?, ?, ?)",
            )
            .bind(id)
            .bind(queue)
            .bind(status)
            .bind(attempts)
            .bind(available_at)
            .bind(locked_until)
            .bind(base)
            .bind(base)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// queue 快照覆盖 depth/age/attempt/lease timeout/dead count，且按 queue 隔离。
#[tokio::test]
async fn snapshot_reports_all_job_metrics_per_queue() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();

    // default 队列：2 queued、1 running（租约未过期）、1 running（租约过期）、
    // 1 retry_wait、1 dead、1 succeeded（不计入待处理）
    insert_job(&pool, "q1", "default", "queued", 0, now - 120_000, None).await;
    insert_job(&pool, "q2", "default", "queued", 0, now - 30_000, None).await;
    insert_job(
        &pool,
        "r1",
        "default",
        "running",
        1,
        now - 10_000,
        Some(now + 60_000),
    )
    .await;
    insert_job(
        &pool,
        "r2",
        "default",
        "running",
        2,
        now - 10_000,
        Some(now - 1_000),
    )
    .await;
    insert_job(&pool, "w1", "default", "retry_wait", 1, now - 5_000, None).await;
    insert_job(&pool, "d1", "default", "dead", 3, now - 5_000, None).await;
    insert_job(&pool, "s1", "default", "succeeded", 1, now - 5_000, None).await;
    // 其他队列（隔离）
    insert_job(&pool, "m1", "mail", "queued", 0, now - 10_000, None).await;

    let s = snapshot(&pool, "default").await.unwrap();
    assert_eq!(s.queue, "default");
    assert_eq!(s.queued, 2, "queue depth (queued)");
    assert_eq!(s.running, 2);
    assert_eq!(s.retry_wait, 1);
    assert_eq!(s.dead, 1, "dead count");
    assert_eq!(s.lease_overdue, 1, "lease timeout：租约过期的 running 数");

    // age：最老待处理 = q1（120s 前）
    let age = s.oldest_pending_age_ms.expect("有待处理任务");
    assert!(age >= 120_000, "最老任务年龄至少 120s，得到 {age}");

    // avg attempts：queued/running/retry_wait = (0+0+1+2+1)/5 = 0.8
    assert!(
        (s.avg_attempts - 0.8).abs() < 1e-9,
        "avg attempts 应为 0.8，得到 {}",
        s.avg_attempts
    );

    // 队列隔离：mail 队列只有 1 queued
    let m = snapshot(&pool, "mail").await.unwrap();
    assert_eq!(m.queued, 1);
    assert_eq!(m.running, 0);
    assert_eq!(m.dead, 0);
    assert_eq!(m.lease_overdue, 0);

    // 空队列：age None、avg 0
    let empty = snapshot(&pool, "search").await.unwrap();
    assert_eq!(empty.oldest_pending_age_ms, None, "空队列无最老任务年龄");
    assert_eq!(empty.avg_attempts, 0.0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 处理延迟追踪器：worker 完成任务后累计 count/均值/最大值。
#[tokio::test]
async fn latency_tracker_tracks_processing_latency() {
    let tracker = LatencyTracker::default();
    assert_eq!(tracker.snapshot().average_ms(), None);

    // 模拟两次处理（10ms / 30ms）
    let t = std::time::Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    tracker.record(t.elapsed().as_millis() as u64);

    let t = std::time::Instant::now();
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    tracker.record(t.elapsed().as_millis() as u64);

    let stats = tracker.snapshot();
    assert_eq!(stats.count, 2);
    assert!(stats.average_ms().unwrap() >= 20, "均值应接近 (10+30)/2");
    assert!(stats.max_ms >= 30, "最大值应接近 30");
}
