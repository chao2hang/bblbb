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
use crate::users::profile::ProfileFields;

/// 公开成就徽章（社交域·成就装备槽投影）：仅 code/name 两个公开字段。
///
/// 来源：`user_achievements.equipped = 1` JOIN `achievements`（仅启用成就），
/// 服务端裁决数量（≤ [`crate::achievements::MAX_EQUIPPED_SLOTS`]）。
/// 已装备 = 用户已解锁并主动佩戴，公开其 code/name 即佩戴语义本身；
/// 不含进度/条件/奖励等内部字段。
#[derive(Debug, Clone, Serialize)]
pub struct PublicEquippedAchievement {
    pub code: String,
    pub name: String,
}

/// 公开用户投影允许的字段（M03-PROFILE-01/02/05）。
///
/// 显式 allowlist，公开投影只能包含这些字段；排除邮箱、IP、Session、
/// 内部处罚（sanction）、私有资产与审计信息。`PublicProfile` 序列化键集
/// 必须与该常量一致（`backend/tests/user_dto.rs` 断言）。
/// GAP-FIX 社交域追加公开社交统计：post_count/followers/following 与
/// 请求方视角的 is_following（匿名恒 false）。
/// 社交域·成就：equipped_achievements 为已装备成就徽章（≤3，服务端裁决）。
pub const PUBLIC_PROFILE_ALLOWLIST: &[&str] = &[
    "id",
    "username",
    "display_name",
    "bio",
    "level",
    "avatar_attachment_id",
    "cover_attachment_id",
    "signature",
    "created_at",
    "post_count",
    "followers",
    "following",
    "is_following",
    "presentation_tokens",
    "equipped_achievements",
];

/// 公开装扮投影（M07-SHOP-SCHEMA-06）：服务端从衣柜装配编译的白名单
/// Token 集合。key 为展示槽位，value 只能是后端注册的白名单 Token 值
/// （`profile_badges` 为字符串数组，≤3）；禁用任意 CSS/HTML/URL。字段
/// 缺省即该槽位未装配，不渲染。封禁/注销中整体置空（降级投影）。
#[derive(Debug, Clone, Default, Serialize)]
pub struct PublicPresentationTokens {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname_color: Option<String>,
    /// 自定义昵称样式（M07-SHOP-UI-10）：样式库定义的名称与结构化样式
    /// （颜色 #rrggbb / 渐变 stops / 动画枚举，服务端校验）。仅样式库
    /// 定义存在；注册枚举色无此字段（前端白名单渲染）。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname_color_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nickname_color_style: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_frame: Option<String>,
    /// 自定义头像框样式（M07-SHOP-UI-10）：样式库名称与结构化参数。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_frame_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_frame_style: Option<serde_json::Value>,
    /// 已发布商品绑定的 ready PNG 附件 UUID；前端通过稳定 content 端点读取。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar_frame_attachment_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_effect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_effect_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_effect_style: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_effect: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_effect_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_effect_style: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_badges: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_badge_names: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_badge_styles: Option<Vec<serde_json::Value>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_prefix: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_prefix_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_prefix_style: Option<serde_json::Value>,
}

impl PublicPresentationTokens {
    pub fn is_empty(&self) -> bool {
        self.nickname_color.is_none()
            && self.avatar_frame.is_none()
            && self.avatar_frame_attachment_id.is_none()
            && self.profile_effect.is_none()
            && self.post_effect.is_none()
            && self.profile_badges.is_none()
            && self.title_prefix.is_none()
    }
}

/// 公开用户资料（作者卡 / 公开主页）。对应 OpenAPI `PublicUser`。
///
/// 严格公开 allowlist：不含邮箱、Session、IP、内部处罚、私有资产与审计
/// 信息；`avatar_attachment_id`/`cover_attachment_id` 只引用附件 UUID
/// （禁止 URL/签名 URL；稳定内容端点 `/api/v1/attachments/{id}`，M6 落地）。
/// 社交统计（GAP-FIX）：post_count/followers/following 为公开计数；
/// is_following 为请求者视角（未登录恒 false）。
#[derive(Debug, Clone, Serialize)]
pub struct PublicProfile {
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub level: i64,
    pub avatar_attachment_id: Option<String>,
    pub cover_attachment_id: Option<String>,
    pub signature: Option<String>,
    pub created_at: i64,
    /// 公开帖子计数（published 且未删除）。
    pub post_count: i64,
    /// 粉丝数（user_follows.followee_id = 本用户）。
    pub followers: i64,
    /// 关注数（user_follows.follower_id = 本用户）。
    pub following: i64,
    /// 请求者是否关注该用户（未登录恒 false）。
    pub is_following: bool,
    /// 公开装扮投影（M07-SHOP-SCHEMA-06）：白名单 Token 集合；无装配/
    /// 降级（banned/pending_delete）时为 null。
    pub presentation_tokens: Option<PublicPresentationTokens>,
    /// 已装备成就徽章（社交域·成就，成就墙装备槽）：≤3，服务端裁决；
    /// 无装备或降级（banned/pending_delete）时为空数组。
    pub equipped_achievements: Vec<PublicEquippedAchievement>,
}

/// 作者资料卡（文章/讨论列表的作者行）。对应 OpenAPI `Author`。
///
/// `profile_url` 为稳定公开主页端点 `/users/{username}`，不返回任何签名/
/// 远程 URL（M03-PROFILE-05）。
#[derive(Debug, Clone, Serialize)]
pub struct Author {
    pub username: String,
    pub display_name: Option<String>,
    pub level: i64,
    pub profile_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation_tokens: Option<PublicPresentationTokens>,
}

impl Author {
    /// 从公开投影构建作者卡。
    pub fn from_public(profile: &PublicProfile) -> Self {
        Self {
            username: profile.username.clone(),
            display_name: profile.display_name.clone(),
            level: profile.level,
            profile_url: format!("/users/{}", profile.username),
            presentation_tokens: profile.presentation_tokens.clone(),
        }
    }
}

/// 本人资料（GET/PATCH `/api/v1/me`）。对应 OpenAPI `Me`。
///
/// 只对当前会话用户本人返回；`signature`/时区/主题/隐私字段来自
/// `user_preferences`/`user_privacy`（M03-PROFILE-03）。
#[derive(Debug, Clone, Serialize)]
pub struct Me {
    pub id: String,
    pub username: String,
    pub email: String,
    pub email_verified: bool,
    pub status: String,
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub signature: Option<String>,
    pub timezone: String,
    pub theme_name: Option<String>,
    pub email_visible_to: String,
    pub profile_visible_to: String,
    pub level: i64,
    pub roles: Vec<String>,
    /// 两步验证（TOTP）是否已启用（M02-UX-06）。
    pub mfa_enabled: bool,
    /// 乐观并发版本（users.version；If-Match 更新来源，M03-PROFILE-04）。
    pub version: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presentation_tokens: Option<PublicPresentationTokens>,
    pub avatar_attachment_id: Option<String>,
}

impl Me {
    /// 从会话用户 + 资料字段显式构建本人投影（避免直接序列化会话/数据库实体）。
    pub fn from_session(
        user: &SessionUser,
        mfa_enabled: bool,
        profile: &ProfileFields,
        presentation_tokens: Option<PublicPresentationTokens>,
    ) -> Self {
        Self {
            id: user.id.clone(),
            username: user.username.clone(),
            email: user.email.clone(),
            email_verified: user.email_verified,
            status: user.status.clone(),
            display_name: profile.display_name.clone(),
            bio: profile.bio.clone(),
            signature: profile.signature.clone(),
            timezone: profile.timezone.clone(),
            theme_name: profile.theme_name.clone(),
            email_visible_to: profile.email_visible_to.clone(),
            profile_visible_to: profile.profile_visible_to.clone(),
            level: user.level,
            roles: user.roles.clone(),
            mfa_enabled,
            version: profile.version,
            presentation_tokens,
            avatar_attachment_id: profile.avatar_attachment_id.clone(),
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
