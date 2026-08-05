//! 幂等记录数据模型（M01-AUDIT-03）。
//!
//! 数据模型（表 `idempotency_records`，迁移 0010）：
//! - `scope` + `key` 唯一标识一次业务操作（唯一约束兜底并发首请求，
//!   M01-AUDIT-05）；
//! - `request_hash`：请求摘要（SHA-256 hex），用于"相同 key+摘要返回原结果、
//!   不同摘要稳定返回 409"（M01-AUDIT-04）；
//! - `status`：`in_progress` / `completed` / `failed`；
//! - `response_reference`：已存储响应/结果的引用（如 job id）；
//! - `expires_at`：保留窗口，过期记录可清理/重试。
//!
//! 本模块只定义模型与校验；创建/读取/冲突判定逻辑在 M01-AUDIT-04/05。

use sha2::{Digest, Sha256};

/// 幂等记录状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdempotencyStatus {
    InProgress,
    Completed,
    Failed,
}

impl IdempotencyStatus {
    pub const ALL: [IdempotencyStatus; 3] = [
        IdempotencyStatus::InProgress,
        IdempotencyStatus::Completed,
        IdempotencyStatus::Failed,
    ];

    /// 数据库表示（与 idempotency_records.status 一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            IdempotencyStatus::InProgress => "in_progress",
            IdempotencyStatus::Completed => "completed",
            IdempotencyStatus::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<IdempotencyStatus> {
        Self::ALL
            .iter()
            .find(|status| status.as_str() == value)
            .copied()
    }
}

impl std::fmt::Display for IdempotencyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 幂等键（scope + key），带校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey {
    pub scope: String,
    pub key: String,
}

impl IdempotencyKey {
    /// 校验并构造幂等键。
    ///
    /// - `scope`：非空，≤ 50 字符（如 `pay`、`download`、`purchase`）；
    /// - `key`：非空，≤ 200 字符（客户端幂等键）。
    pub fn new(scope: impl Into<String>, key: impl Into<String>) -> Result<Self, IdempotencyError> {
        let scope = scope.into();
        let key = key.into();
        if scope.is_empty() || scope.len() > 50 {
            return Err(IdempotencyError::InvalidScope);
        }
        if key.is_empty() || key.len() > 200 {
            return Err(IdempotencyError::InvalidKey);
        }
        Ok(Self { scope, key })
    }
}

/// 幂等记录（对应 `idempotency_records` 一行）。
#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub id: String,
    pub scope: String,
    pub key: String,
    /// 请求摘要（SHA-256 hex，64 字符）。
    pub request_hash: String,
    pub status: IdempotencyStatus,
    /// 已存储响应/结果的引用（如 job id）。
    pub response_reference: Option<String>,
    /// 保留窗口截止（Unix 毫秒）。
    pub expires_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 幂等键/哈希校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyError {
    InvalidScope,
    InvalidKey,
    InvalidRequestHash,
}

impl std::fmt::Display for IdempotencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdempotencyError::InvalidScope => write!(f, "idempotency scope must be 1..=50 chars"),
            IdempotencyError::InvalidKey => write!(f, "idempotency key must be 1..=200 chars"),
            IdempotencyError::InvalidRequestHash => {
                write!(f, "request_hash must be 64-char hex")
            }
        }
    }
}

impl std::error::Error for IdempotencyError {}

/// 计算请求摘要（SHA-256 hex）。
///
/// 用于 M01-AUDIT-04：相同 key+摘要返回原结果；相同 key+不同摘要返回 409。
pub fn request_hash(payload: &[u8]) -> String {
    hex::encode(Sha256::digest(payload))
}

/// 校验请求摘要是否为 64 字符 hex。
pub fn validate_request_hash(hash: &str) -> Result<(), IdempotencyError> {
    if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(IdempotencyError::InvalidRequestHash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips() {
        for status in IdempotencyStatus::ALL {
            assert_eq!(IdempotencyStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(IdempotencyStatus::parse("unknown"), None);
    }

    #[test]
    fn idempotency_key_validation() {
        assert!(IdempotencyKey::new("pay", "order-123").is_ok());
        assert_eq!(
            IdempotencyKey::new("", "order-123"),
            Err(IdempotencyError::InvalidScope)
        );
        assert_eq!(
            IdempotencyKey::new("pay", ""),
            Err(IdempotencyError::InvalidKey)
        );
        assert_eq!(
            IdempotencyKey::new("pay", "x".repeat(201)),
            Err(IdempotencyError::InvalidKey)
        );
        assert_eq!(
            IdempotencyKey::new("x".repeat(51), "order-123"),
            Err(IdempotencyError::InvalidScope)
        );
    }

    #[test]
    fn request_hash_is_deterministic_sha256_hex() {
        let a = request_hash(b"hello");
        let b = request_hash(b"hello");
        let c = request_hash(b"hello!");
        assert_eq!(a, b, "相同请求摘要一致");
        assert_ne!(a, c, "不同请求摘要不同");
        assert_eq!(a.len(), 64);
        assert!(validate_request_hash(&a).is_ok());
        assert_eq!(
            validate_request_hash("short"),
            Err(IdempotencyError::InvalidRequestHash)
        );
        assert_eq!(
            validate_request_hash(&"z".repeat(64)),
            Err(IdempotencyError::InvalidRequestHash)
        );
    }
}
