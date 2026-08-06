//! M04-SCHEMA-01：帖子元数据模型。
//!
//! - [`PostType`]：article/discussion（稳定枚举，与迁移 0032 CHECK 一致）；
//! - [`PostStatus`]：发布状态（docs/STATE-MACHINES.md §Post）。`Locked` 为
//!   0003 骨架遗留值（新代码用 `closed_at`，SCHEMA.md）；`pending_review`/
//!   `rejected` 随 M04-POSTS 迁移扩展 DB CHECK（当前 DB CHECK 保持 0003 值域，
//!   见迁移 0032 注释）；
//! - [`Post`]：元数据聚合（不含正文——正文经 post_contents 与修订随
//!   M04-SCHEMA-02 落地）。

use std::fmt;

/// 帖子类型（迁移 0032 `post_type` CHECK）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PostType {
    Article,
    Discussion,
}

impl PostType {
    pub const ALL: [PostType; 2] = [PostType::Article, PostType::Discussion];

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "article" => Some(Self::Article),
            "discussion" => Some(Self::Discussion),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Article => "article",
            Self::Discussion => "discussion",
        }
    }
}

impl fmt::Display for PostType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 帖子发布状态（STATE-MACHINES.md §Post）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PostStatus {
    Draft,
    PendingReview,
    Rejected,
    Published,
    Hidden,
    Deleted,
    /// 0003 骨架遗留值（`status='locked'`）；新代码使用 [`Post::closed_at`]。
    /// 仅解析兼容遗留行，不产生新值。
    LockedLegacy,
}

impl PostStatus {
    pub const ALL: [PostStatus; 6] = [
        PostStatus::Draft,
        PostStatus::PendingReview,
        PostStatus::Rejected,
        PostStatus::Published,
        PostStatus::Hidden,
        PostStatus::Deleted,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "draft" => Some(Self::Draft),
            "pending_review" => Some(Self::PendingReview),
            "rejected" => Some(Self::Rejected),
            "published" => Some(Self::Published),
            "hidden" => Some(Self::Hidden),
            "deleted" => Some(Self::Deleted),
            "locked" => Some(Self::LockedLegacy),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::PendingReview => "pending_review",
            Self::Rejected => "rejected",
            Self::Published => "published",
            Self::Hidden => "hidden",
            Self::Deleted => "deleted",
            Self::LockedLegacy => "locked",
        }
    }
}

impl fmt::Display for PostStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 帖子元数据（迁移 0032 后 posts 表投影）。
///
/// 与骨架遗留列（content/content_format/visibility/pinned/last_reply_by）无
/// 关——正文与访问策略分别由 M04-SCHEMA-02（post_contents）与
/// M04-SCHEMA-06（content_access_policies）落地。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Post {
    pub id: String,
    pub board_id: String,
    pub author_id: String,
    pub post_type: PostType,
    /// 板块内唯一；文章必须有 slug，草稿可空。
    pub slug: Option<String>,
    pub title: String,
    pub status: PostStatus,
    /// 乐观并发版本（If-Match 更新来源）。
    pub version: i64,
    pub scheduled_at: Option<i64>,
    pub published_at: Option<i64>,
    pub pinned_at: Option<i64>,
    pub featured_at: Option<i64>,
    /// 非空 = 禁止新增回复（不改变发布/可见状态）。
    pub closed_at: Option<i64>,
    pub canonical_url: Option<String>,
    pub seo_title: Option<String>,
    pub seo_description: Option<String>,
    pub view_count: i64,
    pub reply_count: i64,
    pub last_reply_id: Option<String>,
    pub last_reply_at: Option<i64>,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

impl Post {
    /// 是否允许新增回复（STATE-MACHINES.md §Post：`closed_at` 非空即锁帖）。
    pub fn replies_open(&self) -> bool {
        self.closed_at.is_none()
    }
}

/// 帖子当前正文（post_contents，M04-SCHEMA-02；与 posts 1:1）。
///
/// - `body_html` 为后端生成并清洗的公开 HTML（M04-MARKDOWN-02/03）；
/// - `renderer_version` 标识渲染/清洗策略版本，升级时由 Job 重渲染旧修订
///   （M04-MARKDOWN-05）；
/// - `excerpt` 为公开安全摘要（M04-MARKDOWN-06：禁止从隐藏正文截断）；
/// - `restricted_markdown/html` 为受限部分（access policy，M04-SCHEMA-06）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostContent {
    pub post_id: String,
    pub body_markdown: String,
    pub body_html: String,
    pub restricted_markdown: Option<String>,
    pub restricted_html: Option<String>,
    pub renderer_version: String,
    pub excerpt: String,
    pub updated_at: i64,
}

/// 不可变修订快照（post_revisions，M04-SCHEMA-02；M04-POSTS-08 写入）。
///
/// 每次编辑产生一条新快照，`version` 对应 `posts.version`（每版恰好一条，
/// `UNIQUE(post_id, version)`）；普通作者只能查看允许版本，审核员始终可查看
/// （M04-POSTS-11）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostRevision {
    pub id: String,
    pub post_id: String,
    pub editor_id: String,
    pub body_markdown: String,
    pub body_html: String,
    pub restricted_markdown: Option<String>,
    pub restricted_html: Option<String>,
    pub renderer_version: String,
    pub change_reason: Option<String>,
    pub version: i64,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_type_round_trips() {
        for t in PostType::ALL {
            assert_eq!(PostType::parse(t.as_str()), Some(t));
        }
        assert_eq!(PostType::parse("blog"), None);
        assert_eq!(PostType::parse(""), None);
    }

    #[test]
    fn post_status_round_trips_and_parses_legacy_locked() {
        for s in PostStatus::ALL {
            assert_eq!(PostStatus::parse(s.as_str()), Some(s));
        }
        assert_eq!(
            PostStatus::parse("locked"),
            Some(PostStatus::LockedLegacy),
            "遗留 locked 值仍可解析"
        );
        assert_eq!(PostStatus::parse("bogus"), None);
    }

    #[test]
    fn replies_open_reflects_closed_at() {
        let base = Post {
            id: "p1".into(),
            board_id: "b1".into(),
            author_id: "u1".into(),
            post_type: PostType::Discussion,
            slug: None,
            title: "t".into(),
            status: PostStatus::Published,
            version: 1,
            scheduled_at: None,
            published_at: None,
            pinned_at: None,
            featured_at: None,
            closed_at: None,
            canonical_url: None,
            seo_title: None,
            seo_description: None,
            view_count: 0,
            reply_count: 0,
            last_reply_id: None,
            last_reply_at: None,
            created_at: 1,
            updated_at: 1,
            deleted_at: None,
        };
        assert!(base.replies_open());
        let mut closed = base.clone();
        closed.closed_at = Some(2);
        assert!(!closed.replies_open());
    }
}
