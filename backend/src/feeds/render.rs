//! Feed/SEO XML 渲染（M08-FEEDS-01/02/03，纯逻辑无 DB）。
//!
//! - [`xml_escape`]：RSS/Atom/sitemap 全部文本节点与属性统一转义
//!   （`& < > " '`，M08-FEEDS-02 的 XML escaping Fixture）；
//! - [`render_rss`]：RSS 2.0（稳定 pubDate、description 摘要、guid）；
//! - [`render_atom`]：Atom 1.0（feed/entry 的 id/link/updated/published/summary/
//!   author 全字段 + 更新时间）；
//! - [`render_sitemap`] / [`render_sitemap_index`]：sitemap 分片（M08-FEEDS-03）。
//!
//! 渲染输入必须是 [`crate::feeds::projection::FeedPost`]（已执行可见性/退出索引
//! 策略的公开投影）——本模块不做任何裁决，只负责正确转义与结构。

use chrono::{DateTime, Utc};

use crate::feeds::projection::{FeedPost, SeoPost};

/// 站点标题（Feed channel/feed title）。
pub const SITE_TITLE: &str = "BBLBB";
/// Feed 描述。
pub const SITE_DESCRIPTION: &str = "BBLBB 最新公开内容";

/// XML 文本/属性转义（M08-FEEDS-02 Fixture）。
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Unix 毫秒 → RSS RFC 2822 时间（`Thu, 07 Aug 2026 12:34:56 +0000`）。
pub fn rfc2822(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).expect("epoch"))
        .format("%a, %d %b %Y %H:%M:%S %z")
        .to_string()
}

/// Unix 毫秒 → Atom RFC 3339 时间（`2026-08-07T12:34:56+00:00`）。
pub fn rfc3339(ms: i64) -> String {
    DateTime::<Utc>::from_timestamp_millis(ms)
        .unwrap_or_else(|| DateTime::<Utc>::from_timestamp(0, 0).expect("epoch"))
        .format("%Y-%m-%dT%H:%M:%S+00:00")
        .to_string()
}

/// 渲染 RSS 2.0（M08-FEEDS-01）。
pub fn render_rss(items: &[FeedPost], updated_ms: i64) -> String {
    let mut out = String::with_capacity(1024 + items.len() * 320);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<rss version=\"2.0\">\n<channel>\n");
    out.push_str(&format!("<title>{}</title>\n", xml_escape(SITE_TITLE)));
    out.push_str("<link>/</link>\n");
    out.push_str(&format!(
        "<description>{}</description>\n",
        xml_escape(SITE_DESCRIPTION)
    ));
    out.push_str(&format!(
        "<lastBuildDate>{}</lastBuildDate>\n",
        rfc2822(updated_ms)
    ));

    for p in items {
        let url = &p.url;
        out.push_str("<item>\n");
        out.push_str(&format!(
            "<title>{}</title>\n",
            xml_escape(p.seo_title.as_deref().unwrap_or(&p.title))
        ));
        out.push_str(&format!("<link>{}</link>\n", xml_escape(url)));
        out.push_str(&format!(
            "<guid isPermaLink=\"false\">{}</guid>\n",
            xml_escape(url)
        ));
        out.push_str(&format!("<pubDate>{}</pubDate>\n", rfc2822(p.published_at)));
        out.push_str(&format!(
            "<description>{}</description>\n",
            xml_escape(&p.excerpt)
        ));
        out.push_str(&format!(
            "<author>{}</author>\n",
            xml_escape(&p.author_username)
        ));
        out.push_str("</item>\n");
    }
    out.push_str("</channel>\n</rss>\n");
    out
}

/// 渲染 Atom 1.0（M08-FEEDS-02）。
pub fn render_atom(items: &[FeedPost], updated_ms: i64) -> String {
    let mut out = String::with_capacity(1024 + items.len() * 360);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
    out.push_str(&format!("<title>{}</title>\n", xml_escape(SITE_TITLE)));
    out.push_str("<link href=\"/\"/>\n");
    out.push_str("<id>tag:bblbb,2026:main-feed</id>\n");
    out.push_str(&format!("<updated>{}</updated>\n", rfc3339(updated_ms)));
    out.push_str(&format!(
        "<subtitle>{}</subtitle>\n",
        xml_escape(SITE_DESCRIPTION)
    ));

    for p in items {
        let url = &p.url;
        out.push_str("<entry>\n");
        out.push_str(&format!(
            "<title>{}</title>\n",
            xml_escape(p.seo_title.as_deref().unwrap_or(&p.title))
        ));
        out.push_str(&format!("<link href=\"{}\"/>\n", xml_escape(url)));
        out.push_str(&format!("<id>{}</id>\n", xml_escape(url)));
        out.push_str(&format!("<updated>{}</updated>\n", rfc3339(p.updated_at)));
        out.push_str(&format!(
            "<published>{}</published>\n",
            rfc3339(p.published_at)
        ));
        out.push_str(&format!("<summary>{}</summary>\n", xml_escape(&p.excerpt)));
        out.push_str(&format!(
            "<author><name>{}</name></author>\n",
            xml_escape(&p.author_username)
        ));
        out.push_str("</entry>\n");
    }
    out.push_str("</feed>\n");
    out
}

/// 渲染 sitemap `<urlset>`（M08-FEEDS-03；只含允许索引的公开 canonical URL）。
pub fn render_sitemap(items: &[FeedPost]) -> String {
    let mut out = String::with_capacity(512 + items.len() * 160);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for p in items {
        let canonical = p
            .canonical_url
            .as_deref()
            .filter(|u| !u.is_empty())
            .unwrap_or(&p.url);
        out.push_str("<url>\n");
        out.push_str(&format!("<loc>{}</loc>\n", xml_escape(canonical)));
        out.push_str(&format!(
            "<lastmod>{}</lastmod>\n",
            rfc3339(p.updated_at).chars().take(10).collect::<String>()
        ));
        out.push_str("</url>\n");
    }
    out.push_str("</urlset>\n");
    out
}

/// 渲染 sitemap `<sitemapindex>`（分片导航）。
pub fn render_sitemap_index(pages: &[usize], base: &str) -> String {
    let mut out = String::with_capacity(512);
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<sitemapindex xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n");
    for page in pages {
        out.push_str("<sitemap>\n");
        out.push_str(&format!("<loc>{base}?page={page}</loc>\n"));
        out.push_str("</sitemap>\n");
    }
    out.push_str("</sitemapindex>\n");
    out
}

/// OpenGraph/JSON-LD 用 meta 数据投影（M08-FEEDS-05；纯公开字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeoMeta {
    pub canonical: String,
    pub og_title: String,
    pub og_description: String,
    pub og_type: &'static str,
    pub og_image: Option<String>,
    pub article_json_ld: String,
    /// 是否允许索引（`X-Robots-Tag`/meta noindex 决策，M08-FEEDS-04）。
    pub index_allowed: bool,
}

/// 组装 SEO meta（canonical/OG/JSON-LD/图片；索引策略单独判定）。
pub fn seo_meta_for(post: &SeoPost, admin_search_index_denied: bool) -> SeoMeta {
    let index_allowed = !post.search_index_opt_out && !admin_search_index_denied;
    let canonical = post
        .canonical_url
        .as_deref()
        .filter(|u| !u.is_empty())
        .map(|u| u.to_string())
        .unwrap_or_else(|| format!("/posts/{}", post.slug));
    let og_title = post.seo_title.as_deref().unwrap_or(&post.title).to_string();
    let og_description = post
        .seo_description
        .as_deref()
        .filter(|s| !s.is_empty())
        .unwrap_or(&post.excerpt)
        .to_string();
    let og_type = if post.post_type == "article" {
        "article"
    } else {
        "website"
    };
    let og_image = post
        .cover_attachment_id
        .as_ref()
        .map(|id| format!("/api/v1/attachments/{id}/content"));
    let json_ld = json_ld_for(post, &canonical, &og_title, &og_description);
    SeoMeta {
        canonical,
        og_title,
        og_description,
        og_type,
        og_image,
        article_json_ld: json_ld,
        index_allowed,
    }
}

/// `Article`/`DiscussionForumPosting` JSON-LD（不含正文）。
fn json_ld_for(post: &SeoPost, canonical: &str, title: &str, description: &str) -> String {
    let schema = if post.post_type == "article" {
        "Article"
    } else {
        "DiscussionForumPosting"
    };
    let escaped_title = xml_escape(title)
        .replace("&quot;", "\\\"")
        .replace('\'', "\\'");
    let escaped_desc = xml_escape(description)
        .replace("&quot;", "\\\"")
        .replace('\'', "\\'");
    let escaped_author = xml_escape(&post.author_username)
        .replace("&quot;", "\\\"")
        .replace('\'', "\\'");
    format!(
        "{{\"@context\":\"https://schema.org\",\"@type\":\"{schema}\",\
         \"headline\":\"{escaped_title}\",\"description\":\"{escaped_desc}\",\
         \"author\":{{\"@type\":\"Person\",\"name\":\"{escaped_author}\"}},\
         \"url\":\"{canonical}\",\"datePublished\":\"{published}\",\
         \"dateModified\":\"{updated}\"}}",
        schema = schema,
        canonical = xml_escape(canonical),
        published = rfc3339(post.published_at),
        updated = rfc3339(post.updated_at),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_post(title: &str) -> FeedPost {
        FeedPost {
            id: "p1".to_string(),
            title: title.to_string(),
            slug: "p1".to_string(),
            url: "/posts/p1".to_string(),
            excerpt: "摘要 & 更多".to_string(),
            author_username: "alice".to_string(),
            published_at: 1_700_000_000_000,
            updated_at: 1_700_000_100_000,
            seo_title: None,
            seo_description: None,
            canonical_url: None,
        }
    }

    #[test]
    fn xml_escape_covers_five_entities() {
        assert_eq!(
            xml_escape("<a href=\"x\">&'y'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&apos;y&apos;&lt;/a&gt;"
        );
        assert_eq!(xml_escape("plain"), "plain");
        assert_eq!(xml_escape(""), "");
    }

    #[test]
    fn dates_format_rfc2822_and_rfc3339() {
        // 2023-11-14T22:13:20Z
        assert_eq!(
            rfc2822(1_700_000_000_000),
            "Tue, 14 Nov 2023 22:13:20 +0000"
        );
        assert_eq!(rfc3339(1_700_000_000_000), "2023-11-14T22:13:20+00:00");
    }

    #[test]
    fn rss_escapes_title_excerpt_and_has_guid() {
        let xml = render_rss(&[sample_post("<b>&amp;</b>")], 1_700_000_000_000);
        assert!(xml.contains("&lt;b&gt;&amp;amp;&lt;/b&gt;"), "{xml}");
        assert!(
            xml.contains("<description>摘要 &amp; 更多</description>"),
            "{xml}"
        );
        assert!(
            xml.contains("<guid isPermaLink=\"false\">/posts/p1</guid>"),
            "{xml}"
        );
        assert!(
            xml.contains("<pubDate>Tue, 14 Nov 2023 22:13:20 +0000</pubDate>"),
            "{xml}"
        );
    }

    #[test]
    fn atom_has_full_fields_and_escaping() {
        let xml = render_atom(&[sample_post("标题\"&")], 1_700_000_000_000);
        assert!(
            xml.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"),
            "{xml}"
        );
        assert!(xml.contains("<title>标题&quot;&amp;</title>"), "{xml}");
        assert!(xml.contains("<link href=\"/posts/p1\"/>"), "{xml}");
        assert!(xml.contains("<id>/posts/p1</id>"), "{xml}");
        assert!(
            xml.contains("<updated>2023-11-14T22:15:00+00:00</updated>"),
            "{xml}"
        );
        assert!(
            xml.contains("<published>2023-11-14T22:13:20+00:00</published>"),
            "{xml}"
        );
        assert!(xml.contains("<author><name>alice</name></author>"), "{xml}");
        assert!(xml.contains("<summary>摘要 &amp; 更多</summary>"), "{xml}");
    }

    #[test]
    fn sitemap_renders_canonical_urls() {
        let xml = render_sitemap(&[sample_post("x")]);
        assert!(xml.contains("<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">"));
        assert!(xml.contains("<loc>/posts/p1</loc>"), "{xml}");
        assert!(xml.contains("<lastmod>2023-11-14</lastmod>"), "{xml}");
    }

    #[test]
    fn sitemap_index_renders_pages() {
        let xml = render_sitemap_index(&[2, 3], "/api/v1/sitemap.xml");
        assert!(xml.contains("<sitemapindex"));
        assert!(
            xml.contains("<loc>/api/v1/sitemap.xml?page=2</loc>"),
            "{xml}"
        );
        assert!(
            xml.contains("<loc>/api/v1/sitemap.xml?page=3</loc>"),
            "{xml}"
        );
    }

    #[test]
    fn seo_meta_respects_index_policy_and_image() {
        let post = SeoPost {
            id: "p1".to_string(),
            title: "标题".to_string(),
            post_type: "article".to_string(),
            slug: "p1".to_string(),
            author_username: "alice".to_string(),
            author_id: "u1".to_string(),
            excerpt: "摘要".to_string(),
            published_at: 1_700_000_000_000,
            updated_at: 1_700_000_000_000,
            seo_title: None,
            seo_description: None,
            canonical_url: None,
            search_index_opt_out: false,
            cover_attachment_id: Some("att-1".to_string()),
        };
        let meta = seo_meta_for(&post, false);
        assert!(meta.index_allowed);
        assert_eq!(meta.canonical, "/posts/p1");
        assert_eq!(meta.og_type, "article");
        assert_eq!(
            meta.og_image.as_deref(),
            Some("/api/v1/attachments/att-1/content")
        );
        assert!(meta.article_json_ld.contains("\"@type\":\"Article\""));
        assert!(meta.article_json_ld.contains("\"url\":\"/posts/p1\""));

        let denied = seo_meta_for(&post, true);
        assert!(!denied.index_allowed, "管理员 deny 必须输出 noindex 决策");

        let mut opted_out = post.clone();
        opted_out.search_index_opt_out = true;
        assert!(!seo_meta_for(&opted_out, false).index_allowed);
    }
}
