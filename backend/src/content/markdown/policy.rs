//! M04-MARKDOWN-05：渲染/清洗策略版本与 iframe Provider 单一事实来源。
//!
//! - `RENDERER_VERSION` / `SANITIZER_VERSION` 落库（post_contents.
//!   renderer_version），升级时由 Job 重渲染旧 revision（M04-MARKDOWN-05）；
//! - `IFRAME_PROVIDERS` 为视频 embed 主机白名单（清洗器唯一来源）。

/// CommonMark 渲染策略版本（升级渲染行为时递增，触发重渲染 Job）。
pub const RENDERER_VERSION: &str = "markdown-v1";

/// 清洗策略版本（升级 allowlist 时递增，触发重渲染 Job）。
pub const SANITIZER_VERSION: &str = "ammonia-v1";

/// 允许的 iframe 视频 Provider（按主机白名单，其他 iframe 一律剥离）。
pub const IFRAME_PROVIDERS: &[&str] = &[
    "www.youtube.com",
    "www.youtube-nocookie.com",
    "player.vimeo.com",
    "player.bilibili.com",
];
