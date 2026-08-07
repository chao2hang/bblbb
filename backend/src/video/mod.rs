//! Video 插件领域（M10-VIDEO）：Direct/HLS/Xigua Provider Adapter 与服务。
//!
//! 模块划分：
//! - [`classify`]：URL 解析与分类（scheme/host/port/userinfo/Unicode-IDN/
//!   私网 IPv4/IPv6 字面量/签名 URL/扩展名，任务 02/03/04）。
//! - [`egress`]：受控出站边界（重定向计数、DNS 重绑定 IP 复核、响应大小、
//!   超时）。网络访问一律经 [`egress::FetchClient`] 抽象，测试用 mock
//!   （与 M09-GATEWAY 的 `ProviderClient` 同模式，任务 04/10）。
//! - [`hls`]：HLS playlist 解析限制（深度/分片数/总时长/Key/Map/跨域/
//!   签名泄漏，任务 05/10）。
//! - [`xigua`]：西瓜官方公开页面 Host 白名单与视频 id 提取（任务 06）。
//! - [`csp`]：动态 CSP frame-src/media-src/connect-src + sandbox/
//!   referrerpolicy/allow 最小权限（任务 07）。
//! - [`policy`]：`video_provider_policies` 行模型与限制校验（任务 04/12）。
//! - [`provider`]：`VideoProvider` Adapter trait 与 Direct/Hls/Xigua 内置
//!   适配器；领域层不依赖具体 Provider SDK（任务 01）。
//! - [`resolution`]：resolve 的短效一次性 resolution_id 存储（任务 03）。
//! - [`state`]：`video_embeds` 状态机 pending/ready/blocked/error/removed、
//!   异步 refresh 与历史引用重检查（任务 08/09/11）。
//!
//! 安全边界（docs/VIDEO-PLUGIN.md §3）：resolve 不信任客户端——source 只存
//! 规范化官方 URL；签名 URL、HLS Key/Map、iframe HTML 一律拒绝且永不回显。
//! 领域层不依赖 axum；数据库访问走 `&crate::db::DatabasePool`。

pub mod classify;
pub mod csp;
pub mod egress;
pub mod hls;
pub mod policy;
pub mod provider;
pub mod resolution;
pub mod state;
pub mod xigua;

pub use classify::{classify, is_allowed_host, validate_url_shape, Classified, ClassifyError};
pub use csp::{render_for, CspDirectives, RenderPolicy};
pub use egress::{
    egress_validate, EgressError, EgressLimits, FetchClient, FetchError, FetchRequest,
    FetchedResponse, UnavailableClient,
};
pub use hls::{HlsError, HlsLimits, HlsReport};
pub use policy::{validate_host_list, VideoPolicy};
pub use provider::{
    DirectProvider, HlsProvider, ProviderRegistry, RefreshInput, RefreshOutcome, VideoProvider,
    XiguaProvider,
};
pub use resolution::{consume_resolution, issue_resolution, ResolutionRecord};
pub use state::{
    create_embed, delete_embed, get_embed, load_policy, recheck_references, refresh_embed,
    resolve_source, update_embed, update_provider_policy, valid_target_type, EmbedView,
    ResolvedView, VideoTarget,
};
pub use xigua::XiguaHosts;

/// 内建 Provider 枚举（`video_embeds.provider` / `video_provider_policies.provider` 值域）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Provider {
    Direct,
    Hls,
    Xigua,
}

impl Provider {
    pub const ALL: [Provider; 3] = [Provider::Direct, Provider::Hls, Provider::Xigua];

    pub fn as_str(self) -> &'static str {
        match self {
            Provider::Direct => "direct",
            Provider::Hls => "hls",
            Provider::Xigua => "xigua",
        }
    }

    pub fn parse(value: &str) -> Option<Provider> {
        match value {
            "direct" => Some(Provider::Direct),
            "hls" => Some(Provider::Hls),
            "xigua" => Some(Provider::Xigua),
            _ => None,
        }
    }
}

impl std::fmt::Display for Provider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 分辨率一次性短效窗口（resolve 后 create 必须在窗口内消费）。
pub const RESOLUTION_TTL_MS: i64 = 10 * 60 * 1000;

/// Video Service 稳定错误（Problem `code` 与 `video_embeds.error_class` 共用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VideoError {
    // 分类（resolve 阶段）
    Classify(ClassifyError),
    // 策略
    ProviderDisabled,
    HostNotAllowed(String),
    PolicyVersionConflict { expected: i64, current: i64 },
    // 目标权限
    TargetNotFound,
    TargetForbidden,
    TargetConflict,
    PosterAttachmentInvalid,
    // 分辨率
    ResolutionExpired,
    // embed
    EmbedNotFound,
    EmbedReferenced,
    VersionConflict { expected: i64, current: i64 },
    // egress / 传输
    EgressTimeout,
    EgressTooLarge(i64),
    EgressTooManyRedirects,
    EgressPrivateIp(String),
    EgressUnavailable,
    EgressHttp { status: u16 },
    // Provider 结果
    MimeMismatch(String),
    Takedown,
    ProviderRatelimited,
    NoEmbedPermission,
    ProviderUnavailable(String),
    PolicyChanged,
    // HLS
    Hls(HlsError),
    // 请求/策略参数校验失败（400）
    Invalid(String),
    // 存储/内部
    Db(String),
    Internal(String),
}

impl VideoError {
    /// 稳定错误码（Problem `code`；同时写入 `video_embeds.error_class`）。
    pub fn code(&self) -> &'static str {
        match self {
            VideoError::Classify(e) => e.code(),
            VideoError::ProviderDisabled => "video_provider_disabled",
            VideoError::HostNotAllowed(_) => "video_provider_host_not_allowed",
            VideoError::PolicyVersionConflict { .. } => "video_policy_version_conflict",
            VideoError::TargetNotFound => "video_target_not_found",
            VideoError::TargetForbidden => "video_target_forbidden",
            VideoError::TargetConflict => "video_target_conflict",
            VideoError::PosterAttachmentInvalid => "video_poster_attachment_invalid",
            VideoError::ResolutionExpired => "video_resolution_expired",
            VideoError::EmbedNotFound => "video_embed_not_found",
            VideoError::EmbedReferenced => "video_embed_referenced",
            VideoError::VersionConflict { .. } => "video_version_conflict",
            VideoError::EgressTimeout => "video_egress_timeout",
            VideoError::EgressTooLarge(_) => "video_egress_too_large",
            VideoError::EgressTooManyRedirects => "video_egress_too_many_redirects",
            VideoError::EgressPrivateIp(_) => "video_egress_private_ip",
            VideoError::EgressUnavailable => "video_egress_unavailable",
            VideoError::EgressHttp { .. } => "video_egress_http_error",
            VideoError::MimeMismatch(_) => "video_mime_mismatch",
            VideoError::Takedown => "video_takedown",
            VideoError::ProviderRatelimited => "video_provider_ratelimited",
            VideoError::NoEmbedPermission => "video_no_embed_permission",
            VideoError::ProviderUnavailable(_) => "video_provider_unavailable",
            VideoError::PolicyChanged => "video_policy_changed",
            VideoError::Hls(e) => e.class(),
            VideoError::Invalid(_) => "video_invalid",
            VideoError::Db(_) => "internal_error",
            VideoError::Internal(_) => "internal_error",
        }
    }
}

impl From<ClassifyError> for VideoError {
    fn from(e: ClassifyError) -> Self {
        VideoError::Classify(e)
    }
}

impl From<sqlx::Error> for VideoError {
    fn from(e: sqlx::Error) -> Self {
        VideoError::Db(e.to_string())
    }
}
