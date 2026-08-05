//! M03-PROFILE-01：用户三套显式投影 DTO。
//!
//! 约定：
//! - 每个 DTO 只从显式字段构建（`From<SessionUser>` 或数据库行映射），
//!   数据库实体/行不得直接序列化到响应；
//! - 公开投影只含公开字段（allowlist 细化与泄漏测试见 M03-PROFILE-02/09）；
//! - 管理投影只对 `user.manage` 权限返回（M03-AUTHZ 裁决），字段含内部
//!   状态与注销/删除时间，仅用于管理视图。

use serde::Serialize;

use crate::auth::session::SessionUser;

/// 公开用户资料（作者卡 / 公开主页）。对应 OpenAPI `PublicUser`。
///
/// 严格公开 allowlist：不含邮箱、Session、IP、内部处罚、私有资产与审计
/// 信息；`avatar_attachment_id` 只引用附件 UUID（禁止 URL/签名 URL）。
#[derive(Debug, Clone, Serialize)]
pub struct PublicProfile {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub level: i64,
    pub avatar_attachment_id: Option<String>,
    pub signature: Option<String>,
    pub created_at: i64,
}

/// 本人资料（GET/PATCH `/api/v1/me`）。对应 OpenAPI `Me`。
///
/// 只对当前会话用户本人返回；字段集保持与现有契约一致
/// （`signature`/偏好/隐私字段在 M03-PROFILE-03 读取落地后扩展）。
#[derive(Debug, Clone, Serialize)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub status: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub timezone: String,
    pub level: i64,
    pub roles: Vec<String>,
    /// 两步验证（TOTP）是否已启用（M02-UX-06）。
    pub mfa_enabled: bool,
}

impl Me {
    /// 从会话用户显式构建本人投影（避免直接序列化会话/数据库实体）。
    pub fn from_session(
        user: &SessionUser,
        mfa_enabled: bool,
        bio: Option<String>,
        timezone: &str,
    ) -> Self {
        Self {
            id: user.id.clone(),
            username: user.username.clone(),
            email: user.email.clone(),
            email_verified: user.email_verified,
            status: user.status.clone(),
            display_name: user.display_name.clone(),
            bio,
            timezone: timezone.to_string(),
            level: user.level,
            roles: user.roles.clone(),
            mfa_enabled,
        }
    }
}

/// 管理视图（`GET /api/v1/admin/users/{id}` 等）。对应 OpenAPI `AdminUser`。
///
/// 含内部字段（状态、删除/注销时间、最后登录），仅 `user.manage` 权限可读；
/// 不复制 password_hash、恢复码、加密 secret 等凭据。
#[derive(Debug, Clone, Serialize)]
pub struct AdminUser {
    pub id: String,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub status: String,
    pub display_name: Option<String>,
    pub level: i64,
    pub roles: Vec<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub last_login_at: Option<i64>,
    pub delete_requested_at: Option<i64>,
    pub deleted_at: Option<i64>,
}
