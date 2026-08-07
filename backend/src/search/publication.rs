//! 公开索引文档投影（M08-INDEX-01）。
//!
//! 搜索结果/Feed/SEO 只从**公开投影**取数据：标题、slug/URL、清洗后摘要、
//! 标签、作者公开投影、source/policy revision 与 index policy。本模块是唯一
//! 把 `search_documents` 内部行转换为公开投影的纯逻辑层（无 DB），投影字段
//! 全部来自可见性裁决后的安全文本（M03-SEARCH-STORE-05），绝不包含
//! `body`（内部索引正文）或 `restricted_*` 内容。
//!
//! - [`PublicIndexProjection`]：单条结果的公开投影（与 OpenAPI `SearchResult`
//!   对齐，另含 author/tags/revisions 供结果层拼接）；
//! - [`IndexPolicy`]：公开 index policy（search_index / ai_summary 是否允许，
//!   以及来源 author/admin）；
//! - [`url_for`]：按实体类型组装公开 URL（`/posts/{slug}` 等）；
//! - [`highlight_snippet`]：搜索高亮——从**已清洗索引正文**截取命中窗口并截断
//!   到 [`HIGHLIGHT_MAX_LEN`]，输入面禁止受限内容（canary 保证：索引正文本身
//!   不含受限正文，故高亮也不可能泄漏）。

use crate::search::{SearchEntityType, EXCERPT_MAX};

/// 高亮窗口最大长度（字符，M08-INDEX-06）。
pub const HIGHLIGHT_MAX_LEN: usize = 160;

/// 公开 index policy（M08-INDEX-01/03）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexPolicy {
    /// 是否允许搜索引擎公开索引（作者逐帖退出 + 管理员全站/板块策略叠加）。
    pub search_index: bool,
    /// 是否允许 AI 摘要（作者逐帖退出 + 管理员策略叠加；M9 消费）。
    pub ai_summary: bool,
    /// 影响来源（`author` = 作者逐帖退出；`admin` = 管理员策略；`none`）。
    pub source: &'static str,
}

impl IndexPolicy {
    /// 从作者逐帖退出标记与管理员策略构建（管理员 deny 优先，CRAWLER-POLICY §3）。
    pub fn effective(
        search_index_opt_out: bool,
        ai_summary_opt_out: bool,
        admin_search_index: &str,
        admin_ai_summary: &str,
    ) -> Self {
        let denied = |opt_out: bool, admin: &str| admin == "deny" || opt_out;
        let search_allowed = !denied(search_index_opt_out, admin_search_index);
        let ai_allowed = !denied(ai_summary_opt_out, admin_ai_summary);
        let source = if admin_search_index == "deny" || admin_ai_summary == "deny" {
            "admin"
        } else if search_index_opt_out || ai_summary_opt_out {
            "author"
        } else {
            "none"
        };
        IndexPolicy {
            search_index: search_allowed,
            ai_summary: ai_allowed,
            source,
        }
    }
}

/// 作者公开投影（用户名已归一化；display_name 可为空）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicAuthor {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
}

/// 公开索引文档投影（M08-INDEX-01）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicIndexProjection {
    pub id: String,
    pub entity_type: SearchEntityType,
    pub title: String,
    /// 公开 URL（`url_for` 组装，稳定 canonical 来源）。
    pub url: String,
    /// 已清洗摘要（≤ 200 字符，绝不包含受限正文）。
    pub excerpt: String,
    pub tags: Vec<String>,
    pub author: Option<PublicAuthor>,
    /// 源内容版本（内容漂移水位）。
    pub source_revision: i64,
    /// 策略版本（可见性/策略状态水位；条件 upsert 守卫来源）。
    pub policy_revision: i64,
    pub indexed_at: i64,
}

impl PublicIndexProjection {
    /// 构造投影：`excerpt` 截断到 [`crate::search::EXCERPT_MAX`]（字符边界）。
    #[allow(clippy::too_many_arguments)] // 投影字段有界且显式（M08-INDEX-01）
    pub fn new(
        id: String,
        entity_type: SearchEntityType,
        title: String,
        slug: String,
        excerpt: String,
        tags: Vec<String>,
        author: Option<PublicAuthor>,
        source_revision: i64,
        policy_revision: i64,
        indexed_at: i64,
    ) -> Self {
        let excerpt = crate::search::excerpt_from_clean(&excerpt, EXCERPT_MAX);
        Self {
            url: url_for(entity_type, &slug),
            id,
            entity_type,
            title,
            excerpt,
            tags,
            author,
            source_revision,
            policy_revision,
            indexed_at,
        }
    }
}

/// 按实体类型组装公开 URL（与 OpenAPI `SearchResult.url` 一致）。
pub fn url_for(entity_type: SearchEntityType, slug: &str) -> String {
    match entity_type {
        SearchEntityType::Post => format!("/posts/{slug}"),
        SearchEntityType::User => format!("/users/{slug}"),
        SearchEntityType::Board => format!("/boards/{slug}"),
        SearchEntityType::Tag => format!("/tags/{slug}"),
    }
}

/// 序列化为 OpenAPI `SearchResult` 字段（id/type/title/url/excerpt），
/// 可选附加 `highlight`（高亮有长度上限，M08-INDEX-06）。
pub fn search_result_json(
    proj: &PublicIndexProjection,
    highlight: Option<&str>,
) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    map.insert("id".into(), serde_json::Value::String(proj.id.clone()));
    map.insert(
        "type".into(),
        serde_json::Value::String(proj.entity_type.as_str().to_string()),
    );
    map.insert(
        "title".into(),
        serde_json::Value::String(proj.title.clone()),
    );
    map.insert("url".into(), serde_json::Value::String(proj.url.clone()));
    map.insert(
        "excerpt".into(),
        serde_json::Value::String(proj.excerpt.clone()),
    );
    if let Some(hl) = highlight {
        map.insert(
            "highlight".into(),
            serde_json::Value::String(hl.to_string()),
        );
    }
    serde_json::Value::Object(map)
}

/// 从**已清洗索引正文**生成高亮窗口：围绕首个命中词取不超过
/// [`HIGHLIGHT_MAX_LEN`] 字符的片段（字符边界安全，前后补 `…`）。
///
/// 输入必须来自 `search_documents.body`（索引正文经清洗/裁决，不含受限内容），
/// 从而高亮不可能泄漏隐藏正文（M08-INDEX-08 canary）。
pub fn highlight_snippet(clean_body: &str, query_tokens: &[String], max_len: usize) -> String {
    if clean_body.is_empty() || query_tokens.is_empty() || max_len == 0 {
        return String::new();
    }
    let lower = clean_body.to_lowercase();
    let hit_pos = query_tokens
        .iter()
        .filter_map(|t| {
            let t = t.trim_matches('"');
            if t.is_empty() {
                None
            } else {
                lower.find(&t.to_lowercase())
            }
        })
        .min()
        .unwrap_or(0);

    // 窗口：命中词为中心（前 2/3、后 1/3），截断到 max_len 字符。
    let total_chars = clean_body.chars().count();
    let mut start_char = hit_pos.saturating_sub(max_len * 2 / 3);
    if start_char > 0 {
        start_char = adjust_char_boundary(clean_body, start_char);
    }
    let mut end_char = (start_char + max_len).min(total_chars);
    if end_char < total_chars {
        end_char = adjust_char_boundary(clean_body, end_char);
    }

    let mut out: String = clean_body
        .chars()
        .skip(start_char)
        .take(end_char - start_char)
        .collect();
    if start_char > 0 {
        out.insert(0, '…');
    }
    if end_char < total_chars {
        out.push('…');
    }
    crate::search::excerpt_from_clean(&out, max_len + 2)
}

/// 把字符偏移对齐到 UTF-8 字节边界。
fn adjust_char_boundary(s: &str, char_idx: usize) -> usize {
    s.char_indices()
        .nth(char_idx)
        .map(|(byte, _)| byte)
        .unwrap_or(s.len())
}

/// 从索引正文构建可搜索 token 列表（查询校验后使用；见 query 模块）。
#[cfg(test)]
pub(crate) fn clean_query_tokens(q: &str) -> Vec<String> {
    q.split_whitespace()
        .map(|t| t.replace('"', ""))
        .filter(|t| !t.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchEntityType;

    #[test]
    fn url_for_assembles_per_entity() {
        assert_eq!(url_for(SearchEntityType::Post, "my-post"), "/posts/my-post");
        assert_eq!(url_for(SearchEntityType::User, "alice"), "/users/alice");
        assert_eq!(
            url_for(SearchEntityType::Board, "general"),
            "/boards/general"
        );
        assert_eq!(url_for(SearchEntityType::Tag, "rust"), "/tags/rust");
    }

    #[test]
    fn projection_bounds_excerpt_and_url() {
        let proj = PublicIndexProjection::new(
            "p1".to_string(),
            SearchEntityType::Post,
            "标题".to_string(),
            "p1-slug".to_string(),
            "长摘要".repeat(200),
            vec!["rust".to_string()],
            Some(PublicAuthor {
                id: "u1".to_string(),
                username: "alice".to_string(),
                display_name: None,
            }),
            1,
            2,
            3,
        );
        assert_eq!(proj.excerpt.chars().count(), EXCERPT_MAX);
        assert!(proj.excerpt.ends_with('…'));
        assert_eq!(proj.url, "/posts/p1-slug");
        assert_eq!(proj.tags, vec!["rust".to_string()]);
        assert_eq!(proj.author.as_ref().unwrap().username, "alice");

        let json = search_result_json(&proj, None);
        assert_eq!(json["id"], "p1");
        assert_eq!(json["type"], "post");
        assert_eq!(json["url"], "/posts/p1-slug");
        assert_eq!(json.get("highlight"), None);
    }

    #[test]
    fn search_result_json_includes_bounded_highlight() {
        let proj = PublicIndexProjection::new(
            "p1".to_string(),
            SearchEntityType::Post,
            "标题".to_string(),
            "p1".to_string(),
            "摘要".to_string(),
            vec![],
            None,
            1,
            1,
            1,
        );
        let json = search_result_json(&proj, Some("命中"));
        assert_eq!(json["highlight"], "命中");
    }

    #[test]
    fn highlight_snippet_is_bounded_and_char_safe() {
        let body = "rust 是一个系统编程语言。".repeat(20);
        let tokens = vec!["rust".to_string()];
        let hl = highlight_snippet(&body, &tokens, 80);
        assert!(hl.chars().count() <= 82);
        assert!(hl.contains("rust"), "高亮必须包含命中词: {hl}");
        assert!(hl.starts_with('…') || hl.starts_with("rust"));
        assert!(hl.ends_with('…') || hl.ends_with('。'));
    }

    #[test]
    fn highlight_snippet_empty_inputs() {
        assert_eq!(highlight_snippet("", &[], 80), "");
        assert_eq!(highlight_snippet("body", &[], 0), "");
        assert_eq!(highlight_snippet("body", &[], 80), "");
    }

    #[test]
    fn index_policy_precedence_admin_deny_wins() {
        // 作者 allow + 管理员 allow → 允许。
        let p = IndexPolicy::effective(false, false, "allow", "allow");
        assert!(p.search_index && p.ai_summary);
        assert_eq!(p.source, "none");

        // 作者 opt-out → 排除（source=author）。
        let p = IndexPolicy::effective(true, false, "allow", "allow");
        assert!(!p.search_index);
        assert!(p.ai_summary);
        assert_eq!(p.source, "author");

        // 管理员 deny 优先于作者 allow。
        let p = IndexPolicy::effective(false, false, "deny", "allow");
        assert!(!p.search_index);
        assert_eq!(p.source, "admin");
        let p = IndexPolicy::effective(true, true, "deny", "deny");
        assert!(!p.search_index && !p.ai_summary);
        assert_eq!(p.source, "admin");
    }

    #[test]
    fn clean_query_tokens_drops_quotes() {
        assert_eq!(clean_query_tokens("\"rust\" 系统"), vec!["rust", "系统"]);
        assert_eq!(clean_query_tokens("  a   b  "), vec!["a", "b"]);
        assert_eq!(clean_query_tokens(""), Vec::<String>::new());
    }
}
