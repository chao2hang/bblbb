//! M01-DB-11：事务并发关键语义测试。
//!
//! SQLite（本地始终运行）：
//! - `BEGIN IMMEDIATE` 在事务开始即持写锁：并发写者阻塞等待而非 SQLITE_BUSY；
//! - `BEGIN IMMEDIATE` 与 `busy_timeout=0` 组合：锁被占用时立即失败（证明
//!   写锁在 BEGIN 时获取，而不是 deferred 升级）。
//!
//! MySQL/MariaDB（需要服务器，`BBLBB_TEST_MYSQL_URL` 环境变量；标记 `#[ignore]`，
//! 由 CI 的 mysql-family 任务以 `--ignored` 运行）：
//! - 行锁阻塞并发更新者直到提交；
//! - `innodb_lock_wait_timeout` 超时映射为 ER_LOCK_WAIT_TIMEOUT（1205）；
//! - 死锁检测：一个事务以 ER_LOCK_DEADLOCK（1213）回滚，另一个成功。

use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bblbb_backend::db::pool::create_pool;
use sqlx::Either;

// ─────────────────────────── SQLite 语义 ───────────────────────────

fn temp_dir() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("bblbb-tx-{}", uuid::Uuid::now_v7()))
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

/// BEGIN IMMEDIATE 在事务开始即持写锁：
/// A 持有写锁期间，B 的 BEGIN IMMEDIATE 必须阻塞等待（受 busy_timeout 约束），
/// A 提交后 B 继续，两者都成功且写入按序生效。
#[tokio::test]
async fn sqlite_immediate_begin_blocks_concurrent_writer_until_commit() {
    let dir = temp_dir();
    let url = format!("sqlite://{}", dir.display());
    let pool = match create_pool(&url).await.unwrap() {
        Either::Left(p) => p,
        Either::Right(_) => panic!("expected sqlite pool"),
    };

    sqlx::query("CREATE TABLE counters (id INTEGER PRIMARY KEY, val INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO counters (id, val) VALUES (1, 0)")
        .execute(&pool)
        .await
        .unwrap();

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let finished = std::sync::Arc::new(AtomicBool::new(false));
    let finished_b = finished.clone();

    // A：BEGIN IMMEDIATE + 写 + 持锁
    let a_pool = pool.clone();
    let a = tokio::spawn(async move {
        let mut tx = a_pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query("UPDATE counters SET val = val + 1 WHERE id = 1")
            .execute(&mut *tx)
            .await
            .unwrap();
        let _ = ready_tx.send(());
        let _ = release_rx.await;
        tx.commit().await.unwrap();
    });

    // B：BEGIN IMMEDIATE + 写 —— 必须阻塞直到 A 提交
    let b_pool = pool.clone();
    let b = tokio::spawn(async move {
        let mut tx = b_pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query("UPDATE counters SET val = val + 2 WHERE id = 1")
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        finished_b.store(true, Ordering::SeqCst);
    });

    ready_rx.await.unwrap(); // A 已持锁
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(
        !finished.load(Ordering::SeqCst),
        "A 持锁期间 B 的 BEGIN IMMEDIATE 必须阻塞（而不是完成或 SQLITE_BUSY）"
    );
    let _ = release_tx.send(());
    a.await.unwrap();
    b.await.unwrap();
    assert!(finished.load(Ordering::SeqCst), "B 必须在 A 提交后完成");

    let val: i64 = sqlx::query_scalar("SELECT val FROM counters WHERE id = 1")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(val, 3, "两次更新必须按序生效（0 + 1 + 2）");

    pool.close().await;
    cleanup(&dir);
}

/// BEGIN IMMEDIATE 语义判别：锁被占用且 busy_timeout=0 时立即失败（SQLITE_BUSY），
/// 证明写锁在 BEGIN 时获取（deferred BEGIN 要到写语句才报错）。
#[tokio::test]
async fn sqlite_immediate_begin_fails_fast_without_busy_timeout() {
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    let dir = temp_dir();
    let opts = SqliteConnectOptions::new()
        .filename(&dir)
        .create_if_missing(true)
        .busy_timeout(Duration::ZERO);
    let pool = SqlitePoolOptions::new().connect_with(opts).await.unwrap();

    sqlx::query("CREATE TABLE counters (id INTEGER PRIMARY KEY, val INTEGER NOT NULL)")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO counters (id, val) VALUES (1, 0)")
        .execute(&pool)
        .await
        .unwrap();

    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();

    // A：持写锁
    let a_pool = pool.clone();
    let a = tokio::spawn(async move {
        let mut tx = a_pool.begin_with("BEGIN IMMEDIATE").await.unwrap();
        sqlx::query("UPDATE counters SET val = 1 WHERE id = 1")
            .execute(&mut *tx)
            .await
            .unwrap();
        let _ = release_rx.await;
        tx.commit().await.unwrap();
    });

    // 等 A 持锁后，B 的 BEGIN IMMEDIATE 必须立即失败（busy_timeout=0）
    tokio::time::sleep(Duration::from_millis(200)).await;
    let err = pool.begin_with("BEGIN IMMEDIATE").await.unwrap_err();
    assert!(
        format!("{err}")
            .to_lowercase()
            .contains("database is locked")
            || format!("{err}").contains("5"),
        "busy_timeout=0 时被占用锁应立即 SQLITE_BUSY: {err}"
    );

    let _ = release_tx.send(());
    a.await.unwrap();

    pool.close().await;
    cleanup(&dir);
}

// ────────────────────────── MySQL/MariaDB 语义 ──────────────────────────
// 这些测试需要真实 MySQL/MariaDB 服务器；CI 的 mysql-family 任务设置
// BBLBB_TEST_MYSQL_URL 并以 `cargo test --test transaction_concurrency -- --ignored` 运行。

fn mysql_url() -> Option<String> {
    std::env::var("BBLBB_TEST_MYSQL_URL").ok()
}

async fn mysql_pool() -> sqlx::MySqlPool {
    let url = mysql_url().expect("BBLBB_TEST_MYSQL_URL 未设置（CI 环境）");
    sqlx::MySqlPool::connect(&url)
        .await
        .expect("连接 MySQL 失败")
}

/// 建一张带两行的锁测试表（每次测试用唯一表名避免冲突）。
async fn seed_lock_table(pool: &sqlx::MySqlPool, table: &str) {
    sqlx::query(&format!(
        "CREATE TABLE {table} (id INT PRIMARY KEY, val INT NOT NULL)"
    ))
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(&format!(
        "INSERT INTO {table} (id, val) VALUES (1, 0), (2, 0)"
    ))
    .execute(pool)
    .await
    .unwrap();
}

fn mysql_error_code(err: &sqlx::Error) -> Option<String> {
    match err {
        sqlx::Error::Database(db) => db.code().map(|c| c.to_string()),
        _ => None,
    }
}

/// 行锁：tx1 更新行 1 并持锁，tx2 更新同一行必须阻塞；
/// tx1 提交后 tx2 继续，两者成功，最终值按序生效。
#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务）"]
async fn mysql_row_lock_blocks_concurrent_updater_until_commit() {
    let pool = mysql_pool().await;
    let table = format!("test_locks_{}", uuid::Uuid::now_v7().simple());
    seed_lock_table(&pool, &table).await;

    let (ready_tx, ready_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
    let b_finished = std::sync::Arc::new(AtomicBool::new(false));

    let a_pool = pool.clone();
    let a_table = table.clone();
    let a = tokio::spawn(async move {
        let mut tx = a_pool.begin().await.unwrap();
        sqlx::query(&format!("UPDATE {a_table} SET val = val + 1 WHERE id = 1"))
            .execute(&mut *tx)
            .await
            .unwrap();
        let _ = ready_tx.send(());
        let _ = release_rx.await;
        tx.commit().await.unwrap();
    });

    let b_pool = pool.clone();
    let b_table = table.clone();
    let b_finished_flag = b_finished.clone();
    let b = tokio::spawn(async move {
        let mut tx = b_pool.begin().await.unwrap();
        sqlx::query(&format!("UPDATE {b_table} SET val = val + 2 WHERE id = 1"))
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
        b_finished_flag.store(true, Ordering::SeqCst);
    });

    ready_rx.await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(
        !b_finished.load(Ordering::SeqCst),
        "tx1 持行锁期间 tx2 更新同一行必须阻塞"
    );

    let _ = release_tx.send(());
    a.await.unwrap();
    b.await.unwrap();

    let val: i64 = sqlx::query_scalar(&format!("SELECT val FROM {table} WHERE id = 1"))
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(val, 3);

    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

/// 锁等待超时：`innodb_lock_wait_timeout=1` 下，tx2 更新被 tx1 锁定的行
/// 约 1 秒后失败，错误码映射为 ER_LOCK_WAIT_TIMEOUT（1205）。
#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务）"]
async fn mysql_lock_wait_timeout_maps_to_1205() {
    let pool = mysql_pool().await;
    let table = format!("test_locks_{}", uuid::Uuid::now_v7().simple());
    seed_lock_table(&pool, &table).await;

    let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();

    let a_pool = pool.clone();
    let a_table = table.clone();
    let a = tokio::spawn(async move {
        let mut tx = a_pool.begin().await.unwrap();
        sqlx::query(&format!("UPDATE {a_table} SET val = 10 WHERE id = 1"))
            .execute(&mut *tx)
            .await
            .unwrap();
        let _ = release_rx.await;
        tx.commit().await.unwrap();
    });

    // tx2 会话把锁等待超时压到 1 秒
    tokio::time::sleep(Duration::from_millis(200)).await;
    let mut tx2 = pool.begin().await.unwrap();
    sqlx::query("SET SESSION innodb_lock_wait_timeout = 1")
        .execute(&mut *tx2)
        .await
        .unwrap();
    let err = sqlx::query(&format!("UPDATE {table} SET val = 20 WHERE id = 1"))
        .execute(&mut *tx2)
        .await
        .unwrap_err();
    assert_eq!(
        mysql_error_code(&err),
        Some("1205".to_string()),
        "锁等待超时必须是 ER_LOCK_WAIT_TIMEOUT(1205): {err}"
    );
    let _ = tx2.rollback().await;

    let _ = release_tx.send(());
    a.await.unwrap();

    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

/// 死锁检测：tx1 锁行1→行2，tx2 锁行2→行1；InnoDB 检测到环后
/// 以一个事务 ER_LOCK_DEADLOCK（1213）回滚、另一个成功结束。
#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务）"]
async fn mysql_deadlock_detects_and_aborts_one() {
    let pool = mysql_pool().await;
    let table = format!("test_locks_{}", uuid::Uuid::now_v7().simple());
    seed_lock_table(&pool, &table).await;

    let a_pool = pool.clone();
    let a_table = table.clone();
    let (b_holds_row2_tx, b_holds_row2_rx) = tokio::sync::oneshot::channel::<()>();
    let a = tokio::spawn(async move {
        let mut tx = a_pool.begin().await.unwrap();
        sqlx::query(&format!("UPDATE {a_table} SET val = val + 1 WHERE id = 1"))
            .execute(&mut *tx)
            .await
            .unwrap();
        // 等 tx2 确实持有行 2 锁后再申请行 2，形成等待环
        let _ = b_holds_row2_rx.await;
        let r = sqlx::query(&format!("UPDATE {a_table} SET val = val + 1 WHERE id = 2"))
            .execute(&mut *tx)
            .await;
        (tx, r)
    });

    let b_pool = pool.clone();
    let b_table = table.clone();
    let b = tokio::spawn(async move {
        let mut tx = b_pool.begin().await.unwrap();
        sqlx::query(&format!("UPDATE {b_table} SET val = val + 1 WHERE id = 2"))
            .execute(&mut *tx)
            .await
            .unwrap();
        let _ = b_holds_row2_tx.send(());
        let r = sqlx::query(&format!("UPDATE {b_table} SET val = val + 1 WHERE id = 1"))
            .execute(&mut *tx)
            .await;
        (tx, r)
    });

    let (a_tx, a_result) = a.await.unwrap();
    let (b_tx, b_result) = b.await.unwrap();

    // 恰好一个事务以 1213 回滚，另一个成功提交
    let a_code = a_result.as_ref().err().and_then(mysql_error_code);
    let b_code = b_result.as_ref().err().and_then(mysql_error_code);
    let deadlock_count = [&a_code, &b_code]
        .iter()
        .filter(|c| c.as_deref() == Some("1213"))
        .count();
    assert_eq!(
        deadlock_count, 1,
        "死锁必须恰好回滚一个事务: a={a_code:?} b={b_code:?}"
    );

    if a_result.is_ok() {
        a_tx.commit().await.unwrap();
        let _ = b_tx.rollback().await;
    } else {
        b_tx.commit().await.unwrap();
        let _ = a_tx.rollback().await;
    }

    sqlx::query(&format!("DROP TABLE {table}"))
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}
