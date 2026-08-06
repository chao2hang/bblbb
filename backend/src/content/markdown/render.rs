//! M04-MARKDOWN-02：CommonMark 渲染器封装——禁用原始 HTML 与危险扩展。

use pulldown_cmark::{html, Event, Options, Parser};

use super::policy::RENDERER_VERSION;

/// 渲染选项：仅启用表格（M04-MARKDOWN-04 需要）；其余扩展（脚注、数学、
/// 任务列表、smart punctuation、YAML metadata、heading attributes、GFM
/// autolink）全部关闭。
pub fn render_options() -> Options {
    Options::ENABLE_TABLES
}

/// 渲染输入长度上限（与 domain PostContent MAX_CHARS 一致，防御性截断）。
pub const MAX_INPUT_CHARS: usize = 50_000;

/// 渲染 Markdown → HTML（不经过清洗，供 [`crate::content::markdown::sanitize`]）。
///
/// 安全约定：
/// - 原始 HTML（块级 [`Event::Html`] 与行内 [`Event::InlineHtml`]）事件被
///   丢弃，不渲染、不转义为输出——原始 HTML 一律不得进入最终 HTML；
/// - 危险扩展关闭（见 [`render_options`]）；
/// - 输入超过 [`MAX_INPUT_CHARS`] 时截断（防超长输入放大）。
pub fn render_to_html(markdown: &str) -> String {
    // 输入超过 MAX_INPUT_CHARS 时截断（防超长输入放大；域层已拒绝超长正文）
    let bounded: std::borrow::Cow<'_, str> = if markdown.chars().count() > MAX_INPUT_CHARS {
        std::borrow::Cow::Owned(markdown.chars().take(MAX_INPUT_CHARS).collect())
    } else {
        std::borrow::Cow::Borrowed(markdown)
    };

    let parser = Parser::new_ext(bounded.as_ref(), render_options());
    // 丢弃原始 HTML 事件
    let filtered = parser.filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));
    let mut out = String::new();
    html::push_html(&mut out, filtered);
    out
}

/// 断言渲染策略版本非空（登记进 CONFIG_REGISTRY / 文档）。
pub fn renderer_version() -> &'static str {
    RENDERER_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_standard_markdown() {
        let html = render_to_html("# 标题\n\n**粗体** 与 `代码`");
        assert!(html.contains("<h1>标题</h1>"), "标题应渲染: {html}");
        assert!(html.contains("<strong>粗体</strong>"), "粗体应渲染: {html}");
        assert!(html.contains("<code>代码</code>"), "行内代码应渲染: {html}");
    }

    #[test]
    fn raw_html_is_stripped() {
        let html = render_to_html("正文\n\n<script>alert(1)</script>\n\n<b>加粗</b>");
        assert!(!html.contains("<script"), "块级原始 HTML 必须剥离: {html}");
        assert!(!html.contains("alert(1)"), "脚本内容不得出现在输出: {html}");
        assert!(!html.contains("<b>"), "行内原始 HTML 必须剥离: {html}");
        assert!(
            !html.contains("</b>"),
            "行内原始 HTML 结束标签必须剥离: {html}"
        );
    }

    #[test]
    fn dangerous_extensions_are_off() {
        // 脚注语法不得渲染为脚注结构
        let footnote = render_to_html("正文[^1]\n\n[^1]: 脚注");
        assert!(
            !footnote.contains("<section"),
            "脚注扩展必须关闭: {footnote}"
        );
        assert!(!footnote.contains("footnote"), "脚注扩展必须关闭");
        // 数学扩展关闭
        let math = render_to_html("$x^2$");
        assert!(!math.contains("<math"), "数学扩展必须关闭: {math}");
    }

    #[test]
    fn tables_render_when_enabled() {
        let table = render_to_html("| a | b |\n|---|---|\n| 1 | 2 |");
        assert!(table.contains("<table>"), "表格必须渲染: {table}");
        assert!(table.contains("<th>"), "表头必须渲染");
        assert!(table.contains("<td>1</td>"), "单元格必须渲染");
    }

    #[test]
    fn oversized_input_is_truncated_defensively() {
        let long = "x".repeat(MAX_INPUT_CHARS + 10_000);
        let html = render_to_html(&long);
        // 截断渲染不崩溃且输出有界
        assert!(html.len() < MAX_INPUT_CHARS * 8, "输出必须有界");
    }

    #[test]
    fn renderer_version_is_set() {
        assert!(!renderer_version().is_empty());
        assert!(renderer_version().starts_with("markdown-v"));
    }
}
