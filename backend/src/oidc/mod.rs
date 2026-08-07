//! M11-OIDC：OpenID Connect Provider 服务层。
//!
//! 职责边界（与 `routes/oidc.rs` 路由层分离）：
//! - 本模块承载 OAuth/OIDC 协议与数据语义：授权码、PKCE S256、opaque
//!   Access Token、Refresh Token Rotation、RS256 ID Token、pairwise subject、
//!   userinfo 投影、Discovery、JWKS、撤销与 RP-Initiated Logout；
//! - 路由层只做 HTTP 适配（标准 OAuth 错误 JSON，不套业务 Problem 格式）；
//! - 所有高熵 code/token 只存 SHA-256 hash（`crate::auth::token::hash_token`）；
//! - 领域约束：`backend/src/domain/` 之外的普通模块允许 sqlx；协议纯函数
//!   （PKCE/URI 校验/claim 构造）放在 [`protocol`]，可独立单元测试。
//!
//! 安全模型（docs/AUTH-OIDC.md）：
//! - 强制 Authorization Code + PKCE S256；拒绝 implicit/plain/password/device；
//! - redirect/post-logout URI 精确匹配，仅 loopback（localhost 开发）例外；
//! - 授权码一次性消费、过期、client/redirect/request hash 绑定；
//! - Refresh Token 重用 → 撤销整个 family 并通知用户；
//! - 私钥 AES-256-GCM 加密存储，主密钥不可恢复时直接失败。

pub mod clients;
pub mod consent;
pub mod interactions;
pub mod keys;
pub mod protocol;
pub mod tokens;

use serde_json::{json, Value};

use crate::auth::token::hash_token;

// ─────────────────────────── 契约常量 ───────────────────────────

/// 允许的 OIDC scope（docs/AUTH-OIDC.md §10）。
pub const ALLOWED_SCOPES: [&str; 3] = ["openid", "profile", "email"];
/// scope 白名单集合（快速判定）。
pub const SCOPE_SET: &str = "openid profile email";

/// 授权码有效期（5 分钟）。
pub const AUTH_CODE_TTL_MS: i64 = 5 * 60 * 1000;
/// opaque Access Token 有效期（秒，10 分钟）。
pub const ACCESS_TOKEN_TTL_SECS: i64 = 600;
/// Refresh Token 有效期（30 天，绝对期限）。
pub const REFRESH_TOKEN_TTL_MS: i64 = 30 * 24 * 3600 * 1000;
/// consent 交互有效期（15 分钟）。
pub const INTERACTION_TTL_MS: i64 = 15 * 60 * 1000;
/// ID Token 有效期（秒，5 分钟）。
pub const ID_TOKEN_TTL_SECS: i64 = 300;
/// 轮换后旧签名密钥保留余量（24h + 最长 Token 有效期）。
pub const KEY_RETIRE_MARGIN_MS: i64 = 24 * 3600 * 1000;
/// PKCE code_challenge 长度（RFC 7636 §4.2：43–128 个 base64url 字符）。
pub const CODE_CHALLENGE_MIN_LEN: usize = 43;
pub const CODE_CHALLENGE_MAX_LEN: usize = 128;
/// nonce / state 长度上限（防滥用）。
pub const NONCE_MAX_LEN: usize = 256;
pub const STATE_MAX_LEN: usize = 512;

/// OIDC 服务层错误：协议端点统一映射为标准 OAuth 错误 JSON。
#[derive(Debug)]
pub enum OidcError {
    /// 400 `invalid_request`
    InvalidRequest(String),
    /// 401 `invalid_client`
    InvalidClient(String),
    /// 400 `invalid_grant`
    InvalidGrant(String),
    /// 403 `access_denied`（authorize 场景）
    AccessDenied(String),
    /// 404 业务资源不存在（interaction/admin）
    NotFound(String),
    /// 500 `server_error`
    ServerError(String),
    /// 数据库错误（内部）
    Db(String),
}

impl std::fmt::Display for OidcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OidcError::InvalidRequest(m) => write!(f, "invalid_request: {m}"),
            OidcError::InvalidClient(m) => write!(f, "invalid_client: {m}"),
            OidcError::InvalidGrant(m) => write!(f, "invalid_grant: {m}"),
            OidcError::AccessDenied(m) => write!(f, "access_denied: {m}"),
            OidcError::NotFound(m) => write!(f, "not_found: {m}"),
            OidcError::ServerError(m) => write!(f, "server_error: {m}"),
            OidcError::Db(m) => write!(f, "database: {m}"),
        }
    }
}

impl std::error::Error for OidcError {}

impl From<sqlx::Error> for OidcError {
    fn from(e: sqlx::Error) -> Self {
        OidcError::Db(e.to_string())
    }
}

/// 标准 OAuth/OIDC 错误响应体（协议端点专用，不套业务 Problem 格式）。
///
/// `error_description` 只包含协议安全语义，不泄漏账号状态/Token 存在性等内部信息。
pub fn oauth_error_body(error: &str, description: &str) -> Value {
    json!({
        "error": error,
        "error_description": description,
    })
}

/// 计算请求摘要（SHA-256 hex）。
pub fn sha256_hex(input: &str) -> String {
    hash_token(input)
}

/// 当前 Unix 毫秒。
pub fn now_millis() -> i64 {
    crate::outbox::now_millis()
}
