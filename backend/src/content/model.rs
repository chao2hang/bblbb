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

/// 草稿（drafts，M04-SCHEMA-03；OpenAPI Draft，与 posts 分离）。
///
/// - `markdown` 为原文；发布时经 Markdown 管线渲染后写入 post_contents；
/// - `board_id` 可空（草稿可在未选板块时创建，M04-POSTS-01/02）；
/// - `visibility_level`/`access_policy` 为发布预设（M04-SCHEMA-06 校验）；
/// - `scheduled_at` 非空 = 定时发布草稿（M04-POSTS-06 Job 执行）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Draft {
    pub id: String,
    pub owner_id: String,
    pub board_id: Option<String>,
    pub post_type: PostType,
    pub title: String,
    pub markdown: String,
    pub visibility_level: Option<i64>,
    pub access_policy: Option<String>,
    pub scheduled_at: Option<i64>,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
}

/// 评论状态（comments 表 CHECK：published/hidden/deleted；pending 审核态随
/// M04-POSTS 迁移扩展 DB CHECK）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CommentStatus {
    Published,
    Hidden,
    Deleted,
}

impl CommentStatus {
    pub const ALL: [CommentStatus; 3] = [
        CommentStatus::Published,
        CommentStatus::Hidden,
        CommentStatus::Deleted,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "published" => Some(Self::Published),
            "hidden" => Some(Self::Hidden),
            "deleted" => Some(Self::Deleted),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Published => "published",
            Self::Hidden => "hidden",
            Self::Deleted => "deleted",
        }
    }
}

impl std::fmt::Display for CommentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 评论（comments，M04-SCHEMA-04）。
///
/// - `floor` 为主题内楼层号（SCHEMA.md 语义；唯一约束随 M04-SCHEMA-07）；
/// - `quoted_comment_id` 引用回复（删除置空，渲染"已删除"占位）；
/// - 正文（content/content_format）为骨架遗留列，M04-COMMENTS 替换骨架时
///   收口。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Comment {
    pub id: String,
    pub post_id: String,
    pub author_id: String,
    pub parent_id: Option<String>,
    pub quoted_comment_id: Option<String>,
    pub floor: i64,
    pub status: CommentStatus,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub deleted_at: Option<i64>,
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

/// 帖子附件引用（post_attachments，M04-SCHEMA-05）。
///
/// - `attachment_id` 只存附件 UUID（attachments 表 M6 落地后补 FK；
///   禁止存远程/签名 URL）；
/// - `kind`：cover（封面，与 posts.cover_attachment_id 一致）或 gallery；
/// - `position` 决定渲染顺序。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostAttachment {
    pub id: String,
    pub post_id: String,
    pub attachment_id: String,
    pub kind: AttachmentKind,
    pub position: i64,
    pub created_at: i64,
}

/// 附件引用类型（post_attachments.kind CHECK）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AttachmentKind {
    Cover,
    Gallery,
}

impl AttachmentKind {
    pub const ALL: [AttachmentKind; 2] = [AttachmentKind::Cover, AttachmentKind::Gallery];

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "cover" => Some(Self::Cover),
            "gallery" => Some(Self::Gallery),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Cover => "cover",
            Self::Gallery => "gallery",
        }
    }
}

/// 帖子-标签关联（post_tags，M04-SCHEMA-05；0003 已建表，补 created_at）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostTag {
    pub post_id: String,
    pub tag_id: String,
    pub created_at: i64,
}

/// 内容访问策略（content_access_policies，M04-SCHEMA-06）。
///
/// - `kind` 复用 [`crate::domain::posts::AccessPolicy`] 封闭枚举
///   （public/logged_in/after_reply/level/paid，M04-VISIBILITY-01）；
/// - 字段组合由 [`ContentAccessPolicy::validate`] 强制（level 需 min_level；
///   paid 需 currency_id+amount；after_reply 可设 reply_grant_persists）；
/// - `policy_version` 标识策略版本，评估行为变更时递增。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentAccessPolicy {
    pub id: String,
    pub kind: crate::domain::posts::AccessPolicy,
    pub min_level: Option<i64>,
    pub currency_id: Option<String>,
    pub amount: Option<i64>,
    pub reply_grant_persists: bool,
    pub policy_version: i64,
    pub created_by: String,
    pub created_at: i64,
}

impl ContentAccessPolicy {
    /// 字段组合校验（与 M04-VISIBILITY-03 越级校验分离——这里是结构性校验）。
    pub fn validate(&self) -> Result<(), &'static str> {
        use crate::domain::posts::AccessPolicy::*;
        match self.kind {
            Level => {
                if self.min_level.is_none() {
                    return Err("level 策略必须指定 min_level");
                }
            }
            Paid => {
                if self.currency_id.is_none() || self.amount.is_none() {
                    return Err("paid 策略必须指定 currency_id 与 amount");
                }
                if self.amount.is_some_and(|a| a <= 0) {
                    return Err("amount 必须为正");
                }
            }
            _ => {}
        }
        Ok(())
    }
}
