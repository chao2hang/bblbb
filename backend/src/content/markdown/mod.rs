//! M04-MARKDOWN：安全 Markdown 渲染管线。
//!
//! - [`render::render_to_html`]：CommonMark → HTML（禁用原始 HTML 与危险扩展，
//!   M04-MARKDOWN-02）；
//! - [`sanitize::sanitize_html`]：标签/属性/协议 allowlist + 外链 rel/target +
//!   iframe Provider 白名单（M04-MARKDOWN-03）；
//! - [`policy`]：renderer/sanitizer 策略版本与 iframe Provider 单一事实来源
//!   （M04-MARKDOWN-05 升级触发 Job 重渲染）；
//! - [`excerpt::render_excerpt`]：公开安全摘要（M04-MARKDOWN-06）。

pub mod excerpt;
pub mod policy;
pub mod render;
pub mod sanitize;

/// 完整渲染管线：Markdown → 清洗 HTML。
pub fn render_and_sanitize(markdown: &str) -> String {
    sanitize::sanitize_html(&render::render_to_html(markdown))
}
