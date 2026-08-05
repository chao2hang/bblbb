use std::{path::Path, time::SystemTime};

use sha2::{Digest, Sha256};
use sqlx::Either;

use crate::db::pool::DatabasePool;

/// 迁移文件记录
#[derive(Debug, Clone)]
pub struct MigrationFile {
    pub version: u64,
    pub name: String,
    pub sql: String,
    pub checksum: String,
}

/// 已执行的迁移记录
#[derive(Debug, sqlx::FromRow)]
pub struct MigrationRecord {
    pub version: i64,
    pub name: String,
    pub checksum: String,
    pub applied_at: i64,
}

/// 读取迁移目录中的所有 SQL 文件
pub fn read_migration_files(dir: &Path) -> Result<Vec<MigrationFile>, String> {
    let mut files = Vec::new();
    if !dir.exists() {
        return Err(format!("migration directory not found: {}", dir.display()));
    }

    let entries = std::fs::read_dir(dir).map_err(|e| format!("read dir: {e}"))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or("invalid filename")?;
        let parts: Vec<&str> = filename.splitn(2, '_').collect();
        if parts.len() != 2 {
            return Err(format!("invalid migration filename: {filename}"));
        }
        let version: u64 = parts[0]
            .parse()
            .map_err(|e| format!("invalid version in {filename}: {e}"))?;
        let name = parts[1].trim_end_matches(".sql").to_string();
        let sql =
            std::fs::read_to_string(&path).map_err(|e| format!("read {}: {e}", path.display()))?;
        let checksum = hex::encode(Sha256::digest(sql.as_bytes()));

        files.push(MigrationFile {
            version,
            name,
            sql,
            checksum,
        });
    }

    files.sort_by_key(|f| f.version);
    Ok(files)
}

/// 创建迁移历史表 `schema_migrations`（M01-DB-07）。
///
/// 契约（与 docs/SCHEMA.md §3 一致，三数据库结构等价）：
/// - `version`：迁移版本，主键，防止同版本重复应用；
/// - `name`：迁移文件名（不含版本前缀）；
/// - `checksum`：迁移文件全文 SHA-256；同一版本内容一旦应用后变更必须失败；
/// - `applied_at`：应用时间（Unix 毫秒）。
///
/// 该表由迁移执行器在首次应用前创建，不进入迁移文件本身（不可变迁移不修改
/// 已发布文件）。`ReadOnly` 检查路径不创建此表（见 `check_migrations_with_mode`）。
pub async fn ensure_migration_table(pool: &DatabasePool) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version INTEGER PRIMARY KEY NOT NULL,
                    name TEXT NOT NULL,
                    checksum TEXT NOT NULL,
                    applied_at INTEGER NOT NULL
                )",
            )
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS schema_migrations (
                    version BIGINT PRIMARY KEY NOT NULL,
                    name VARCHAR(255) NOT NULL,
                    checksum VARCHAR(64) NOT NULL,
                    applied_at BIGINT NOT NULL
                ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin",
            )
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

/// 查询已执行的迁移记录
pub async fn list_applied_migrations(
    pool: &DatabasePool,
) -> Result<Vec<MigrationRecord>, sqlx::Error> {
    let sql = "SELECT version, name, checksum, applied_at FROM schema_migrations ORDER BY version";
    match pool {
        Either::Left(p) => sqlx::query_as::<_, MigrationRecord>(sql).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as::<_, MigrationRecord>(sql).fetch_all(p).await,
    }
}

/// 查询数据库已应用的最大迁移版本
///
/// 迁移表不存在（数据库尚未迁移）或为空时返回 `Ok(None)`，
/// 供 `/readyz` 做版本比对，不修改数据库（M00-BACKEND-07/08）。
pub async fn max_applied_version(pool: &DatabasePool) -> Result<Option<i64>, sqlx::Error> {
    let sql = "SELECT MAX(version) FROM schema_migrations";
    match pool {
        Either::Left(p) => match sqlx::query_scalar::<_, Option<i64>>(sql).fetch_one(p).await {
            Ok(value) => Ok(value),
            Err(e) if is_missing_table(&e) => Ok(None),
            Err(e) => Err(e),
        },
        Either::Right(p) => match sqlx::query_scalar::<_, Option<i64>>(sql).fetch_one(p).await {
            Ok(value) => Ok(value),
            Err(e) if is_missing_table(&e) => Ok(None),
            Err(e) => Err(e),
        },
    }
}

/// 判断数据库错误是否为"表不存在"
fn is_missing_table(e: &sqlx::Error) -> bool {
    match e {
        sqlx::Error::Database(db) => {
            // SQLite: "no such table"; MySQL: ER_NO_SUCH_TABLE 1146
            db.message().to_lowercase().contains("no such table")
                || db.code().map(|c| c == "1146").unwrap_or(false)
        }
        _ => false,
    }
}

/// 应用单个迁移
pub async fn apply_migration(pool: &DatabasePool, file: &MigrationFile) -> Result<(), sqlx::Error> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);

    tracing::info!(
        version = file.version,
        name = %file.name,
        "applying migration"
    );

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            // SQLite 支持执行多条语句
            sqlx::raw_sql(&file.sql).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?, ?, ?, ?)")
                .bind(file.version as i64)
                .bind(&file.name)
                .bind(&file.checksum)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            // MySQL/MariaDB 需要逐条执行
            for stmt in file.sql.split(';') {
                let stmt = stmt.trim();
                if stmt.is_empty() || stmt.starts_with("--") {
                    continue;
                }
                sqlx::query(stmt).execute(&mut *tx).await?;
            }
            sqlx::query("INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (?, ?, ?, ?)")
                .bind(file.version as i64)
                .bind(&file.name)
                .bind(&file.checksum)
                .bind(now)
                .execute(&mut *tx)
                .await?;
            tx.commit().await?;
        }
    }
    Ok(())
}

/// 迁移检查模式
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckMode {
    /// 只读检查（`migrate --check`）：迁移表不存在时视为未迁移，
    /// 不创建表、不写入任何数据。
    ReadOnly,
    /// 确保迁移表存在后再检查（默认路径，可能创建表）。
    EnsureTable,
}

/// 校验迁移文件版本顺序：严格递增且无重复。
///
/// `migrate --check` 的顺序检查；纯文件校验，不触碰数据库。
pub fn validate_file_order(files: &[MigrationFile]) -> Result<(), String> {
    let mut previous: Option<u64> = None;
    for file in files {
        if let Some(previous_version) = previous {
            if file.version <= previous_version {
                return Err(format!(
                    "migration versions must be strictly increasing; found {} after {}",
                    file.version, previous_version
                ));
            }
        }
        previous = Some(file.version);
    }
    Ok(())
}

/// 检查迁移状态（不修改迁移内容；`EnsureTable` 模式可能创建历史表）。
pub async fn check_migrations(
    pool: &DatabasePool,
    files: &[MigrationFile],
) -> Result<MigrationCheckResult, sqlx::Error> {
    check_migrations_with_mode(CheckMode::EnsureTable, pool, files).await
}

/// 只读迁移检查（`migrate --check`）：不创建迁移表、不写入任何数据。
pub async fn check_migrations_readonly(
    pool: &DatabasePool,
    files: &[MigrationFile],
) -> Result<MigrationCheckResult, sqlx::Error> {
    check_migrations_with_mode(CheckMode::ReadOnly, pool, files).await
}

/// 按模式执行迁移检查：比对文件版本、checksum 与已执行记录，并检测超前版本。
pub async fn check_migrations_with_mode(
    mode: CheckMode,
    pool: &DatabasePool,
    files: &[MigrationFile],
) -> Result<MigrationCheckResult, sqlx::Error> {
    let applied = match mode {
        CheckMode::EnsureTable => {
            ensure_migration_table(pool).await?;
            list_applied_migrations(pool).await?
        }
        CheckMode::ReadOnly => match list_applied_migrations(pool).await {
            Ok(records) => records,
            // 表不存在 = 尚未迁移：全部 pending，而不是创建表。
            Err(e) if is_missing_table(&e) => Vec::new(),
            Err(e) => return Err(e),
        },
    };

    let mut pending = Vec::new();
    let mut checksum_mismatches = Vec::new();

    for file in files {
        match applied.iter().find(|r| r.version == file.version as i64) {
            None => pending.push(file.version),
            Some(record) => {
                if record.checksum != file.checksum {
                    checksum_mismatches.push((
                        file.version,
                        file.name.clone(),
                        record.checksum.clone(),
                        file.checksum.clone(),
                    ));
                }
            }
        }
    }

    // 检查是否有额外的已执行迁移（版本号超前）
    let future_versions: Vec<i64> = applied
        .iter()
        .filter(|r| !files.iter().any(|f| f.version as i64 == r.version))
        .map(|r| r.version)
        .collect();

    Ok(MigrationCheckResult {
        pending,
        checksum_mismatches,
        future_versions,
        applied_count: applied.len(),
        total_count: files.len(),
    })
}

/// 迁移检查结果
#[derive(Debug)]
pub struct MigrationCheckResult {
    pub pending: Vec<u64>,
    pub checksum_mismatches: Vec<(u64, String, String, String)>,
    pub future_versions: Vec<i64>,
    pub applied_count: usize,
    pub total_count: usize,
}

impl MigrationCheckResult {
    pub fn is_clean(&self) -> bool {
        self.pending.is_empty()
            && self.checksum_mismatches.is_empty()
            && self.future_versions.is_empty()
    }

    /// 一致性判定（`migrate --check`）：忽略待应用迁移，只要 checksum 匹配、
    /// 无超前版本即视为一致。
    pub fn is_consistent(&self) -> bool {
        self.checksum_mismatches.is_empty() && self.future_versions.is_empty()
    }
}

/// 执行所有待应用迁移
pub async fn run_migrations(
    pool: &DatabasePool,
    files: &[MigrationFile],
) -> Result<usize, sqlx::Error> {
    ensure_migration_table(pool).await?;
    let check = check_migrations(pool, files).await?;

    if !check.checksum_mismatches.is_empty() {
        return Err(sqlx::Error::Configuration(
            format!(
                "checksum mismatch for migrations: {:?}",
                check.checksum_mismatches
            )
            .into(),
        ));
    }

    // M01-DB-06：数据库超前于迁移文件（存在代码未知/未来的已执行版本）时
    // 拒绝应用，防止对新数据库降级覆盖或静默跳过未知迁移。
    if !check.future_versions.is_empty() {
        return Err(sqlx::Error::Configuration(
            format!(
                "database is ahead of migration files (future versions {:?}); refusing to apply",
                check.future_versions
            )
            .into(),
        ));
    }

    let mut applied = 0;
    for file in files {
        if check.pending.contains(&file.version) {
            apply_migration(pool, file).await?;
            applied += 1;
        }
    }

    tracing::info!(applied, total = files.len(), "migrations complete");
    Ok(applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 验证 readyz 版本比对所用的 max_applied_version：
    /// 表不存在 → None；空表 → None；有记录 → 最大版本
    #[tokio::test]
    async fn max_applied_version_tracks_applied_migrations() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        // 尚未迁移：表不存在 → Ok(None)
        assert_eq!(max_applied_version(&pool).await.unwrap(), None);

        // 建表后仍为空 → Ok(None)
        ensure_migration_table(&pool).await.unwrap();
        assert_eq!(max_applied_version(&pool).await.unwrap(), None);

        // 写入版本 5 → Some(5)
        let now = 1_700_000_000_i64;
        match &pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (5, 'x', 'y', ?)",
                )
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT INTO schema_migrations (version, name, checksum, applied_at) VALUES (5, 'x', 'y', ?)",
                )
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            }
        }
        assert_eq!(max_applied_version(&pool).await.unwrap(), Some(5));

        // 清理
        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        let _ = std::fs::remove_file(&dir);
        let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
        let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
    }

    /// 构造测试用迁移文件记录
    fn test_file(version: u64, name: &str, sql: &str) -> MigrationFile {
        MigrationFile {
            version,
            name: name.to_string(),
            sql: sql.to_string(),
            checksum: hex::encode(Sha256::digest(sql.as_bytes())),
        }
    }

    /// 清理 SQLite 临时数据库文件
    fn cleanup_sqlite(dir: &std::path::Path) {
        let _ = std::fs::remove_file(dir);
        let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
        let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
    }

    /// `migrate --check` 的核心保证：只读检查不创建迁移表、不写入任何数据。
    #[tokio::test]
    async fn readonly_check_does_not_touch_database() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        let files = vec![test_file(
            1,
            "skeleton",
            "CREATE TABLE t (id INTEGER PRIMARY KEY);",
        )];
        let result = check_migrations_readonly(&pool, &files).await.unwrap();
        assert_eq!(result.pending, vec![1]);
        assert!(
            result.is_consistent(),
            "missing table must be treated as all-pending and consistent"
        );
        assert!(
            !result.is_clean(),
            "pending migrations mean not clean, yet consistent"
        );

        // 关键断言：数据库文件中没有任何表（schema_migrations 也未被创建）。
        match &pool {
            Either::Left(p) => {
                let tables: i64 =
                    sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table'")
                        .fetch_one(p)
                        .await
                        .unwrap();
                assert_eq!(tables, 0, "readonly check must not create any table");
            }
            Either::Right(_) => panic!("this test is SQLite-only"),
        }

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 只读检查能检测已执行迁移的 checksum 篡改。
    #[tokio::test]
    async fn readonly_check_detects_checksum_tampering() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        // 应用版本 1（写入迁移表）
        let original = test_file(1, "skeleton", "CREATE TABLE t (id INTEGER PRIMARY KEY);");
        ensure_migration_table(&pool).await.unwrap();
        apply_migration(&pool, &original).await.unwrap();

        // 同一版本、同名，但 SQL 内容被篡改 → checksum 不同
        let tampered = test_file(1, "skeleton", "CREATE TABLE t (id TEXT PRIMARY KEY);");
        let result = check_migrations_readonly(&pool, &[tampered]).await.unwrap();
        assert_eq!(result.checksum_mismatches.len(), 1);
        let (version, _, db_checksum, file_checksum) = &result.checksum_mismatches[0];
        assert_eq!(*version, 1);
        assert_ne!(db_checksum, file_checksum);
        assert!(!result.is_consistent());

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 只读检查能检测版本超前：已执行但文件不存在的迁移。
    #[tokio::test]
    async fn readonly_check_detects_future_versions() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        let v1 = test_file(1, "skeleton", "CREATE TABLE a (id INTEGER PRIMARY KEY);");
        let v2 = test_file(2, "community", "CREATE TABLE b (id INTEGER PRIMARY KEY);");
        ensure_migration_table(&pool).await.unwrap();
        apply_migration(&pool, &v1).await.unwrap();
        apply_migration(&pool, &v2).await.unwrap();

        // 文件只剩 v1 → v2 成为超前版本
        let result = check_migrations_readonly(&pool, &[v1]).await.unwrap();
        assert_eq!(result.future_versions, vec![2]);
        assert!(!result.is_consistent());

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 顺序校验：严格递增通过。
    #[test]
    fn order_accepts_strictly_increasing_versions() {
        let files = vec![
            test_file(1, "a", "SELECT 1;"),
            test_file(2, "b", "SELECT 2;"),
            test_file(3, "c", "SELECT 3;"),
        ];
        assert!(validate_file_order(&files).is_ok());
    }

    /// 顺序校验：重复版本必须失败。
    #[test]
    fn order_rejects_duplicate_versions() {
        let files = vec![
            test_file(1, "a", "SELECT 1;"),
            test_file(1, "b", "SELECT 2;"),
        ];
        let err = validate_file_order(&files).unwrap_err();
        assert!(err.contains("strictly increasing"), "{err}");
    }

    /// 顺序校验：乱序（递减）必须失败。
    #[test]
    fn order_rejects_decreasing_versions() {
        let files = vec![
            test_file(2, "b", "SELECT 2;"),
            test_file(1, "a", "SELECT 1;"),
        ];
        let err = validate_file_order(&files).unwrap_err();
        assert!(err.contains("strictly increasing"), "{err}");
    }

    /// 显式 migrate 命令：按版本顺序应用全部 pending 迁移。
    #[tokio::test]
    async fn run_migrations_applies_pending_in_order() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        let files = vec![
            test_file(1, "a", "CREATE TABLE a (id INTEGER PRIMARY KEY);"),
            test_file(2, "b", "CREATE TABLE b (id INTEGER PRIMARY KEY);"),
        ];
        let applied = run_migrations(&pool, &files).await.unwrap();
        assert_eq!(applied, 2);

        let records = list_applied_migrations(&pool).await.unwrap();
        let versions: Vec<i64> = records.iter().map(|r| r.version).collect();
        assert_eq!(versions, vec![1, 2]);

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 显式 migrate 命令：第二次运行是幂等的，应用 0 个迁移。
    #[tokio::test]
    async fn run_migrations_is_idempotent() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        let files = vec![test_file(
            1,
            "a",
            "CREATE TABLE a (id INTEGER PRIMARY KEY);",
        )];
        assert_eq!(run_migrations(&pool, &files).await.unwrap(), 1);
        assert_eq!(run_migrations(&pool, &files).await.unwrap(), 0);
        assert_eq!(list_applied_migrations(&pool).await.unwrap().len(), 1);

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 迁移历史表契约：version/name/checksum(SHA-256)/applied_at 全部正确记录。
    #[tokio::test]
    async fn history_table_records_full_contract() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        let sql = "CREATE TABLE a (id INTEGER PRIMARY KEY);";
        let files = vec![test_file(1, "skeleton", sql)];
        run_migrations(&pool, &files).await.unwrap();

        let records = list_applied_migrations(&pool).await.unwrap();
        assert_eq!(records.len(), 1);
        let record = &records[0];
        assert_eq!(record.version, 1);
        assert_eq!(record.name, "skeleton");
        // checksum 必须是迁移文件全文的 SHA-256
        let expected = hex::encode(Sha256::digest(sql.as_bytes()));
        assert_eq!(record.checksum, expected);
        // applied_at 是 Unix 毫秒，必须为正值
        assert!(
            record.applied_at > 1_000_000_000,
            "applied_at={}",
            record.applied_at
        );

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 迁移历史表结构契约：schema_migrations 恰好包含契约声明的 4 列，
    /// version 是主键，且所有列 NOT NULL。
    #[tokio::test]
    async fn history_table_schema_contract() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        ensure_migration_table(&pool).await.unwrap();

        match &pool {
            Either::Left(p) => {
                let columns: Vec<(i64, String, String, i64, String, i64)> = sqlx::query_as(
                    "SELECT cid, name, type, \"notnull\", dflt_value, pk
                     FROM pragma_table_info('schema_migrations')
                     ORDER BY cid",
                )
                .fetch_all(p)
                .await
                .unwrap();
                let names: Vec<&str> = columns.iter().map(|c| c.1.as_str()).collect();
                assert_eq!(names, vec!["version", "name", "checksum", "applied_at"]);
                // version 是主键（pk=1），其余列非空
                assert_eq!(columns[0].5, 1, "version must be primary key");
                for column in &columns {
                    assert_eq!(column.3, 1, "column {} must be NOT NULL", column.1);
                }
            }
            Either::Right(_) => panic!("this test is SQLite-only"),
        }

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 已执行迁移内容被篡改（checksum 不匹配）时拒绝继续应用。
    #[tokio::test]
    async fn run_migrations_refuses_checksum_mismatch() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        let original = test_file(1, "a", "CREATE TABLE a (id INTEGER PRIMARY KEY);");
        run_migrations(&pool, &[original]).await.unwrap();

        // 同一版本内容被篡改
        let tampered = test_file(1, "a", "CREATE TABLE a (id TEXT PRIMARY KEY);");
        let err = run_migrations(&pool, &[tampered]).await.unwrap_err();
        assert!(format!("{err}").contains("checksum mismatch"), "{err}");

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 数据库超前（未来版本）时拒绝应用，防止降级覆盖。
    #[tokio::test]
    async fn run_migrations_refuses_future_versions() {
        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();

        let v1 = test_file(1, "a", "CREATE TABLE a (id INTEGER PRIMARY KEY);");
        let v2 = test_file(2, "b", "CREATE TABLE b (id INTEGER PRIMARY KEY);");
        run_migrations(&pool, &[v1.clone(), v2]).await.unwrap();

        // 文件只剩 v1 → v2 是代码未知的未来版本，必须拒绝
        let err = run_migrations(&pool, &[v1]).await.unwrap_err();
        assert!(
            format!("{err}").contains("ahead of migration files"),
            "{err}"
        );

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }

    /// 真实迁移集端到端：种子 board 的 id 是合法 UUID v7（36 字符小写），
    /// created_at/updated_at 是 Unix 毫秒（M01-DB-08 跨库表示约定）。
    #[tokio::test]
    async fn seed_boards_conform_to_uuid7_and_millis() {
        let manifest = std::env::var("CARGO_MANIFEST_DIR").unwrap();
        let migrations_dir = Path::new(&manifest).join("../migrations/sqlite");
        let files = read_migration_files(&migrations_dir).unwrap();
        assert!(files.len() >= 6, "expected at least 6 migrations");

        let dir = std::env::temp_dir().join(format!("bblbb-migrate-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();
        run_migrations(&pool, &files).await.unwrap();

        match &pool {
            Either::Left(p) => {
                let rows: Vec<(String, i64, i64)> = sqlx::query_as(
                    "SELECT id, created_at, updated_at FROM boards ORDER BY sort_order",
                )
                .fetch_all(p)
                .await
                .unwrap();
                assert_eq!(rows.len(), 5, "seed must contain 5 boards");
                for (id, created_at, updated_at) in &rows {
                    // UUID v7：36 字符、小写、4 个连字符、时间前缀
                    assert_eq!(id.len(), 36, "board id must be 36-char UUID v7: {id}");
                    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4, "{id}");
                    assert_eq!(id, &id.to_lowercase(), "board id must be lowercase: {id}");
                    assert!(
                        id.starts_with("01911fd5-f00"),
                        "board id must be time-ordered UUID v7: {id}"
                    );
                    // Unix 毫秒：必须远大于秒级种子（1722816000）
                    assert!(
                        *created_at >= 1_700_000_000_000,
                        "created_at must be unix millis: {created_at}"
                    );
                    assert!(
                        *updated_at >= 1_700_000_000_000,
                        "updated_at must be unix millis: {updated_at}"
                    );
                }
            }
            Either::Right(_) => panic!("this test is SQLite-only"),
        }

        match &pool {
            Either::Left(p) => p.close().await,
            Either::Right(p) => p.close().await,
        }
        cleanup_sqlite(&dir);
    }
}
