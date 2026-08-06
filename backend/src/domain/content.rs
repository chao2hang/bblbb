//! 内容格式（M04-MARKDOWN-01）：请求只接受 Markdown；显式拒绝原始 HTML、
//! BBCode 与未知内容格式。
//!
//! 设计约束：
//! - 单一可接受格式 `markdown`（v1 契约）；`content_format` 字段不存在时
//!   按 markdown 处理（骨架语义）；
//! - 拒绝时返回结构化 [`ContentFormatRejected`]，路由层映射为稳定 Problem
//!   detail（如 `content_format_unsupported`），禁止把原始输入回显进错误。

/// 内容格式（v1 只接受 Markdown）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentFormat {
    Markdown,
}

impl ContentFormat {
    /// 可接受的全部格式名（单一事实来源）。
    pub const ACCEPTED: &'static [&'static str] = &["markdown"];

    /// 解析并校验内容格式；仅 `markdown`（大小写不敏感、去空白）通过。
    pub fn parse(raw: &str) -> Result<Self, ContentFormatRejected> {
        let normalized = raw.trim().to_ascii_lowercase();
        match normalized.as_str() {
            "markdown" | "" => Ok(Self::Markdown),
            "html" | "text/html" | "xhtml" => Err(ContentFormatRejected::RawHtml),
            "bbcode" | "bbc" => Err(ContentFormatRejected::Bbcode),
            other => Err(ContentFormatRejected::Unknown(other.to_string())),
        }
    }

    pub fn as_str(self) -> &'static str {
        "markdown"
    }
}

impl std::fmt::Display for ContentFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 拒绝原因（稳定错误消息来源；不含原始输入，防 XSS/注入回显）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContentFormatRejected {
    /// 声明/提交为 HTML（原始 HTML 一律拒绝，必须经 Markdown 转义）。
    RawHtml,
    /// 声明/提交为 BBCode（不支持）。
    Bbcode,
    /// 其他未知内容格式。
    Unknown(String),
}

impl std::fmt::Display for ContentFormatRejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RawHtml => write!(f, "content format not supported: raw HTML"),
            Self::Bbcode => write!(f, "content format not supported: BBCode"),
            Self::Unknown(_) => write!(f, "content format not supported"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_markdown() {
        assert_eq!(
            ContentFormat::parse("markdown"),
            Ok(ContentFormat::Markdown)
        );
        // 空/缺省按 markdown（骨架兼容）
        assert_eq!(ContentFormat::parse(""), Ok(ContentFormat::Markdown));
        assert_eq!(ContentFormat::parse("  "), Ok(ContentFormat::Markdown));
        // 大小写/空白不敏感
        assert_eq!(
            ContentFormat::parse("Markdown"),
            Ok(ContentFormat::Markdown)
        );
        assert_eq!(
            ContentFormat::parse(" MARKDOWN "),
            Ok(ContentFormat::Markdown)
        );
    }

    #[test]
    fn rejects_raw_html_explicitly() {
        assert_eq!(
            ContentFormat::parse("html"),
            Err(ContentFormatRejected::RawHtml)
        );
        assert_eq!(
            ContentFormat::parse("text/html"),
            Err(ContentFormatRejected::RawHtml)
        );
        assert_eq!(
            ContentFormat::parse("HTML"),
            Err(ContentFormatRejected::RawHtml)
        );
    }

    #[test]
    fn rejects_bbcode_explicitly() {
        assert_eq!(
            ContentFormat::parse("bbcode"),
            Err(ContentFormatRejected::Bbcode)
        );
        assert_eq!(
            ContentFormat::parse("bbc"),
            Err(ContentFormatRejected::Bbcode)
        );
    }

    #[test]
    fn rejects_unknown_formats() {
        match ContentFormat::parse("org") {
            Err(ContentFormatRejected::Unknown(f)) => assert_eq!(f, "org"),
            other => panic!("预期 Unknown，实际 {other:?}"),
        }
        assert!(matches!(
            ContentFormat::parse("rst"),
            Err(ContentFormatRejected::Unknown(_))
        ));
    }

    #[test]
    fn error_message_is_stable_and_input_free() {
        // 错误消息不得包含原始输入（防回显 XSS/注入）
        let err = ContentFormatRejected::Unknown("<script>alert(1)</script>".to_string());
        let msg = err.to_string();
        assert!(!msg.contains("script"), "错误消息不得回显原始输入");
        assert_eq!(msg, "content format not supported");
        assert_eq!(
            ContentFormatRejected::RawHtml.to_string(),
            "content format not supported: raw HTML"
        );
    }

    #[test]
    fn accepted_list_matches_domain() {
        assert_eq!(ContentFormat::ACCEPTED, &["markdown"]);
        assert_eq!(ContentFormat::Markdown.as_str(), "markdown");
    }
}
