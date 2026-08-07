//! Feed/SEO 缓存（M08-FEEDS-06）。
//!
//! 进程内**维度隔离**缓存：键 = `(endpoint, 查询参数, policy_revision,
//! content_revision, 公开投影维度)`。策略 revision（M08-INDEX-03 逐帖退出/
//! 管理员策略/状态/可见性变更 bump）与内容 revision（编辑 bump）任一变化都
//! 使键失效——**登录后/付费/审核中内容永远不会以陈旧键被缓存给匿名用户**
//! （CRAWLER-POLICY §3/§6；M08-FEEDS-06 验收）。
//!
//! 有界（≤ [`MAX_ENTRIES`]）+ TTL（[`TTL_MS`]），写时淘汰过期/最旧项；
//! 多节点部署下各节点独立，正确性以 ETag/`Cache-Control` 与数据库实时查询兜底。

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use sqlx::Either;

use crate::db::DatabasePool;

/// 缓存条目上限（防无限增长）。
pub const MAX_ENTRIES: usize = 128;
/// 缓存 TTL（毫秒；Feed 变更传播窗口）。
pub const TTL_MS: i64 = 60_000;

/// 缓存键（维度隔离：endpoint + 参数 + **数据源身份** + policy/content
/// revision + 投影维度）。数据源身份（SQLite 数据库文件路径 / MySQL
/// DATABASE()）防止不同数据库之间以相同的 revision 值串用缓存项
/// （M08-FEEDS-06 的三库一致性要求）。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FeedCacheKey {
    pub endpoint: &'static str,
    pub params: String,
    /// 数据源身份（`cache_pool_identity`）。
    pub identity: String,
    pub policy_revision: i64,
    pub content_revision: i64,
    /// 公开投影维度（如 `public|rss`、`public|sitemap`）。
    pub projection_dim: &'static str,
}

/// 缓存项。
#[derive(Debug, Clone)]
pub struct FeedCacheEntry {
    pub body: String,
    pub etag: String,
    pub computed_at: i64,
}

/// 全局有界缓存（进程内；单测可直接构造 [`FeedCache::new`]）。
#[derive(Debug, Default)]
pub struct FeedCache {
    inner: Mutex<HashMap<FeedCacheKey, FeedCacheEntry>>,
}

static GLOBAL: OnceLock<FeedCache> = OnceLock::new();

impl FeedCache {
    pub fn global() -> &'static FeedCache {
        GLOBAL.get_or_init(FeedCache::default)
    }

    pub fn new() -> Self {
        Self::default()
    }

    /// 读取缓存（TTL 内命中）；未命中/过期 → `None`。
    pub fn get(&self, key: &FeedCacheKey, now: i64) -> Option<FeedCacheEntry> {
        let guard = self.inner.lock().expect("feed cache poisoned");
        let entry = guard.get(key)?;
        if now - entry.computed_at > TTL_MS {
            return None;
        }
        Some(entry.clone())
    }

    /// 写入缓存（有界淘汰：TTL 过期优先，其次最旧）。
    pub fn put(&self, key: FeedCacheKey, entry: FeedCacheEntry) {
        let mut guard = self.inner.lock().expect("feed cache poisoned");
        if guard.len() >= MAX_ENTRIES && !guard.contains_key(&key) {
            let oldest = guard
                .iter()
                .min_by_key(|(_, e)| e.computed_at)
                .map(|(k, _)| k.clone());
            if let Some(k) = oldest {
                guard.remove(&k);
            }
        }
        guard.insert(key, entry);
    }
}

/// 计算 Feed/SEO 缓存的 revision 维度：`(policy_revision, content_revision)`。
///
/// Feed 直接读 posts/boards/users（不经索引），因此维度取自相关源表 + 索引：
/// - content_revision：`search_documents.source_revision` ∪ posts/post_contents
///   `updated_at` 的最大值——任何内容发布/编辑都使缓存键失效；
/// - policy_revision：`search_documents.policy_revision` ∪ posts/boards/users/
///   管理员策略行 `updated_at` 的最大值——隐藏/恢复/删除/逐帖退出/管理员策略
///   变更都使缓存键失效。
///
/// 被排除内容从投影移除后由其余行的 max 决定（不泄漏已退出内容的缓存）。
pub async fn compute_cache_revisions(pool: &DatabasePool) -> Result<(i64, i64), String> {
    let policy = scalar_max(
        pool,
        "SELECT MAX(v) FROM (
            SELECT COALESCE(MAX(policy_revision), 0) AS v FROM search_documents
            UNION ALL SELECT COALESCE(MAX(updated_at), 0) FROM posts
            UNION ALL SELECT COALESCE(MAX(updated_at), 0) FROM boards
            UNION ALL SELECT COALESCE(MAX(updated_at), 0) FROM users
            UNION ALL SELECT COALESCE(MAX(updated_at), 0) FROM search_site_index_policy
            UNION ALL SELECT COALESCE(MAX(updated_at), 0) FROM board_index_policies
        ) t",
    )
    .await?;
    let content = scalar_max(
        pool,
        "SELECT MAX(v) FROM (
            SELECT COALESCE(MAX(source_revision), 0) AS v FROM search_documents
            UNION ALL SELECT COALESCE(MAX(updated_at), 0) FROM posts
            UNION ALL SELECT COALESCE(MAX(updated_at), 0) FROM post_contents
        ) t",
    )
    .await?;
    Ok((policy, content))
}

/// 数据源身份（缓存键维度之一，防止跨数据库串用）：
/// SQLite 取主库数据库文件路径；MySQL/MariaDB 取 `DATABASE()`。
pub async fn cache_pool_identity(pool: &DatabasePool) -> Result<String, String> {
    match pool {
        Either::Left(p) => {
            let file: Option<String> =
                sqlx::query_scalar("SELECT file FROM pragma_database_list WHERE name = 'main'")
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(file.unwrap_or_else(|| "sqlite-memory".to_string()))
        }
        Either::Right(p) => {
            let db: Option<String> = sqlx::query_scalar("SELECT DATABASE()")
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?;
            Ok(db.unwrap_or_else(|| "mysql-unknown".to_string()))
        }
    }
}

async fn scalar_max(pool: &DatabasePool, sql: &str) -> Result<i64, String> {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string()),
        Either::Right(p) => sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(endpoint: &'static str, policy: i64, content: i64) -> FeedCacheKey {
        FeedCacheKey {
            endpoint,
            params: "limit=20".to_string(),
            identity: "test-db-1".to_string(),
            policy_revision: policy,
            content_revision: content,
            projection_dim: "public|rss",
        }
    }

    #[test]
    fn cache_is_isolated_by_revision_dimensions() {
        let cache = FeedCache::new();
        let now = 1_700_000_000_000i64;
        cache.put(
            key("rss", 10, 10),
            FeedCacheEntry {
                body: "body-v1".to_string(),
                etag: "\"e1\"".to_string(),
                computed_at: now,
            },
        );
        // 同键命中。
        assert_eq!(cache.get(&key("rss", 10, 10), now).unwrap().body, "body-v1");
        // policy revision 变化 → 键失效。
        assert!(cache.get(&key("rss", 11, 10), now).is_none());
        // content revision 变化 → 键失效。
        assert!(cache.get(&key("rss", 10, 11), now).is_none());
        // 投影维度变化 → 键失效。
        let mut k = key("rss", 10, 10);
        k.projection_dim = "public|atom";
        assert!(cache.get(&k, now).is_none());
        // 数据源身份变化 → 键失效（跨数据库不串用）。
        let mut k2 = key("rss", 10, 10);
        k2.identity = "other-db".to_string();
        assert!(cache.get(&k2, now).is_none());
    }

    #[test]
    fn cache_expires_after_ttl() {
        let cache = FeedCache::new();
        let now = 1_700_000_000_000i64;
        let k = key("rss", 1, 1);
        cache.put(
            k.clone(),
            FeedCacheEntry {
                body: "b".to_string(),
                etag: "\"e\"".to_string(),
                computed_at: now,
            },
        );
        assert!(cache.get(&k, now).is_some());
        assert!(cache.get(&k, now + TTL_MS + 1).is_none());
    }

    #[test]
    fn cache_evicts_oldest_when_full() {
        let cache = FeedCache::new();
        let now = 1_700_000_000_000i64;
        for i in 0..(MAX_ENTRIES + 5) {
            cache.put(
                key("rss", i as i64, i as i64),
                FeedCacheEntry {
                    body: format!("b{i}"),
                    etag: "e".to_string(),
                    computed_at: now + i as i64,
                },
            );
        }
        let guard = cache.inner.lock().unwrap();
        assert!(guard.len() <= MAX_ENTRIES, "缓存必须有界");
    }
}
