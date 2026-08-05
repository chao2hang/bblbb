//! M01-JOBS-08：Worker 收到停机信号后停止领取新任务，完成/释放当前任务，
//! 并受总超时约束。

use std::path::{Path, PathBuf};
use std::time::Duration;

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::worker::ClaimedJob;
use bblbb_backend::jobs::worker_loop::{run_worker, JobOutcome, WorkerConfig};
use sqlx::Either;
use tokio::sync::watch;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

/// 建库并应用全部真实迁移；返回 (pool, sqlite 文件路径)。
async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-worker-{}", uuid::Uuid::now_v7()));
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

/// 插入一个 queued 任务。
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

async fn job_status(pool: &DatabasePool, id: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?")
            .bind(id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn fast_config() -> WorkerConfig {
    WorkerConfig {
        poll_interval: Duration::from_millis(25),
        drain_timeout: Duration::from_secs(2),
        ..WorkerConfig::default()
    }
}

/// 停机后停止领取新任务：已领取的任务完成，停机后新入队任务保持 queued。
#[tokio::test]
async fn worker_stops_claiming_new_tasks_after_shutdown() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;
    insert_queued_job(&pool, "j2", now - 10_000).await;

    let (tx, rx) = watch::channel(false);
    let pool2 = pool.clone();
    let worker = tokio::spawn(async move {
        run_worker(&pool2, fast_config(), rx, |_job: ClaimedJob| async move {
            JobOutcome::Succeeded
        })
        .await;
    });

    // 等 worker 领取并完成 j1/j2
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(job_status(&pool, "j1").await, "succeeded");
    assert_eq!(job_status(&pool, "j2").await, "succeeded");

    // 触发停机并等待 worker 退出
    tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(3), worker)
        .await
        .expect("worker 应在停机后退出")
        .unwrap();

    // 停机后再入队 → 不再被领取
    insert_queued_job(&pool, "j3", now_ms() - 1_000).await;
    tokio::time::sleep(Duration::from_millis(120)).await;
    assert_eq!(
        job_status(&pool, "j3").await,
        "queued",
        "停机后不得领取新任务"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 停机信号在任务执行期间到达：正在执行的任务仍完成（不丢弃）。
#[tokio::test]
async fn worker_finishes_in_flight_job_after_shutdown() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "j1", now - 10_000).await;

    let (tx, rx) = watch::channel(false);
    let pool2 = pool.clone();
    let worker = tokio::spawn(async move {
        run_worker(&pool2, fast_config(), rx, |_job: ClaimedJob| async move {
            // 模拟慢任务：停机信号很可能在此时到达
            tokio::time::sleep(Duration::from_millis(150)).await;
            JobOutcome::Succeeded
        })
        .await;
    });

    // 等到任务进入 running（已被领取）
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while job_status(&pool, "j1").await != "running" {
        assert!(
            tokio::time::Instant::now() < deadline,
            "j1 未在限时内被领取"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // 任务执行中触发停机
    tx.send(true).unwrap();
    tokio::time::timeout(Duration::from_secs(3), worker)
        .await
        .expect("worker 应在收尾后退出")
        .unwrap();

    assert_eq!(
        job_status(&pool, "j1").await,
        "succeeded",
        "在途任务必须完成，不得丢弃"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 总超时约束：handler 永不返回时，worker 在 drain_timeout 内退出，
/// 任务保持 running 交由租约恢复，不阻塞停机。
#[tokio::test]
async fn worker_drain_is_bounded_by_timeout() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    insert_queued_job(&pool, "hung", now - 10_000).await;

    let (tx, rx) = watch::channel(false);
    let pool2 = pool.clone();
    let config = WorkerConfig {
        poll_interval: Duration::from_millis(25),
        drain_timeout: Duration::from_millis(300),
        ..WorkerConfig::default()
    };
    let worker = tokio::spawn(async move {
        run_worker(&pool2, config, rx, |_job: ClaimedJob| async move {
            // 永不返回
            loop {
                tokio::time::sleep(Duration::from_secs(60)).await;
            }
        })
        .await;
    });

    // 等到任务被领取（进入 running）
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while job_status(&pool, "hung").await != "running" {
        assert!(
            tokio::time::Instant::now() < deadline,
            "hung 未在限时内被领取"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    // 触发停机：worker 必须在 drain_timeout 量级内退出
    tx.send(true).unwrap();
    let start = tokio::time::Instant::now();
    tokio::time::timeout(Duration::from_secs(3), worker)
        .await
        .expect("worker 必须受总超时约束退出")
        .unwrap();
    let elapsed = start.elapsed();
    assert!(
        elapsed < Duration::from_secs(2),
        "收尾应被 drain_timeout 约束，实际 {elapsed:?}"
    );

    // 超时任务保持 running：租约到期后由其他 worker 安全重领
    assert_eq!(job_status(&pool, "hung").await, "running");

    close_pool(&pool).await;
    cleanup(&dir);
}
