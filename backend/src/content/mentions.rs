//! @提及（mention）：正文中的 `@用户名` 渲染为用户资料页链接，并为通知
//! 提供提及解析（M05-NOTIFY-10）。
//!
//! 设计约定：
//!
//! - **渲染**（[`linkify_mentions`]）作用于**清洗后**的 HTML（完整管线
//!   `render_to_html → sanitize_html → linkify_mentions`，见
//!   [`super::markdown::render_and_sanitize`]）。只重写文本节点：跳过标签
//!   内部（属性值不受影响）与 `<code>`/`<pre>`/`<a>` 内部文本；生成的锚点
//!   完全由本模块构造（href 仅含 `/users/` + `[a-z0-9_-]`），不引入用户可
//!   控的属性或协议，不放松清洗 allowlist（相对 URL 拒绝契约不变）。
//! - **解析**（[`extract_mentions`]）作用于 **Markdown 原文**，经
//!   pulldown-cmark 事件流过滤：跳过代码块/行内代码与链接文本（与渲染端
//!   跳过 `<code>`/`<pre>`/`<a>` 的可见语义一致），输出规范化（小写、去重、
//!   限量）后的用户名列表，供通知解析收件人。
//! - **提及语法**：`@` + 3..=20 个 `[A-Za-z0-9_-]`；`@` 前一个字符不得是
//!   用户名字符（排除 `mail@example.com` 这类邮箱形态）；用户名整体贪心
//!   匹配（`@alice-bob` 是一个整体，不拆成 `@alice`）。显示文本保持原样
//!   （`@Alice` 链接到 `/users/alice`，`username_normalized` 即小写约定）。
//! - **不校验存在性**：渲染是纯函数（无 DB 访问），不存在的用户链接到
//!   404 资料页，与"通知只发给真实存在的用户"（[`crate::notifications`]
//!   解析）解耦；渲染输出对同一输入严格确定。

/// 提及用户名最小长度（与注册校验一致：3..=20，letters/digits/`_`/`-`）。
pub const MENTION_USERNAME_MIN: usize = 3;

/// 提及用户名最大长度（与注册校验一致）。
pub const MENTION_USERNAME_MAX: usize = 20;

/// 单条内容最多通知的提及用户数（超出部分只渲染链接，不创建通知）。
pub const MAX_MENTIONS_PER_CONTENT: usize = 10;

/// 用户名字符集（ASCII 字母/数字/下划线/连字符）。
fn is_mention_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '-'
}

/// 在一段文本中找出 `@username` 提及的 `(起始字节, 用户名字节串)`。
///
/// 边界规则：`@` 的前一个字符存在且属于用户名字符集 → 不是提及（邮箱
/// `mail@example.com`）；用户名贪心取 `[A-Za-z0-9_-]{3,20}`，长度越界
/// （过短或超过 20，如哈希/长 token）→ 不是提及。
fn find_mentions(text: &str) -> Vec<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'@' {
            i += 1;
            continue;
        }
        // 前一个字节必须是"非用户名字符"（或文本起点），排除邮箱形态。
        // text 是合法 UTF-8：`@` 是 ASCII，其前一个 ASCII 字节（若有）就是
        // 完整字符边界；非 ASCII 前导字节（如中文）不属于用户名字符集。
        let prev_ok = i == 0 || {
            let prev = bytes[i - 1];
            !(prev as char).is_ascii_alphanumeric() && prev != b'_' && prev != b'-'
        };
        if prev_ok {
            let mut end = i + 1;
            while end < bytes.len() && is_mention_char(bytes[end] as char) {
                end += 1;
            }
            let len = end - (i + 1);
            if (MENTION_USERNAME_MIN..=MENTION_USERNAME_MAX).contains(&len) {
                out.push((i, end));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

/// 将清洗后 HTML 文本节点中的 `@username` 替换为资料页链接。
///
/// 跳过：标签内部（`<...>` 与其中的引号属性）、`<code>`/`<pre>`/`<a>` 的
/// 内部文本（行内代码/代码块/既有链接不二次包装）。输入必须是本管线
/// [`super::markdown::sanitize_html`] 的产物（well-formed，无裸 `<`/`&`）。
/// 用户名字符集不含 HTML 特殊字符，锚点无需再转义。
pub fn linkify_mentions(html: &str) -> String {
    if !html.contains('@') {
        return html.to_string();
    }
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len() + 64);
    let mut skip_depth = 0usize; // code/pre/a 嵌套深度（内容跳过）
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // 标签区域：整体拷贝；解析标签名维护跳过深度。
            let start = i;
            i += 1;
            // 引号属性值可能包含 `>`（氨水输出已转义 `<`，`>` 保守处理）
            let mut quote: Option<u8> = None;
            while i < bytes.len() {
                let b = bytes[i];
                if let Some(q) = quote {
                    if b == q {
                        quote = None;
                    }
                } else if b == b'"' || b == b'\'' {
                    quote = Some(b);
                } else if b == b'>' {
                    break;
                }
                i += 1;
            }
            if i < bytes.len() {
                i += 1; // 消费 '>'
            }
            let tag = &html[start..i.min(html.len())];
            let lower = tag.to_ascii_lowercase();
            let closing = lower.starts_with("</");
            let name = lower
                .trim_start_matches('/')
                .trim_start_matches('<')
                .split(|c: char| !c.is_ascii_alphanumeric())
                .find(|s| !s.is_empty())
                .unwrap_or("");
            if matches!(name, "code" | "pre" | "a") {
                if closing {
                    skip_depth = skip_depth.saturating_sub(1);
                } else if !lower.ends_with("/>") {
                    skip_depth += 1;
                }
            }
            out.push_str(tag);
            continue;
        }
        if skip_depth > 0 {
            // 跳过 code/pre/a 内部文本：原样拷贝到下一个 '<'
            let next = bytes[i..]
                .iter()
                .position(|&b| b == b'<')
                .map(|p| i + p)
                .unwrap_or(bytes.len());
            out.push_str(&html[i..next]);
            i = next;
            continue;
        }
        // 文本节点区域
        let next = bytes[i..]
            .iter()
            .position(|&b| b == b'<')
            .map(|p| i + p)
            .unwrap_or(bytes.len());
        let text = &html[i..next];
        let mentions = find_mentions(text);
        if mentions.is_empty() {
            out.push_str(text);
        } else {
            let mut last = 0usize;
            for (start, end) in mentions {
                out.push_str(&text[last..start]);
                let token = &text[start + 1..end]; // 去掉 '@'
                let username = token.to_ascii_lowercase();
                out.push_str("<a href=\"/users/");
                out.push_str(&username);
                out.push_str("\" class=\"mention\">@");
                out.push_str(token);
                out.push_str("</a>");
                last = end;
            }
            out.push_str(&text[last..]);
        }
        i = next;
    }
    out
}

/// 从 Markdown 原文解析提及用户名（规范化小写、按出现顺序去重、限量
/// [`MAX_MENTIONS_PER_CONTENT`]），供通知解析收件人。
///
/// 经 pulldown-cmark 事件流过滤：跳过代码块/行内代码（[`Event::Code`]/
/// [`Tag::CodeBlock`]）与链接文本（[`Tag::Link`]）——与渲染端跳过
/// `<code>`/`<pre>`/`<a>` 的可见语义一致。
pub fn extract_mentions(markdown: &str) -> Vec<String> {
    use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

    let mut names: Vec<String> = Vec::new();
    let mut in_link = 0usize;
    let mut in_code_block = 0usize;
    let parser = Parser::new_ext(markdown, Options::ENABLE_TABLES);
    for event in parser {
        match event {
            Event::Start(Tag::Link { .. }) => in_link += 1,
            Event::End(TagEnd::Link) => in_link = in_link.saturating_sub(1),
            Event::Start(Tag::CodeBlock(_)) => in_code_block += 1,
            Event::End(TagEnd::CodeBlock) => in_code_block = in_code_block.saturating_sub(1),
            Event::Code(_) => {}
            Event::Text(t) if in_link == 0 && in_code_block == 0 => {
                for (start, end) in find_mentions(&t) {
                    let token = &t[start + 1..end];
                    let username = token.to_ascii_lowercase();
                    if !names.contains(&username) {
                        if names.len() >= MAX_MENTIONS_PER_CONTENT {
                            return names;
                        }
                        names.push(username);
                    }
                }
            }
            _ => {}
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- find_mentions 边界规则 ----

    #[test]
    fn mention_boundaries() {
        let r = find_mentions("hi @alice!");
        assert_eq!(r, vec![(3, 9)], "空格后 @、标点结尾是提及");
        assert!(
            find_mentions("mail@example.com").is_empty(),
            "邮箱形态不匹配"
        );
        assert!(
            find_mentions("x@alice").is_empty(),
            "@ 前是用户名字符 → 邮箱"
        );
        assert_eq!(find_mentions("(@bob)"), vec![(1, 5)], "括号后可提及");
        assert!(find_mentions("@ab").is_empty(), "过短（<3）");
        let long = format!("@{}", "a".repeat(21));
        assert!(find_mentions(&long).is_empty(), "过长（>20）不是提及");
        let max = format!("@{}", "a".repeat(20));
        assert_eq!(find_mentions(&max).len(), 1, "恰好 20 合法");
        assert!(
            find_mentions("你好@alice").len() == 1,
            "@ 前是中文（非用户名字符）→ 是提及"
        );
        assert!(
            find_mentions("你好 @小明").is_empty(),
            "中文字符不构成用户名（字符集外）"
        );
    }

    #[test]
    fn mention_matches_registered_username_charset() {
        // 与注册校验一致：letters/digits/_/-，3..=20
        assert_eq!(find_mentions("@alice_1-x"), vec![(0, 10)]);
    }

    // ---- linkify_mentions ----

    fn link(markdown: &str) -> String {
        // 模拟完整管线前两步：渲染 + 清洗（提及链接化之前）
        crate::content::markdown::sanitize::sanitize_html(
            &crate::content::markdown::render::render_to_html(markdown),
        )
    }

    #[test]
    fn linkify_wraps_mentions_with_profile_links() {
        let html = link("hi @Alice!\n");
        let out = linkify_mentions(&html);
        assert!(
            out.contains("<a href=\"/users/alice\" class=\"mention\">@Alice</a>"),
            "显示保持原样、href 小写: {out}"
        );
    }

    #[test]
    fn linkify_skips_emails_and_code_and_links() {
        let out = linkify_mentions("<p>mail@example.com 好</p>");
        assert!(!out.contains("<a href"), "邮箱不链接: {out}");

        let code = link("`@alice`");
        let out = linkify_mentions(&code);
        assert!(out.contains("<code>@alice</code>"), "行内代码不链接: {out}");
        assert!(!out.contains("class=\"mention\""), "行内代码不链接: {out}");

        let pre = link("```\n@alice\n```");
        let out = linkify_mentions(&pre);
        assert!(!out.contains("class=\"mention\""), "代码块不链接: {out}");

        let anchor = link("[@alice](https://ok.example/)");
        let out = linkify_mentions(&anchor);
        assert!(
            out.matches("<a").count() == 1,
            "既有链接文本不二次包装: {out}"
        );
    }

    #[test]
    fn linkify_keeps_mentions_in_tag_attributes_untouched() {
        let out =
            linkify_mentions("<img src=\"https://a.example/@alice/i.png\" alt=\"@alice\">@bob");
        assert!(
            out.contains("src=\"https://a.example/@alice/i.png\""),
            "属性不动: {out}"
        );
        assert!(!out.contains("alt=\"<a"), "属性不动: {out}");
        assert!(
            out.ends_with("<a href=\"/users/bob\" class=\"mention\">@bob</a>"),
            "文本提及仍链接: {out}"
        );
    }

    #[test]
    fn linkify_is_deterministic_and_bounded() {
        let html = link("@alice @alice @alice\n");
        let a = linkify_mentions(&html);
        let b = linkify_mentions(&html);
        assert_eq!(a, b, "同一输入严格确定");
        assert_eq!(a.matches("class=\"mention\"").count(), 3);
    }

    // ---- extract_mentions ----

    #[test]
    fn extract_is_normalized_deduped_and_capped() {
        let names = extract_mentions("@Alice 和 @alice 与 @Bob_1 你好 @bob_1");
        assert_eq!(names, vec!["alice", "bob_1"], "小写去重、保持出现顺序");

        let many: Vec<String> = (0..15).map(|i| format!("@user{i:02}")).collect();
        let text = many.join(" ");
        assert_eq!(
            extract_mentions(&text).len(),
            MAX_MENTIONS_PER_CONTENT,
            "超出上限截断"
        );
    }

    #[test]
    fn extract_skips_code_and_links() {
        assert!(
            extract_mentions("看代码 `@alice` 就知道").is_empty(),
            "行内代码不通知"
        );
        assert!(
            extract_mentions("```\n@alice\n```").is_empty(),
            "代码块不通知"
        );
        assert!(
            extract_mentions("链接里 [@alice](https://x.example) 不算").is_empty(),
            "链接文本不通知"
        );
        assert_eq!(
            extract_mentions("正文 @alice 与邮箱 a@bob.com"),
            vec!["alice"],
            "邮箱不通知"
        );
    }
}
