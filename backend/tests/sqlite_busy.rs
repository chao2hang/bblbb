//! M01-JOBS-09：SQLite busy 时指数退避并计数，禁止无延迟高频自旋。
//!
//! 用 `busy_timeout = 0` 的独立连接制造真实 `SQLITE_BUSY`，验证：
//! 1. `is_busy_error` 能识别；
//! 2. `retry_on_busy` 指数退避重试并在锁释放后成功，且累计计数；
//! 3. 非 busy 错误不重试、不计数。

use std::str::FromStr;
use std::time::Duration;

use bblbb_backend::db::busy::{is_busy_error, retry_on_busy, BusyCounter, BusyPolicy};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};

/// 一个 busy_timeout=0 的 SQLite 池（真实 busy 立即返回，不等 5s）。
async fn no_busy_timeout_pool(tag: &str) -> (SqlitePool, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-busy-{tag}-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let options = SqliteConnectOptions::from_str(&url)
        .unwrap()
        .create_if_missing(true)
        .busy_timeout(Duration::ZERO);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::query("CREATE TABLE t (id INTEGER PRIMARY KEY, v TEXT)")
        .execute(&pool)
        .await
        .unwrap();
    (pool, dir)
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

/// 真实 SQLITE_BUSY：一个连接持写锁时，另一连接的写入立即返回 busy。
#[tokio::test]
async fn is_busy_error_detects_real_sqlite_lock() {
    let (pool, dir) = no_busy_timeout_pool("detect").await;

    // 连接 A 持写锁（BEGIN IMMEDIATE + INSERT，未提交）
    let mut a = pool.acquire().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *a)
        .await
        .unwrap();
    sqlx::query("INSERT INTO t (v) VALUES ('a')")
        .execute(&mut *a)
        .await
        .unwrap();

    // 连接 B：busy_timeout=0 → 立即 busy
    let mut b = pool.acquire().await.unwrap();
    let err = sqlx::query("INSERT INTO t (v) VALUES ('b')")
        .execute(&mut *b)
        .await
        .unwrap_err();
    assert!(is_busy_error(&err), "写锁冲突应识别为 busy，得到: {err}");

    // 释放锁，后续写入正常
    sqlx::query("ROLLBACK").execute(&mut *a).await.unwrap();
    drop(a);
    sqlx::query("INSERT INTO t (v) VALUES ('c')")
        .execute(&pool)
        .await
        .unwrap();

    pool.close().await;
    cleanup(&dir);
}

/// 指数退避：锁释放后重试成功；每次 busy 都计数；至少等待 base 而非自旋。
#[tokio::test]
async fn retry_on_busy_backs_off_then_succeeds_and_counts() {
    let (pool, dir) = no_busy_timeout_pool("retry").await;
    let counter = BusyCounter::default();
    let policy = BusyPolicy {
        base_delay_ms: 20,
        max_delay_ms: 200,
        max_attempts: 20,
        jitter_ms: 10,
    };

    // 连接 A 持写锁，150ms 后回滚释放
    let mut a = pool.acquire().await.unwrap();
    sqlx::query("BEGIN IMMEDIATE")
        .execute(&mut *a)
        .await
        .unwrap();
    sqlx::query("INSERT INTO t (v) VALUES ('a')")
        .execute(&mut *a)
        .await
        .unwrap();
    let mut a2 = a;
    let release = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(150)).await;
        sqlx::query("ROLLBACK").execute(&mut *a2).await.unwrap();
    });

    // busy 重试：前几次失败（每次至少等 base），锁释放后成功
    let started = std::time::Instant::now();
    let value = retry_on_busy(&policy, &counter, || async {
        let res = sqlx::query("INSERT INTO t (v) VALUES ('b')")
            .execute(&pool)
            .await?;
        Ok(res.rows_affected())
    })
    .await
    .unwrap();
    release.await.unwrap();

    assert_eq!(value, 1, "重试成功后插入一行");
    assert!(
        counter.count() >= 1,
        "busy 必须计数，实际 {}",
        counter.count()
    );
    let elapsed = started.elapsed();
    assert!(
        elapsed >= Duration::from_millis(20),
        "至少等待 base_delay，不得无延迟自旋，实际 {elapsed:?}"
    );

    // 验证插入确实发生且只发生一次
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM t WHERE v = 'b'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);

    pool.close().await;
    cleanup(&dir);
}

/// 非 busy 错误（唯一约束冲突）不重试、不计数、立即返回。
#[tokio::test]
async fn retry_on_busy_passes_through_non_busy_errors() {
    let (pool, dir) = no_busy_timeout_pool("pass").await;
    sqlx::query("CREATE TABLE uq (id INTEGER PRIMARY KEY, k TEXT UNIQUE)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO uq (k) VALUES ('dup')")
        .execute(&pool)
        .await
        .unwrap();

    let counter = BusyCounter::default();
    let policy = BusyPolicy {
        base_delay_ms: 10,
        max_delay_ms: 100,
        max_attempts: 5,
        jitter_ms: 0,
    };
    let attempts = std::sync::atomic::AtomicU32::new(0);
    let err = retry_on_busy(&policy, &counter, || {
        attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        sqlx::query("INSERT INTO uq (k) VALUES ('dup')").execute(&pool)
    })
    .await
    .unwrap_err();

    assert!(!is_busy_error(&err), "唯一约束冲突不是 busy，得到: {err}");
    assert_eq!(
        attempts.load(std::sync::atomic::Ordering::SeqCst),
        1,
        "非 busy 不重试"
    );
    assert_eq!(counter.count(), 0, "非 busy 不计数");

    pool.close().await;
    cleanup(&dir);
}
