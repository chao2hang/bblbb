//! 全量重建索引（M08-INDEX-05）。
//!
//! [`rebuild_all_index`] 按**当前权限与策略**重新生成全部索引文档：
//!
//! - 逐源行调用 [`crate::search::index_job`] 的 `reindex_*`（统一公开可索引性
//!   裁决 + 条件 upsert 守卫）——**旧 revision 不覆盖新**：持旧策略快照的
//!   写回者在守卫下被拒绝（`stored.policy_revision > candidate`）；
//! - 源行缺失的残留文档按实体类型清理（幂等）；
//! - 收尾重建全文索引（[`crate::search::rebuild_fts`]：SQLite FTS5 rebuild /
//!   MySQL/MariaDB `OPTIMIZE TABLE`）。
//!
//! 与单文档 Job 共用同一裁决/写入面，保证重建结果与增量路径一致。

use sqlx::Either;

use crate::db::DatabasePool;
use crate::search::index_job::{reindex_board, reindex_post, reindex_tag, reindex_user};
use crate::search::rebuild_fts;

/// 重建结果摘要。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RebuildSummary {
    pub posts: usize,
    pub users: usize,
    pub boards: usize,
    pub tags: usize,
}

impl RebuildSummary {
    pub fn total(&self) -> usize {
        self.posts + self.users + self.boards + self.tags
    }
}

/// 全量重建索引（按当前权限/策略重新生成；幂等）。
pub async fn rebuild_all_index(pool: &DatabasePool) -> Result<RebuildSummary, String> {
    let posts = reindex_all(pool, "posts", "post", "post").await?;
    let users = reindex_all(pool, "users", "user", "user").await?;
    let boards = reindex_all(pool, "boards", "board", "board").await?;
    let tags = reindex_all(pool, "tags", "tag", "tag").await?;
    rebuild_fts(pool).await?;
    Ok(RebuildSummary {
        posts,
        users,
        boards,
        tags,
    })
}

/// 逐源行重建并清理残留文档。`table`/`doc_type`/`entity_type` 为内部固定
/// 字面量（只由 [`rebuild_all_index`] 以白名单值调用，无注入面）。
async fn reindex_all(
    pool: &DatabasePool,
    table: &str,
    doc_type: &str,
    entity_type: &str,
) -> Result<usize, String> {
    let ids: Vec<String> = match pool {
        Either::Left(p) => sqlx::query_scalar::<_, String>(&format!("SELECT id FROM {table}"))
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_scalar::<_, String>(&format!("SELECT id FROM {table}"))
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?,
    };
    for id in &ids {
        match entity_type {
            "post" => reindex_post(pool, id).await?,
            "user" => reindex_user(pool, id).await?,
            "board" => reindex_board(pool, id).await?,
            "tag" => reindex_tag(pool, id).await?,
            _ => unreachable!(),
        }
    }
    // 清理源行已不存在的残留文档（幂等；FTS 由 0030 触发器同步）。
    let sql = format!(
        "DELETE FROM search_documents
         WHERE entity_type = ? AND doc_id NOT IN (SELECT id FROM {table})"
    );
    match pool {
        Either::Left(p) => {
            sqlx::query(&sql)
                .bind(doc_type)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        Either::Right(p) => {
            sqlx::query(&sql)
                .bind(doc_type)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(ids.len())
}
