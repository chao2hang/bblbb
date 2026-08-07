//! 搜索仓储领域/服务层（M03-SEARCH-STORE；M08-INDEX 扩展）。
//!
//! - [`SearchDocument`]：搜索文档最小模型（M03-SEARCH-STORE-01）——内部索引
//!   一行为一个可搜索实体（post/user/board/tag），只允许写入经过可见性裁决的
//!   安全文本（M03-SEARCH-STORE-05 强制），绝不包含 restricted_html 或隐藏正文；
//! - [`SearchEntityType`]：可搜索实体类型（与 OpenAPI `SearchResult.type` 枚举一致）；
//! - [`clean_index_text`]：索引正文清洗——控制字符折叠、连续空白折叠、长度上限；
//! - [`excerpt_from_clean`]：公开投影摘要——从已清洗文本按字符边界截断；
//! - [`source_revision_for`] / [`policy_revision_for`]：source revision 与 policy
//!   revision 语义（旧 revision 不覆盖新 revision 的单调性来源，docs/SEARCH.md §4/§5）；
//! - [`fts`]：全文索引维护——重建命令与触发器/Job 更新策略（M03-SEARCH-STORE-02）；
//! - [`gate`]：索引写入可见性裁决 + **M08-INDEX-02 统一排除规则**
//!   （[`decide_public_post_indexability`]：状态/访问策略/板块/作者/删除/审核中/
//!   逐帖退出/管理员策略）；
//! - [`policy`]：作者逐帖退出与管理员全站/板块索引策略（M08-INDEX-03）；
//! - [`publication`]：公开索引文档投影（M08-INDEX-01，标题/slug/摘要/标签/作者/
//!   revision/index policy）；
//! - [`query`]：公开搜索查询与限制（M08-INDEX-06/07，长度/语法/结果数/分页深度/
//!   匿名频率/高亮长度 + 返回前实时重检）；
//! - [`rebuild`]：按当前权限与策略全量重建（M08-INDEX-05）；
//! - [`text`]：索引安全纯文本转换——剥离 HTML 标签后清洗截断（M03-SEARCH-STORE-06）；
//! - [`index_job`]：索引幂等 Job——创建/更新/隐藏/删除/恢复/退出索引
//!   （M03-SEARCH-STORE-06）。

pub mod fts;
pub mod gate;
pub mod index_job;
pub mod metrics;
pub mod policy;
pub mod publication;
pub mod query;
pub mod rebuild;
pub mod text;

pub use fts::rebuild_fts;
pub use gate::{
    decide_board_indexability, decide_post_indexability, decide_public_post_indexability,
    decide_tag_indexability, decide_user_indexability, vet_index_text, ExclusionReason,
    IndexDecision, IndexTextError, PostPublicIndexInput,
};
pub use index_job::{enqueue_index_job, handle_index_job, INDEX_JOB_KIND};
pub use metrics::{index_queue_metrics, SearchIndexMetrics, INDEX_SCHEMA_VERSION};
pub use policy::{
    load_board_policy, load_site_policy, set_board_policy, set_post_opt_out, set_site_policy,
    AdminIndexPolicy, POLICY_ALLOW, POLICY_DENY,
};
pub use publication::{
    highlight_snippet, search_result_json, url_for, IndexPolicy, PublicAuthor,
    PublicIndexProjection, HIGHLIGHT_MAX_LEN,
};
pub use query::{
    build_fts_query, build_mysql_boolean_query, decode_cursor, effective_admin_for_board,
    effective_ai_summary_denied, encode_cursor, execute_public_search, recheck_doc_visibility,
    SearchPage, SearchQueryError, SearchRequest, ANON_SEARCH_LIMIT, DEFAULT_LIMIT,
    LOGGED_IN_SEARCH_LIMIT, MAX_LIMIT, MAX_PAGE_DEPTH, QUERY_MAX_LEN, SEARCH_RATE_WINDOW_MS,
};
pub use rebuild::{rebuild_all_index, RebuildSummary};
pub use text::to_index_plain_text;

/// 索引标题长度上限（字符；与帖子标题 OpenAPI `maxLength: 240` 一致）。
pub const TITLE_MAX: usize = 240;
/// 索引正文长度上限（字符；索引预算，源正文可更长，文档构造时截断）。
pub const BODY_MAX: usize = 100_000;
/// 公开投影摘要长度上限（字符）。
pub const EXCERPT_MAX: usize = 200;
/// 公开投影 slug/username 长度上限（字符；与板块 slug `maxLength: 120` 一致）。
pub const SLUG_MAX: usize = 120;

/// 可搜索实体类型（与 OpenAPI `SearchResult.type` 枚举一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchEntityType {
    Post,
    User,
    Board,
    Tag,
}

impl SearchEntityType {
    pub const ALL: [SearchEntityType; 4] = [
        SearchEntityType::Post,
        SearchEntityType::User,
        SearchEntityType::Board,
        SearchEntityType::Tag,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            SearchEntityType::Post => "post",
            SearchEntityType::User => "user",
            SearchEntityType::Board => "board",
            SearchEntityType::Tag => "tag",
        }
    }

    pub fn parse(value: &str) -> Option<SearchEntityType> {
        Self::ALL.iter().find(|v| v.as_str() == value).copied()
    }
}

/// 搜索文档最小模型（内部索引行，M03-SEARCH-STORE-01）。
///
/// 一行为一个可搜索实体；`body` 为内部索引正文（对外绝不投影），`excerpt`、
/// `slug`、`tags` 为公开投影字段（`author_id` 为 post 的作者公开 id，仅入索引
/// 供结果层拼接作者卡）。构造时强制安全文本与长度上限（docs/SEARCH.md §2/§6）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDocument {
    /// 源实体 id（UUID，与 posts/users/boards/tags.id 一致）。
    pub id: String,
    /// 实体类型。
    pub entity_type: SearchEntityType,
    /// 标题（post.title / user.display_name / board.name / tag.name）。
    pub title: String,
    /// 内部索引正文（清洗后的可搜索文本；绝不对外投影，绝不包含受限正文）。
    pub body: String,
    /// 公开投影摘要（可见性裁决后的安全摘要；绝不包含受限正文）。
    pub excerpt: String,
    /// 公开投影 URL 段（post.slug / user.username_normalized / board.slug / tag.slug）。
    pub slug: String,
    /// post 作者的公开 id（仅 `entity_type == Post` 时 `Some`）。
    pub author_id: Option<String>,
    /// post 的标签名列表（公开投影；仅 `entity_type == Post` 时非空）。
    pub tags: Vec<String>,
    /// 源内容修订：源实体 `updated_at`（毫秒），内容每次变更单调递增。
    pub source_revision: i64,
    /// 策略修订：可见性/策略状态版本，策略相关变更单调递增（docs/SEARCH.md §5）。
    pub policy_revision: i64,
    /// 入索引时间（毫秒）。
    pub indexed_at: i64,
}

impl SearchDocument {
    /// 构造新文档：清洗正文/摘要、校验标题与 slug、按长度上限截断。
    ///
    /// `excerpt` 为 `None` 时从清洗后的正文按 `EXCERPT_MAX` 推导；否则先清洗
    /// 再截断。`body` 经 [`clean_index_text`] 清洗并截断到 `BODY_MAX`——
    /// 调用方仍必须只传入已通过可见性裁决的安全文本（M03-SEARCH-STORE-05）。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        entity_type: SearchEntityType,
        title: String,
        body: String,
        excerpt: Option<String>,
        slug: String,
        author_id: Option<String>,
        tags: Vec<String>,
        source_revision: i64,
        policy_revision: i64,
        indexed_at: i64,
    ) -> Result<Self, SearchValidationError> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(SearchValidationError::TitleEmpty);
        }
        if title.chars().count() > TITLE_MAX {
            return Err(SearchValidationError::TitleTooLong);
        }

        let body = clean_index_text(&body, BODY_MAX);
        if body.is_empty() {
            return Err(SearchValidationError::BodyEmpty);
        }

        let excerpt = match excerpt {
            Some(e) => excerpt_from_clean(&clean_index_text(&e, EXCERPT_MAX + 1), EXCERPT_MAX),
            None => excerpt_from_clean(&body, EXCERPT_MAX),
        };

        if slug.is_empty() {
            return Err(SearchValidationError::SlugEmpty);
        }
        if slug.chars().count() > SLUG_MAX {
            return Err(SearchValidationError::SlugTooLong);
        }
        if !slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(SearchValidationError::SlugInvalid);
        }

        Ok(SearchDocument {
            id,
            entity_type,
            title,
            body,
            excerpt,
            slug,
            author_id,
            tags,
            source_revision,
            policy_revision,
            indexed_at,
        })
    }
}

/// 文档构造校验错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchValidationError {
    TitleEmpty,
    TitleTooLong,
    BodyEmpty,
    SlugEmpty,
    SlugTooLong,
    SlugInvalid,
}

/// 清洗可索引文本：控制字符与空白折叠为单个空格、首尾 trim、截断到 `max_len`
/// 个字符（字符边界安全，输出不超过 `max_len` 个字符）。
///
/// 只应接收已通过可见性裁决的安全文本（M03-SEARCH-STORE-05）；本函数只做形式
/// 清洗，不做任何内容或可见性裁决。
pub fn clean_index_text(raw: &str, max_len: usize) -> String {
    if max_len == 0 {
        return String::new();
    }
    let mut out = String::with_capacity(raw.len().min(max_len));
    let mut consumed = 0usize;
    let mut prev_space = false;
    for ch in raw.chars() {
        if ch.is_control() || ch.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(ch);
            prev_space = false;
        }
        consumed += 1;
        if consumed >= max_len {
            break;
        }
    }
    out.trim().to_string()
}

/// 从已清洗文本构建公开投影摘要：`len <= max_len` 原样返回；否则截取
/// `max_len - 1` 个字符并以省略号 `…` 结尾（字符边界安全）。
pub fn excerpt_from_clean(clean: &str, max_len: usize) -> String {
    if clean.chars().count() <= max_len {
        return clean.to_string();
    }
    if max_len == 0 {
        return String::new();
    }
    let mut out: String = clean.chars().take(max_len - 1).collect();
    out.push('…');
    out
}

/// source revision：源实体内容版本 = 源实体 `updated_at`（毫秒）。
///
/// 内容每次变更必须递增（沿用 boards/tags/users If-Match 乐观并发语义，
/// SCHEMA.md §6；M03-BOARDS-05/07 与 M03-PROFILE-04 同源，docs/SEARCH.md §4）。
pub fn source_revision_for(updated_at: i64) -> i64 {
    updated_at
}

/// policy revision：可见性/策略状态版本 = 全部策略相关行 `updated_at` 的最大值。
///
/// 策略输入（对 post 文档）：post.status、post.visibility、board.is_active、
/// board.visibility、作者账号状态/deleted_at、作者索引退出标记（M08-INDEX-03）。
/// 不变式：任何策略相关变更必须 bump 对应行 `updated_at`（docs/SEARCH.md §5）。
/// `policy_revision` 非递减——随最新输入行变更单调不减；陈旧写防覆盖由条件
/// upsert 守卫实现：仅当 `stored.policy_revision <= candidate.policy_revision`
/// 才应用（持旧策略快照的写回者 max 更小被拒绝，旧 revision 不覆盖新）。
pub fn policy_revision_for(updated_at: &[i64]) -> i64 {
    updated_at.iter().copied().max().unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_type_roundtrip() {
        for t in SearchEntityType::ALL {
            assert_eq!(SearchEntityType::parse(t.as_str()), Some(t));
        }
        assert_eq!(
            SearchEntityType::parse("post"),
            Some(SearchEntityType::Post)
        );
        assert_eq!(
            SearchEntityType::parse("user"),
            Some(SearchEntityType::User)
        );
        assert_eq!(
            SearchEntityType::parse("board"),
            Some(SearchEntityType::Board)
        );
        assert_eq!(SearchEntityType::parse("tag"), Some(SearchEntityType::Tag));
        assert_eq!(SearchEntityType::parse("article"), None);
        assert_eq!(SearchEntityType::parse("POST"), None);
        assert_eq!(SearchEntityType::parse(""), None);
    }

    #[test]
    fn clean_index_text_strips_control_and_collapses_whitespace() {
        assert_eq!(clean_index_text("a\tb\nc", 100), "a b c");
        assert_eq!(clean_index_text("  a   b  ", 100), "a b");
        assert_eq!(clean_index_text("a\u{0000}b", 100), "a b");
        assert_eq!(clean_index_text("\n\r\t", 100), "");
        assert_eq!(clean_index_text("", 100), "");
    }

    #[test]
    fn clean_index_text_truncates_on_char_boundary() {
        // 多字节字符："中文测试文本" 每字 3 字节；截断必须发生在字符边界。
        let s = "中文测试文本".repeat(10);
        let cleaned = clean_index_text(&s, 10);
        assert_eq!(cleaned.chars().count(), 10);
        assert!(s.starts_with(&cleaned));
        assert_eq!(clean_index_text("anything", 0), "");
        assert_eq!(clean_index_text("abc", 2), "ab");
    }

    #[test]
    fn excerpt_short_passthrough_and_long_truncation() {
        assert_eq!(excerpt_from_clean("short", 200), "short");
        assert_eq!(excerpt_from_clean("", 10), "");
        assert_eq!(excerpt_from_clean("abc", 0), "");

        let long = "a".repeat(300);
        let e = excerpt_from_clean(&long, 200);
        assert_eq!(e.chars().count(), 200);
        assert!(e.ends_with('…'));
        assert!(long.starts_with(&e[..e.len() - '…'.len_utf8()]));
    }

    #[test]
    fn excerpt_truncates_multibyte_on_char_boundary() {
        let long = "你好世界".repeat(100); // 400 字符
        let e = excerpt_from_clean(&long, 10);
        assert_eq!(e.chars().count(), 10);
        assert!(long.starts_with(&e[..e.len() - '…'.len_utf8()]));
    }

    #[test]
    fn policy_revision_is_max_and_nondecreasing() {
        assert_eq!(policy_revision_for(&[]), 0);
        assert_eq!(policy_revision_for(&[100, 50, 200]), 200);
        // 加入更小/相等行不改变 max（非递减）。
        assert_eq!(
            policy_revision_for(&[100, 50]),
            policy_revision_for(&[100, 50, 75])
        );
        // 最新输入行 updated_at 更大时 max 严格递增。
        let old = policy_revision_for(&[100, 50]);
        let newer = policy_revision_for(&[100, 50, 120]);
        assert!(newer > old);
        // 陈旧快照（max 更小）在条件 upsert 守卫下被拒绝：stored <= candidate 不成立。
        let stale = policy_revision_for(&[100, 50]);
        let fresh = policy_revision_for(&[100, 50, 120, 80]);
        assert!(stale < fresh);
    }

    #[test]
    fn source_revision_passthrough() {
        assert_eq!(source_revision_for(1234), 1234);
        assert_eq!(source_revision_for(0), 0);
    }

    #[test]
    fn document_new_cleans_and_derives_excerpt() {
        let doc = SearchDocument::new(
            "01911fd5-f000-7000-8000-000000000001".to_string(),
            SearchEntityType::Post,
            "  你好  世界  ".to_string(),
            "正文\t内容\n第二行".to_string(),
            None,
            "my-post".to_string(),
            Some("author-1".to_string()),
            vec!["rust".to_string()],
            100,
            100,
            100,
        )
        .unwrap();
        assert_eq!(doc.title, "你好  世界");
        assert_eq!(doc.body, "正文 内容 第二行");
        assert_eq!(doc.excerpt, "正文 内容 第二行"); // 清洗后 9 字符 ≤ EXCERPT_MAX
        assert_eq!(doc.author_id.as_deref(), Some("author-1"));
        assert_eq!(doc.tags, vec!["rust".to_string()]);
    }

    #[test]
    fn document_new_uses_explicit_excerpt_truncated() {
        let long_body = "长".repeat(500);
        let doc = SearchDocument::new(
            "id-1".to_string(),
            SearchEntityType::Post,
            "标题".to_string(),
            long_body,
            None,
            "post-1".to_string(),
            None,
            vec![],
            1,
            1,
            1,
        )
        .unwrap();
        assert_eq!(doc.excerpt.chars().count(), EXCERPT_MAX);
        assert!(doc.excerpt.ends_with('…'));

        // 显式摘要同样截断。
        let doc2 = SearchDocument::new(
            "id-2".to_string(),
            SearchEntityType::Post,
            "标题".to_string(),
            "正文".to_string(),
            Some("显式摘要".repeat(300)),
            "post-2".to_string(),
            None,
            vec![],
            1,
            1,
            1,
        )
        .unwrap();
        assert_eq!(doc2.excerpt.chars().count(), EXCERPT_MAX);
        assert!(doc2.excerpt.starts_with("显式摘要"));
    }

    #[test]
    fn document_new_rejects_invalid_inputs() {
        let base = |title: &str, body: &str, slug: &str| {
            SearchDocument::new(
                "id".to_string(),
                SearchEntityType::Post,
                title.to_string(),
                body.to_string(),
                None,
                slug.to_string(),
                None,
                vec![],
                1,
                1,
                1,
            )
        };

        assert_eq!(
            base("", "body", "slug"),
            Err(SearchValidationError::TitleEmpty)
        );
        assert_eq!(
            base("   ", "body", "slug"),
            Err(SearchValidationError::TitleEmpty)
        );
        assert_eq!(
            base(&"x".repeat(241), "body", "slug"),
            Err(SearchValidationError::TitleTooLong)
        );
        assert_eq!(
            base("title", "   \n\t ", "slug"),
            Err(SearchValidationError::BodyEmpty)
        );
        assert_eq!(
            base("title", "body", ""),
            Err(SearchValidationError::SlugEmpty)
        );
        assert_eq!(
            base("title", "body", &"x".repeat(121)),
            Err(SearchValidationError::SlugTooLong)
        );
        assert_eq!(
            base("title", "body", "Bad Slug!"),
            Err(SearchValidationError::SlugInvalid)
        );
        assert_eq!(
            base("title", "body", "中文slug"),
            Err(SearchValidationError::SlugInvalid)
        );

        // 120 长度边界与下划线允许（username_normalized 可含 '_'）。
        assert!(base("title", "body", &"x".repeat(120)).is_ok());
        assert!(base("title", "body", "user_name-1").is_ok());
        assert!(base("title", "body", "a-1_b").is_ok());
    }
}
