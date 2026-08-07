//! M06-ADAPTER：存储错误类型与稳定 Problem code 映射。
//!
//! `StorageError` 覆盖本地/S3 适配器的全部失败分类（M06-ADAPTER-08/10）：
//! 对象不存在、权限错误、速率/冲突、超时、DNS/TLS、部分上传与校验失败。
//! 每个变体携带稳定错误码，由路由层映射为 RFC 9457 Problem 响应，避免把
//! 供应商原始错误泄漏给客户端。

/// 存储错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StorageError {
    /// 数据库/SQL 失败（包装 sqlx::Error 消息，不含语句原文）。
    Db(String),
    /// 对象或记录不存在。
    NotFound(String),
    /// 参数/配置非法（路径越界、空 key、非法 TTL 等）。
    Invalid(String),
    /// 权限错误（S3 403 / 本地文件系统拒绝）。
    Forbidden(String),
    /// 认证错误（S3 401）。
    Auth(String),
    /// 供应商速率限制（S3 429）。
    RateLimited(String),
    /// 请求冲突（S3 409 / 并发 complete）。
    Conflict(String),
    /// 供应商 5xx 或未知服务错误。
    Upstream(String),
    /// 网络超时 / DNS / TLS 失败。
    Network(String),
    /// 部分上传或 multipart 生命周期错误（超限 part、错误顺序、abort 失败）。
    PartialUpload(String),
    /// 校验失败（大小/hash/Content-Type 与声明不符）。
    Verification(String),
    /// 对象大小/hash 与迁移清单不符。
    Mismatch(String),
    /// 配额错误（预留超卖、负数释放、超过上限）。
    Quota(String),
    /// 状态机非法迁移（如未 ready 附件关联公开内容）。
    State(String),
    /// 未实现的适配器能力。
    Unsupported(String),
    /// 内部错误（保留原因为字符串，经 AppError::sanitize 脱敏）。
    Internal(String),
}

impl StorageError {
    /// 稳定 Problem code（ERROR-CODES.md / OpenAPI）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Db(_) => "storage_db_error",
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_storage_request",
            Self::Forbidden(_) => "storage_forbidden",
            Self::Auth(_) => "storage_auth_failed",
            Self::RateLimited(_) => "storage_rate_limited",
            Self::Conflict(_) => "storage_conflict",
            Self::Upstream(_) => "storage_upstream_error",
            Self::Network(_) => "storage_network_error",
            Self::PartialUpload(_) => "storage_partial_upload",
            Self::Verification(_) => "storage_verification_failed",
            Self::Mismatch(_) => "storage_hash_mismatch",
            Self::Quota(_) => "quota_exceeded",
            Self::State(_) => "storage_state_error",
            Self::Unsupported(_) => "storage_unsupported",
            Self::Internal(_) => "internal_error",
        }
    }

    /// 是否为可重试的瞬时失败（供 worker 重试与指标分类）。
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            Self::RateLimited(_) | Self::Upstream(_) | Self::Network(_) | Self::Conflict(_)
        )
    }
}

impl From<sqlx::Error> for StorageError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        match e.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(e.to_string()),
            std::io::ErrorKind::PermissionDenied => Self::Forbidden(e.to_string()),
            std::io::ErrorKind::TimedOut => Self::Network(e.to_string()),
            _ => Self::Internal(e.to_string()),
        }
    }
}

impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "storage db error: {msg}"),
            Self::NotFound(msg) => write!(f, "storage object not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid storage request: {msg}"),
            Self::Forbidden(msg) => write!(f, "storage forbidden: {msg}"),
            Self::Auth(msg) => write!(f, "storage auth failed: {msg}"),
            Self::RateLimited(msg) => write!(f, "storage rate limited: {msg}"),
            Self::Conflict(msg) => write!(f, "storage conflict: {msg}"),
            Self::Upstream(msg) => write!(f, "storage upstream error: {msg}"),
            Self::Network(msg) => write!(f, "storage network error: {msg}"),
            Self::PartialUpload(msg) => write!(f, "storage partial upload: {msg}"),
            Self::Verification(msg) => write!(f, "storage verification failed: {msg}"),
            Self::Mismatch(msg) => write!(f, "storage hash mismatch: {msg}"),
            Self::Quota(msg) => write!(f, "storage quota: {msg}"),
            Self::State(msg) => write!(f, "storage state error: {msg}"),
            Self::Unsupported(msg) => write!(f, "storage unsupported: {msg}"),
            Self::Internal(msg) => write!(f, "storage internal error: {msg}"),
        }
    }
}

impl std::error::Error for StorageError {}
