//! 注册 DTO 与校验（M02-IDENTITY-03）。
//!
//! 领域层纯逻辑（无 axum/sqlx/环境变量，`make check-domain` 强制）：
//! - 请求体使用 `#[serde(deny_unknown_fields)]`——未知字段直接拒绝；
//! - 校验长度、格式、保留名与密码策略；
//! - 校验成功输出 [`NormalizedRegistration`]（已规范化），供
//!   M02-IDENTITY-05 的事务创建使用。

use serde::Deserialize;

use crate::auth::{normalize_email, normalize_username};

/// 注册请求体（deny_unknown_fields：任何未知字段使反序列化失败）。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

/// 保留用户名（规范化后大小写不敏感匹配，禁止注册）。
pub const RESERVED_USERNAMES: &[&str] = &[
    "admin",
    "administrator",
    "root",
    "system",
    "service",
    "moderator",
    "mod",
    "staff",
    "support",
    "bblbb",
    "api",
    "user",
    "guest",
    "null",
    "undefined",
    "deleted",
];

/// 校验通过的规范化注册数据（构造后即合法）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedRegistration {
    /// 展示用户名（trim 后原样）。
    pub username: String,
    /// 规范化用户名（入库值）。
    pub username_normalized: String,
    /// 展示邮箱（trim 后原样）。
    pub email: String,
    /// 规范化邮箱（入库值）。
    pub email_normalized: String,
    /// 密码明文（仅用于立即哈希，绝不落库/日志）。
    pub password: String,
}

/// 注册校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegisterValidationError {
    /// 用户名长度 3..=20（字符数）。
    UsernameLength,
    /// 用户名含非法字符（仅允许字母/数字/`_`/`-`）。
    UsernameInvalidChars,
    /// 用户名为保留名。
    UsernameReserved,
    /// 邮箱长度 > 254。
    EmailTooLong,
    /// 邮箱格式非法。
    EmailInvalid,
    /// 密码长度 8..=128。
    PasswordLength,
    /// 密码必须同时包含字母与数字。
    PasswordComplexity,
}

impl std::fmt::Display for RegisterValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegisterValidationError::UsernameLength => {
                write!(f, "username must be 3..=20 characters")
            }
            RegisterValidationError::UsernameInvalidChars => {
                write!(f, "username may only contain letters, digits, '_' and '-'")
            }
            RegisterValidationError::UsernameReserved => {
                write!(f, "username is reserved")
            }
            RegisterValidationError::EmailTooLong => {
                write!(f, "email must be <= 254 characters")
            }
            RegisterValidationError::EmailInvalid => {
                write!(f, "email format is invalid")
            }
            RegisterValidationError::PasswordLength => {
                write!(f, "password must be 8..=128 characters")
            }
            RegisterValidationError::PasswordComplexity => {
                write!(f, "password must contain both letters and digits")
            }
        }
    }
}

impl std::error::Error for RegisterValidationError {}

/// 校验注册请求；成功输出规范化注册数据。
pub fn validate_register(
    req: &RegisterRequest,
) -> Result<NormalizedRegistration, RegisterValidationError> {
    let username = req.username.trim();
    let username_normalized = normalize_username(username);

    // 用户名长度（按规范化后的字符数）
    let username_len = username_normalized.chars().count();
    if !(3..=20).contains(&username_len) {
        return Err(RegisterValidationError::UsernameLength);
    }
    // 用户名格式：字母/数字/_/-
    if !username_normalized
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
    {
        return Err(RegisterValidationError::UsernameInvalidChars);
    }
    // 保留名（大小写不敏感）
    if RESERVED_USERNAMES
        .iter()
        .any(|reserved| *reserved == username_normalized)
    {
        return Err(RegisterValidationError::UsernameReserved);
    }

    let email = req.email.trim();
    let email_normalized = normalize_email(email);
    if email_normalized.chars().count() > 254 {
        return Err(RegisterValidationError::EmailTooLong);
    }
    if !valid_email(&email_normalized) {
        return Err(RegisterValidationError::EmailInvalid);
    }

    let password = req.password.as_str();
    let password_len = password.chars().count();
    if !(8..=128).contains(&password_len) {
        return Err(RegisterValidationError::PasswordLength);
    }
    let has_letter = password.chars().any(|c| c.is_alphabetic());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    if !(has_letter && has_digit) {
        return Err(RegisterValidationError::PasswordComplexity);
    }

    Ok(NormalizedRegistration {
        username: username.to_owned(),
        username_normalized,
        email: email.to_owned(),
        email_normalized,
        password: password.to_owned(),
    })
}

/// 基础邮箱格式校验：恰好一个 `@`、本地/域名非空、域名含 `.`、无空白。
fn valid_email(email: &str) -> bool {
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if parts.next().is_some() || local.is_empty() || domain.is_empty() {
        return false;
    }
    if local.contains(char::is_whitespace) || domain.contains(char::is_whitespace) {
        return false;
    }
    domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn req(username: &str, email: &str, password: &str) -> RegisterRequest {
        serde_json::from_value(json!({
            "username": username,
            "email": email,
            "password": password,
        }))
        .unwrap()
    }

    #[test]
    fn valid_registration_normalizes_fields() {
        let out = validate_register(&req(" Alice ", "User@Example.COM", "passw0rd9")).unwrap();
        assert_eq!(out.username_normalized, "alice");
        assert_eq!(out.email_normalized, "user@example.com");
        assert_eq!(out.email, "User@Example.COM");
        assert_eq!(out.password, "passw0rd9");
    }

    #[test]
    fn username_length_rules() {
        assert!(matches!(
            validate_register(&req("ab", "a@b.co", "passw0rd9")),
            Err(RegisterValidationError::UsernameLength)
        ));
        assert!(matches!(
            validate_register(&req(&"x".repeat(21), "a@b.co", "passw0rd9")),
            Err(RegisterValidationError::UsernameLength)
        ));
        // 恰好 3 与 20 通过
        assert!(validate_register(&req("abc", "a@b.co", "passw0rd9")).is_ok());
        assert!(validate_register(&req(&"x".repeat(20), "a@b.co", "passw0rd9")).is_ok());
    }

    #[test]
    fn username_invalid_chars_rejected() {
        assert!(matches!(
            validate_register(&req("bad name!", "a@b.co", "passw0rd9")),
            Err(RegisterValidationError::UsernameInvalidChars)
        ));
        assert!(matches!(
            validate_register(&req("user..x", "a@b.co", "passw0rd9")),
            Err(RegisterValidationError::UsernameInvalidChars)
        ));
        // 下划线与连字符合法
        assert!(validate_register(&req("user_x-1", "a@b.co", "passw0rd9")).is_ok());
    }

    #[test]
    fn reserved_usernames_rejected_case_insensitively() {
        for reserved in ["admin", "ADMIN", "Root", "moderator", "SYSTEM"] {
            assert!(
                matches!(
                    validate_register(&req(reserved, "a@b.co", "passw0rd9")),
                    Err(RegisterValidationError::UsernameReserved)
                ),
                "{reserved} 必须被拒绝"
            );
        }
        // 前缀相似不拒绝（user123 合法）
        assert!(validate_register(&req("user123", "a@b.co", "passw0rd9")).is_ok());
    }

    #[test]
    fn email_rules() {
        assert!(matches!(
            validate_register(&req("alice", "no-at-sign", "passw0rd9")),
            Err(RegisterValidationError::EmailInvalid)
        ));
        assert!(matches!(
            validate_register(&req("alice", "a@b", "passw0rd9")),
            Err(RegisterValidationError::EmailInvalid)
        ));
        assert!(matches!(
            validate_register(&req("alice", "a@b.co x", "passw0rd9")),
            Err(RegisterValidationError::EmailInvalid)
        ));
        assert!(matches!(
            validate_register(&req(
                "alice",
                &format!("{}@b.co", "a".repeat(250)),
                "passw0rd9"
            )),
            Err(RegisterValidationError::EmailTooLong)
        ));
        assert!(validate_register(&req("alice", "a.b+c@sub.example.org", "passw0rd9")).is_ok());
    }

    #[test]
    fn password_policy() {
        assert!(matches!(
            validate_register(&req("alice", "a@b.co", "short7")),
            Err(RegisterValidationError::PasswordLength)
        ));
        assert!(matches!(
            validate_register(&req("alice", "a@b.co", "alllettersonly")),
            Err(RegisterValidationError::PasswordComplexity)
        ));
        assert!(matches!(
            validate_register(&req("alice", "a@b.co", "1234567890")),
            Err(RegisterValidationError::PasswordComplexity)
        ));
        assert!(validate_register(&req("alice", "a@b.co", "passw0rd9")).is_ok());
    }

    #[test]
    fn unknown_fields_are_rejected() {
        let result: Result<RegisterRequest, _> = serde_json::from_value(json!({
            "username": "alice",
            "email": "a@b.co",
            "password": "passw0rd9",
            "extra_field": true
        }));
        assert!(result.is_err(), "未知请求体字段必须使反序列化失败");
    }

    #[test]
    fn missing_fields_are_rejected() {
        let result: Result<RegisterRequest, _> =
            serde_json::from_value(json!({ "username": "alice" }));
        assert!(result.is_err(), "缺少字段必须失败");
    }
}
