//! 公开搜索查询与限制（M08-INDEX-06/07）。
//!
//! - 限制：查询长度（≤ [`QUERY_MAX_LEN`]）、语法（token 清洗、无控制字符）、
//!   结果数（limit ≤ [`MAX_LIMIT`]）、分页深度（cursor 内编码，≤
//!   [`MAX_PAGE_DEPTH`]）、匿名频率（路由层限流）与高亮长度
//!   （[`crate::search::publication::HIGHLIGHT_MAX_LEN`]）；
//! - 执行：只查 `search_documents`（内部索引行）——FTS5（SQLite）/ FULLTEXT
//!   BOOLEAN（MySQL/MariaDB）命中后返回**公开投影**（不投影 `body`）；
//! - **返回前实时重检（M08-INDEX-07）**：[`recheck_doc_visibility`] 对每条
//!   候选重新执行实时可见性/处罚/索引退出判断——索引只是候选集，不是授权裁决。

use sqlx::Either;
use sqlx::Row;

use crate::db::DatabasePool;
use crate::search::gate::{
    decide_board_indexability, decide_public_post_indexability, decide_tag_indexability,
    decide_user_indexability, IndexDecision, PostPublicIndexInput,
};
use crate::search::policy::{load_board_policy, load_site_policy, AdminIndexPolicy, POLICY_DENY};
use crate::search::publication::{
    highlight_snippet, PublicAuthor, PublicIndexProjection, HIGHLIGHT_MAX_LEN,
};
use crate::search::{SearchEntityType, SLUG_MAX};

/// 查询最大长度（字符；OpenAPI `q` maxLength 200 一致）。
pub const QUERY_MAX_LEN: usize = 200;
/// 默认每页结果数。
pub const DEFAULT_LIMIT: i64 = 20;
/// 每页结果数上限。
pub const MAX_LIMIT: i64 = 50;
/// 分页深度上限（cursor 内编码页码，M08-INDEX-06）。
pub const MAX_PAGE_DEPTH: usize = 10;
/// 匿名搜索限流：30 次 / 分钟（独立桶，CRAWLER-POLICY §5）。
pub const ANON_SEARCH_LIMIT: u32 = 30;
/// 登录用户搜索限流：120 次 / 分钟。
pub const LOGGED_IN_SEARCH_LIMIT: u32 = 120;
/// 限流窗口（毫秒）。
pub const SEARCH_RATE_WINDOW_MS: i64 = 60_000;

/// 查询校验错误（M08-INDEX-06）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchQueryError {
    Empty,
    TooLong,
    InvalidSyntax,
    InvalidCursor,
    DepthExceeded,
}

impl std::fmt::Display for SearchQueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Empty => write!(f, "search query must not be empty"),
            Self::TooLong => write!(f, "search query exceeds {QUERY_MAX_LEN} characters"),
            Self::InvalidSyntax => write!(f, "search query contains invalid characters"),
            Self::InvalidCursor => write!(f, "invalid pagination cursor"),
            Self::DepthExceeded => write!(
                f,
                "search pagination depth exceeds the maximum of {MAX_PAGE_DEPTH} pages"
            ),
        }
    }
}

/// 解析后的搜索请求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchRequest {
    pub q: String,
    /// 清洗后的查询 token（高亮与 FTS 查询共用）。
    pub tokens: Vec<String>,
    pub limit: i64,
    /// 游标页码（1-based 下一页序号；首页 = 1）。
    pub depth: usize,
    /// keyset：上一页最后一条 `(indexed_at, doc_id)`。
    pub after: Option<(i64, String)>,
}

impl SearchRequest {
    /// 校验并解析 `q/limit/after`。
    pub fn parse(q: &str, limit: i64, after: Option<&str>) -> Result<Self, SearchQueryError> {
        let trimmed = q.trim();
        if trimmed.is_empty() {
            return Err(SearchQueryError::Empty);
        }
        if trimmed.chars().count() > QUERY_MAX_LEN {
            return Err(SearchQueryError::TooLong);
        }
        if trimmed.chars().any(|c| c.is_control()) {
            return Err(SearchQueryError::InvalidSyntax);
        }
        let tokens: Vec<String> = trimmed
            .split_whitespace()
            .map(sanitize_token)
            .filter(|t| !t.is_empty())
            .collect();
        if tokens.is_empty() {
            return Err(SearchQueryError::InvalidSyntax);
        }
        let limit = limit.clamp(1, MAX_LIMIT);
        let (depth, after) = match after {
            None | Some("") => (1usize, None),
            Some(raw) => {
                let (depth, rev, id) = decode_cursor(raw)?;
                (depth, Some((rev, id)))
            }
        };
        if depth > MAX_PAGE_DEPTH {
            return Err(SearchQueryError::DepthExceeded);
        }
        Ok(SearchRequest {
            q: trimmed.to_string(),
            tokens,
            limit,
            depth,
            after,
        })
    }
}

/// 清洗查询 token：仅保留 Unicode 字母/数字（含 CJK）与 `_`/`-`；
/// 丢弃 FTS 特殊字符与标点（`"`/`+`/`*`/`(` 等不进入查询，语法限制）。
fn sanitize_token(raw: &str) -> String {
    raw.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '-')
        .collect()
}

/// 构建 SQLite FTS5 MATCH 表达式：`"tok1" AND "tok2"`（引号内字面匹配，
/// 引号自身加倍转义——语法安全，M08-INDEX-06）。
pub fn build_fts_query(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// 构建 MySQL/MariaDB BOOLEAN MODE 查询：`+"tok1" +"tok2"`（token 已清洗）。
pub fn build_mysql_boolean_query(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|t| format!("+\"{}\"", t.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// 游标：`base64url("{depth}|{indexed_at}|{doc_id}")`。
pub fn encode_cursor(depth: usize, indexed_at: i64, doc_id: &str) -> String {
    use base64::Engine;
    let raw = format!("{depth}|{indexed_at}|{doc_id}");
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(raw.as_bytes())
}

/// 解码游标：`(depth, indexed_at, doc_id)`。
pub fn decode_cursor(raw: &str) -> Result<(usize, i64, String), SearchQueryError> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(raw)
        .map_err(|_| SearchQueryError::InvalidCursor)?;
    let text = String::from_utf8(bytes).map_err(|_| SearchQueryError::InvalidCursor)?;
    let mut parts = text.splitn(3, '|');
    let depth = parts
        .next()
        .and_then(|d| d.parse::<usize>().ok())
        .ok_or(SearchQueryError::InvalidCursor)?;
    let indexed_at = parts
        .next()
        .and_then(|r| r.parse::<i64>().ok())
        .ok_or(SearchQueryError::InvalidCursor)?;
    let doc_id = parts.next().ok_or(SearchQueryError::InvalidCursor)?;
    if doc_id.is_empty() {
        return Err(SearchQueryError::InvalidCursor);
    }
    Ok((depth, indexed_at, doc_id.to_string()))
}

/// 内部索引行投影（查询结果的原始字段；`body` 只用于高亮，绝不对外）。
#[derive(Debug, Clone)]
pub struct IndexedDoc {
    pub doc_id: String,
    pub entity_type: String,
    pub title: String,
    pub body: String,
    pub excerpt: String,
    pub slug: String,
    pub author_id: Option<String>,
    pub tags_json: String,
    pub source_revision: i64,
    pub policy_revision: i64,
    pub indexed_at: i64,
}

fn row_to_doc(row: &sqlx::sqlite::SqliteRow) -> IndexedDoc {
    IndexedDoc {
        doc_id: row.get("doc_id"),
        entity_type: row.get("entity_type"),
        title: row.get("title"),
        body: row.get("body"),
        excerpt: row.get("excerpt"),
        slug: row.get("slug"),
        author_id: row.get("author_id"),
        tags_json: row.get("tags_json"),
        source_revision: row.get("source_revision"),
        policy_revision: row.get("policy_revision"),
        indexed_at: row.get("indexed_at"),
    }
}

fn row_to_doc_mysql(row: &sqlx::mysql::MySqlRow) -> IndexedDoc {
    IndexedDoc {
        doc_id: row.get("doc_id"),
        entity_type: row.get("entity_type"),
        title: row.get("title"),
        body: row.get("body"),
        excerpt: row.get("excerpt"),
        slug: row.get("slug"),
        author_id: row.get("author_id"),
        tags_json: row.get("tags_json"),
        source_revision: row.get("source_revision"),
        policy_revision: row.get("policy_revision"),
        indexed_at: row.get("indexed_at"),
    }
}

/// 搜索结果页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchPage {
    pub items: Vec<PublicIndexProjection>,
    pub highlights: Vec<Option<String>>,
    pub next_cursor: Option<String>,
    pub has_more: bool,
}

/// 执行公开搜索（M08-INDEX-06/07）：
/// FTS 命中候选 → 逐条实时重检可见性/处罚/退出 → 公开投影。
#[allow(clippy::too_many_arguments)]
pub async fn execute_public_search(
    pool: &DatabasePool,
    req: &SearchRequest,
    now: i64,
) -> Result<SearchPage, String> {
    let fts_query = build_fts_query(&req.tokens);
    let mysql_query = build_mysql_boolean_query(&req.tokens);
    let fetch_limit = req.limit + 1;
    let (after_at, after_id) = req.after.clone().unwrap_or((i64::MAX, String::new()));

    let mut rows: Vec<IndexedDoc> = match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(
                "SELECT sd.doc_id, sd.entity_type, sd.title, sd.body, sd.excerpt, sd.slug,
                        sd.author_id, sd.tags_json, sd.source_revision, sd.policy_revision, sd.indexed_at
                 FROM search_documents sd
                 WHERE sd.rowid IN (SELECT rowid FROM search_fts WHERE search_fts MATCH ?)
                   AND (sd.indexed_at < ? OR (sd.indexed_at = ? AND sd.doc_id < ?))
                 ORDER BY sd.indexed_at DESC, sd.doc_id DESC
                 LIMIT ?",
            );
            q = q
                .bind(&fts_query)
                .bind(after_at)
                .bind(after_at)
                .bind(&after_id)
                .bind(fetch_limit);
            let rows = q.fetch_all(p).await.map_err(|e| e.to_string())?;
            rows.iter().map(row_to_doc).collect()
        }
        Either::Right(p) => {
            let mut q = sqlx::query(
                "SELECT sd.doc_id, sd.entity_type, sd.title, sd.body, sd.excerpt, sd.slug,
                        sd.author_id, sd.tags_json, sd.source_revision, sd.policy_revision, sd.indexed_at
                 FROM search_documents sd
                 WHERE MATCH(sd.title, sd.body) AGAINST (? IN BOOLEAN MODE)
                   AND (sd.indexed_at < ? OR (sd.indexed_at = ? AND sd.doc_id < ?))
                 ORDER BY sd.indexed_at DESC, sd.doc_id DESC
                 LIMIT ?",
            );
            q = q
                .bind(&mysql_query)
                .bind(after_at)
                .bind(after_at)
                .bind(&after_id)
                .bind(fetch_limit);
            let rows = q.fetch_all(p).await.map_err(|e| e.to_string())?;
            rows.iter().map(row_to_doc_mysql).collect()
        }
    };

    let has_more = rows.len() as i64 > req.limit;
    rows.truncate(req.limit as usize);

    let mut items = Vec::with_capacity(rows.len());
    let mut highlights = Vec::with_capacity(rows.len());
    for doc in rows {
        // M08-INDEX-07：返回前实时重检（候选集不是授权裁决）。
        let entity_type = SearchEntityType::parse(&doc.entity_type)
            .ok_or_else(|| format!("unknown indexed entity_type {}", doc.entity_type))?;
        if !recheck_doc_visibility(pool, entity_type, &doc.doc_id, now).await? {
            continue;
        }
        let tags = serde_json::from_str::<Vec<String>>(&doc.tags_json).unwrap_or_default();
        let author = match (entity_type, doc.author_id.as_deref()) {
            (SearchEntityType::Post, Some(author_id)) => {
                load_public_author(pool, author_id).await?
            }
            _ => None,
        };
        let slug = if doc.slug.chars().count() > SLUG_MAX {
            &doc.slug[..SLUG_MAX]
        } else {
            &doc.slug
        };
        let highlight = if entity_type == SearchEntityType::Post {
            Some(highlight_snippet(&doc.body, &req.tokens, HIGHLIGHT_MAX_LEN))
        } else {
            None
        };
        items.push(PublicIndexProjection::new(
            doc.doc_id,
            entity_type,
            doc.title,
            slug.to_string(),
            doc.excerpt,
            tags,
            author,
            doc.source_revision,
            doc.policy_revision,
            doc.indexed_at,
        ));
        highlights.push(highlight);
    }

    let last = items.last().map(|p| p.indexed_at);
    let last_id = items.last().map(|p| p.id.as_str());
    let next_cursor = if has_more {
        match (last, last_id) {
            (Some(at), Some(id)) => Some(encode_cursor(req.depth + 1, at, id)),
            _ => None,
        }
    } else {
        None
    };
    let more = has_more && !items.is_empty();

    Ok(SearchPage {
        items,
        highlights,
        next_cursor,
        has_more: more,
    })
}

/// 读取作者公开投影（users 行缺失 → None；display_name 可空）。
async fn load_public_author(
    pool: &DatabasePool,
    author_id: &str,
) -> Result<Option<PublicAuthor>, String> {
    let row: Option<(String, Option<String>)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT username_normalized, display_name FROM users WHERE id = ?")
                .bind(author_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT username_normalized, display_name FROM users WHERE id = ?")
                .bind(author_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?
        }
    };
    Ok(row.map(|(username, display_name)| PublicAuthor {
        id: author_id.to_string(),
        username,
        display_name,
    }))
}

/// M08-INDEX-07：结果返回前的实时重检——重新执行实时可见性、处罚
/// （sanctions 封禁）与索引退出判断（作者 opt-out / 管理员策略实时读取）。
pub async fn recheck_doc_visibility(
    pool: &DatabasePool,
    entity_type: SearchEntityType,
    doc_id: &str,
    now: i64,
) -> Result<bool, String> {
    match entity_type {
        SearchEntityType::Post => recheck_post(pool, doc_id, now).await,
        SearchEntityType::User => recheck_user(pool, doc_id, now).await,
        SearchEntityType::Board => recheck_board(pool, doc_id).await,
        SearchEntityType::Tag => recheck_tag(pool, doc_id).await,
    }
}

#[derive(sqlx::FromRow)]
struct PostRecheckRow {
    status: String,
    visibility: String,
    board_id: String,
    author_id: String,
    deleted_at: Option<i64>,
    review_status: Option<String>,
    search_index_opt_out: i64,
    board_active: i64,
    board_visibility: String,
    author_status: String,
    author_deleted_at: Option<i64>,
    policy_kind: Option<String>,
}

async fn recheck_post(pool: &DatabasePool, post_id: &str, now: i64) -> Result<bool, String> {
    let row: Option<PostRecheckRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, PostRecheckRow>(
            "SELECT p.status, p.visibility, p.board_id, p.author_id, p.deleted_at,
                    p.review_status, p.search_index_opt_out,
                    COALESCE(b.is_active, 0) AS board_active,
                    COALESCE(b.visibility, 'hidden') AS board_visibility,
                    COALESCE(u.status, 'deleted') AS author_status,
                    u.deleted_at AS author_deleted_at,
                    pol.kind AS policy_kind
             FROM posts p
             LEFT JOIN boards b ON b.id = p.board_id
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             WHERE p.id = ?",
        )
        .bind(post_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, PostRecheckRow>(
            "SELECT p.status, p.visibility, p.board_id, p.author_id, p.deleted_at,
                    p.review_status, p.search_index_opt_out,
                    COALESCE(b.is_active, 0) AS board_active,
                    COALESCE(b.visibility, 'hidden') AS board_visibility,
                    COALESCE(u.status, 'deleted') AS author_status,
                    u.deleted_at AS author_deleted_at,
                    pol.kind AS policy_kind
             FROM posts p
             LEFT JOIN boards b ON b.id = p.board_id
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             WHERE p.id = ?",
        )
        .bind(post_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    let Some(row) = row else {
        return Ok(false);
    };

    // 实时处罚：作者存在生效封禁（含预约窗口）→ 排除。
    let sanctions =
        crate::moderation::sanctions::service::effective_sanctions(pool, &row.author_id, None, now)
            .await
            .map_err(|e| e.to_string())?;
    let author_banned = sanctions
        .iter()
        .any(|s| s.kind == crate::moderation::model::SanctionKind::Ban);

    let admin = effective_admin_for_board(pool, &row.board_id).await?;
    let decision = decide_public_post_indexability(&PostPublicIndexInput {
        status: &row.status,
        visibility: &row.visibility,
        policy_kind: row.policy_kind.as_deref(),
        board_active: row.board_active != 0,
        board_visibility: &row.board_visibility,
        author_status: if author_banned {
            "banned"
        } else {
            &row.author_status
        },
        author_deleted_at: row.author_deleted_at,
        deleted_at: row.deleted_at,
        review_status: row.review_status.as_deref(),
        search_index_opt_out: row.search_index_opt_out != 0,
        admin_search_index: &admin.search_index,
    });
    Ok(matches!(decision, IndexDecision::Indexable))
}

async fn recheck_user(pool: &DatabasePool, user_id: &str, now: i64) -> Result<bool, String> {
    let row: Option<(String, Option<i64>)> = match pool {
        Either::Left(p) => sqlx::query_as("SELECT status, deleted_at FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as("SELECT status, deleted_at FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?,
    };
    let Some((status, deleted_at)) = row else {
        return Ok(false);
    };
    let sanctions =
        crate::moderation::sanctions::service::effective_sanctions(pool, user_id, None, now)
            .await
            .map_err(|e| e.to_string())?;
    let banned = sanctions
        .iter()
        .any(|s| s.kind == crate::moderation::model::SanctionKind::Ban);
    let status = if banned { "banned" } else { &status };
    Ok(matches!(
        decide_user_indexability(status, deleted_at),
        IndexDecision::Indexable
    ))
}

async fn recheck_board(pool: &DatabasePool, board_id: &str) -> Result<bool, String> {
    let row: Option<(i64, String)> = match pool {
        Either::Left(p) => sqlx::query_as("SELECT is_active, visibility FROM boards WHERE id = ?")
            .bind(board_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as("SELECT is_active, visibility FROM boards WHERE id = ?")
            .bind(board_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?,
    };
    let Some((is_active, visibility)) = row else {
        return Ok(false);
    };
    Ok(matches!(
        decide_board_indexability(is_active != 0, &visibility),
        IndexDecision::Indexable
    ))
}

async fn recheck_tag(pool: &DatabasePool, tag_id: &str) -> Result<bool, String> {
    let row: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT is_active FROM tags WHERE id = ?")
            .bind(tag_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_scalar("SELECT is_active FROM tags WHERE id = ?")
            .bind(tag_id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?,
    };
    Ok(matches!(
        row.map(|active| decide_tag_indexability(active != 0)),
        Some(IndexDecision::Indexable)
    ))
}

/// 读取板块相关管理员索引策略（site ∪ board，deny 优先）。
pub async fn effective_admin_for_board(
    pool: &DatabasePool,
    board_id: &str,
) -> Result<AdminIndexPolicy, String> {
    let site = load_site_policy(pool).await?;
    let board = load_board_policy(pool, board_id).await?;
    Ok(AdminIndexPolicy::effective(&site, &board))
}

/// 管理员 AI 摘要策略（site ∪ board 并集；供 SEO/AI 投影消费）。
pub async fn effective_ai_summary_denied(
    pool: &DatabasePool,
    board_id: &str,
) -> Result<bool, String> {
    let admin = effective_admin_for_board(pool, board_id).await?;
    Ok(admin.ai_summary == POLICY_DENY)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_token_keeps_letters_digits_cjk_and_dash() {
        assert_eq!(sanitize_token("rust"), "rust");
        assert_eq!(sanitize_token("系统编程"), "系统编程");
        assert_eq!(sanitize_token("c++"), "c");
        assert_eq!(sanitize_token("\"quoted\""), "quoted");
        assert_eq!(sanitize_token("a*b"), "ab");
        assert_eq!(sanitize_token("--"), "--");
    }

    #[test]
    fn parse_accepts_valid_queries() {
        let req = SearchRequest::parse(" rust  系统 ", 50, None).unwrap();
        assert_eq!(req.tokens, vec!["rust".to_string(), "系统".to_string()]);
        assert_eq!(req.limit, 50);
        assert_eq!(req.depth, 1);
        assert_eq!(req.after, None);
        // limit 钳制
        assert_eq!(
            SearchRequest::parse("a", 999, None).unwrap().limit,
            MAX_LIMIT
        );
        assert_eq!(SearchRequest::parse("a", 0, None).unwrap().limit, 1);
    }

    #[test]
    fn parse_rejects_invalid_queries() {
        assert_eq!(
            SearchRequest::parse("   ", 20, None).unwrap_err(),
            SearchQueryError::Empty
        );
        assert_eq!(
            SearchRequest::parse(&"x".repeat(201), 20, None).unwrap_err(),
            SearchQueryError::TooLong
        );
        assert_eq!(
            SearchRequest::parse("+++***", 20, None).unwrap_err(),
            SearchQueryError::InvalidSyntax
        );
        assert_eq!(
            SearchRequest::parse("ok\u{0000}", 20, None).unwrap_err(),
            SearchQueryError::InvalidSyntax
        );
        assert_eq!(
            SearchRequest::parse("ok", 20, Some("not-base64!!")).unwrap_err(),
            SearchQueryError::InvalidCursor
        );
    }

    #[test]
    fn cursor_roundtrip_and_depth_limit() {
        let cursor = encode_cursor(2, 12345, "doc-1");
        assert_eq!(
            decode_cursor(&cursor).unwrap(),
            (2, 12345, "doc-1".to_string())
        );
        assert_eq!(
            decode_cursor("").unwrap_err(),
            SearchQueryError::InvalidCursor
        );
        assert_eq!(
            decode_cursor("YQ==").unwrap_err(), // "a"
            SearchQueryError::InvalidCursor
        );

        // 深度超过上限 → 拒绝。
        let deep = encode_cursor(11, 1, "doc");
        assert_eq!(
            SearchRequest::parse("ok", 20, Some(&deep)).unwrap_err(),
            SearchQueryError::DepthExceeded
        );
    }

    #[test]
    fn fts_query_builds_quoted_and_escaped() {
        assert_eq!(
            build_fts_query(&["rust".to_string(), "系统".to_string()]),
            "\"rust\" AND \"系统\""
        );
        assert_eq!(build_fts_query(&["a\"b".to_string()]), "\"a\"\"b\"");
    }

    #[test]
    fn mysql_boolean_query_prefixes_plus() {
        assert_eq!(
            build_mysql_boolean_query(&["rust".to_string(), "sqlite".to_string()]),
            "+\"rust\" +\"sqlite\""
        );
    }
}
