//! 全文索引维护（M03-SEARCH-STORE-02）。
//!
//! - [`rebuild_fts`]：重建命令——SQLite 走 FTS5 external content 表的
//!   `INSERT INTO search_fts(search_fts) VALUES('rebuild')`（从
//!   `search_documents` 重建全文索引，幂等）；MySQL/MariaDB 走
//!   `OPTIMIZE TABLE search_documents`（0031/0032 加入 FULLTEXT 后重建索引）。
//!
//! 更新策略（docs/SEARCH.md §7）：`search_documents` 由索引 Job
//! （M03-SEARCH-STORE-06）维护；SQLite FTS5 由 0030 迁移中的触发器同步
//! （Job 不直接写 FTS 表）；MySQL/MariaDB FULLTEXT 由 InnoDB 原生随行更新，
//! 无需触发器。

use sqlx::Either;

use crate::db::DatabasePool;

/// 重建全文索引（幂等）。
///
/// SQLite：FTS5 external content 表从 `search_documents` 全量重建；
/// MySQL/MariaDB：`OPTIMIZE TABLE` 重建表与 FULLTEXT 索引。
pub async fn rebuild_fts(pool: &DatabasePool) -> Result<(), String> {
    match pool {
        Either::Left(db) => {
            sqlx::query("INSERT INTO search_fts(search_fts) VALUES('rebuild')")
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
        }
        Either::Right(db) => {
            sqlx::query("OPTIMIZE TABLE search_documents")
                .execute(db)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
