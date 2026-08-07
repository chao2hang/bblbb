//! M04-VISIBILITY-07/09：可复用投影过滤器。
//!
//! 所有内容可读路径（list/detail/notifications/Feed/SEO/AI/attachments）必须
//! 经 [`ProjectionFilter`] / [`project_post`] / [`project_comment`] 输出：
//!
//! - 未解锁（`!grant.unlocked`）：`body_html`、`excerpt`、附件列表、搜索高亮、
//!   受限 HTML 等**任何可逆编码内容**的键**完全缺失**（不置 null）；
//! - 解锁：包含正文/摘要/附件/高亮/受限块；
//! - `access_summary`（OpenAPI `AccessSummary`）与 `capabilities` 恒存在。
//!
//! 本模块是唯一允许“全文 DTO → 响应 DTO”的过滤层，防泄漏在此收口。

use serde::Serialize;
use serde_json::{json, Map, Value};

use super::evaluate::AccessGrant;

/// 访问摘要（OpenAPI `AccessSummary`；恒随投影返回）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccessSummary {
    pub policy: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub required_level: Option<u32>,
    pub unlocked: bool,
}

impl AccessSummary {
    pub fn from_grant(grant: &AccessGrant) -> Self {
        Self {
            policy: grant.policy.as_str(),
            required_level: grant.required_level,
            unlocked: grant.unlocked,
        }
    }
}

/// 附件引用（公开元数据；未解锁时整个 `attachments` 键缺失）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AttachmentRef {
    pub attachment_id: String,
    pub kind: String,
    pub position: i64,
}

/// 帖子全字段（含敏感正文；过滤在此进行）。
#[derive(Debug, Clone)]
pub struct PostFields {
    // ── 公开元数据 ──
    pub id: String,
    pub title: String,
    pub author_id: String,
    pub author_username: Option<String>,
    pub author_display_name: Option<String>,
    pub author_level: i64,
    pub post_type: String,
    pub status: String,
    pub board_id: String,
    pub slug: Option<String>,
    pub reply_count: i64,
    pub view_count: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub pinned_at: Option<i64>,
    pub scheduled_at: Option<i64>,
    pub published_at: Option<i64>,
    pub last_reply_at: Option<i64>,
    pub closed_at: Option<i64>,
    // ── 敏感字段（仅解锁时输出）──
    pub body_html: Option<String>,
    pub excerpt: Option<String>,
    pub attachments: Vec<AttachmentRef>,
    pub search_highlight: Option<String>,
    pub restricted_html: Option<String>,
}

/// 评论全字段（含敏感正文；过滤在此进行）。
#[derive(Debug, Clone)]
pub struct CommentFields {
    pub id: String,
    pub post_id: String,
    pub author_id: String,
    pub author_username: Option<String>,
    pub floor: i64,
    pub created_at: i64,
    pub updated_at: i64,
    // ── 敏感字段（仅解锁时输出）──
    pub content_html: Option<String>,
    pub content_markdown: Option<String>,
    pub search_highlight: Option<String>,
}

/// 帖子投影：`author_level` 为内容作者当前等级（服务端权威，供调用方
/// 审计/上报；不随 JSON 输出，避免扩展 OpenAPI 契约）。
pub fn project_post(fields: PostFields, grant: AccessGrant, _author_level: u32) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), Value::String(fields.id));
    map.insert("board_id".into(), Value::String(fields.board_id));
    map.insert("post_type".into(), Value::String(fields.post_type));
    map.insert("title".into(), Value::String(fields.title));
    map.insert("status".into(), Value::String(fields.status));
    if let Some(slug) = fields.slug {
        map.insert("slug".into(), Value::String(slug));
    }
    let author_username = fields.author_username.clone().unwrap_or_default();
    map.insert(
        "author".into(),
        json!({
            "id": fields.author_id,
            "username": author_username,
            "display_name": fields.author_display_name,
            "level": fields.author_level,
            "profile_url": format!("/users/{author_username}"),
        }),
    );
    map.insert("reply_count".into(), json!(fields.reply_count));
    map.insert("view_count".into(), json!(fields.view_count));
    map.insert("created_at".into(), json!(fields.created_at));
    map.insert("updated_at".into(), json!(fields.updated_at));
    insert_opt(&mut map, "pinned_at", fields.pinned_at);
    insert_opt(&mut map, "scheduled_at", fields.scheduled_at);
    insert_opt(&mut map, "published_at", fields.published_at);
    insert_opt(&mut map, "last_reply_at", fields.last_reply_at);
    insert_opt(&mut map, "closed_at", fields.closed_at);

    if grant.unlocked {
        if let Some(body) = fields.body_html {
            map.insert("body_html".into(), Value::String(body));
        }
        if let Some(excerpt) = fields.excerpt {
            map.insert("excerpt".into(), Value::String(excerpt));
        }
        if !fields.attachments.is_empty() {
            map.insert(
                "attachments".into(),
                serde_json::to_value(fields.attachments).unwrap_or(Value::Array(Vec::new())),
            );
        }
        if let Some(hl) = fields.search_highlight {
            map.insert("search_highlight".into(), Value::String(hl));
        }
        if let Some(restricted) = fields.restricted_html {
            map.insert("restricted_html".into(), Value::String(restricted));
        }
    }

    map.insert(
        "access_summary".into(),
        serde_json::to_value(AccessSummary::from_grant(&grant)).unwrap_or(Value::Null),
    );
    map.insert("capabilities".into(), json!(grant.capabilities));
    Value::Object(map)
}

/// 插入可空字段（None → 键完全缺失，不置 null，符合 API-CONTRACTS.md）。
fn insert_opt(map: &mut Map<String, Value>, key: &str, value: Option<i64>) {
    if let Some(v) = value {
        map.insert(key.into(), json!(v));
    }
}

/// 评论投影：未解锁时正文/高亮键完全缺失。
pub fn project_comment(fields: CommentFields, grant: AccessGrant) -> Value {
    let mut map = Map::new();
    map.insert("id".into(), Value::String(fields.id));
    map.insert("post_id".into(), Value::String(fields.post_id));
    map.insert(
        "author".into(),
        json!({ "id": fields.author_id, "username": fields.author_username }),
    );
    map.insert("floor".into(), json!(fields.floor));
    map.insert("created_at".into(), json!(fields.created_at));
    map.insert("updated_at".into(), json!(fields.updated_at));

    if grant.unlocked {
        if let Some(html) = fields.content_html {
            map.insert("content_html".into(), Value::String(html));
        }
        if let Some(md) = fields.content_markdown {
            map.insert("content_markdown".into(), Value::String(md));
        }
        if let Some(hl) = fields.search_highlight {
            map.insert("search_highlight".into(), Value::String(hl));
        }
    }

    map.insert(
        "access_summary".into(),
        serde_json::to_value(AccessSummary::from_grant(&grant)).unwrap_or(Value::Null),
    );
    map.insert("capabilities".into(), json!(grant.capabilities));
    Value::Object(map)
}

/// 可复用投影过滤器：构造一次 grant，批量过滤多个资源（帖子/评论）。
#[derive(Debug, Clone, Copy)]
pub struct ProjectionFilter {
    pub grant: AccessGrant,
}

impl ProjectionFilter {
    pub fn new(grant: AccessGrant) -> Self {
        Self { grant }
    }

    pub fn unlocked(&self) -> bool {
        self.grant.unlocked
    }

    pub fn access_summary(&self) -> AccessSummary {
        AccessSummary::from_grant(&self.grant)
    }

    pub fn project_post(&self, fields: PostFields, author_level: u32) -> Value {
        project_post(fields, self.grant, author_level)
    }

    pub fn project_comment(&self, fields: CommentFields) -> Value {
        project_comment(fields, self.grant)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::visibility::evaluate::{AccessGrant, CAP_UNLOCK_AFTER_REPLY};
    use crate::domain::posts::AccessPolicy;

    const CANARY: &str = "CANARY-SECRET-BODY-MARKER";

    fn grant(unlocked: bool) -> AccessGrant {
        AccessGrant {
            unlocked,
            policy: AccessPolicy::AfterReply,
            reason: "after_reply",
            required_level: None,
            capabilities: if unlocked {
                &[]
            } else {
                CAP_UNLOCK_AFTER_REPLY
            },
        }
    }

    fn post_fields(canary_body: bool) -> PostFields {
        PostFields {
            id: "p1".into(),
            title: "标题".into(),
            author_id: "u1".into(),
            author_username: Some("alice".into()),
            author_display_name: Some("爱丽丝".into()),
            author_level: 5,
            post_type: "discussion".into(),
            status: "published".into(),
            board_id: "b1".into(),
            slug: Some("p1-slug".into()),
            reply_count: 2,
            view_count: 10,
            created_at: 1,
            updated_at: 2,
            pinned_at: None,
            scheduled_at: None,
            published_at: Some(1),
            last_reply_at: None,
            closed_at: None,
            body_html: if canary_body {
                Some(format!("<p>{CANARY}</p>"))
            } else {
                Some("<p>hello</p>".into())
            },
            excerpt: Some("excerpt".into()),
            attachments: vec![AttachmentRef {
                attachment_id: "a1".into(),
                kind: "cover".into(),
                position: 1,
            }],
            search_highlight: Some("<mark>hl</mark>".into()),
            restricted_html: Some("<div>restricted</div>".into()),
        }
    }

    fn comment_fields() -> CommentFields {
        CommentFields {
            id: "c1".into(),
            post_id: "p1".into(),
            author_id: "u2".into(),
            author_username: Some("bob".into()),
            floor: 1,
            created_at: 3,
            updated_at: 4,
            content_html: Some("<p>comment</p>".into()),
            content_markdown: Some("comment".into()),
            search_highlight: Some("comment".into()),
        }
    }

    /// M04-VISIBILITY-07：未解锁 → 敏感键完全缺失（get 返回 None，非 null）。
    #[test]
    fn locked_post_omits_sensitive_keys_entirely() {
        let value = project_post(post_fields(true), grant(false), 5);
        for key in [
            "body_html",
            "excerpt",
            "attachments",
            "search_highlight",
            "restricted_html",
        ] {
            assert_eq!(
                value.get(key),
                None,
                "未解锁时 {key} 键必须完全缺失（不置 null），实际 {value:?}"
            );
        }
        // 公开元数据仍在
        assert_eq!(value["id"], "p1");
        assert_eq!(value["title"], "标题");
        assert_eq!(value["author"]["username"], "alice");
        // access_summary 恒存在
        assert_eq!(value["access_summary"]["policy"], "after_reply");
        assert_eq!(value["access_summary"]["unlocked"], false);
        assert_eq!(value["capabilities"], json!(["unlock_after_reply"]));
        // 序列化字符串不得包含任何正文痕迹
        let s = value.to_string();
        assert!(!s.contains(CANARY), "正文 canary 泄漏: {s}");
        assert!(!s.contains("hello"));
    }

    /// M04-VISIBILITY-07：解锁 → 敏感键齐全。
    #[test]
    fn unlocked_post_includes_sensitive_keys() {
        let value = project_post(post_fields(false), grant(true), 5);
        for key in [
            "body_html",
            "excerpt",
            "attachments",
            "search_highlight",
            "restricted_html",
        ] {
            assert!(value.get(key).is_some(), "解锁时 {key} 必须存在: {value:?}");
        }
        assert_eq!(value["body_html"], "<p>hello</p>");
        assert_eq!(value["access_summary"]["unlocked"], true);
        assert_eq!(value["capabilities"], json!([]));
    }

    /// M04-VISIBILITY-07：评论未解锁 → 正文/高亮键完全缺失。
    #[test]
    fn locked_comment_omits_sensitive_keys_entirely() {
        let value = project_comment(comment_fields(), grant(false));
        for key in ["content_html", "content_markdown", "search_highlight"] {
            assert_eq!(value.get(key), None, "未解锁时 {key} 必须缺失");
        }
        assert_eq!(value["floor"], 1);
        assert_eq!(value["access_summary"]["policy"], "after_reply");
        let s = value.to_string();
        assert!(!s.contains("comment"), "评论正文泄漏: {s}");
    }

    #[test]
    fn unlocked_comment_includes_content() {
        let value = project_comment(comment_fields(), grant(true));
        assert_eq!(value["content_html"], "<p>comment</p>");
        assert_eq!(value["content_markdown"], "comment");
        assert_eq!(value["search_highlight"], "comment");
        assert_eq!(value["access_summary"]["unlocked"], true);
    }

    /// M04-VISIBILITY-09：批量混合策略 —— 每个 item 按自己的 grant 投影。
    #[test]
    fn batch_projection_applies_each_grants_own_policy() {
        let locked = ProjectionFilter::new(grant(false));
        let unlocked = ProjectionFilter::new(grant(true));

        let items = [
            locked.project_post(post_fields(true), 5), // 锁定帖（canary 正文）
            unlocked.project_post(post_fields(false), 5), // 解锁帖
            locked.project_comment(comment_fields()),  // 锁定评论
        ];

        // item 0（锁定）：canary 不可见，键缺失
        let s0 = items[0].to_string();
        assert!(!s0.contains(CANARY), "锁定帖 canary 泄漏: {s0}");
        assert!(items[0].get("body_html").is_none());
        assert_eq!(items[0]["access_summary"]["unlocked"], false);

        // item 1（解锁）：正文可见
        assert!(items[1].get("body_html").is_some());
        assert_eq!(items[1]["access_summary"]["unlocked"], true);

        // item 2（锁定评论）：正文不可见
        assert!(items[2].get("content_html").is_none());
        assert_eq!(items[2]["access_summary"]["unlocked"], false);
    }

    /// M04-VISIBILITY-09：excerpt 只在解锁时出现。
    #[test]
    fn excerpt_present_only_when_unlocked() {
        let locked = project_post(post_fields(false), grant(false), 5);
        assert_eq!(locked.get("excerpt"), None, "锁定帖不得暴露 excerpt");
        let unlocked = project_post(post_fields(false), grant(true), 5);
        assert_eq!(unlocked["excerpt"], "excerpt");
    }

    /// 混合批次的 canary 全局断言：任一锁定投影输出不得含 canary。
    #[test]
    fn hidden_canary_never_appears_in_any_projection() {
        let locked = ProjectionFilter::new(grant(false));
        let outputs = vec![
            locked.project_post(post_fields(true), 5),
            locked.project_comment(comment_fields()),
        ];
        for out in outputs {
            assert!(
                !out.to_string().contains(CANARY),
                "锁定投影泄漏 canary: {out}"
            );
        }
    }

    #[test]
    fn access_summary_from_grant_keeps_required_level() {
        let g = AccessGrant {
            unlocked: false,
            policy: AccessPolicy::Level,
            reason: "level",
            required_level: Some(4),
            capabilities: &["request_access"],
        };
        let summary = AccessSummary::from_grant(&g);
        assert_eq!(summary.policy, "level");
        assert_eq!(summary.required_level, Some(4));
        assert!(!summary.unlocked);
        let value = serde_json::to_value(&summary).unwrap();
        assert_eq!(value["required_level"], 4);
        // 非 level 策略 required_level 省略（非 null）
        let g2 = AccessGrant {
            policy: AccessPolicy::Public,
            unlocked: true,
            reason: "public",
            required_level: None,
            capabilities: &[],
        };
        let value2 = serde_json::to_value(AccessSummary::from_grant(&g2)).unwrap();
        assert_eq!(
            value2.get("required_level"),
            None,
            "required_level 不适用时必须缺失"
        );
    }
}
