//! Feed/SEO 公开帖子投影（M08-FEEDS-05/07）。
//!
//! 全部 Feed/SEO 通道（RSS/Atom/sitemap/OG/JSON-LD）从本模块取数据，加载时
//! **重新执行可见性/退出索引策略**（M08-FEEDS-05/07）：
//!
//! - `status='published'` 且未删除、非审核中（`review_status != 'pending_review'`）；
//! - 有效访问策略 public（`content_access_policies.kind` 为 NULL 或 `public`，
//!   排除 logged_in/after_reply/level/paid）；
//! - 板块启用且 public；作者 active 且未删除；
//! - 作者逐帖退出（`search_index_opt_out`）与管理员全站/板块 deny 一律排除
//!   （CRAWLER-POLICY §3/§5）；
//! - 排序：`published_at DESC, id DESC`（稳定 cursor，M08-FEEDS-01）。
//!
//! 领域边界：service 层（可用 sqlx）；XML 渲染在 [`crate::feeds::render`]（纯逻辑）。

use sqlx::Either;

use crate::db::DatabasePool;
use crate::search::policy::POLICY_DENY;

/// 站点策略缺省值（无策略行 = allow）。
const POLICY_ALLOW_FALLBACK: &str = "allow";

/// Feed 单条帖子投影（全部为公开字段；`excerpt` 为安全摘要，绝不包含受限正文）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedPost {
    pub id: String,
    pub title: String,
    pub slug: String,
    /// 公开 canonical 相对 URL（`/posts/{slug}`）。
    pub url: String,
    pub excerpt: String,
    pub author_username: String,
    pub published_at: i64,
    pub updated_at: i64,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub canonical_url: Option<String>,
}

/// Feed 分页页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedPage {
    pub items: Vec<FeedPost>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

#[derive(sqlx::FromRow)]
struct FeedPostRow {
    id: String,
    title: String,
    slug: Option<String>,
    excerpt: Option<String>,
    author_username: Option<String>,
    published_at: Option<i64>,
    updated_at: i64,
    seo_title: Option<String>,
    seo_description: Option<String>,
    canonical_url: Option<String>,
}

const FEED_SELECT: &str =
    "SELECT p.id, p.title, p.slug, pc.excerpt, u.username_normalized AS author_username,
            p.published_at, p.updated_at, p.seo_title, p.seo_description, p.canonical_url
     FROM posts p
     LEFT JOIN boards b ON b.id = p.board_id
     LEFT JOIN users u ON u.id = p.author_id
     LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
     LEFT JOIN post_contents pc ON pc.post_id = p.id
     LEFT JOIN board_index_policies bip ON bip.board_id = p.board_id
     WHERE p.status = 'published' AND p.deleted_at IS NULL
       AND COALESCE(p.review_status, 'none') <> 'pending_review'
       AND p.search_index_opt_out = 0
       AND (pol.kind IS NULL OR pol.kind = 'public')
       AND b.is_active = 1 AND b.visibility = 'public'
       AND u.status = 'active' AND u.deleted_at IS NULL
       AND COALESCE(bip.search_index, 'allow') <> 'deny'";

/// 加载 Feed 帖子页（cursor：`base64url("published_at|id")`；稳定排序）。
pub async fn load_feed_posts(
    pool: &DatabasePool,
    limit: i64,
    after: Option<&str>,
    site_search_index: &str,
) -> Result<FeedPage, String> {
    let after_key = after.and_then(parse_feed_cursor);
    let fetch_limit = limit + 1;
    let rows: Vec<FeedPostRow> = match pool {
        Either::Left(p) => {
            let mut sql = String::from(FEED_SELECT);
            if after_key.is_some() {
                sql.push_str(" AND (p.published_at < ? OR (p.published_at = ? AND p.id < ?))");
            }
            sql.push_str(" ORDER BY p.published_at DESC, p.id DESC LIMIT ?");
            let mut q = sqlx::query_as::<_, FeedPostRow>(&sql);
            if let Some((at, id)) = &after_key {
                q = q.bind(at).bind(at).bind(id);
            }
            q.bind(fetch_limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?
        }
        Either::Right(p) => {
            let mut sql = String::from(FEED_SELECT);
            if after_key.is_some() {
                sql.push_str(" AND (p.published_at < ? OR (p.published_at = ? AND p.id < ?))");
            }
            sql.push_str(" ORDER BY p.published_at DESC, p.id DESC LIMIT ?");
            let mut q = sqlx::query_as::<_, FeedPostRow>(&sql);
            if let Some((at, id)) = &after_key {
                q = q.bind(at).bind(at).bind(id);
            }
            q.bind(fetch_limit)
                .fetch_all(p)
                .await
                .map_err(|e| e.to_string())?
        }
    };
    let has_more = rows.len() as i64 > limit;
    let rows = rows.into_iter().take(limit as usize);
    let mut items = Vec::new();
    for r in rows {
        // 全站策略 deny（Rust 侧过滤；site 单行，查询内不便参数化）。
        if site_search_index == POLICY_DENY {
            continue;
        }
        let slug = r.slug.unwrap_or_else(|| r.id.clone());
        items.push(FeedPost {
            url: format!("/posts/{slug}"),
            id: r.id,
            title: r.title,
            slug,
            excerpt: r.excerpt.unwrap_or_default(),
            author_username: r.author_username.unwrap_or_default(),
            published_at: r.published_at.unwrap_or(r.updated_at),
            updated_at: r.updated_at,
            seo_title: r.seo_title,
            seo_description: r.seo_description,
            canonical_url: r.canonical_url,
        });
    }
    let last = items.last();
    let next_cursor = if has_more {
        last.map(|p| encode_feed_cursor(p.published_at, &p.id))
    } else {
        None
    };
    let more = has_more && !items.is_empty();
    Ok(FeedPage {
        items,
        next_cursor,
        has_more: more,
    })
}

/// 加载 sitemap 帖子（offset 分页/分片，M08-FEEDS-03）。
pub async fn load_sitemap_posts(
    pool: &DatabasePool,
    offset: i64,
    limit: i64,
    site_search_index: &str,
) -> Result<Vec<FeedPost>, String> {
    let rows: Vec<FeedPostRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, FeedPostRow>(&format!(
            "{FEED_SELECT} ORDER BY p.published_at DESC, p.id DESC LIMIT ? OFFSET ?"
        ))
        .bind(limit)
        .bind(offset)
        .fetch_all(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, FeedPostRow>(&format!(
            "{FEED_SELECT} ORDER BY p.published_at DESC, p.id DESC LIMIT ? OFFSET ?"
        ))
        .bind(limit)
        .bind(offset)
        .fetch_all(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    let mut items = Vec::new();
    for r in rows {
        if site_search_index == POLICY_DENY {
            continue;
        }
        let slug = r.slug.unwrap_or_else(|| r.id.clone());
        items.push(FeedPost {
            url: format!("/posts/{slug}"),
            id: r.id,
            title: r.title,
            slug,
            excerpt: r.excerpt.unwrap_or_default(),
            author_username: r.author_username.unwrap_or_default(),
            published_at: r.published_at.unwrap_or(r.updated_at),
            updated_at: r.updated_at,
            seo_title: r.seo_title,
            seo_description: r.seo_description,
            canonical_url: r.canonical_url,
        });
    }
    Ok(items)
}

/// sitemap 可列入的公开 URL 总数（用于分片元数据）。
pub async fn count_sitemap_posts(
    pool: &DatabasePool,
    site_search_index: &str,
) -> Result<i64, String> {
    if site_search_index == POLICY_DENY {
        return Ok(0);
    }
    let sql = "SELECT COUNT(*)
     FROM posts p
     LEFT JOIN boards b ON b.id = p.board_id
     LEFT JOIN users u ON u.id = p.author_id
     LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
     LEFT JOIN board_index_policies bip ON bip.board_id = p.board_id
     WHERE p.status = 'published' AND p.deleted_at IS NULL
       AND COALESCE(p.review_status, 'none') <> 'pending_review'
       AND p.search_index_opt_out = 0
       AND (pol.kind IS NULL OR pol.kind = 'public')
       AND b.is_active = 1 AND b.visibility = 'public'
       AND u.status = 'active' AND u.deleted_at IS NULL
       AND COALESCE(bip.search_index, 'allow') <> 'deny'";
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string()),
        Either::Right(p) => sqlx::query_scalar(sql)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string()),
    }
}

/// Feed cursor 编码：`base64url("published_at|id")`。
pub fn encode_feed_cursor(published_at: i64, id: &str) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(format!("{published_at}|{id}").as_bytes())
}

/// Feed cursor 解码：`(published_at, id)`。
pub fn parse_feed_cursor(raw: &str) -> Option<(i64, String)> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .ok()?;
    let text = String::from_utf8(bytes).ok()?;
    let mut parts = text.splitn(2, '|');
    let at = parts.next()?.parse::<i64>().ok()?;
    let id = parts.next()?;
    if id.is_empty() {
        return None;
    }
    Some((at, id.to_string()))
}

/// SEO 帖子投影（OpenGraph/JSON-LD/canonical，M08-FEEDS-05）：对单帖重新执行
/// 可见性与退出索引策略；不可见 → `None`。
pub async fn load_seo_post(pool: &DatabasePool, post_id: &str) -> Result<Option<SeoPost>, String> {
    let site = crate::search::load_site_policy(pool).await?;
    let site_search_index = site
        .as_ref()
        .map(|p| p.search_index.as_str())
        .unwrap_or(POLICY_ALLOW_FALLBACK);
    let row: Option<SeoPostRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, SeoPostRow>(
            "SELECT p.id, p.title, p.slug, p.post_type, p.status, p.published_at, p.updated_at,
                    p.seo_title, p.seo_description, p.canonical_url, p.search_index_opt_out,
                    p.cover_attachment_id, p.author_id,
                    u.username_normalized AS author_username,
                    pc.excerpt,
                    pol.kind AS policy_kind,
                    COALESCE(b.is_active, 0) AS board_active,
                    COALESCE(b.visibility, 'hidden') AS board_visibility,
                    COALESCE(u.status, 'deleted') AS author_status,
                    u.deleted_at AS author_deleted_at,
                    bip.search_index AS board_search_index
             FROM posts p
             LEFT JOIN boards b ON b.id = p.board_id
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             LEFT JOIN post_contents pc ON pc.post_id = p.id
             LEFT JOIN board_index_policies bip ON bip.board_id = p.board_id
             WHERE p.id = ? AND p.deleted_at IS NULL",
        )
        .bind(post_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, SeoPostRow>(
            "SELECT p.id, p.title, p.slug, p.post_type, p.status, p.published_at, p.updated_at,
                    p.seo_title, p.seo_description, p.canonical_url, p.search_index_opt_out,
                    p.cover_attachment_id, p.author_id,
                    u.username_normalized AS author_username,
                    pc.excerpt,
                    pol.kind AS policy_kind,
                    COALESCE(b.is_active, 0) AS board_active,
                    COALESCE(b.visibility, 'hidden') AS board_visibility,
                    COALESCE(u.status, 'deleted') AS author_status,
                    u.deleted_at AS author_deleted_at,
                    bip.search_index AS board_search_index
             FROM posts p
             LEFT JOIN boards b ON b.id = p.board_id
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             LEFT JOIN post_contents pc ON pc.post_id = p.id
             LEFT JOIN board_index_policies bip ON bip.board_id = p.board_id
             WHERE p.id = ? AND p.deleted_at IS NULL",
        )
        .bind(post_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    let Some(r) = row else {
        return Ok(None);
    };
    // 可见性/退出索引策略（M08-FEEDS-05）：非公开 / 全站或板块 deny → 无 SEO 投影。
    let visible = r.status == "published"
        && r.policy_kind.as_deref().is_none_or(|k| k == "public")
        && r.board_active != 0
        && r.board_visibility == "public"
        && r.author_status == "active"
        && r.author_deleted_at.is_none()
        && site_search_index != POLICY_DENY
        && r.board_search_index.as_deref().unwrap_or("allow") != POLICY_DENY;
    if !visible {
        return Ok(None);
    }
    let slug = r.slug.unwrap_or_else(|| r.id.clone());
    Ok(Some(SeoPost {
        id: r.id,
        title: r.title,
        post_type: r.post_type,
        slug,
        author_username: r.author_username.unwrap_or_default(),
        author_id: r.author_id,
        excerpt: r.excerpt.unwrap_or_default(),
        published_at: r.published_at.unwrap_or(r.updated_at),
        updated_at: r.updated_at,
        seo_title: r.seo_title,
        seo_description: r.seo_description,
        canonical_url: r.canonical_url,
        search_index_opt_out: r.search_index_opt_out != 0,
        cover_attachment_id: r.cover_attachment_id,
    }))
}

#[derive(sqlx::FromRow)]
struct SeoPostRow {
    id: String,
    title: String,
    slug: Option<String>,
    post_type: String,
    status: String,
    published_at: Option<i64>,
    updated_at: i64,
    seo_title: Option<String>,
    seo_description: Option<String>,
    canonical_url: Option<String>,
    search_index_opt_out: i64,
    cover_attachment_id: Option<String>,
    author_id: String,
    author_username: Option<String>,
    excerpt: Option<String>,
    policy_kind: Option<String>,
    board_active: i64,
    board_visibility: String,
    author_status: String,
    author_deleted_at: Option<i64>,
    board_search_index: Option<String>,
}

/// SEO 帖子投影（公开字段；`index_allowed` 供 `X-Robots-Tag`/meta noindex 使用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeoPost {
    pub id: String,
    pub title: String,
    pub post_type: String,
    pub slug: String,
    pub author_username: String,
    pub author_id: String,
    pub excerpt: String,
    pub published_at: i64,
    pub updated_at: i64,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub canonical_url: Option<String>,
    /// 作者逐帖退出（供管理员/作者策略叠加）。
    pub search_index_opt_out: bool,
    pub cover_attachment_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_cursor_roundtrip() {
        let cursor = encode_feed_cursor(1_700_000_000_000, "post-1");
        assert_eq!(
            parse_feed_cursor(&cursor),
            Some((1_700_000_000_000, "post-1".to_string()))
        );
        assert_eq!(parse_feed_cursor(""), None);
        assert_eq!(parse_feed_cursor("YQ=="), None); // "a"
        assert_eq!(parse_feed_cursor("bm90LWJhc2U2NA=="), None); // 无效结构
    }
}
