//! 搜索索引指标与版本（M08-INDEX-09）。
//!
//! - [`INDEX_SCHEMA_VERSION`]：索引 schema 版本（v1 = M03-SEARCH-STORE；
//!   v2 = M08-INDEX 统一排除/退出/重建）。
//! - [`index_queue_metrics`]：`search.index` 队列的失败/堆积指标——待处理数、
//!   running 数、dead-letter 数、最老待处理年龄与索引文档总量（M08-INDEX-09
//!   的失败/堆积观测输入）。

use sqlx::Either;

use crate::db::DatabasePool;
use crate::outbox::now_millis;

/// 索引 schema 版本（迁移 0030 + 0053；重建/迁移校验用）。
pub const INDEX_SCHEMA_VERSION: u64 = 2;

/// 搜索索引队列指标快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SearchIndexMetrics {
    pub queued: i64,
    pub running: i64,
    pub dead: i64,
    /// 最老待处理索引任务年龄（毫秒；None = 无待处理）。
    pub oldest_pending_age_ms: Option<i64>,
    /// 索引文档总量。
    pub indexed_docs: i64,
}

/// 读取 `search.index` 队列失败/堆积指标（M08-INDEX-09）。
pub async fn index_queue_metrics(pool: &DatabasePool) -> Result<SearchIndexMetrics, String> {
    // 按 kind 精确统计 `search.index` 队列（与 default 队列内其他 job 隔离）。
    let (queued, running, dead): (i64, i64, i64) = match pool {
        Either::Left(p) => {
            let q = sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index' AND status = 'queued'",
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            let r = sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index' AND status = 'running'",
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            let d = sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index' AND status = 'dead'",
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            (q, r, d)
        }
        Either::Right(p) => {
            let q = sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index' AND status = 'queued'",
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            let r = sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index' AND status = 'running'",
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            let d = sqlx::query_scalar(
                "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index' AND status = 'dead'",
            )
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            (q, r, d)
        }
    };

    let oldest: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT MIN(available_at) FROM jobs
             WHERE kind = 'search.index' AND status IN ('queued', 'retry_wait') AND available_at <= ?",
        )
        .bind(now_millis())
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?
        .flatten(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT MIN(available_at) FROM jobs
             WHERE kind = 'search.index' AND status IN ('queued', 'retry_wait') AND available_at <= ?",
        )
        .bind(now_millis())
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?
        .flatten(),
    };
    let oldest_pending_age_ms = oldest.map(|at| (now_millis() - at).max(0));

    let indexed_docs: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM search_documents")
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_scalar("SELECT COUNT(*) FROM search_documents")
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?,
    };

    Ok(SearchIndexMetrics {
        queued,
        running,
        dead,
        oldest_pending_age_ms,
        indexed_docs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schema_version_is_monotonic() {
        const _: () = assert!(INDEX_SCHEMA_VERSION >= 2);
    }

    #[test]
    fn snapshot_struct_is_copy() {
        let m = SearchIndexMetrics {
            queued: 0,
            running: 0,
            dead: 0,
            oldest_pending_age_ms: None,
            indexed_docs: 0,
        };
        let m2 = m;
        assert_eq!(m.queued, m2.queued);
    }
}
