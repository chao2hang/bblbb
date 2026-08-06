//! M04-MARKDOWN-06：公开安全摘要生成。
//!
//! [`render_excerpt`] 从 Markdown 提取**纯文本**摘要（不输出任何 HTML 标签，
//! 公开可见）；截断后加省略号。[`render_public_excerpt`] 在此之上执行**可见性
//! 语义**：摘要只从公开正文生成，隐藏正文（`restricted_markdown`）无论多长
//! 都不参与截断与拼接——这是 M04-VISIBILITY 投影层显示摘要时调用的入口。

use pulldown_cmark::{Event, Parser};

use super::render::{render_options, MAX_INPUT_CHARS};

/// 摘要最大字符数（公开可见，Unicode char 计）。
pub const EXCERPT_MAX_CHARS: usize = 300;

/// 从 Markdown 生成纯文本摘要。
///
/// - 提取文本/行内代码内容（标题、段落、列表项文本）；
/// - 结构性标记剔除：链接目标（`[文字](url)` 只取文字）、图片（取 alt 文字）、
///   表格/引用/代码块不产生 HTML 标签；
/// - 输出**纯文本**（绝不含 `<`/`>` 标签结构）；
/// - 超过 [`EXCERPT_MAX_CHARS`] 截断并追加 `…`。
pub fn render_excerpt(markdown: &str) -> String {
    let bounded: std::borrow::Cow<'_, str> = if markdown.chars().count() > MAX_INPUT_CHARS {
        std::borrow::Cow::Owned(markdown.chars().take(MAX_INPUT_CHARS).collect())
    } else {
        std::borrow::Cow::Borrowed(markdown)
    };

    let parser = Parser::new_ext(bounded.as_ref(), render_options());
    let mut out = String::new();
    let mut truncated = false;

    for event in parser {
        match event {
            Event::Text(t) | Event::Code(t) => {
                for ch in t.chars() {
                    if out.chars().count() >= EXCERPT_MAX_CHARS {
                        truncated = true;
                        break;
                    }
                    out.push(ch);
                }
            }
            Event::SoftBreak | Event::HardBreak if !out.is_empty() && !out.ends_with(' ') => {
                out.push(' ');
            }
            _ => {}
        }
        if truncated {
            break;
        }
    }

    let trimmed = out.trim_end().to_string();
    if truncated || trimmed.chars().count() >= EXCERPT_MAX_CHARS {
        let mut base: String = trimmed.chars().take(EXCERPT_MAX_CHARS).collect();
        base.push('…');
        base
    } else {
        trimmed
    }
}

/// 生成**公开**安全摘要（M04-MARKDOWN-06 入口，投影层展示时调用）。
///
/// 可见性语义：
/// 1. **先判定可见性**：摘要只源于公开正文 `body_markdown`；隐藏正文
///    `restricted_markdown` 无论多长、即使公开正文为空，都**绝不**参与
///    截断与拼接——公开正文为空（或仅空白）时返回空摘要，不回落到隐藏
///    正文；
/// 2. **Markdown 安全处理**：复用 [`render_excerpt`] 的纯文本提取（原始
///    HTML 事件剔除、链接目标剔除、标签结构不输出），摘要公开可见。
pub fn render_public_excerpt(body_markdown: &str, restricted_markdown: Option<&str>) -> String {
    if body_markdown.trim().is_empty() {
        return String::new();
    }
    // 结构保证：摘要输入永远是公开正文；隐藏正文仅作为签名参数明确语义
    // 边界（调用方必须显式传入两部分，防误用隐藏正文生成摘要）。
    let _ = restricted_markdown;
    render_excerpt(body_markdown)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_text() {
        let excerpt =
            render_excerpt("## 标题\n\n这是**一段**正文，带 [链接](https://example.com)。");
        assert!(!excerpt.contains('<'), "摘要不得含 HTML 标签: {excerpt}");
        assert!(!excerpt.contains('>'), "摘要不得含 HTML 标签");
        assert!(excerpt.contains("标题"), "标题文字应提取");
        assert!(excerpt.contains("这是"), "正文应提取");
        assert!(excerpt.contains("链接"), "链接文字应提取");
        assert!(!excerpt.contains("https://"), "链接目标不得进摘要");
    }

    #[test]
    fn truncates_with_ellipsis() {
        let long = "词".repeat(500);
        let excerpt = render_excerpt(&long);
        assert!(excerpt.ends_with('…'), "截断必须加省略号");
        assert!(excerpt.chars().count() <= EXCERPT_MAX_CHARS + 1);
    }

    #[test]
    fn short_text_untouched() {
        let excerpt = render_excerpt("短文本");
        assert_eq!(excerpt, "短文本");
        assert!(!excerpt.ends_with('…'));
    }

    #[test]
    fn raw_html_never_leaks_into_excerpt() {
        let excerpt = render_excerpt("正文 <script>alert(1)</script>");
        assert!(
            !excerpt.contains("script"),
            "原始 HTML 不得进摘要: {excerpt}"
        );
        assert!(!excerpt.contains('<'));
        assert!(!excerpt.contains('>'));
    }

    // ---- M04-MARKDOWN-06：公开摘要可见性语义 ----

    #[test]
    fn public_excerpt_comes_only_from_public_body() {
        let excerpt = render_public_excerpt("公开正文开头", Some("隐藏正文很长很长"));
        assert!(
            excerpt.contains("公开正文"),
            "摘要应来自公开正文: {excerpt}"
        );
        assert!(
            !excerpt.contains("隐藏正文"),
            "隐藏正文不得进入摘要: {excerpt}"
        );
    }

    #[test]
    fn public_excerpt_never_falls_back_to_hidden_body() {
        // 公开正文为空、隐藏正文很长 → 摘要为空，绝不从隐藏正文截断
        let empty = render_public_excerpt("", Some(&"隐藏内容".repeat(500)));
        assert!(empty.is_empty(), "空公开正文必须返回空摘要: {empty}");
        let whitespace = render_public_excerpt("   \n\t ", Some("隐藏内容"));
        assert!(
            whitespace.is_empty(),
            "仅空白的公开正文必须返回空摘要: {whitespace}"
        );
    }

    #[test]
    fn public_excerpt_is_safe_plain_text() {
        let excerpt = render_public_excerpt("<script>alert(1)</script> 公开 `<b>代码</b>`", None);
        assert!(!excerpt.contains('<'), "摘要不得含标签结构: {excerpt}");
        assert!(!excerpt.contains("script"), "原始 HTML 不得进摘要");
    }

    #[test]
    fn public_excerpt_truncates_with_ellipsis() {
        let body = "词".repeat(500);
        let excerpt = render_public_excerpt(&body, Some("隐藏"));
        assert!(excerpt.ends_with('…'), "超长公开正文截断加省略号");
        assert!(excerpt.chars().count() <= EXCERPT_MAX_CHARS + 1);
    }
}
