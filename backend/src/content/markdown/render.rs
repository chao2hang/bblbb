//! M04-MARKDOWN-02/04：CommonMark 渲染器封装——禁用原始 HTML 与危险扩展，
//! 并对标题锚点、代码块、引用、表格和超长嵌套设置**确定性输出与上限**。

use std::collections::HashMap;

use pulldown_cmark::{html, Event, Options, Parser, Tag, TagEnd};

use super::policy::{
    MAX_BLOCKQUOTE_DEPTH, MAX_BLOCK_NESTING, MAX_CODE_BLOCK_CHARS, MAX_HEADING_SLUG_CHARS,
    MAX_INLINE_NESTING, MAX_RENDERED_CHARS, MAX_TABLE_CELL_CHARS, MAX_TABLE_COLUMNS,
    RENDERER_VERSION,
};

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
/// 安全与确定性约定：
/// - 原始 HTML（块级 [`Event::Html`] 与行内 [`Event::InlineHtml`]）事件被
///   丢弃，不渲染、不转义为输出——原始 HTML 一律不得进入最终 HTML；
/// - 危险扩展关闭（见 [`render_options`]）；
/// - 输入超过 [`MAX_INPUT_CHARS`] 时截断（防超长输入放大）；
/// - 标题注入确定性锚点 id（slug 去重，见 [`heading_slug`]）；
/// - 代码块 / 表格单元格按 char 截断到上限；
/// - blockquote 深度、块级/行内嵌套深度超限时展平（保留内容，裁剪结构）；
/// - 表格超出列数上限的单元格整体丢弃；
/// - 输出总量达到 [`MAX_RENDERED_CHARS`] 估算上限后停止继续渲染（防御兜底）。
pub fn render_to_html(markdown: &str) -> String {
    // 输入超过 MAX_INPUT_CHARS 时截断（防超长输入放大；域层已拒绝超长正文）
    let bounded: std::borrow::Cow<'_, str> = if markdown.chars().count() > MAX_INPUT_CHARS {
        std::borrow::Cow::Owned(markdown.chars().take(MAX_INPUT_CHARS).collect())
    } else {
        std::borrow::Cow::Borrowed(markdown)
    };

    let parser = Parser::new_ext(bounded.as_ref(), render_options());
    // 丢弃原始 HTML 事件
    let events: Vec<Event<'_>> = parser
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)))
        .collect();

    let slugs = compute_heading_slugs(&events);
    let limited = apply_limits(events, &slugs);

    let mut out = String::new();
    html::push_html(&mut out, limited.iter().cloned());
    out
}

/// 断言渲染策略版本非空（登记进 CONFIG_REGISTRY / 文档）。
pub fn renderer_version() -> &'static str {
    RENDERER_VERSION
}

// ---------------------------------------------------------------------------
// 标题锚点
// ---------------------------------------------------------------------------

/// 从标题内联文本生成确定性 slug。
///
/// 规则（与 GitHub 风格一致但文档化为本产品契约）：
/// - 仅保留字母数字（Unicode alphanumeric，含中文等），转小写；
/// - 其余字符折叠为单个 `-`；
/// - 首尾 `-` 剔除；空 slug 回退为 `section`；
/// - 截断到 [`MAX_HEADING_SLUG_CHARS`]。
fn heading_slug(text: &str) -> String {
    let mut slug = String::new();
    let mut pending_dash = false;
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            if pending_dash && !slug.is_empty() {
                slug.push('-');
            }
            pending_dash = false;
            for lc in ch.to_lowercase() {
                slug.push(lc);
            }
        } else {
            pending_dash = true;
        }
    }
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug.chars().take(MAX_HEADING_SLUG_CHARS).collect()
    }
}

/// 生成唯一 slug：重复基础 slug 依次追加 `-1`、`-2`…；若自然文本本身
/// 已占用候选值，则继续递增直到不冲突（确定性，且同一文档内保证唯一）。
fn unique_slug(seen: &mut HashMap<String, usize>, base: &str) -> String {
    // k = 已为该 base 分配过的 slug 数（0 表示 base 本身尚未占用）
    let mut k = seen.get(base).copied().unwrap_or(0);
    loop {
        let candidate = if k == 0 {
            base.to_string()
        } else {
            format!("{base}-{k}")
        };
        if !seen.contains_key(&candidate) {
            seen.insert(candidate.clone(), 1);
            // 记录：base 名下已分配 k+1 个（含本次）
            seen.insert(base.to_string(), k + 1);
            return candidate;
        }
        k += 1;
    }
}

/// 计算每个标题 Start 事件（原事件流索引）对应的确定性锚点 id。
fn compute_heading_slugs(events: &[Event<'_>]) -> HashMap<usize, String> {
    let mut slugs = HashMap::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut i = 0;
    while i < events.len() {
        if matches!(events[i], Event::Start(Tag::Heading { .. })) {
            // 标题不能嵌套，匹配的下一个 End(Heading) 即其闭合
            let mut j = i + 1;
            while j < events.len() && !matches!(events[j], Event::End(TagEnd::Heading(_))) {
                j += 1;
            }
            let mut text = String::new();
            for ev in &events[i + 1..j.min(events.len())] {
                match ev {
                    Event::Text(t) | Event::Code(t) => text.push_str(t),
                    _ => {}
                }
            }
            let base = heading_slug(&text);
            let slug = unique_slug(&mut seen, &base);
            slugs.insert(i, slug);
            i = j + 1;
        } else {
            i += 1;
        }
    }
    slugs
}

// ---------------------------------------------------------------------------
// 上限过滤（确定性事件重写）
// ---------------------------------------------------------------------------

/// 对事件流应用全部输出上限（见 [`super::policy`]），返回过滤后的事件流。
///
/// 展平语义：超限的容器（blockquote/list/行内元素）只丢弃其**开/合标签**，
/// 内部内容保留；超出列数的表格单元格整体丢弃；代码块/单元格内容按 char
/// 截断保留开头。所有裁剪只依赖输入事件，输出对同一输入严格确定。
fn apply_limits<'a>(events: Vec<Event<'a>>, slugs: &HashMap<usize, String>) -> Vec<Event<'a>> {
    let mut out: Vec<Event<'a>> = Vec::with_capacity(events.len());
    let mut quote_depth = 0usize;
    let mut quote_skips = 0usize; // 当前被展平的引用对数
    let mut block_depth = 0usize; // blockquote + list + item
    let mut block_skips = 0usize; // 当前被展平的块级容器对数
    let mut inline_depth = 0usize;
    let mut inline_skips = 0usize; // 当前被展平的行内元素对数
    let mut in_code_block = false;
    let mut code_chars_left = 0usize;
    let mut in_cell = false;
    let mut cell_chars_left = 0usize;
    let mut cell_index = 0usize;
    let mut skipping_cell = false;
    let mut rendered_estimate = 0usize;

    for (idx, event) in events.into_iter().enumerate() {
        let mut kept = event;

        // 整格丢弃：消费到该单元格的 End(TableCell)
        if skipping_cell {
            if matches!(kept, Event::End(TagEnd::TableCell)) {
                skipping_cell = false;
            }
            continue;
        }

        let mut emit = true;
        match &mut kept {
            Event::Start(tag) => match tag {
                Tag::Heading {
                    level: _,
                    id,
                    classes: _,
                    attrs: _,
                } => {
                    if let Some(slug) = slugs.get(&idx) {
                        *id = Some(slug.clone().into());
                    }
                }
                Tag::BlockQuote => {
                    quote_depth += 1;
                    block_depth += 1;
                    if quote_depth > MAX_BLOCKQUOTE_DEPTH || block_depth > MAX_BLOCK_NESTING {
                        quote_skips += 1;
                        block_skips += 1;
                        emit = false;
                    }
                }
                Tag::List(_) => {
                    block_depth += 1;
                    if block_depth > MAX_BLOCK_NESTING {
                        block_skips += 1;
                        emit = false;
                    }
                }
                Tag::Item => {
                    block_depth += 1;
                    if block_depth > MAX_BLOCK_NESTING {
                        block_skips += 1;
                        emit = false;
                    }
                }
                Tag::CodeBlock(_) => {
                    in_code_block = true;
                    code_chars_left = MAX_CODE_BLOCK_CHARS;
                }
                Tag::TableHead | Tag::TableRow => {
                    cell_index = 0;
                    in_cell = false;
                }
                Tag::TableCell => {
                    cell_index += 1;
                    if cell_index > MAX_TABLE_COLUMNS {
                        skipping_cell = true;
                        emit = false;
                    } else {
                        in_cell = true;
                        cell_chars_left = MAX_TABLE_CELL_CHARS;
                    }
                }
                Tag::Emphasis
                | Tag::Strong
                | Tag::Strikethrough
                | Tag::Link { .. }
                | Tag::Image { .. } => {
                    inline_depth += 1;
                    if inline_depth > MAX_INLINE_NESTING {
                        inline_skips += 1;
                        emit = false;
                    }
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::BlockQuote => {
                    if quote_skips > 0 {
                        quote_skips -= 1;
                        block_skips -= 1;
                        emit = false;
                    }
                    quote_depth = quote_depth.saturating_sub(1);
                    block_depth = block_depth.saturating_sub(1);
                }
                TagEnd::List(_) | TagEnd::Item => {
                    if block_skips > 0 {
                        block_skips -= 1;
                        emit = false;
                    }
                    block_depth = block_depth.saturating_sub(1);
                }
                TagEnd::CodeBlock => {
                    in_code_block = false;
                }
                TagEnd::TableCell => {
                    in_cell = false;
                }
                TagEnd::Emphasis
                | TagEnd::Strong
                | TagEnd::Strikethrough
                | TagEnd::Link
                | TagEnd::Image => {
                    if inline_skips > 0 {
                        inline_skips -= 1;
                        emit = false;
                    }
                    inline_depth = inline_depth.saturating_sub(1);
                }
                _ => {}
            },
            Event::Text(t) | Event::Code(t) => {
                if in_code_block {
                    let n = t.chars().count();
                    if n > code_chars_left {
                        if code_chars_left > 0 {
                            let kept_str: String = t.chars().take(code_chars_left).collect();
                            *t = kept_str.into();
                        } else {
                            emit = false;
                        }
                        code_chars_left = 0;
                    } else {
                        code_chars_left -= n;
                    }
                } else if in_cell {
                    let n = t.chars().count();
                    if n > cell_chars_left {
                        if cell_chars_left > 0 {
                            let kept_str: String = t.chars().take(cell_chars_left).collect();
                            *t = kept_str.into();
                        } else {
                            emit = false;
                        }
                        cell_chars_left = 0;
                    } else {
                        cell_chars_left -= n;
                    }
                }
            }
            _ => {}
        }

        if !emit {
            continue;
        }

        // 渲染总量估算兜底（文本按 char，标签按固定估算）
        match &kept {
            Event::Text(t) | Event::Code(t) => rendered_estimate += t.chars().count(),
            _ => rendered_estimate += 8,
        }
        if rendered_estimate > MAX_RENDERED_CHARS {
            break;
        }
        out.push(kept);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use pulldown_cmark::CowStr;

    #[test]
    fn renders_standard_markdown() {
        let html = render_to_html("# 标题\n\n**粗体** 与 `代码`");
        assert!(
            html.contains("<h1 id=\"标题\">标题</h1>"),
            "标题应渲染且带确定性锚点: {html}"
        );
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

    // ---- M04-MARKDOWN-04：确定性输出与上限 ----

    #[test]
    fn heading_anchors_are_deterministic_slugs() {
        let html = render_to_html("## Hello, World!\n\n### 你好 世界");
        assert!(
            html.contains("<h2 id=\"hello-world\">Hello, World!</h2>"),
            "标题锚点应为确定性 slug: {html}"
        );
        assert!(
            html.contains("<h3 id=\"你好-世界\">你好 世界</h3>"),
            "中文锚点应保留（Unicode alphanumeric）: {html}"
        );
    }

    #[test]
    fn heading_anchors_deduplicate_with_suffix() {
        let html = render_to_html("# Same\n\n## Same\n\n### Same");
        assert!(
            html.contains("<h1 id=\"same\">Same</h1>"),
            "首个标题锚点: {html}"
        );
        assert!(
            html.contains("<h2 id=\"same-1\">Same</h2>"),
            "重复标题应追加 -1: {html}"
        );
        assert!(
            html.contains("<h3 id=\"same-2\">Same</h3>"),
            "重复标题应追加 -2: {html}"
        );
    }

    #[test]
    fn heading_anchor_falls_back_for_symbol_only() {
        let html = render_to_html("# !!!");
        assert!(
            html.contains("<h1 id=\"section\">!!!</h1>"),
            "纯符号标题回退 section: {html}"
        );
    }

    #[test]
    fn code_block_is_truncated_to_cap() {
        let code = "fn main() {}\n".repeat(MAX_CODE_BLOCK_CHARS);
        let html = render_to_html(&format!("```rust\n{code}```"));
        assert!(html.contains("<pre><code"), "代码块应渲染");
        assert!(
            html.contains("fn main() {}"),
            "代码块开头应保留: {}",
            &html[..html.len().min(80)]
        );
        // 单个代码块输出有界
        assert!(
            html.len() <= MAX_CODE_BLOCK_CHARS * 3 + 256,
            "代码块输出必须有界: {}",
            html.len()
        );
    }

    #[test]
    fn blockquote_depth_is_capped() {
        let input: String = (0..15).map(|_| "> ").collect::<String>() + "deep";
        let html = render_to_html(&input);
        assert_eq!(
            html.matches("<blockquote>").count(),
            MAX_BLOCKQUOTE_DEPTH,
            "引用嵌套应被裁剪到上限: {html}"
        );
        assert!(html.contains("deep"), "被裁剪层级的内容应保留");
    }

    #[test]
    fn deep_list_nesting_is_flattened() {
        // 2 空格缩进 + "- " 构造 100 层嵌套列表
        let mut md = String::new();
        for i in 0..100 {
            md.push_str(&format!("{}- item{i}\n", "  ".repeat(i)));
        }
        let html = render_to_html(&md);
        let ul_count = html.matches("<ul>").count();
        assert!(
            ul_count <= MAX_BLOCK_NESTING / 2,
            "列表嵌套应被裁剪（{ul_count} 层）"
        );
        assert!(ul_count > 0, "深层列表不应完全消失");
        assert!(html.contains("item99"), "深层内容应保留");
    }

    #[test]
    fn table_columns_beyond_cap_are_dropped() {
        let mut header = String::new();
        let mut body = String::new();
        for i in 0..30 {
            header.push_str(&format!("| h{i} "));
            body.push_str(&format!("| c{i} "));
        }
        header.push('|');
        body.push('|');
        let sep = format!("|{}", "---|".repeat(30));
        let md = format!("{header}\n{sep}\n{body}\n");
        let html = render_to_html(&md);
        assert_eq!(html.matches("<th>").count(), MAX_TABLE_COLUMNS);
        assert_eq!(html.matches("<td>").count(), MAX_TABLE_COLUMNS);
        assert!(!html.contains("h30"), "超出列数的表头必须丢弃: {html}");
        assert!(!html.contains("c20"), "超出列数的单元格必须丢弃: {html}");
    }

    #[test]
    fn table_cell_content_is_truncated() {
        let cell = "x".repeat(MAX_TABLE_CELL_CHARS + 1_000);
        let md = format!("| a |\n|---|\n| {cell} |");
        let html = render_to_html(&md);
        let td = html
            .split("<td>")
            .nth(1)
            .map(|s| s.split("</td>").next().unwrap_or(""))
            .unwrap_or("");
        assert!(
            td.chars().count() <= MAX_TABLE_CELL_CHARS,
            "单元格内容必须截断到上限: {}",
            td.chars().count()
        );
    }

    #[test]
    fn inline_nesting_is_capped() {
        // CommonMark 解析本身难以产生深嵌套强调，直接以合成事件流验证上限机制
        let mut events = Vec::new();
        for _ in 0..(MAX_INLINE_NESTING * 2) {
            events.push(Event::Start(Tag::Emphasis));
        }
        events.push(Event::Text(CowStr::Borrowed("x")));
        for _ in 0..(MAX_INLINE_NESTING * 2) {
            events.push(Event::End(TagEnd::Emphasis));
        }
        let limited = apply_limits(events, &HashMap::new());
        let starts = limited
            .iter()
            .filter(|e| matches!(e, Event::Start(Tag::Emphasis)))
            .count();
        assert_eq!(starts, MAX_INLINE_NESTING, "行内嵌套 Start 应裁剪到上限");
        let ends = limited
            .iter()
            .filter(|e| matches!(e, Event::End(TagEnd::Emphasis)))
            .count();
        assert_eq!(ends, MAX_INLINE_NESTING, "行内嵌套 End 应与 Start 配对");
        assert!(
            limited
                .iter()
                .any(|e| matches!(e, Event::Text(t) if t.as_ref() == "x")),
            "被裁剪层级的内容应保留"
        );

        // 渲染路径：病态强调输入输出有界且普通强调正常
        let mut md = String::new();
        for i in 0..40 {
            md.push(if i % 2 == 0 { '*' } else { '_' });
        }
        md.push('x');
        for i in (0..40).rev() {
            md.push(if i % 2 == 0 { '*' } else { '_' });
        }
        let html = render_to_html(&md);
        assert!(html.len() < MAX_INPUT_CHARS, "病态强调输出必须有界");
        let normal = render_to_html("*a _b_*");
        assert_eq!(
            normal.matches("<em>").count(),
            2,
            "普通两层强调仍正常: {normal}"
        );
    }

    #[test]
    fn output_is_deterministic_for_same_input() {
        let doc = concat!(
            "# Title\n\n",
            "> quote\n\n",
            "```rust\nfn main() {}\n```\n\n",
            "| a | b |\n|---|---|\n| 1 | 2 |\n\n",
            "**bold** and *em*\n\n",
            "### Another\n\n",
            "### Another\n"
        );
        let a = render_to_html(doc);
        let b = render_to_html(doc);
        assert_eq!(a, b, "同一输入必须产生同一输出");
        assert!(a.contains("id=\"title\""), "标题锚点确定性: {a}");
    }

    #[test]
    fn heading_anchor_slug_helpers() {
        assert_eq!(heading_slug("Hello, World!"), "hello-world");
        assert_eq!(heading_slug("  spaced  out  "), "spaced-out");
        assert_eq!(heading_slug("中文标题"), "中文标题");
        assert_eq!(heading_slug("!!!"), "section");
        assert_eq!(heading_slug("A"), "a");
        let mut seen = HashMap::new();
        assert_eq!(unique_slug(&mut seen, "x"), "x");
        assert_eq!(unique_slug(&mut seen, "x"), "x-1");
        assert_eq!(unique_slug(&mut seen, "x-1"), "x-1-1");
        assert_eq!(unique_slug(&mut seen, "x"), "x-2");
    }
}
