//! M04-MARKDOWN-06：公开安全摘要生成。
//!
//! `render_excerpt` 从 Markdown 提取**纯文本**摘要（不输出任何 HTML 标签，
//! 公开可见）；截断后加省略号。可见性语义（隐藏正文不得截断生成摘要、
//! 摘要生成前先执行可见性判定）由 M04-VISIBILITY 投影层执行——本模块保证
//! 摘要本身为清洗后的纯文本。

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
}
