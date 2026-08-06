//! M04-POSTS-01：创建命令与服务端字段校验。
//!
//! **不信任原则**：客户端提交的只是"内容输入"；author、status、version、
//! 统计值（view_count/reply_count）、置顶/精选等全部由服务端在写路径赋值，
//! 命令结构里**不存在**这些字段，从类型层面杜绝信任。
//!
//! 校验规则（全部服务端权威，客户端放宽上限无效）：
//! - `post_type` ∈ {article, discussion}；
//! - `title` 1–200 字符（trim 后，[`PostTitle`]）；
//! - `markdown` 1–50000 字符（Unicode char，[`PostContent`]），隐含
//!   markdown 格式（M04-MARKDOWN-01 只接受 Markdown）；
//! - `board_id` 必须为合法 UUID；
//! - `visibility_level` ≥1 且**不得超过作者当前等级**（防低等级作者把内容
//!   设到更高隐藏级别）；
//! - `access_policy` ∈ 封闭枚举（策略明细校验在发布前 POSTS-05）；
//! - `scheduled_at` 可选，若存在必须严格晚于服务端当前时间（毫秒）；
//! - `client_request_id` 16–200 字符（幂等键，M04-POSTS-03 使用）。

use uuid::Uuid;

use crate::content::model::PostType;
use crate::domain::posts::{AccessPolicy, PostContent, PostTitle};

/// `visibility_level` 校验基准（等级由作者当前等级裁决，这里是纯服务端上限）。
pub const MAX_VISIBILITY_LEVEL: u32 = 255;

/// 幂等键最小长度（OpenAPI `client_request_id` minLength）。
pub const CLIENT_REQUEST_ID_MIN: usize = 16;
/// 幂等键最大长度（OpenAPI `client_request_id` maxLength）。
pub const CLIENT_REQUEST_ID_MAX: usize = 200;

/// 创建命令校验错误（稳定 Display，不含原始输入回显）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PostCreateError {
    /// 未知帖子类型（只接受 article/discussion）。
    InvalidPostType,
    /// 标题非法（原因消息稳定，见 [`PostTitle`]）。
    InvalidTitle(&'static str),
    /// 正文非法（原因消息稳定，见 [`PostContent`]）。
    InvalidMarkdown(&'static str),
    /// board_id 不是合法 UUID。
    InvalidBoardId,
    /// visibility_level 非法（<1）。
    InvalidVisibilityLevel,
    /// visibility_level 超过作者当前等级。
    VisibilityExceedsAuthorLevel { requested: u32, author_level: u32 },
    /// access_policy 未知。
    InvalidAccessPolicy,
    /// scheduled_at 非法（必须严格晚于服务端当前时间）。
    InvalidScheduledAt,
    /// client_request_id 长度/字符非法。
    InvalidClientRequestId,
}

impl std::fmt::Display for PostCreateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPostType => write!(f, "post type must be article or discussion"),
            Self::InvalidTitle(reason) => write!(f, "invalid title: {reason}"),
            Self::InvalidMarkdown(reason) => write!(f, "invalid markdown: {reason}"),
            Self::InvalidBoardId => write!(f, "board_id must be a valid UUID"),
            Self::InvalidVisibilityLevel => write!(f, "visibility_level must be >= 1"),
            Self::VisibilityExceedsAuthorLevel {
                requested,
                author_level,
            } => write!(
                f,
                "visibility_level {requested} exceeds author level {author_level}"
            ),
            Self::InvalidAccessPolicy => write!(f, "access_policy is not supported"),
            Self::InvalidScheduledAt => write!(f, "scheduled_at must be in the future"),
            Self::InvalidClientRequestId => write!(
                f,
                "client_request_id must be {CLIENT_REQUEST_ID_MIN}-{CLIENT_REQUEST_ID_MAX} characters"
            ),
        }
    }
}

impl std::error::Error for PostCreateError {}

/// 客户端提交的**原样**创建输入（未信任；仅内容字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePostInput {
    pub post_type: String,
    pub title: String,
    pub markdown: String,
    pub board_id: String,
    pub visibility_level: Option<u32>,
    pub access_policy: String,
    pub scheduled_at: Option<i64>,
    pub client_request_id: String,
}

/// 客户端提交的**原样**草稿创建输入（`board_id` 可空：草稿可在未选板块时
/// 创建，发布时再定，模型 `drafts.board_id` 可空）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDraftInput {
    pub post_type: String,
    pub title: String,
    pub markdown: String,
    pub board_id: Option<String>,
    pub visibility_level: Option<u32>,
    pub access_policy: String,
    pub scheduled_at: Option<i64>,
    pub client_request_id: String,
}

/// 服务端校验通过后的**权威**创建命令（文章/讨论直接发布）。
///
/// 不含 author_id/status/version/统计值——这些由服务端写路径从会话与
/// 服务器状态赋值。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatePostCommand {
    pub post_type: PostType,
    pub title: PostTitle,
    pub markdown: PostContent,
    pub board_id: Uuid,
    pub visibility_level: Option<u32>,
    pub access_policy: AccessPolicy,
    pub scheduled_at: Option<i64>,
    pub client_request_id: String,
}

/// 服务端校验通过后的**权威**草稿创建命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDraftCommand {
    pub post_type: PostType,
    pub title: PostTitle,
    pub markdown: PostContent,
    pub board_id: Option<Uuid>,
    pub visibility_level: Option<u32>,
    pub access_policy: AccessPolicy,
    pub scheduled_at: Option<i64>,
    pub client_request_id: String,
}

/// 校验文章/讨论创建命令。
///
/// `author_level`：作者当前等级（服务端从 DB 读取，不信任客户端）。
/// `now_ms`：服务端当前时间（毫秒），用于 scheduled_at 校验。
pub fn validate_post_create(
    input: CreatePostInput,
    author_level: u32,
    now_ms: i64,
) -> Result<CreatePostCommand, PostCreateError> {
    let post_type = PostType::parse(&input.post_type).ok_or(PostCreateError::InvalidPostType)?;
    let title = PostTitle::parse(&input.title).map_err(PostCreateError::InvalidTitle)?;
    let markdown = PostContent::parse(&input.markdown).map_err(PostCreateError::InvalidMarkdown)?;
    let board_id = Uuid::parse_str(&input.board_id).map_err(|_| PostCreateError::InvalidBoardId)?;
    let visibility_level = validate_visibility_level(input.visibility_level, author_level)?;
    let access_policy =
        AccessPolicy::parse(&input.access_policy).ok_or(PostCreateError::InvalidAccessPolicy)?;
    let scheduled_at = validate_scheduled_at(input.scheduled_at, now_ms)?;
    validate_client_request_id(&input.client_request_id)?;

    Ok(CreatePostCommand {
        post_type,
        title,
        markdown,
        board_id,
        visibility_level,
        access_policy,
        scheduled_at,
        client_request_id: input.client_request_id,
    })
}

/// 校验草稿创建命令（board_id 可选；其余与文章创建一致）。
pub fn validate_draft_create(
    input: CreateDraftInput,
    author_level: u32,
    now_ms: i64,
) -> Result<CreateDraftCommand, PostCreateError> {
    let post_type = PostType::parse(&input.post_type).ok_or(PostCreateError::InvalidPostType)?;
    let title = PostTitle::parse(&input.title).map_err(PostCreateError::InvalidTitle)?;
    let markdown = PostContent::parse(&input.markdown).map_err(PostCreateError::InvalidMarkdown)?;
    let board_id = match input.board_id {
        Some(raw) => Some(Uuid::parse_str(&raw).map_err(|_| PostCreateError::InvalidBoardId)?),
        None => None,
    };
    let visibility_level = validate_visibility_level(input.visibility_level, author_level)?;
    let access_policy =
        AccessPolicy::parse(&input.access_policy).ok_or(PostCreateError::InvalidAccessPolicy)?;
    let scheduled_at = validate_scheduled_at(input.scheduled_at, now_ms)?;
    validate_client_request_id(&input.client_request_id)?;

    Ok(CreateDraftCommand {
        post_type,
        title,
        markdown,
        board_id,
        visibility_level,
        access_policy,
        scheduled_at,
        client_request_id: input.client_request_id,
    })
}

/// visibility_level：缺省按 1；必须在 1..=min(author_level, MAX) 内。
fn validate_visibility_level(
    level: Option<u32>,
    author_level: u32,
) -> Result<Option<u32>, PostCreateError> {
    let lv = level.unwrap_or(1);
    if lv < 1 {
        return Err(PostCreateError::InvalidVisibilityLevel);
    }
    let cap = author_level.min(MAX_VISIBILITY_LEVEL);
    if lv > cap {
        return Err(PostCreateError::VisibilityExceedsAuthorLevel {
            requested: lv,
            author_level,
        });
    }
    Ok(Some(lv))
}

/// scheduled_at：可选；存在时必须严格晚于服务端当前时间。
fn validate_scheduled_at(
    scheduled_at: Option<i64>,
    now_ms: i64,
) -> Result<Option<i64>, PostCreateError> {
    match scheduled_at {
        Some(ts) if ts > now_ms => Ok(Some(ts)),
        Some(_) => Err(PostCreateError::InvalidScheduledAt),
        None => Ok(None),
    }
}

/// client_request_id：16–200 字符（幂等键）。
fn validate_client_request_id(raw: &str) -> Result<(), PostCreateError> {
    let n = raw.chars().count();
    if !(CLIENT_REQUEST_ID_MIN..=CLIENT_REQUEST_ID_MAX).contains(&n) {
        return Err(PostCreateError::InvalidClientRequestId);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(over: impl FnOnce(&mut CreatePostInput)) -> CreatePostInput {
        let mut i = CreatePostInput {
            post_type: "article".to_string(),
            title: "标题".to_string(),
            markdown: "正文".to_string(),
            board_id: "01911fd5-f000-7561-a2a5-3dd6434157f0".to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: "test-request-id-0001".to_string(),
        };
        over(&mut i);
        i
    }

    fn draft_input(over: impl FnOnce(&mut CreateDraftInput)) -> CreateDraftInput {
        let mut i = CreateDraftInput {
            post_type: "article".to_string(),
            title: "草稿标题".to_string(),
            markdown: "草稿正文".to_string(),
            board_id: Some("01911fd5-f000-7561-a2a5-3dd6434157f0".to_string()),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: "test-request-id-0001".to_string(),
        };
        over(&mut i);
        i
    }

    fn valid_input() -> CreatePostInput {
        input(|_| {})
    }

    #[test]
    fn valid_post_create_passes() {
        let cmd = validate_post_create(valid_input(), 5, 1_000_000).unwrap();
        assert_eq!(cmd.post_type, PostType::Article);
        assert_eq!(cmd.title.as_str(), "标题");
        assert_eq!(cmd.markdown.as_str(), "正文");
        assert_eq!(cmd.visibility_level, Some(1));
        assert_eq!(cmd.access_policy, AccessPolicy::Public);
        assert!(cmd.scheduled_at.is_none());
    }

    #[test]
    fn rejects_unknown_post_type() {
        let i = input(|i| i.post_type = "question".into());
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidPostType
        );
    }

    #[test]
    fn accepts_discussion_type() {
        let i = input(|i| i.post_type = "discussion".into());
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap().post_type,
            PostType::Discussion
        );
    }

    #[test]
    fn rejects_invalid_title_and_markdown() {
        let i = input(|i| i.title = "   ".into());
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidTitle("title must be 1-200 characters")
        );
        let i = input(|i| i.markdown = "x".repeat(50_001));
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidMarkdown("content must be 1-50000 characters")
        );
    }

    #[test]
    fn rejects_invalid_board_uuid() {
        let i = input(|i| i.board_id = "not-a-uuid".into());
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidBoardId
        );
    }

    #[test]
    fn visibility_level_respects_author_level() {
        // 缺省=1 且 ≤ 作者等级
        let i = input(|i| i.visibility_level = None);
        assert_eq!(
            validate_post_create(i, 5, 1_000_000)
                .unwrap()
                .visibility_level,
            Some(1)
        );
        // 作者等级内合法
        let i = input(|i| i.visibility_level = Some(3));
        assert_eq!(
            validate_post_create(i, 5, 1_000_000)
                .unwrap()
                .visibility_level,
            Some(3)
        );
        // 超过作者等级拒绝
        let i = input(|i| i.visibility_level = Some(6));
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::VisibilityExceedsAuthorLevel {
                requested: 6,
                author_level: 5
            }
        );
        // <1 拒绝
        let i = input(|i| i.visibility_level = Some(0));
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidVisibilityLevel
        );
    }

    #[test]
    fn rejects_unknown_access_policy() {
        let i = input(|i| i.access_policy = "private".into());
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidAccessPolicy
        );
    }

    #[test]
    fn scheduled_at_must_be_future() {
        let now = 1_000_000;
        let i = input(|i| i.scheduled_at = Some(now + 60_000));
        assert_eq!(
            validate_post_create(i, 5, now).unwrap().scheduled_at,
            Some(now + 60_000)
        );
        // 过去/当前时间拒绝
        for past in [now, now - 1, 0] {
            let i = input(|i| i.scheduled_at = Some(past));
            assert_eq!(
                validate_post_create(i, 5, now).unwrap_err(),
                PostCreateError::InvalidScheduledAt
            );
        }
    }

    #[test]
    fn client_request_id_length_checked() {
        let i = input(|i| i.client_request_id = "short".into());
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidClientRequestId
        );
        let i = input(|i| i.client_request_id = "x".repeat(201));
        assert_eq!(
            validate_post_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidClientRequestId
        );
        let i = input(|i| i.client_request_id = "x".repeat(16));
        assert!(validate_post_create(i, 5, 1_000_000).is_ok());
    }

    #[test]
    fn draft_create_allows_missing_board() {
        let i = draft_input(|i| i.board_id = None);
        let cmd = validate_draft_create(i, 5, 1_000_000).unwrap();
        assert_eq!(cmd.board_id, None);
        let i = draft_input(|i| i.board_id = Some("01911fd5-f001-758e-a95d-a58489fbb61d".into()));
        let cmd = validate_draft_create(i, 5, 1_000_000).unwrap();
        assert_eq!(
            cmd.board_id,
            Some(Uuid::parse_str("01911fd5-f001-758e-a95d-a58489fbb61d").unwrap())
        );
        // 存在但非法 UUID 仍拒绝
        let i = draft_input(|i| i.board_id = Some("bad".into()));
        assert_eq!(
            validate_draft_create(i, 5, 1_000_000).unwrap_err(),
            PostCreateError::InvalidBoardId
        );
    }

    #[test]
    fn command_has_no_trusted_server_fields() {
        // 类型层面保证：命令只含 8 个客户端内容字段；author/status/version/
        // 统计值/置顶精选字段**不存在**于结构体（serde 反序列化会直接拒绝）。
        // 这里用 Debug 表示核对关键字段名不会出现。
        let cmd = validate_post_create(valid_input(), 5, 1_000_000).unwrap();
        let debug = format!("{cmd:?}");
        for forbidden in [
            "author_id",
            "status",
            "version",
            "view_count",
            "reply_count",
            "pinned_at",
            "featured_at",
            "created_by",
        ] {
            assert!(
                !debug.contains(forbidden),
                "命令不得包含服务端权威字段 {forbidden}: {debug}"
            );
        }
        // 正向：命令确有内容字段
        assert!(debug.contains("post_type"));
        assert!(debug.contains("title"));
        assert!(debug.contains("markdown"));
        assert!(debug.contains("board_id"));
        assert!(debug.contains("client_request_id"));
    }

    #[test]
    fn error_messages_are_stable() {
        assert_eq!(
            PostCreateError::InvalidPostType.to_string(),
            "post type must be article or discussion"
        );
        assert_eq!(
            PostCreateError::InvalidBoardId.to_string(),
            "board_id must be a valid UUID"
        );
        assert_eq!(
            PostCreateError::VisibilityExceedsAuthorLevel {
                requested: 9,
                author_level: 3
            }
            .to_string(),
            "visibility_level 9 exceeds author level 3"
        );
        assert_eq!(
            PostCreateError::InvalidScheduledAt.to_string(),
            "scheduled_at must be in the future"
        );
    }
}
