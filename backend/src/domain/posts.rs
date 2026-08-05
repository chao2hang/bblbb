//! 帖子领域模型与校验规则。
//!
//! 字符长度按 Unicode `chars().count()` 计，与契约的 min/max 语义一致
//! （多字节字符各计 1）。本模块不依赖任何框架或数据库。

/// 帖子标题：去除首尾空白后 1–200 字符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostTitle(String);

impl PostTitle {
    pub const MAX_CHARS: usize = 200;

    /// 解析并校验标题；失败时返回稳定的错误消息（与路由层 Problem detail 一致）。
    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        let trimmed = raw.trim();
        if trimmed.is_empty() || trimmed.chars().count() > Self::MAX_CHARS {
            return Err("title must be 1-200 characters");
        }
        Ok(Self(trimmed.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PostTitle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 帖子正文：1–50000 字符（按 Unicode 字符计）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostContent(String);

impl PostContent {
    pub const MAX_CHARS: usize = 50_000;

    pub fn parse(raw: &str) -> Result<Self, &'static str> {
        if raw.trim().is_empty() || raw.chars().count() > Self::MAX_CHARS {
            return Err("content must be 1-50000 characters");
        }
        Ok(Self(raw.to_string()))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for PostContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// 访问策略（契约 `access_policy` 枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessPolicy {
    Public,
    LoggedIn,
    AfterReply,
    Level,
    Paid,
}

impl AccessPolicy {
    pub const ALL: &'static [AccessPolicy] = &[
        AccessPolicy::Public,
        AccessPolicy::LoggedIn,
        AccessPolicy::AfterReply,
        AccessPolicy::Level,
        AccessPolicy::Paid,
    ];

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "public" => Some(Self::Public),
            "logged_in" => Some(Self::LoggedIn),
            "after_reply" => Some(Self::AfterReply),
            "level" => Some(Self::Level),
            "paid" => Some(Self::Paid),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::LoggedIn => "logged_in",
            Self::AfterReply => "after_reply",
            Self::Level => "level",
            Self::Paid => "paid",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_title_accepts_valid() {
        assert_eq!(PostTitle::parse("你好世界").unwrap().as_str(), "你好世界");
        assert_eq!(PostTitle::parse("a").unwrap().as_str(), "a");
    }

    #[test]
    fn post_title_rejects_empty_and_whitespace_only() {
        assert!(PostTitle::parse("").is_err());
        assert!(PostTitle::parse("   ").is_err());
    }

    #[test]
    fn post_title_trims_leading_trailing_whitespace() {
        assert_eq!(PostTitle::parse("  hello  ").unwrap().as_str(), "hello");
    }

    #[test]
    fn post_title_rejects_over_200_chars() {
        let long = "a".repeat(201);
        assert!(PostTitle::parse(&long).is_err());
        let ok = "a".repeat(200);
        assert!(PostTitle::parse(&ok).is_ok());
    }

    #[test]
    fn post_title_counts_unicode_chars() {
        // 150 个中文字符 = 150 chars（不是 450 bytes）
        let cn = "好".repeat(150);
        assert!(PostTitle::parse(&cn).is_ok());
    }

    #[test]
    fn post_content_accepts_valid() {
        assert_eq!(PostContent::parse("正文").unwrap().as_str(), "正文");
    }

    #[test]
    fn post_content_rejects_empty_and_too_long() {
        assert!(PostContent::parse("").is_err());
        assert!(PostContent::parse("   ").is_err());
        assert!(PostContent::parse(&"x".repeat(50_001)).is_err());
        assert!(PostContent::parse(&"x".repeat(50_000)).is_ok());
    }

    #[test]
    fn access_policy_roundtrip_all_values() {
        for policy in AccessPolicy::ALL {
            assert_eq!(AccessPolicy::parse(policy.as_str()), Some(*policy));
        }
    }

    #[test]
    fn access_policy_rejects_unknown() {
        assert_eq!(AccessPolicy::parse("private"), None);
        assert_eq!(AccessPolicy::parse(""), None);
        assert_eq!(AccessPolicy::parse("PUBLIC"), None);
    }
}
