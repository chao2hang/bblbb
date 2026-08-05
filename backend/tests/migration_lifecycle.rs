//! M01-DB-10：迁移生命周期测试。
//!
//! 覆盖工作包验收项：
//! 1. 空库迁移：全新数据库应用全部真实迁移；
//! 2. 第二次幂等运行：重复执行不重复应用；
//! 3. 失败迁移不标成功：SQL 失败时事务回滚，history 无记录、DDL 不残留；
//! 4. 上一发布版本升级：旧版本库升级只应用新增迁移；
//! 5. 反向守卫：旧代码遇到超前数据库必须拒绝。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{
    apply_migration, ensure_migration_table, list_applied_migrations, read_migration_files,
    run_migrations, MigrationFile,
};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sha2::{Digest, Sha256};
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

/// 创建临时 SQLite 数据库的连接池（Either 形式，与迁移 API 一致）。
async fn sqlite_pool(dir: &std::path::Path) -> DatabasePool {
    let url = format!("sqlite://{}", dir.display());
    create_pool(&url).await.unwrap()
}

/// 查询 sqlite_master 中某表是否残留（失败回滚验证）。
async fn sqlite_table_count(pool: &DatabasePool, table: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?")
                .bind(table)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("this test is SQLite-only"),
    }
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

fn test_file(version: u64, name: &str, sql: &str) -> MigrationFile {
    MigrationFile {
        version,
        name: name.to_string(),
        sql: sql.to_string(),
        checksum: hex::encode(Sha256::digest(sql.as_bytes())),
    }
}

/// 1. 空库迁移：全新数据库应用全部真实迁移，history 完整记录。
#[tokio::test]
async fn empty_database_applies_all_migrations() {
    let dir = std::env::temp_dir().join(format!("bblbb-lifecycle-{}", uuid::Uuid::now_v7()));
    let pool = sqlite_pool(&dir).await;
    let files = read_migration_files(&migrations_dir()).unwrap();
    assert!(files.len() >= 6, "expected at least 6 real migrations");

    let applied = run_migrations(&pool, &files).await.unwrap();
    assert_eq!(applied, files.len());

    let records = list_applied_migrations(&pool).await.unwrap();
    assert_eq!(records.len(), files.len());
    let versions: Vec<i64> = records.iter().map(|r| r.version).collect();
    assert_eq!(versions, (1..=files.len() as i64).collect::<Vec<_>>());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 2. 第二次幂等运行：真实迁移集重复执行应用 0 个，history 不变。
#[tokio::test]
async fn second_run_is_idempotent() {
    let dir = std::env::temp_dir().join(format!("bblbb-lifecycle-{}", uuid::Uuid::now_v7()));
    let pool = sqlite_pool(&dir).await;
    let files = read_migration_files(&migrations_dir()).unwrap();

    let first = run_migrations(&pool, &files).await.unwrap();
    assert_eq!(first, files.len());

    let second = run_migrations(&pool, &files).await.unwrap();
    assert_eq!(second, 0, "第二次运行必须幂等");

    let records = list_applied_migrations(&pool).await.unwrap();
    assert_eq!(records.len(), files.len());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 3. 失败迁移不标成功：SQL 中途失败 → 事务整体回滚，history 无记录、
///    已执行的 DDL 也不残留。
#[tokio::test]
async fn failed_migration_is_rolled_back_and_not_recorded() {
    let dir = std::env::temp_dir().join(format!("bblbb-lifecycle-{}", uuid::Uuid::now_v7()));
    let pool = sqlite_pool(&dir).await;

    // 前半段会建表成功，后半段触发错误 → 整个事务必须回滚
    let bad = test_file(
        1,
        "doomed",
        "CREATE TABLE doomed (id INTEGER PRIMARY KEY); INSERT INTO missing_table VALUES (1);",
    );
    let err = run_migrations(&pool, &[bad]).await.unwrap_err();
    assert!(!format!("{err}").is_empty(), "失败迁移必须返回错误");

    // history 无记录
    let records = list_applied_migrations(&pool).await.unwrap();
    assert_eq!(records.len(), 0, "失败迁移不得标记成功");

    // 建表被回滚：doomed 表不存在
    assert_eq!(
        sqlite_table_count(&pool, "doomed").await,
        0,
        "失败迁移的 DDL 必须回滚"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 3b. 失败迁移后数据库仍可正常迁移（history 与状态一致）。
#[tokio::test]
async fn database_recovers_after_failed_migration() {
    let dir = std::env::temp_dir().join(format!("bblbb-lifecycle-{}", uuid::Uuid::now_v7()));
    let pool = sqlite_pool(&dir).await;

    let bad = test_file(
        1,
        "doomed",
        "CREATE TABLE doomed (id INTEGER PRIMARY KEY); INSERT INTO missing_table VALUES (1);",
    );
    assert!(run_migrations(&pool, std::slice::from_ref(&bad))
        .await
        .is_err());

    // 用可用的迁移重新应用同一版本 → 成功
    let good = test_file(1, "doomed", "CREATE TABLE doomed (id INTEGER PRIMARY KEY);");
    let applied = run_migrations(&pool, &[good]).await.unwrap();
    assert_eq!(applied, 1);
    assert_eq!(list_applied_migrations(&pool).await.unwrap().len(), 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 4. 上一发布版本升级：旧版本库升级到新代码只应用新增迁移。
#[tokio::test]
async fn previous_release_upgrade_applies_only_new_migrations() {
    let dir = std::env::temp_dir().join(format!("bblbb-lifecycle-{}", uuid::Uuid::now_v7()));
    let pool = sqlite_pool(&dir).await;
    let files = read_migration_files(&migrations_dir()).unwrap();
    let total = files.len();
    assert!(total >= 6);

    // 旧发布只有前 total-1 个
    let previous: Vec<MigrationFile> = files.iter().take(total - 1).cloned().collect();
    let applied_old = run_migrations(&pool, &previous).await.unwrap();
    assert_eq!(applied_old, (total - 1) as usize);

    // 新发布全部 → 只应用最后新增的一个
    let applied_new = run_migrations(&pool, &files).await.unwrap();
    assert_eq!(applied_new, 1, "升级只应应用新增迁移");

    let records = list_applied_migrations(&pool).await.unwrap();
    let versions: Vec<i64> = records.iter().map(|r| r.version).collect();
    assert_eq!(versions, (1..=total as i64).collect::<Vec<_>>());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 5. 反向守卫：新库（1..6）遇到旧代码（1..5）必须拒绝，防止降级覆盖。
#[tokio::test]
async fn old_code_refuses_future_database() {
    let dir = std::env::temp_dir().join(format!("bblbb-lifecycle-{}", uuid::Uuid::now_v7()));
    let pool = sqlite_pool(&dir).await;
    let files = read_migration_files(&migrations_dir()).unwrap();

    // 数据库被更新到 1..6
    run_migrations(&pool, &files).await.unwrap();

    // 旧代码只有前 total-1 个 → 数据库超前，拒绝
    let previous: Vec<MigrationFile> = files.iter().take(files.len() - 1).cloned().collect();
    let err = run_migrations(&pool, &previous).await.unwrap_err();
    assert!(
        format!("{err}").contains("ahead of migration files"),
        "{err}"
    );

    // 数据库状态未被修改
    let records = list_applied_migrations(&pool).await.unwrap();
    assert_eq!(records.len(), files.len());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 6. 单条 apply_migration 也按事务执行（失败同样回滚）。
#[tokio::test]
async fn apply_migration_is_transactional() {
    let dir = std::env::temp_dir().join(format!("bblbb-lifecycle-{}", uuid::Uuid::now_v7()));
    let pool = sqlite_pool(&dir).await;
    ensure_migration_table(&pool).await.unwrap();

    let bad = test_file(
        1,
        "doomed",
        "CREATE TABLE doomed (id INTEGER PRIMARY KEY); INSERT INTO missing_table VALUES (1);",
    );
    assert!(apply_migration(&pool, &bad).await.is_err());
    assert_eq!(sqlite_table_count(&pool, "doomed").await, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M01-JOBS-01：jobs 表与 outbox 扩展的契约——状态/attempt/run_at/lease/
/// payload version/幂等约束（deduplication_key / idempotency_key 唯一）。
#[tokio::test]
async fn jobs_and_outbox_schema_contract() {
    let dir = std::env::temp_dir().join(format!("bblbb-lifecycle-{}", uuid::Uuid::now_v7()));
    let pool = sqlite_pool(&dir).await;
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();

    match &pool {
        Either::Left(p) => {
            // jobs 表关键列
            let job_columns: Vec<String> =
                sqlx::query_scalar("SELECT name FROM pragma_table_info('jobs') ORDER BY cid")
                    .fetch_all(p)
                    .await
                    .unwrap();
            for expected in [
                "id",
                "queue",
                "kind",
                "payload",
                "payload_version",
                "status",
                "attempts",
                "max_attempts",
                "available_at",
                "locked_by",
                "locked_until",
                "deduplication_key",
                "last_error",
                "completed_at",
                "created_at",
                "updated_at",
            ] {
                assert!(
                    job_columns.contains(&expected.to_string()),
                    "jobs 缺少列 {expected}"
                );
            }

            // outbox 扩展列
            let outbox_columns: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM pragma_table_info('outbox_events') ORDER BY cid",
            )
            .fetch_all(p)
            .await
            .unwrap();
            assert!(
                outbox_columns.contains(&"payload_version".to_string()),
                "outbox 缺少 payload_version"
            );
            assert!(
                outbox_columns.contains(&"idempotency_key".to_string()),
                "outbox 缺少 idempotency_key"
            );

            // 幂等约束：唯一索引存在
            let unique_indexes: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM sqlite_master WHERE type='index' AND sql LIKE '%UNIQUE%'",
            )
            .fetch_all(p)
            .await
            .unwrap();
            assert!(
                unique_indexes
                    .iter()
                    .any(|i| i.contains("jobs_deduplication_key_uq")),
                "jobs 缺少 deduplication_key 唯一索引: {unique_indexes:?}"
            );
            assert!(
                unique_indexes
                    .iter()
                    .any(|i| i.contains("outbox_events_idempotency_key_uq")),
                "outbox 缺少 idempotency_key 唯一索引"
            );

            // 幂等约束生效：重复 deduplication_key 被拒绝（id 不同，主键不冲突）
            let now = 1_700_000_000_000i64;
            let insert = |id: &str, key: Option<&str>| {
                let pool2 = p.clone();
                let id2 = id.to_owned();
                let key2 = key.map(str::to_owned);
                async move {
                    sqlx::query(
                        "INSERT INTO jobs (id, kind, payload, payload_version, status, attempts, max_attempts, available_at, deduplication_key, created_at, updated_at)
                         VALUES (?, 'mail', '{}', 1, 'queued', 0, 5, ?, ?, ?, ?)",
                    )
                    .bind(id2)
                    .bind(now)
                    .bind(key2)
                    .bind(now)
                    .bind(now)
                    .execute(&pool2)
                    .await
                }
            };
            insert("job-1", Some("dedup-1")).await.unwrap();
            assert!(
                insert("job-2", Some("dedup-1")).await.is_err(),
                "相同 deduplication_key 必须被唯一约束拒绝"
            );
            insert("job-3", None).await.unwrap();
            insert("job-4", None).await.unwrap(); // NULL 不冲突
        }
        Either::Right(_) => panic!("this test is SQLite-only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}
