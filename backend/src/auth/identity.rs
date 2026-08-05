//! 身份规范化（M02-IDENTITY-02）。
//!
//! 用户名/邮箱在写入 `users` 的 `*_normalized` 列前先规范化：
//! `trim → Unicode NFKC → to_lowercase`。规范化列使用大小写敏感的
//! 排序规则（SQLite `COLLATE BINARY` / MySQL `utf8mb4_bin`），因此
//! 唯一索引能对大小写与 Unicode 变体（全角、连字等）去重。
//!
//! 若跳过规范化直接写入，`User` 与 `user` 会被视为不同值——应用层必须
//! 先调用本模块再入库。

use unicode_normalization::UnicodeNormalization;

/// 规范化用户名：trim + NFKC + lowercase。
pub fn normalize_username(input: &str) -> String {
    input.trim().nfkc().collect::<String>().to_lowercase()
}

/// 规范化邮箱：trim + NFKC + lowercase。
pub fn normalize_email(input: &str) -> String {
    input.trim().nfkc().collect::<String>().to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_lowercases_and_trims() {
        assert_eq!(normalize_username("  UserName  "), "username");
        assert_eq!(normalize_username("USER"), "user");
        assert_eq!(normalize_username("AliceSmith"), "alicesmith");
    }

    #[test]
    fn username_handles_unicode_variants() {
        // 全角 → 半角（NFKC）
        assert_eq!(normalize_username("Ｕｓｅｒ"), "user");
        // 大小写折叠
        assert_eq!(normalize_username("ÅNGSTRÖM"), "ångström");
        // 连字拆分（NFKC）
        assert_eq!(normalize_username("ﬁle"), "file");
    }

    #[test]
    fn email_lowercases_and_trims() {
        assert_eq!(normalize_email("  User@Example.COM  "), "user@example.com");
        assert_eq!(
            normalize_email("A.B+tag@Sub.Example.Org"),
            "a.b+tag@sub.example.org"
        );
    }

    #[test]
    fn email_handles_fullwidth_variants() {
        // 全角邮箱 → 半角（NFKC）
        assert_eq!(
            normalize_email("ｕｓｅｒ＠ｅｘａｍｐｌｅ．ｃｏｍ"),
            "user@example.com"
        );
    }

    #[test]
    fn case_and_unicode_do_not_produce_collisions_after_normalization() {
        // 同一用户名的大小写/全角变体规范化后必须完全一致
        let variants = ["User", "USER", "user", "Ｕｓｅｒ", "  user  "];
        let normalized: Vec<String> = variants.iter().map(|v| normalize_username(v)).collect();
        assert!(
            normalized.iter().all(|n| n == "user"),
            "所有变体必须规范化为同一个值: {normalized:?}"
        );

        let emails = [
            "User@Example.COM",
            "USER@example.com",
            "ｕｓｅｒ＠ｅｘａｍｐｌｅ．ｃｏｍ",
        ];
        let normalized: Vec<String> = emails.iter().map(|v| normalize_email(v)).collect();
        assert!(
            normalized.iter().all(|n| n == "user@example.com"),
            "所有邮箱变体必须规范化为同一个值: {normalized:?}"
        );
    }
}
