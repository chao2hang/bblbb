//! M04-MARKDOWN：安全 Markdown 渲染管线。
//!
//! - [`render::render_to_html`]：CommonMark → HTML（禁用原始 HTML 与危险扩展，
//!   M04-MARKDOWN-02）；
//! - [`sanitize::sanitize_html`]：标签/属性/协议 allowlist + 外链 rel/target +
//!   iframe Provider 白名单（M04-MARKDOWN-03）；
//! - `render_and_sanitize` 管线末端做 **@提及链接化**（M05-NOTIFY-10，见
//!   [`crate::content::mentions`]）：清洗后 HTML 文本节点中的 `@username`
//!   转为 `/users/{username}` 资料页锚点；
//! - [`policy`]：renderer/sanitizer 策略版本与 iframe Provider 单一事实来源
//!   （M04-MARKDOWN-05 升级触发 Job 重渲染）；
//! - [`excerpt::render_excerpt`]：公开安全摘要（M04-MARKDOWN-06）。

pub mod excerpt;
pub mod policy;
pub mod render;
pub mod rerender;
pub mod sanitize;

use crate::content::mentions::linkify_mentions;

/// 完整渲染管线：Markdown → 清洗 HTML → @提及 链接化（M05-NOTIFY-10）。
///
/// 提及链接化在清洗**之后**执行：只重写文本节点中的 `@username` 为
/// `/users/{username}` 资料页锚点（锚点完全由 [`crate::content::mentions`]
/// 构造，不引入用户可控属性/协议；清洗 allowlist 与相对 URL 拒绝契约不变）。
pub fn render_and_sanitize(markdown: &str) -> String {
    linkify_mentions(&sanitize::sanitize_html(&render::render_to_html(markdown)))
}
