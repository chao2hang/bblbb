//! M01-JOBS-04：批量领取、owner、lease 延期与 lease 到期后的安全重领。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::worker;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

/// 建库并应用全部真实迁移；返回 (pool, sqlite 文件路径)。
async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-jobs-{}", uuid::Uuid::now_v7()));
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

fn now() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 插入一个 queued 任务（固定基准 created_at/updated_at）。
async fn insert_queued_job(pool: &DatabasePool, id: &str, queue: &str, available_at: i64) {
    let base = now();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, created_at, updated_at)
                 VALUES (?, ?, 'mail', '{}', 1, 'queued', 0, 5, ?, ?, ?)",
            )
            .bind(id)
            .bind(queue)
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

/// 模拟一次崩溃：任务保持 running 但把 lease 改到过去（worker 未释放）。
async fn expire_lease(pool: &DatabasePool, id: &str) {
    let base = now();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE jobs SET locked_until = ? WHERE id = ? AND status = 'running'")
                .bind(base - 1_000)
                .bind(id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 读取任务的 (status, locked_by, locked_until, attempts)。
async fn job_row(pool: &DatabasePool, id: &str) -> (String, Option<String>, Option<i64>, i64) {
    match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT status, locked_by, locked_until, attempts FROM jobs WHERE id = ?",
        )
        .bind(id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 批量领取：最老优先、最多 limit 个，领取后进入 running 且 attempts+1。
#[tokio::test]
async fn claim_batch_claims_oldest_first_up_to_limit() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now();
    insert_queued_job(&pool, "j1", "default", now - 30_000).await;
    insert_queued_job(&pool, "j2", "default", now - 20_000).await;
    insert_queued_job(&pool, "j3", "default", now - 10_000).await;

    let claimed = worker::claim_batch(&pool, "worker-1", "default", 2, 30_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 2, "limit=2 只应领取 2 个");
    assert_eq!(claimed[0].id, "j1", "最老 available_at 先领取");
    assert_eq!(claimed[1].id, "j2");
    assert_eq!(claimed[0].attempts, 1);
    assert_eq!(claimed[1].attempts, 1);
    assert!(claimed[0].locked_until > now, "locked_until 应为未来时刻");

    let (status, locked_by, locked_until, attempts) = job_row(&pool, "j1").await;
    assert_eq!(status, "running");
    assert_eq!(
        locked_by.as_deref(),
        Some("worker-1"),
        "owner 必须记录 worker"
    );
    assert!(locked_until.unwrap() > now);
    assert_eq!(attempts, 1, "领取时 attempts + 1");

    // 剩余任务仍可被领取
    let rest = worker::claim_batch(&pool, "worker-1", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(rest.len(), 1);
    assert_eq!(rest[0].id, "j3");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 定时（未来 available_at）与已被其他 worker 锁定（未过期 lease）的任务不可领取。
#[tokio::test]
async fn claim_batch_skips_scheduled_and_locked_jobs() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now();
    insert_queued_job(&pool, "j1", "default", now - 10_000).await;
    insert_queued_job(&pool, "scheduled", "default", now + 60_000).await;
    // 已被 other 领取：running + 未过期 lease
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, locked_by, locked_until, created_at, updated_at)
                 VALUES ('locked', 'default', 'mail', '{}', 1, 'running', 1, 5, ?, 'other', ?, ?, ?)",
            )
            .bind(now - 10_000)
            .bind(now + 60_000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let claimed = worker::claim_batch(&pool, "worker-1", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1, "定时与已锁定任务都不可领取");
    assert_eq!(claimed[0].id, "j1");

    // 未领取任务状态未被改动
    let (status, locked_by, _, _) = job_row(&pool, "scheduled").await;
    assert_eq!(status, "queued");
    assert_eq!(locked_by, None, "定时任务不应被锁定");
    let (status, locked_by, _, _) = job_row(&pool, "locked").await;
    assert_eq!(status, "running");
    assert_eq!(locked_by.as_deref(), Some("other"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 多 worker 不会重复领取同一任务（CAS 抢占）。
#[tokio::test]
async fn claim_batch_never_double_claims() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now();
    insert_queued_job(&pool, "j1", "default", now - 10_000).await;

    let a = worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].id, "j1");

    // worker-b 再来领取同一批 → 一个都拿不到
    let b = worker::claim_batch(&pool, "worker-b", "default", 10, 30_000)
        .await
        .unwrap();
    assert!(b.is_empty(), "已 running 的任务不得被第二个 worker 领取");

    let (status, locked_by, _, _) = job_row(&pool, "j1").await;
    assert_eq!(status, "running");
    assert_eq!(locked_by.as_deref(), Some("worker-a"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// lease 延期：仅 owner 可在 lease 未过期时续租；lease 过期后续租失败。
#[tokio::test]
async fn renew_lease_requires_owner_and_live_lease() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now();
    insert_queued_job(&pool, "j1", "default", now - 10_000).await;

    let claimed = worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    let before = claimed[0].locked_until;

    // owner 续租成功，locked_until 延后
    assert!(worker::renew_lease(&pool, "worker-a", "j1", 30_000)
        .await
        .unwrap());
    let (_, _, after, _) = job_row(&pool, "j1").await;
    assert!(after.unwrap() > before, "续租后 locked_until 必须延后");

    // 非 owner 续租失败，locked_until 不变
    assert!(!worker::renew_lease(&pool, "worker-b", "j1", 30_000)
        .await
        .unwrap());
    let (_, _, unchanged, _) = job_row(&pool, "j1").await;
    assert_eq!(unchanged, after);

    // lease 过期（模拟 worker 长时间不续租）后，即使 owner 也不能再续租
    expire_lease(&pool, "j1").await;
    assert!(
        !worker::renew_lease(&pool, "worker-a", "j1", 30_000)
            .await
            .unwrap(),
        "lease 过期后续租必须失败"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 崩溃恢复：running 任务的 lease 过期后，可被其他 worker 安全重领，
/// 旧 owner 的续租失败。
#[tokio::test]
async fn expired_lease_is_safely_reclaimed_by_another_worker() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now();
    insert_queued_job(&pool, "j1", "default", now - 10_000).await;

    let a = worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(a.len(), 1);

    // 模拟 worker-a 崩溃：任务停在 running，lease 已过期
    expire_lease(&pool, "j1").await;

    // worker-b 领取 → 自动重新入队并重领
    let b = worker::claim_batch(&pool, "worker-b", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(b.len(), 1, "过期 lease 的任务必须可被重领");
    assert_eq!(b[0].id, "j1");
    assert_eq!(b[0].attempts, 2, "重领是一次新的执行尝试");

    let (status, locked_by, locked_until, attempts) = job_row(&pool, "j1").await;
    assert_eq!(status, "running");
    assert_eq!(
        locked_by.as_deref(),
        Some("worker-b"),
        "owner 切换到 worker-b"
    );
    assert!(locked_until.unwrap() > now);
    assert_eq!(attempts, 2);

    // 旧 owner 已失去租约，续租失败
    assert!(!worker::renew_lease(&pool, "worker-a", "j1", 30_000)
        .await
        .unwrap());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 队列隔离：claim 只处理本 queue；其他 queue 的过期 lease 不被触碰。
#[tokio::test]
async fn claim_batch_is_queue_scoped() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now();

    // 两个 queue 各有一个 running 且 lease 过期的任务（模拟崩溃）
    for (id, queue) in [("mail-1", "mail"), ("security-1", "security")] {
        match &pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, locked_by, locked_until, created_at, updated_at)
                     VALUES (?, ?, 'mail', '{}', 1, 'running', 1, 5, ?, 'crashed', ?, ?, ?)",
                )
                .bind(id)
                .bind(queue)
                .bind(now - 10_000)
                .bind(now - 1_000)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            }
            Either::Right(_) => panic!("SQLite only"),
        }
    }

    // mail worker 领取 → 只处理 mail 队列
    let mail = worker::claim_batch(&pool, "mail-worker", "mail", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(mail.len(), 1);
    assert_eq!(mail[0].id, "mail-1");

    // security 队列的任务未被 mail worker 触碰
    let (status, locked_by, _, _) = job_row(&pool, "security-1").await;
    assert_eq!(status, "running");
    assert_eq!(locked_by.as_deref(), Some("crashed"));

    // security worker 领取 → 处理自己的队列
    let sec = worker::claim_batch(&pool, "sec-worker", "security", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(sec.len(), 1);
    assert_eq!(sec[0].id, "security-1");

    close_pool(&pool).await;
    cleanup(&dir);
}
