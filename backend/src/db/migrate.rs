use std::{path::Path, time::SystemTime};

use sha2::{Digest, Sha256};
use sqlx::Either;

use crate::db::pool::DatabasePool;

/// 迁移文件记录
#[derive(Debug)]
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

/// 创建迁移历史表
pub async fn ensure_migration_table(pool: &DatabasePool) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
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
                "CREATE TABLE IF NOT EXISTS _sqlx_migrations (
                    version BIGINT PRIMARY KEY NOT NULL,
                    name VARCHAR(255) NOT NULL,
                    checksum VARCHAR(64) NOT NULL,
                    applied_at BIGINT NOT NULL
                ) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4",
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
    let sql = "SELECT version, name, checksum, applied_at FROM _sqlx_migrations ORDER BY version";
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
    let sql = "SELECT MAX(version) FROM _sqlx_migrations";
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
            sqlx::query("INSERT INTO _sqlx_migrations (version, name, checksum, applied_at) VALUES (?, ?, ?, ?)")
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
            sqlx::query("INSERT INTO _sqlx_migrations (version, name, checksum, applied_at) VALUES (?, ?, ?, ?)")
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

/// 检查迁移状态（不修改数据库）
pub async fn check_migrations(
    pool: &DatabasePool,
    files: &[MigrationFile],
) -> Result<MigrationCheckResult, sqlx::Error> {
    ensure_migration_table(pool).await?;
    let applied = list_applied_migrations(pool).await?;

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
                    "INSERT INTO _sqlx_migrations (version, name, checksum, applied_at) VALUES (5, 'x', 'y', ?)",
                )
                .bind(now)
                .execute(p)
                .await
                .unwrap();
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT INTO _sqlx_migrations (version, name, checksum, applied_at) VALUES (5, 'x', 'y', ?)",
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
}
