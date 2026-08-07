//! Feed/SEO 公开投影服务（M08-FEEDS）。
//!
//! - [`projection`]：Feed/SEO 公开帖子投影——加载时重新执行可见性/退出索引
//!   策略（隐藏/回复/等级/付费/审核/删除/封禁/退出一律不进投影，M08-FEEDS-05/07）；
//! - [`render`]：RSS/Atom/sitemap/OG/JSON-LD 渲染（XML escaping Fixture，M08-FEEDS-02）；
//! - [`robots`]：动态 robots.txt + `X-Robots-Tag` + meta noindex（M08-FEEDS-04）；
//! - [`cache`]：Feed/SEO 缓存——按策略 revision/内容 revision/公开投影维度隔离
//!   （M08-FEEDS-06）。
//!
//! 路由接入：`backend/src/routes/feeds.rs`（RSS/Atom/sitemap/robots 端点）。

pub mod cache;
pub mod projection;
pub mod render;
pub mod robots;

pub use cache::{
    cache_pool_identity, compute_cache_revisions, FeedCache, FeedCacheEntry, FeedCacheKey,
    MAX_ENTRIES, TTL_MS,
};
pub use projection::{
    count_sitemap_posts, encode_feed_cursor, load_feed_posts, load_seo_post, load_sitemap_posts,
    parse_feed_cursor, FeedPage, FeedPost, SeoPost,
};
pub use render::{
    render_atom, render_rss, render_sitemap, render_sitemap_index, rfc2822, rfc3339, seo_meta_for,
    xml_escape, SeoMeta, SITE_DESCRIPTION, SITE_TITLE,
};
pub use robots::{meta_robots, render_robots, x_robots_tag, AI_TRAINING_CRAWLERS};

use crate::db::DatabasePool;
use crate::search::policy::{load_site_policy, POLICY_ALLOW, POLICY_DENY};

/// 读取全站搜索索引策略值（`allow`/`deny`；供 Feed/SEO 路由过滤）。
pub async fn site_search_index(pool: &DatabasePool) -> Result<String, String> {
    Ok(load_site_policy(pool)
        .await?
        .map(|p| p.search_index)
        .unwrap_or_else(|| POLICY_ALLOW.to_string()))
}

/// 全站索引策略是否强制关闭（供 sitemap/feed 空集短路）。
pub async fn site_index_denied(pool: &DatabasePool) -> Result<bool, String> {
    Ok(site_search_index(pool).await? == POLICY_DENY)
}
