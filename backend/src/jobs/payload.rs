//! 邮件任务 payload 与日志的令牌安全（M01-JOBS-12）。
//!
//! 规则：
//! - 邮件任务 payload 只允许 token **引用**（`*_token_id`）或密文所需最小
//!   信息，禁止明文验证/重置 token 进入 payload；
//! - 任何日志/错误文本不得输出验证或重置 token——用 [`redact_token`] 在
//!   写入 `last_error` 与日志前脱敏。
//!
//! 令牌由 [`crate::auth::token::generate_token`] 生成（32 字节熵、
//! URL-safe base64，约 43 字符），数据库只存 SHA-256 哈希。

use serde_json::Value;

/// 禁止以明文出现在邮件任务 payload 中的敏感字段。
pub const FORBIDDEN_PAYLOAD_KEYS: &[&str] = &[
    "verification_token",
    "email_verification_token",
    "reset_token",
    "password_reset_token",
    "magic_link_token",
    "login_token",
    "invite_token",
];

/// 明文 token 的最小形态：≥ 40 字符的 URL-safe base64 连续段。
const MIN_TOKEN_RUN: usize = 40;

/// payload 校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadTokenError {
    /// payload 不是 JSON 对象。
    NotAnObject,
    /// 命中明文 token 字段。
    PlaintextToken { key: String },
    /// 检测到疑似明文 token 形态的长随机串。
    LikelyToken { path: String },
}

impl std::fmt::Display for PayloadTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadTokenError::NotAnObject => {
                write!(f, "mail payload must be a JSON object")
            }
            PayloadTokenError::PlaintextToken { key } => {
                write!(f, "mail payload must not contain plaintext token field `{key}`; use `{key}_id` reference instead")
            }
            PayloadTokenError::LikelyToken { path } => {
                write!(
                    f,
                    "mail payload field `{path}` looks like a plaintext token"
                )
            }
        }
    }
}

impl std::error::Error for PayloadTokenError {}

/// 校验邮件任务 payload：不得携带明文验证/重置 token。
///
/// 只允许 token 引用（`*_token_id`）或密文所需最小信息。递归检查嵌套对象，
/// 并对所有字符串值做 token 形态检测（≥ 40 字符的 URL-safe base64）。
pub fn validate_mail_payload(payload: &Value) -> Result<(), PayloadTokenError> {
    let obj = match payload {
        Value::Object(map) => map,
        _ => return Err(PayloadTokenError::NotAnObject),
    };
    walk_object(obj, "")
}

/// 深度优先遍历对象，检测明文 token。
fn walk_object(
    obj: &serde_json::Map<String, Value>,
    prefix: &str,
) -> Result<(), PayloadTokenError> {
    for (key, value) in obj {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match value {
            Value::Object(nested) => walk_object(nested, &path)?,
            Value::String(text) => {
                // 1) 命中敏感字段名且非空 → 明文 token
                if !text.is_empty() && FORBIDDEN_PAYLOAD_KEYS.contains(&key.as_str()) {
                    return Err(PayloadTokenError::PlaintextToken { key: key.clone() });
                }
                // 2) 任何字段出现 token 形态长随机串 → 疑似明文 token
                if !text.is_empty() && contains_token_shape(text) {
                    return Err(PayloadTokenError::LikelyToken { path });
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// 字符串中是否包含 token 形态连续段（≥ 40 字符、仅 URL-safe base64 字母表）。
/// 手工扫描，避免在热路径引入正则依赖；能识别 magic-link URL 内嵌的 token。
fn contains_token_shape(text: &str) -> bool {
    let mut run = 0usize;
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
            run += 1;
            if run >= MIN_TOKEN_RUN {
                return true;
            }
        } else {
            run = 0;
        }
    }
    false
}

/// 从日志/错误文本中剔除明文 token（[`crate::auth::token::generate_token`]
/// 形态：≥ 40 字符 URL-safe base64），替换为 `[REDACTED]`。
///
/// 写入 `last_error`、`tracing` 日志前必须调用；这是"任何日志不得输出
/// 验证或重置 token"的强制入口。
pub fn redact_token(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        // 把 start 之前的普通内容并入输出
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        // 找到连续 token 字母表片段的边界
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(rest.len());
        let candidate = &rest[..end];
        if candidate.len() >= 40 {
            out.push_str("[REDACTED]");
        } else {
            out.push_str(candidate);
        }
        rest = &rest[end..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::token::generate_token;
    use serde_json::json;

    #[test]
    fn valid_payload_with_token_reference_passes() {
        let payload = json!({
            "email": "user@example.com",
            "user_id": "u_123",
            "verification_token_id": "tok_abc123",
            "expires_in_ms": 900_000
        });
        assert!(validate_mail_payload(&payload).is_ok());
    }

    #[test]
    fn plaintext_verification_token_is_rejected() {
        let payload = json!({
            "email": "user@example.com",
            "verification_token": generate_token()
        });
        let err = validate_mail_payload(&payload).unwrap_err();
        assert!(matches!(
            err,
            PayloadTokenError::PlaintextToken { ref key } if key == "verification_token"
        ));
    }

    #[test]
    fn plaintext_reset_token_is_rejected() {
        let payload = json!({
            "reset_token": generate_token()
        });
        assert!(validate_mail_payload(&payload).is_err());
    }

    #[test]
    fn nested_token_field_is_rejected() {
        let payload = json!({
            "data": { "magic_link_token": generate_token() }
        });
        assert!(validate_mail_payload(&payload).is_err());
    }

    #[test]
    fn token_shaped_value_anywhere_is_rejected() {
        // 即使字段名不是敏感名，长 URL-safe base64 也被判定为疑似 token
        let payload = json!({
            "email": "user@example.com",
            "link": format!("https://x/{}", generate_token())
        });
        let err = validate_mail_payload(&payload).unwrap_err();
        assert!(matches!(err, PayloadTokenError::LikelyToken { .. }));
    }

    #[test]
    fn short_values_and_uuids_are_allowed() {
        let payload = json!({
            "user_id": "u-abc",
            "email_id": "01234567-89ab-cdef-0123-456789abcdef",
            "display_name": "hello world"
        });
        assert!(validate_mail_payload(&payload).is_ok());
    }

    #[test]
    fn non_object_payload_is_rejected() {
        assert!(validate_mail_payload(&Value::String("x".into())).is_err());
        assert!(validate_mail_payload(&Value::Null).is_err());
    }

    #[test]
    fn redact_token_masks_generated_tokens() {
        let token = generate_token();
        let text = format!("smtp 550 rejected recipient, token={token}");
        let redacted = redact_token(&text);
        assert!(!redacted.contains(&token), "token 必须被脱敏");
        assert!(redacted.contains("[REDACTED]"));
        assert!(redacted.contains("smtp 550 rejected recipient"));
    }

    #[test]
    fn redact_token_keeps_short_identifiers() {
        let text = "user u_123, id 01234567-89ab, state running";
        assert_eq!(redact_token(text), text, "短标识符不得被误伤");
    }
}
