//! M04-MARKDOWN-03：HTML 清洗 allowlist——标签、属性、协议、图片、外链
//! rel/target 与 iframe Provider 白名单。

use std::collections::{HashMap, HashSet};

use ammonia::{Builder, UrlRelative};

use super::policy::{IFRAME_PROVIDERS, SANITIZER_VERSION};

/// 清洗 HTML（allowlist 策略，[`sanitizer_version`] 标识版本）。
///
/// 策略（v2）：
/// - **标签 allowlist**：内容语义标签（标题/段落/列表/表格/代码/引用/图片/
///   链接/强调）+ iframe（仅视频 Provider，见下）；`div`/`span` 受限保留；
/// - **属性 allowlist**：`a[href,title,rel]`、`img[src,alt,title,width,height,
///   loading]`、`code|pre[class]`（仅 `language-*` 高亮类）、`h1..h6[id]`
///   （仅后端生成的确定性锚点）、`iframe[src,title,width,height,loading,
///   allowfullscreen]`、`th|td[align,colspan,rowspan]`；其余属性（style/on*
///   等）一律剥离；
/// - **协议 allowlist**：`http`/`https`/`mailto`；相对 URL 仅放行**站内附件
///   内容端点**前缀（v2，见 [`ATTACHMENT_CONTENT_PREFIX`]：编辑器上传的
///   图片/附件以同源相对路径 `/api/v1/attachments/{id}/content` 进入正文，
///   Deny 会剥掉 img[src]/a[href] 导致图片无法展示）；其余相对路径（含协议
///   相对 `//evil.com` 与 `..` 路径穿越）一律拒绝；
/// - **外链 rel/target**：所有 `<a>` 强制 `rel="nofollow noopener noreferrer"`
///   与 `target="_blank"`；
/// - **iframe Provider 白名单**：仅 [`policy::IFRAME_PROVIDERS`] 主机的视频
///   embed 保留，其余 iframe 剥离。
pub fn sanitize_html(html: &str) -> String {
    let mut builder = Builder::default();

    // 标签 allowlist
    let tags: HashSet<&str> = [
        "p",
        "br",
        "hr",
        "h1",
        "h2",
        "h3",
        "h4",
        "h5",
        "h6",
        "blockquote",
        "pre",
        "code",
        "ul",
        "ol",
        "li",
        "table",
        "thead",
        "tbody",
        "tr",
        "th",
        "td",
        "a",
        "img",
        "strong",
        "em",
        "del",
        "s",
        "b",
        "i",
        "u",
        "span",
        "figure",
        "figcaption",
        "iframe",
    ]
    .into_iter()
    .collect();
    builder.tags(tags);

    // 属性 allowlist
    let mut attrs: HashMap<&str, HashSet<&str>> = HashMap::new();
    attrs.insert("a", ["href", "title"].into_iter().collect());
    // 标题锚点 id 仅由后端渲染器生成（确定性 slug，M04-MARKDOWN-04）——
    // 原始 HTML 在渲染层已剥离，此处放行不会引入用户可控 id
    for h in ["h1", "h2", "h3", "h4", "h5", "h6"] {
        attrs.insert(h, ["id"].into_iter().collect());
    }
    attrs.insert(
        "img",
        ["src", "alt", "title", "width", "height", "loading"]
            .into_iter()
            .collect(),
    );
    attrs.insert("code", ["class"].into_iter().collect());
    attrs.insert("pre", ["class"].into_iter().collect());
    attrs.insert(
        "iframe",
        [
            "src",
            "title",
            "width",
            "height",
            "loading",
            "allowfullscreen",
        ]
        .into_iter()
        .collect(),
    );
    attrs.insert("th", ["align", "colspan", "rowspan"].into_iter().collect());
    attrs.insert("td", ["align", "colspan", "rowspan"].into_iter().collect());
    builder.tag_attributes(attrs);

    // 协议 allowlist + 相对 URL 策略（v2）
    // ammonia 对绝对 URL 按 url_schemes（http/https/mailto）裁决；相对 URL
    // （无 base 可解析）在 PassThrough 下保留原值并进入 attribute_filter，
    // 由 [`is_allowed_url_value`] 做唯一例外（站内附件端点）裁决。
    let schemes: HashSet<&str> = ["http", "https", "mailto"].into_iter().collect();
    builder.url_schemes(schemes);
    builder.url_relative(UrlRelative::PassThrough);

    // 外链 rel/target
    builder.link_rel(Some("nofollow noopener noreferrer"));
    builder.set_tag_attribute_value("a", "target", "_blank");

    // URL 属性相对值裁决 + iframe src 主机白名单 + 代码高亮类白名单
    // （attribute_filter）
    builder.attribute_filter(|tag, attr, value| {
        // v2：a[href] / img[src] 的相对值仅放行站内附件内容端点；其余相对
        // 路径（含协议相对 `//` 与 `..` 穿越路径）一律剥离。绝对 URL 的
        // scheme 已由 url_schemes 在属性保留阶段裁决，此处重复校验属纵深
        // 防御（不依赖 ammonia 内部处理顺序）。
        if (tag == "a" && attr == "href") || (tag == "img" && attr == "src") {
            return if is_allowed_url_value(value) {
                Some(value.into())
            } else {
                None
            };
        }
        if tag == "iframe" && attr == "src" {
            return match url_host(value) {
                Some(host) if IFRAME_PROVIDERS.contains(&host.as_str()) => Some(value.into()),
                _ => None,
            };
        }
        if tag == "code" && attr == "class" && !value.starts_with("language-") {
            return None;
        }
        if tag == "pre" && attr == "class" && !value.starts_with("language-") {
            return None;
        }
        Some(value.into())
    });

    builder.clean(html).to_string()
}

/// 站内附件内容端点前缀（同源相对路径；编辑器上传产物插入正文的唯一
/// 形态，见 `frontend/src/lib/components/editor/upload.ts` 的
/// `attachmentContentUrl`）。
pub const ATTACHMENT_CONTENT_PREFIX: &str = "/api/v1/attachments/";

/// URL 属性值（a[href] / img[src]）裁决：
/// - 相对 URL：仅放行 [`ATTACHMENT_CONTENT_PREFIX`] 前缀，且拒绝含 `..`
///   段的值（浏览器会把 `/a/../b` 规范化为 `/b`，前缀匹配可被穿越绕过）；
/// - 绝对 URL：沿用 http/https/mailto scheme allowlist（与 url_schemes 一致）。
fn is_allowed_url_value(value: &str) -> bool {
    if value.starts_with(ATTACHMENT_CONTENT_PREFIX) {
        return !value.contains("..");
    }
    match url::Url::parse(value) {
        Ok(u) => matches!(u.scheme(), "http" | "https" | "mailto"),
        Err(_) => false,
    }
}

/// 从 URL 提取主机名（小写；解析失败或相对 → `None`）。
fn url_host(raw: &str) -> Option<String> {
    let parsed = url::Url::parse(raw).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    Some(host)
}

/// 断言清洗策略版本非空（登记进文档）。
pub fn sanitizer_version() -> &'static str {
    SANITIZER_VERSION
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_allowed_tags_and_strips_forbidden() {
        let out = sanitize_html(
            "<p>ok</p><script>alert(1)</script><svg onload='x'></svg><style>a{}</style><iframe src='https://www.youtube.com/embed/abc'></iframe>",
        );
        assert!(out.contains("<p>ok</p>"), "允许的 p 必须保留: {out}");
        assert!(!out.contains("script"), "script 必须剥离: {out}");
        assert!(!out.contains("svg"), "svg 必须剥离: {out}");
        assert!(!out.contains("style"), "style 必须剥离: {out}");
        assert!(out.contains("youtube.com"), "白名单 iframe 必须保留: {out}");
    }

    #[test]
    fn strips_event_attributes_and_style() {
        let out = sanitize_html("<p onclick='alert(1)' style='color:red'>x</p><img src='https://a.example/i.png' onerror='x'>");
        assert!(!out.contains("onclick"), "事件属性必须剥离");
        assert!(!out.contains("onerror"), "事件属性必须剥离");
        assert!(!out.contains("style="), "style 属性必须剥离");
        assert!(out.contains("<p>x</p>"), "内容保留");
    }

    #[test]
    fn url_protocols_and_relative_denied() {
        let out = sanitize_html(
            "<a href='https://ok.example/'>good</a><a href='javascript:alert(1)'>bad1</a><a href='//evil.example/'>bad2</a><a href='/local'>bad3</a>",
        );
        assert!(out.contains("https://ok.example"), "https 链接保留");
        assert!(!out.contains("javascript:"), "javascript: 协议剥离");
        assert!(!out.contains("//evil.example"), "协议相对 URL 剥离");
        assert!(!out.contains("/local"), "相对 URL 剥离");
    }

    #[test]
    fn attachment_relative_urls_survive_v2() {
        // v2：站内附件内容端点是相对 URL 的唯一例外（编辑器上传图片/附件）
        let out = sanitize_html(
            "<p><img src='/api/v1/attachments/01a08ea8/content' alt='logo'></p>\
             <a href='/api/v1/attachments/01a08ea8/content'>file</a>",
        );
        assert!(
            out.contains("src=\"/api/v1/attachments/01a08ea8/content\""),
            "站内附件图片 src 必须保留: {out}"
        );
        assert!(
            out.contains("href=\"/api/v1/attachments/01a08ea8/content\""),
            "站内附件链接 href 必须保留: {out}"
        );
    }

    #[test]
    fn attachment_prefix_traversal_and_protocol_relative_denied() {
        // 前缀匹配不得被 `..` 穿越绕过；协议相对 URL 继续拒绝
        let out = sanitize_html(
            "<img src='/api/v1/attachments/../admin/users'><img src='//evil.example/x.png'>",
        );
        assert!(!out.contains("../"), "`..` 穿越必须剥离: {out}");
        assert!(
            !out.contains("evil.example"),
            "协议相对 URL 必须剥离: {out}"
        );
        assert!(out.contains("<img"), "img 标签本身保留（仅剥属性）: {out}");
    }

    #[test]
    fn external_links_get_rel_and_target() {
        let out = sanitize_html("<a href='https://example.com/'>link</a>");
        assert!(
            out.contains("rel=\"nofollow noopener noreferrer\""),
            "外链必须带 rel: {out}"
        );
        assert!(
            out.contains("target=\"_blank\""),
            "外链必须 target=_blank: {out}"
        );
    }

    #[test]
    fn iframe_provider_allowlist_enforced() {
        let out = sanitize_html(
            "<iframe src='https://player.vimeo.com/video/1'></iframe><iframe src='https://evil.example/x'></iframe><iframe src='https://www.youtube.com/embed/abc'></iframe>",
        );
        assert!(out.contains("player.vimeo.com"), "Vimeo 白名单保留");
        assert!(out.contains("youtube.com"), "YouTube 白名单保留");
        assert!(!out.contains("evil.example"), "非白名单 iframe 剥离");
    }

    #[test]
    fn code_classes_limited_to_language_prefix() {
        let out = sanitize_html(
            "<pre><code class='language-rust'>fn main() {}</code></pre><code class='xss'>x</code>",
        );
        assert!(out.contains("language-rust"), "language-* 高亮类保留");
        assert!(!out.contains("class=\"xss\""), "非 language-* 类剥离");
    }

    #[test]
    fn heading_id_from_renderer_is_kept() {
        // 标题锚点 id 仅由后端渲染器生成（M04-MARKDOWN-04）；清洗器放行
        let out =
            sanitize_html("<h2 id=\"hello-world\">标题</h2><h2 id=\"bad\" onclick=\"x\">x</h2>");
        assert!(
            out.contains("<h2 id=\"hello-world\">标题</h2>"),
            "渲染器锚点 id 必须保留: {out}"
        );
        assert!(
            !out.contains("onclick"),
            "事件属性仍然剥离（不能借 id 放行引入）: {out}"
        );
    }

    #[test]
    fn sanitizer_version_is_set() {
        assert!(sanitizer_version().starts_with("ammonia-v"));
    }
}
