//! M04-MARKDOWN-03：HTML 清洗 allowlist——标签、属性、协议、图片、外链
//! rel/target 与 iframe Provider 白名单。

use std::collections::{HashMap, HashSet};

use ammonia::{Builder, UrlRelative};

use super::policy::{IFRAME_PROVIDERS, SANITIZER_VERSION};

/// 清洗 HTML（allowlist 策略，[`sanitizer_version`] 标识版本）。
///
/// 策略（v1）：
/// - **标签 allowlist**：内容语义标签（标题/段落/列表/表格/代码/引用/图片/
///   链接/强调）+ iframe（仅视频 Provider，见下）；`div`/`span` 受限保留；
/// - **属性 allowlist**：`a[href,title,rel]`、`img[src,alt,title,width,height,
///   loading]`、`code|pre[class]`（仅 `language-*` 高亮类）、`iframe[src,
///   title,width,height,loading,allowfullscreen]`、`th|td[align,colspan,
///   rowspan]`；其余属性（style/on* 等）一律剥离；
/// - **协议 allowlist**：`http`/`https`/`mailto`；相对 URL 拒绝（防协议相对
///   `//evil.com` 与路径绕过）；
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

    // 协议 allowlist + 相对 URL 拒绝
    let schemes: HashSet<&str> = ["http", "https", "mailto"].into_iter().collect();
    builder.url_schemes(schemes);
    builder.url_relative(UrlRelative::Deny);

    // 外链 rel/target
    builder.link_rel(Some("nofollow noopener noreferrer"));
    builder.set_tag_attribute_value("a", "target", "_blank");

    // iframe src 主机白名单 + 代码高亮类白名单（attribute_filter）
    builder.attribute_filter(|tag, attr, value| {
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
        assert!(!out.contains("javascript:"), "javascript 协议剥离");
        assert!(!out.contains("//evil.example"), "协议相对 URL 剥离");
        assert!(!out.contains("/local"), "相对 URL 剥离");
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
    fn sanitizer_version_is_set() {
        assert!(sanitizer_version().starts_with("ammonia-v"));
    }
}
