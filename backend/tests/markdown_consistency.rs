//! M04-MARKDOWN-09：渲染一致性——纯文本、代码、链接、图片、长文与无 JS。
//!
//! 对完整管线（`render_and_sanitize`：CommonMark 渲染 → 原始 HTML 剥离 →
//! ammonia allowlist 清洗）验证各类内容可预测、可复现地渲染：
//! - 纯文本 → 段落；多段 → 多个 `<p>`；
//! - 代码块 → `<pre><code class="language-*">`，内部文本转义（不可执行）；
//! - 链接 → http(s)/mailto 保留 + rel/target 注入；危险 scheme 被剥离；
//! - 图片 → src/alt 保留；
//! - 长文 → 输出有界、逐字节确定；
//! - **无 JS 友好**：输出不含 `<script>`、事件属性、`style=`，纯静态 HTML
//!   即可完整展示（前端 SSR + SafeHtml 配合，见
//!   frontend/src/lib/testing/ssr/post-content-nojs.test.ts）。

use bblbb_backend::content::markdown::excerpt::render_excerpt;
use bblbb_backend::content::markdown::render_and_sanitize;
use bblbb_backend::content::markdown::rerender::render_content;
use bblbb_backend::content::markdown::sanitize::sanitize_html;

fn clean(md: &str) -> String {
    render_and_sanitize(md)
}

#[test]
fn plain_text_renders_as_paragraphs() {
    let out = clean("普通文本一行");
    assert!(out.contains("<p>普通文本一行</p>"), "单段纯文本: {out}");
    let out = clean("第一段\n\n第二段");
    assert_eq!(out.matches("<p>").count(), 2, "空行分隔应得两段: {out}");
    assert!(out.contains("第一段") && out.contains("第二段"));
}

#[test]
fn code_block_preserves_language_and_escapes_content() {
    let out = clean("```rust\nfn main() { println!(\"hi\"); }\n```");
    assert!(
        out.contains("<pre><code class=\"language-rust\">fn main() { println!(\"hi\"); }"),
        "代码块应保留语言类与原内容: {out}"
    );
    assert!(out.contains("</code></pre>"), "代码块闭合: {out}");
    // 代码块内的 HTML 不得成为可执行结构
    let out = clean("```html\n<script>alert(1)</script>\n```");
    assert!(!out.contains("<script"), "代码块内 script 必须转义: {out}");
    assert!(
        out.contains("&lt;script&gt;"),
        "代码块内 HTML 应转义显示: {out}"
    );
}

#[test]
fn links_preserve_safe_destinations_and_add_rel_target() {
    let out = clean("[文档](https://example.com/a) 和 [联系](mailto:a@b.example)");
    assert!(
        out.contains("<a href=\"https://example.com/a\""),
        "https 链接保留: {out}"
    );
    assert!(
        out.contains("rel=\"nofollow noopener noreferrer\""),
        "外链 rel: {out}"
    );
    assert!(out.contains("target=\"_blank\""), "外链 target: {out}");
    assert!(out.contains("mailto:a@b.example"), "mailto 保留: {out}");
    // 危险 scheme 一律剥离
    let out = clean("[x](javascript:alert(1))");
    assert!(
        !out.contains("href"),
        "javascript scheme 链接必须剥离: {out}"
    );
}

#[test]
fn images_preserve_src_and_alt() {
    let out = clean("![示意图](https://example.com/i.png)");
    assert!(
        out.contains("<img src=\"https://example.com/i.png\" alt=\"示意图\""),
        "图片应保留 src/alt: {out}"
    );
}

#[test]
fn long_text_is_bounded_and_deterministic() {
    let body = "这是一段很长的中文正文。".repeat(800); // ~6400 字符
    let out = clean(&body);
    assert!(
        out.chars().count() < body.chars().count() * 4,
        "长文输出必须有界"
    );
    assert_eq!(out, clean(&body), "同一输入必须逐字节一致");
}

#[test]
fn no_js_friendly_output_has_no_active_constructs() {
    // 无 JS 一致性：输出为纯静态 HTML，无需脚本/事件/样式即可完整展示
    let out = clean("## 标题\n\n正文 **粗** *斜*\n\n- 列表\n\n> 引用\n\n```js\nlet x = 1;\n```");
    assert!(!out.contains("<script"), "不得含 script 元素");
    for handler in [
        "onclick",
        "onerror",
        "onload",
        "onmouseover",
        "onfocus",
        "onchange",
        "onsubmit",
        "oninput",
    ] {
        assert!(!out.contains(handler), "不得含事件属性 {handler}: {out}");
    }
    assert!(!out.contains("style="), "不得含 style 属性: {out}");
    assert!(out.contains("<h2"), "标题渲染");
    assert!(out.contains("<strong>粗</strong>"), "强调渲染");
    assert!(out.contains("<ul>"), "列表渲染");
    assert!(out.contains("<blockquote>"), "引用渲染");
    assert!(out.contains("language-js"), "代码块渲染");
}

#[test]
fn excerpt_consistency_for_content_types() {
    let excerpt = render_excerpt("## 标题\n\n正文内容 `code` 和 [链接](https://x.example)");
    assert!(excerpt.contains("标题"), "标题进入摘要");
    assert!(excerpt.contains("正文内容"), "正文进入摘要");
    assert!(excerpt.contains("链接"), "链接文字进入摘要");
    assert!(!excerpt.contains("https://"), "链接目标不进摘要");
    assert!(!excerpt.contains('<'), "摘要为纯文本");
}

#[test]
fn write_path_render_content_is_consistent() {
    let rendered = render_content("# 公开\n\n公开正文", Some("> 受限正文"));
    assert!(rendered.body_html.contains("<h1 id=\"公开\">公开</h1>"));
    assert!(rendered
        .restricted_html
        .as_deref()
        .unwrap()
        .contains("<blockquote>"));
    assert!(rendered.excerpt.contains("公开正文"));
    assert!(!rendered.excerpt.contains("受限正文"), "摘要不得含受限内容");
    assert!(rendered
        .renderer_version
        .starts_with("markdown-v1+ammonia-v1"));
    // 清洗器出口与管线出口一致
    assert_eq!(
        rendered.body_html,
        sanitize_html(&bblbb_backend::content::markdown::render::render_to_html(
            "# 公开\n\n公开正文"
        ))
    );
}
