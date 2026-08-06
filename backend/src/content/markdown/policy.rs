//! M04-MARKDOWN-05：渲染/清洗策略版本与 iframe Provider 单一事实来源。
//!
//! - `RENDERER_VERSION` / `SANITIZER_VERSION` 落库（post_contents.
//!   renderer_version），升级时由 Job 重渲染旧 revision（M04-MARKDOWN-05）；
//! - `IFRAME_PROVIDERS` 为视频 embed 主机白名单（清洗器唯一来源）。

/// CommonMark 渲染策略版本（升级渲染行为时递增，触发重渲染 Job）。
pub const RENDERER_VERSION: &str = "markdown-v1";

/// 清洗策略版本（升级 allowlist 时递增，触发重渲染 Job）。
pub const SANITIZER_VERSION: &str = "ammonia-v1";

/// 当前策略组合版本，写入 post_contents/post_revisions.renderer_version。
///
/// renderer 或 sanitizer 任一升级都会改变该值，使存量行判定为 stale，
/// 由 [`super::rerender`] 的重渲染 Job 覆盖为新版本渲染结果。
pub fn policy_version() -> String {
    format!("{RENDERER_VERSION}+{SANITIZER_VERSION}")
}

/// 允许的 iframe 视频 Provider（按主机白名单，其他 iframe 一律剥离）。
pub const IFRAME_PROVIDERS: &[&str] = &[
    "www.youtube.com",
    "www.youtube-nocookie.com",
    "player.vimeo.com",
    "player.bilibili.com",
];

// ---- M04-MARKDOWN-04：确定性输出与上限 ----
//
// 所有上限均为“渲染输出确定性”的一部分：同一输入永远得到同一输出；
// 超限内容被裁剪或展平，但绝不产生非法/非确定 HTML。

/// 标题锚点 slug 最大长度（超过截断；截断后仍按去重规则生成唯一 id）。
pub const MAX_HEADING_SLUG_CHARS: usize = 64;

/// 单个代码块最大字符数（超长代码块按 char 截断，保留开头）。
pub const MAX_CODE_BLOCK_CHARS: usize = 20_000;

/// blockquote 最大嵌套深度（超出层级的引用对整体展平，内容保留）。
pub const MAX_BLOCKQUOTE_DEPTH: usize = 10;

/// 块级容器（blockquote + list）最大总嵌套深度（超深层级展平，防病态嵌套）。
pub const MAX_BLOCK_NESTING: usize = 64;

/// 行内元素（emphasis/strong/strikethrough/link/image）最大嵌套深度。
pub const MAX_INLINE_NESTING: usize = 16;

/// 表格最大列数（超出列丢弃单元格及其内容，保留行结构）。
pub const MAX_TABLE_COLUMNS: usize = 20;

/// 单个表格单元格内容最大字符数（超出截断，保留开头）。
pub const MAX_TABLE_CELL_CHARS: usize = 5_000;

/// 渲染输出总长估算上限（防御性兜底：达到后停止继续渲染事件，
/// 交由清洗器补齐结构；主上限由以上各项构成）。
pub const MAX_RENDERED_CHARS: usize = 300_000;
