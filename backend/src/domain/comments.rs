//! 评论领域模型与校验规则。
//!
//! 字符长度按 Unicode `chars().count()` 计，与契约的 min/max 语义一致。

/// 评论内容：1–10000 字符（按 Unicode 字符计）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentContent(String);

impl CommentContent {
    pub const MAX_CHARS: usize = 10_000;

    /// 解析并校验评论内容；失败时返回稳定错误消息（与路由层 Problem detail 一致）。
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        if raw.trim().is_empty() || raw.chars().count() > Self::MAX_CHARS {
            return Err("content must be 1-10000 characters");
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for CommentContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_content_accepts_valid() {
        assert_eq!(CommentContent::parse("沙发").unwrap().as_str(), "沙发");
    }

    #[test]
    fn comment_content_rejects_empty() {
        assert!(CommentContent::parse("").is_err());
        assert!(CommentContent::parse("  ").is_err());
    }

    #[test]
    fn comment_content_rejects_too_long() {
        assert!(CommentContent::parse(&"x".repeat(10_001)).is_err());
        assert!(CommentContent::parse(&"x".repeat(10_000)).is_ok());
    }

    #[test]
    fn comment_content_counts_unicode_chars() {
        assert!(CommentContent::parse(&"中".repeat(10_000)).is_ok());
        assert!(CommentContent::parse(&"中".repeat(10_001)).is_err());
    }
}
