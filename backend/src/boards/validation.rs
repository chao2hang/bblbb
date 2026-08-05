//! M03-BOARDS-02：板块 slug、标题、说明、排序、状态与发帖规则校验。
//!
//! 服务层校验（DB CHECK 只兜底稳定枚举；业务规则在此裁决）：
//! - **slug**：`[a-z0-9-]+`（小写字母/数字/连字符，与种子 slug 与原型
//!   `tech-essay`/`web-dev` 一致），长度 1..=120（与 OpenAPI `Slug` 参数
//!   `minLength:1 / maxLength:120` 一致）；唯一性由 `boards_slug_uq` 兜底，
//!   服务层用 [`super::slug_exists`] 给出友好冲突错误；
//! - **标题（name）**：trim 后非空、≤ 100 字符、单行纯文本（无控制字符、
//!   无角括号）；
//! - **说明（description）**：≤ 2000 字符、无控制字符、无富文本角括号；
//!   链接仅 http/https scheme（防 `javascript:`/`data:` 等）；
//! - **排序（sort_order）**：`[-100_000, 100_000]`（同级排序，BIGINT）；
//! - **状态（is_active）**：布尔（停用 = `is_active: false`，移出活跃投影）；
//! - **发帖规则（posting_mode）**：`normal/approval/readonly/closed`
//!   （与 0022/0025 CHECK 及 authz `BoardPostingMode` 一致）；
//!   `readonly/closed` 禁止新增帖子（服务层强制，M03-BOARDS-03）。

use crate::error::AppError;

/// slug 最小长度（与 OpenAPI `Slug` 参数一致）。
pub const SLUG_MIN: usize = 1;
/// slug 最大长度（与 OpenAPI `Slug` 参数一致）。
pub const SLUG_MAX: usize = 120;
/// 标题（name）最大长度。
pub const NAME_MAX: usize = 100;
/// 说明（description）最大长度。
pub const DESCRIPTION_MAX: usize = 2000;
/// 排序号下限。
pub const SORT_ORDER_MIN: i64 = -100_000;
/// 排序号上限。
pub const SORT_ORDER_MAX: i64 = 100_000;

/// 发帖规则稳定枚举（与迁移 0022/0025 CHECK 及 authz `BoardPostingMode` 一致）。
pub const POSTING_MODES: [&str; 4] = ["normal", "approval", "readonly", "closed"];

/// 板块字段校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BoardValidationError {
    SlugEmpty,
    SlugTooLong { len: usize },
    SlugInvalidChars,
    NameEmpty,
    NameTooLong { len: usize },
    NameControlChar,
    NameRichText,
    NameNewline,
    DescriptionTooLong { len: usize },
    DescriptionControlChar,
    DescriptionRichText,
    DescriptionDangerousLink { scheme: String },
    SortOrderOutOfRange { value: i64 },
    InvalidPostingMode { value: String },
}

impl std::fmt::Display for BoardValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BoardValidationError::SlugEmpty => write!(f, "slug 不能为空"),
            BoardValidationError::SlugTooLong { len } => {
                write!(f, "slug 长度不能超过 {SLUG_MAX}（当前 {len}）")
            }
            BoardValidationError::SlugInvalidChars => {
                write!(f, "slug 只能包含小写字母、数字和连字符（[a-z0-9-]+）")
            }
            BoardValidationError::NameEmpty => write!(f, "name（标题）不能为空"),
            BoardValidationError::NameTooLong { len } => {
                write!(f, "name（标题）长度不能超过 {NAME_MAX}（当前 {len}）")
            }
            BoardValidationError::NameControlChar => write!(f, "name（标题）包含非法控制字符"),
            BoardValidationError::NameRichText => {
                write!(f, "name（标题）不允许富文本/HTML 标记")
            }
            BoardValidationError::NameNewline => write!(f, "name（标题）不允许换行"),
            BoardValidationError::DescriptionTooLong { len } => write!(
                f,
                "description 长度不能超过 {DESCRIPTION_MAX}（当前 {len}）"
            ),
            BoardValidationError::DescriptionControlChar => {
                write!(f, "description 包含非法控制字符")
            }
            BoardValidationError::DescriptionRichText => {
                write!(f, "description 不允许富文本/HTML 标记")
            }
            BoardValidationError::DescriptionDangerousLink { scheme } => {
                write!(
                    f,
                    "description 仅允许 http/https 链接（当前 scheme: {scheme}）"
                )
            }
            BoardValidationError::SortOrderOutOfRange { value } => {
                write!(
                    f,
                    "sort_order 必须在 [{SORT_ORDER_MIN}, {SORT_ORDER_MAX}] 内（当前 {value}）"
                )
            }
            BoardValidationError::InvalidPostingMode { value } => write!(
                f,
                "posting_mode 必须是 {POSTING_MODES:?} 之一（当前 {value}）"
            ),
        }
    }
}

impl std::error::Error for BoardValidationError {}

/// 校验错误 → 400 `invalid_request`（BOARDS-05 路由层统一映射）。
pub fn validation_to_error(err: &BoardValidationError, request_id: &str) -> AppError {
    AppError::bad_request(err.to_string(), request_id, None)
}

/// slug 格式校验：`[a-z0-9-]+`，长度 1..=120。
pub fn validate_slug(slug: &str) -> Result<(), BoardValidationError> {
    let slug = slug.trim();
    if slug.is_empty() {
        return Err(BoardValidationError::SlugEmpty);
    }
    let len = slug.chars().count();
    if len > SLUG_MAX {
        return Err(BoardValidationError::SlugTooLong { len });
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err(BoardValidationError::SlugInvalidChars);
    }
    Ok(())
}

/// 标题（name）校验：trim 后非空、≤ 100、单行纯文本。
pub fn validate_name(name: &str) -> Result<(), BoardValidationError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(BoardValidationError::NameEmpty);
    }
    let len = name.chars().count();
    if len > NAME_MAX {
        return Err(BoardValidationError::NameTooLong { len });
    }
    validate_plain_text(name, true).map_err(|e| match e {
        PlainTextError::ControlChar => BoardValidationError::NameControlChar,
        PlainTextError::Newline => BoardValidationError::NameNewline,
        PlainTextError::RichText => BoardValidationError::NameRichText,
    })
}

/// 说明（description）校验：≤ 2000、纯文本（多行）、链接仅 http/https。
pub fn validate_description(description: &str) -> Result<(), BoardValidationError> {
    let len = description.chars().count();
    if len > DESCRIPTION_MAX {
        return Err(BoardValidationError::DescriptionTooLong { len });
    }
    validate_plain_text(description, false).map_err(|e| match e {
        PlainTextError::ControlChar => BoardValidationError::DescriptionControlChar,
        PlainTextError::RichText => BoardValidationError::DescriptionRichText,
        PlainTextError::Newline => BoardValidationError::DescriptionRichText,
    })?;
    validate_links(description)
        .map_err(|scheme| BoardValidationError::DescriptionDangerousLink { scheme })
}

/// 排序号校验：`[-100_000, 100_000]`。
pub fn validate_sort_order(value: i64) -> Result<(), BoardValidationError> {
    if !(SORT_ORDER_MIN..=SORT_ORDER_MAX).contains(&value) {
        return Err(BoardValidationError::SortOrderOutOfRange { value });
    }
    Ok(())
}

/// 发帖规则校验：`normal/approval/readonly/closed`。
pub fn validate_posting_mode(mode: &str) -> Result<(), BoardValidationError> {
    if POSTING_MODES.contains(&mode) {
        Ok(())
    } else {
        Err(BoardValidationError::InvalidPostingMode {
            value: mode.to_string(),
        })
    }
}

/// 创建板块全字段校验。
#[allow(clippy::too_many_arguments)] // 创建输入：全部字段均显式
pub fn validate_board_fields(
    slug: &str,
    name: &str,
    description: Option<&str>,
    sort_order: i64,
    _is_active: bool,
    posting_mode: &str,
) -> Result<(), BoardValidationError> {
    validate_slug(slug)?;
    validate_name(name)?;
    if let Some(description) = description {
        validate_description(description)?;
    }
    validate_sort_order(sort_order)?;
    // is_active 是布尔，天然合法（无额外约束）
    validate_posting_mode(posting_mode)
}

/// 更新板块部分字段校验（只校验出现的字段）。
pub fn validate_board_update(
    slug: Option<&str>,
    name: Option<&str>,
    description: Option<&str>,
    sort_order: Option<i64>,
    posting_mode: Option<&str>,
) -> Result<(), BoardValidationError> {
    if let Some(slug) = slug {
        validate_slug(slug)?;
    }
    if let Some(name) = name {
        validate_name(name)?;
    }
    if let Some(description) = description {
        validate_description(description)?;
    }
    if let Some(sort_order) = sort_order {
        validate_sort_order(sort_order)?;
    }
    if let Some(mode) = posting_mode {
        validate_posting_mode(mode)?;
    }
    Ok(())
}

enum PlainTextError {
    ControlChar,
    Newline,
    RichText,
}

/// 纯文本校验：无控制字符（保留 \n\t\r）、单行时无换行、无角括号。
fn validate_plain_text(value: &str, single_line: bool) -> Result<(), PlainTextError> {
    for c in value.chars() {
        if c.is_control() && c != '\n' && c != '\t' && c != '\r' {
            return Err(PlainTextError::ControlChar);
        }
        if single_line && (c == '\n' || c == '\r') {
            return Err(PlainTextError::Newline);
        }
        if c == '<' || c == '>' {
            return Err(PlainTextError::RichText);
        }
    }
    Ok(())
}

/// 链接校验：仅 http/https scheme（防 `javascript:`/`data:`/`vbscript:`/`file:`）。
fn validate_links(value: &str) -> Result<(), String> {
    for word in value.split_whitespace() {
        if let Some(pos) = word.find("://") {
            let scheme = word[..pos].to_lowercase();
            if !matches!(scheme.as_str(), "http" | "https") {
                return Err(scheme);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_rules() {
        assert!(validate_slug("general").is_ok());
        assert!(validate_slug("tech-essay").is_ok());
        assert!(validate_slug("a").is_ok());
        assert!(validate_slug("web-dev-2026").is_ok());
        assert_eq!(validate_slug("  "), Err(BoardValidationError::SlugEmpty));
        assert_eq!(validate_slug(""), Err(BoardValidationError::SlugEmpty));
        assert!(matches!(
            validate_slug(&"x".repeat(121)),
            Err(BoardValidationError::SlugTooLong { len: 121 })
        ));
        assert_eq!(
            validate_slug("Tech_Essay"),
            Err(BoardValidationError::SlugInvalidChars)
        );
        assert_eq!(
            validate_slug("tech essay"),
            Err(BoardValidationError::SlugInvalidChars)
        );
        assert_eq!(
            validate_slug("中文"),
            Err(BoardValidationError::SlugInvalidChars)
        );
        // 120 长度边界允许
        assert!(validate_slug(&"x".repeat(120)).is_ok());
    }

    #[test]
    fn name_rules() {
        assert!(validate_name("综合讨论").is_ok());
        assert!(validate_name(" 技术分享 ").is_ok(), "trim 后非空即合法");
        assert_eq!(validate_name("   "), Err(BoardValidationError::NameEmpty));
        assert!(matches!(
            validate_name(&"长".repeat(101)),
            Err(BoardValidationError::NameTooLong { len: 101 })
        ));
        assert_eq!(
            validate_name("a<b>"),
            Err(BoardValidationError::NameRichText),
            "角括号按单行纯文本拒绝"
        );
        assert_eq!(
            validate_name("a\nb"),
            Err(BoardValidationError::NameNewline),
            "标题不允许换行"
        );
    }

    #[test]
    fn description_rules() {
        assert!(validate_description("自由讨论各类话题").is_ok());
        assert!(validate_description("官网 https://example.com 欢迎").is_ok());
        assert!(matches!(
            validate_description(&"长".repeat(2001)),
            Err(BoardValidationError::DescriptionTooLong { len: 2001 })
        ));
        assert_eq!(
            validate_description("带 \u{0} 控制字符"),
            Err(BoardValidationError::DescriptionControlChar)
        );
        assert_eq!(
            validate_description("<script>"),
            Err(BoardValidationError::DescriptionRichText)
        );
        assert_eq!(
            validate_description("javascript:alert(1)"),
            Ok(()),
            "无 :// 前缀的不视为链接"
        );
        assert_eq!(
            validate_description("evil javascript://x"),
            Err(BoardValidationError::DescriptionDangerousLink {
                scheme: "javascript".to_string()
            })
        );
        assert_eq!(
            validate_description("data://x"),
            Err(BoardValidationError::DescriptionDangerousLink {
                scheme: "data".to_string()
            })
        );
    }

    #[test]
    fn sort_order_rules() {
        assert!(validate_sort_order(0).is_ok());
        assert!(validate_sort_order(SORT_ORDER_MIN).is_ok());
        assert!(validate_sort_order(SORT_ORDER_MAX).is_ok());
        assert_eq!(
            validate_sort_order(SORT_ORDER_MIN - 1),
            Err(BoardValidationError::SortOrderOutOfRange {
                value: SORT_ORDER_MIN - 1
            })
        );
        assert_eq!(
            validate_sort_order(SORT_ORDER_MAX + 1),
            Err(BoardValidationError::SortOrderOutOfRange {
                value: SORT_ORDER_MAX + 1
            })
        );
    }

    #[test]
    fn posting_mode_rules() {
        for mode in POSTING_MODES {
            assert!(validate_posting_mode(mode).is_ok(), "{mode}");
        }
        assert_eq!(
            validate_posting_mode("chaos"),
            Err(BoardValidationError::InvalidPostingMode {
                value: "chaos".to_string()
            })
        );
    }

    #[test]
    fn combined_create_validation() {
        assert!(
            validate_board_fields("meta", "站务公告", Some("规则与公告"), 0, true, "normal")
                .is_ok()
        );
        assert_eq!(
            validate_board_fields("", "站务", None, 0, true, "normal"),
            Err(BoardValidationError::SlugEmpty)
        );
        assert_eq!(
            validate_board_fields("meta", "", None, 0, true, "normal"),
            Err(BoardValidationError::NameEmpty)
        );
        assert_eq!(
            validate_board_fields("meta", "站务", None, 0, true, "lockdown"),
            Err(BoardValidationError::InvalidPostingMode {
                value: "lockdown".to_string()
            })
        );
    }

    #[test]
    fn partial_update_validation() {
        assert!(
            validate_board_update(None, None, None, None, None).is_ok(),
            "空更新合法"
        );
        assert!(validate_board_update(Some("meta"), None, None, None, None).is_ok());
        assert_eq!(
            validate_board_update(Some("META"), None, None, None, None),
            Err(BoardValidationError::SlugInvalidChars)
        );
        assert_eq!(
            validate_board_update(None, Some(""), None, None, None),
            Err(BoardValidationError::NameEmpty)
        );
        assert!(matches!(
            validate_board_update(None, None, None, Some(1_000_000), None),
            Err(BoardValidationError::SortOrderOutOfRange { .. })
        ));
    }
}
