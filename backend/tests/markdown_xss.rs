//! M04-MARKDOWN-07：XSS corpus——事件属性、javascript/data URL、SVG、MathML、
//! 畸形 HTML 与 Unicode 绕过。
//!
//! 对每个 payload 运行**完整管线**（`render_and_sanitize`：CommonMark 渲染 →
//! 原始 HTML 事件剥离 → ammonia allowlist 清洗），然后用 [`active_structures`]
//! 扫描**最终输出**的标签结构，断言不存在任何可执行结构：
//! - 元素级：`script`/`svg`/`math`/`iframe`/`style`/`embed`/`object` 等；
//! - 属性级：事件处理器 `on*`、`style`、命名空间属性（`xlink:*`/`xmlns:*`）、
//!   非 http(s)/mailto 绝对 URL 的 `href`/`src`。
//!
//! 说明：pulldown-cmark 会把无法解析为合法 inline HTML 的输入（如无引号属性
//! 之间缺空格、全宽字符标签名）**转义为纯文本**（`&lt;...&gt;`）。该文本不可
//! 执行；扫描器只识别真正的标签结构，因此不会把转义文本误报为泄漏。

use bblbb_backend::content::markdown::render_and_sanitize;

struct XssCase {
    name: &'static str,
    markdown: &'static str,
    /// 输出中不得出现的子串（大小写不敏感；仅用于 `javascript:`/`data:` 等
    /// 即使转义也不应出现的内容）。
    forbidden_substrings: &'static [&'static str],
}

const CASES: &[XssCase] = &[
    // ── 原始 HTML：渲染层剥离事件，双重防护 ──
    XssCase {
        name: "raw script tag",
        markdown: "<script>alert(1)</script>",
        forbidden_substrings: &["script", "alert(1)"],
    },
    XssCase {
        name: "img onerror",
        markdown: "<img src=x onerror=alert(1)>",
        forbidden_substrings: &["onerror", "alert(1)"],
    },
    XssCase {
        name: "svg onload",
        markdown: "<svg onload=alert(1)>",
        forbidden_substrings: &["svg", "onload", "alert(1)"],
    },
    XssCase {
        name: "mathml script",
        // `<script>` 块被渲染层剥离，残余文本 `alert(1)` 不可执行（无标签）
        markdown: "<math><mtext><script>alert(1)</script></mtext></math>",
        forbidden_substrings: &["math", "script"],
    },
    XssCase {
        name: "style url javascript",
        markdown: "<p style=\"background:url(javascript:alert(1))\">x</p>",
        forbidden_substrings: &["style", "javascript", "alert(1)"],
    },
    XssCase {
        name: "body onload",
        markdown: "<body onload=alert(1)>x</body>",
        forbidden_substrings: &["onload", "alert(1)"],
    },
    XssCase {
        name: "iframe javascript src",
        markdown: "<iframe src=\"javascript:alert(1)\"></iframe>",
        forbidden_substrings: &["iframe", "javascript", "alert(1)"],
    },
    // ── Markdown 语法注入（渲染后经清洗——真实攻击面） ──
    XssCase {
        name: "md link javascript scheme",
        markdown: "[click](javascript:alert(1))",
        forbidden_substrings: &["javascript:", "alert(1)"],
    },
    XssCase {
        name: "md link mixed-case scheme",
        markdown: "[click](JaVaScRiPt:alert(1))",
        forbidden_substrings: &["javascript:", "alert(1)"],
    },
    XssCase {
        name: "md image data url",
        markdown: "![x](data:image/svg+xml;base64,PHN2Zz4=)",
        forbidden_substrings: &["data:", "svg", "alert"],
    },
    XssCase {
        name: "md link data html",
        markdown: "[x](data:text/html,<script>alert(1)</script>)",
        forbidden_substrings: &["data:", "script", "alert(1)"],
    },
    XssCase {
        name: "md link tab in scheme",
        // 制表符破坏链接目标解析 → 整段渲染为转义文本，不可执行
        markdown: "[x](java\tscript:alert(1))",
        forbidden_substrings: &["javascript:"],
    },
    XssCase {
        name: "md link newline in scheme",
        // 换行破坏链接目标解析 → 整段渲染为转义文本，不可执行
        markdown: "[x](java\nscript:alert(1))",
        forbidden_substrings: &["javascript:"],
    },
    XssCase {
        name: "md link html-entity scheme",
        markdown: "[x](javas&#x63;ript:alert(1))",
        forbidden_substrings: &["javascript:", "alert(1)"],
    },
    XssCase {
        name: "md link numeric-entity scheme",
        markdown: "[x](&#106;avascript:alert(1))",
        forbidden_substrings: &["javascript:", "alert(1)"],
    },
    XssCase {
        name: "md link entity newline before colon",
        markdown: "[x](javascript&#x0A;:alert(1))",
        forbidden_substrings: &["javascript:", "alert(1)"],
    },
    XssCase {
        name: "md link percent-encoded scheme",
        markdown: "[x](javascript%3Aalert(1))",
        forbidden_substrings: &["javascript", "alert(1)"],
    },
    XssCase {
        name: "md link null byte",
        // 空字节使链接目标解析失败并从不透明文本中剔除 → 整段为转义文本
        markdown: "[x](java\0script:alert(1))",
        forbidden_substrings: &[],
    },
    XssCase {
        name: "md link relative url",
        markdown: "[x](/local/path)",
        forbidden_substrings: &[],
    },
    XssCase {
        name: "md link protocol-relative url",
        markdown: "[x](//evil.example/x)",
        forbidden_substrings: &["//evil"],
    },
    XssCase {
        name: "md image alt attribute breakout",
        // alt 值内含 `" onerror="` 仅为属性值数据（html5ever 已将其视为 alt 的值，
        // 不产生新属性）；结构扫描器保证不存在真实的 on* 属性
        markdown: "![x\" onerror=\"alert(1)](https://a.example/i.png)",
        forbidden_substrings: &[],
    },
    // ── 畸形 HTML（渲染层转义为文本或剥离） ──
    XssCase {
        name: "nested script tag",
        markdown: "<scr<script>ipt>alert(1)</scr<script>ipt>",
        forbidden_substrings: &[],
    },
    XssCase {
        name: "unterminated img tag",
        markdown: "<img src=x onerror=alert(1)",
        forbidden_substrings: &[],
    },
    XssCase {
        name: "attribute without space",
        markdown: "<img src=\"x\"onerror=\"alert(1)\">",
        forbidden_substrings: &[],
    },
    XssCase {
        name: "double angle bracket",
        markdown: "<<script>alert(1)//<</script>",
        forbidden_substrings: &[],
    },
    // ── Unicode / 编码绕过 ──
    XssCase {
        name: "fullwidth javascript scheme",
        markdown: "[x](ｊａｖａｓｃｒｉｐｔ:alert(1))",
        forbidden_substrings: &["alert(1)"],
    },
    XssCase {
        name: "fullwidth onerror",
        // 全宽属性名使标签非法 → 渲染层整体转义为纯文本（&lt;...&gt;），不可执行
        markdown: "<img src=x ｏｎｅｒｒｏｒ=alert(1)>",
        forbidden_substrings: &[],
    },
    XssCase {
        name: "mixed-width script",
        // 全宽字符破坏标签名 → 渲染层整体转义为纯文本，不可执行
        markdown: "<sｃript>alert(1)</sｃript>",
        forbidden_substrings: &[],
    },
];

/// 扫描输出 HTML，返回所有可执行结构违规（元素/属性级）。
fn active_structures(output: &str) -> Vec<String> {
    let mut violations = Vec::new();
    let cs: Vec<char> = output.chars().collect();
    let n = cs.len();
    let mut i = 0;
    while i < n {
        if cs[i] == '<' {
            // 标签区域：`>` 结束，但引号内的 `>` 不结束（氨水输出为良构 HTML）
            let mut j = i + 1;
            let mut quote: Option<char> = None;
            while j < n {
                match cs[j] {
                    '"' | '\'' if quote.is_none() => quote = Some(cs[j]),
                    c if Some(c) == quote => quote = None,
                    '>' if quote.is_none() => break,
                    _ => {}
                }
                j += 1;
            }
            if j >= n {
                break;
            }
            let body: String = cs[i + 1..j].iter().collect();
            analyze_tag(&body, &mut violations);
            i = j + 1;
        } else {
            i += 1;
        }
    }
    violations
}

fn analyze_tag(body: &str, violations: &mut Vec<String>) {
    let body = body.trim_start();
    if body.is_empty() {
        return;
    }
    let first = body.chars().next().unwrap();
    if first == '!' || first == '?' {
        violations.push(format!("注释/处理指令标签: <{body}>"));
        return;
    }
    if first == '/' {
        return; // 结束标签无属性
    }
    // 标签名
    let mut name = String::new();
    for c in body.chars() {
        if c.is_ascii_alphanumeric() || c == '-' || c == ':' {
            name.push(c);
        } else {
            break;
        }
    }
    let lname = name.to_lowercase();
    const FORBIDDEN: &[&str] = &[
        "script", "style", "iframe", "svg", "math", "embed", "object", "form", "meta", "link",
        "base", "template", "noscript",
    ];
    if FORBIDDEN.contains(&lname.as_str()) {
        violations.push(format!("禁用元素 <{name}>"));
        return;
    }
    // 属性
    for (an, av) in split_attributes(&body[name.len()..]) {
        let la = an.to_lowercase();
        if la.starts_with("on") {
            violations.push(format!("事件属性 {an}"));
        }
        if la == "style" {
            violations.push("style 属性".to_string());
        }
        if an.contains(':') {
            violations.push(format!("命名空间属性 {an}"));
        }
        if la == "href" || la == "src" {
            match av {
                Some(v) if is_safe_absolute_url(&v) => {}
                _ => violations.push(format!("不安全 {an} 值: {av:?}")),
            }
        }
    }
}

fn split_attributes(rest: &str) -> Vec<(String, Option<String>)> {
    let mut out = Vec::new();
    let cs: Vec<char> = rest.chars().collect();
    let n = cs.len();
    let mut i = 0;
    while i < n {
        while i < n && cs[i].is_whitespace() {
            i += 1;
        }
        if i >= n {
            break;
        }
        let mut name = String::new();
        while i < n && (cs[i].is_ascii_alphanumeric() || ":_-".contains(cs[i])) {
            name.push(cs[i]);
            i += 1;
        }
        if name.is_empty() {
            i += 1;
            continue;
        }
        let mut value = None;
        while i < n && cs[i].is_whitespace() {
            i += 1;
        }
        if i < n && cs[i] == '=' {
            i += 1;
            while i < n && cs[i].is_whitespace() {
                i += 1;
            }
            if i < n && (cs[i] == '"' || cs[i] == '\'') {
                let q = cs[i];
                i += 1;
                let mut v = String::new();
                while i < n && cs[i] != q {
                    v.push(cs[i]);
                    i += 1;
                }
                if i < n {
                    i += 1;
                }
                value = Some(v);
            } else {
                let mut v = String::new();
                while i < n && !cs[i].is_whitespace() && cs[i] != '>' {
                    v.push(cs[i]);
                    i += 1;
                }
                value = Some(v);
            }
        }
        out.push((name, value));
    }
    out
}

fn is_safe_absolute_url(v: &str) -> bool {
    let lower = v.trim().to_lowercase();
    lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("mailto:")
}

#[test]
fn xss_corpus_is_neutralized() {
    let mut failures = Vec::new();
    for case in CASES {
        let out = render_and_sanitize(case.markdown);
        // 1) 结构扫描：不得存在可执行元素/属性
        for violation in active_structures(&out) {
            failures.push(format!(
                "case '{}': 结构违规 {violation}\n  markdown: {:?}\n  output:   {:?}",
                case.name, case.markdown, out
            ));
        }
        // 2) 全局子串：危险 scheme 不得以任何形式出现
        let lower = out.to_lowercase();
        for marker in case.forbidden_substrings {
            if lower.contains(&marker.to_lowercase()) {
                failures.push(format!(
                    "case '{}': 输出包含禁止子串 '{marker}'\n  markdown: {:?}\n  output:   {:?}",
                    case.name, case.markdown, out
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "XSS corpus 存在 {} 个泄漏:\n{}",
        failures.len(),
        failures.join("\n---\n")
    );
}

#[test]
fn benign_markdown_still_renders() {
    // corpus 不得误伤合法内容
    let link = render_and_sanitize("[ok](https://example.com)");
    assert!(
        link.contains("https://example.com"),
        "合法 https 链接保留: {link}"
    );
    assert!(
        link.contains("rel=\"nofollow noopener noreferrer\""),
        "外链 rel 保留: {link}"
    );
    let img = render_and_sanitize("![logo](https://example.com/i.png)");
    assert!(
        img.contains("https://example.com/i.png"),
        "合法图片保留: {img}"
    );
    let bold = render_and_sanitize("**粗体**");
    assert!(bold.contains("<strong>粗体</strong>"), "强调保留: {bold}");
    let table = render_and_sanitize("| a | b |\n|---|---|\n| 1 | 2 |");
    assert!(table.contains("<table>"), "表格保留: {table}");
    assert_eq!(
        active_structures(&table),
        Vec::<String>::new(),
        "合法表格不得误报"
    );
    let mail = render_and_sanitize("[mail](mailto:user@example.com)");
    assert!(mail.contains("mailto:"), "mailto 链接保留: {mail}");
    assert_eq!(
        active_structures(&mail),
        Vec::<String>::new(),
        "mailto 链接不得误报"
    );
}
