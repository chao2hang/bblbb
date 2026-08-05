//! 索引安全纯文本转换（M03-SEARCH-STORE-06）。
//!
//! [`to_index_plain_text`] 把源公开文本（Markdown，可能内嵌 HTML 片段）转换为
//! 索引安全纯文本：剥离 HTML 标签（`<字母`/`</` 起止的标签串，保留标签内文本），
//! 再经 [`clean_index_text`] 折叠空白/控制字符并截断到 `max_len` 字符。
//!
//! 转换结果必须通过 [`vet_index_text`]（M03-SEARCH-STORE-05 P0 门）——
//! `restricted_html`/`restricted_markdown` 特征串或残留 HTML 一律拒绝，绝不入索引。
//! 本函数只做形式转换，不做任何内容/可见性裁决。

use crate::search::clean_index_text;

/// 把源公开文本转换为索引安全纯文本。
///
/// - 剥离 `<字母`/`</` 开头的标签串（找到匹配的 `>`，标签内文本保留）；
/// - 比较符（`x < y`）、表情（`<3`）等非标签 `<` 原样保留；
/// - 多字节安全（按字符扫描），输出经 `clean_index_text` 折叠与截断。
pub fn to_index_plain_text(raw: &str, max_len: usize) -> String {
    let chars: Vec<char> = raw.chars().collect();
    let mut out = String::with_capacity(raw.len().min(max_len + 16));
    let mut i = 0usize;
    while i < chars.len() {
        if chars[i] == '<' {
            let is_tag = chars
                .get(i + 1)
                .is_some_and(|&c| c.is_ascii_alphabetic() || c == '/');
            if is_tag {
                if let Some(rel) = chars[i + 1..].iter().position(|&c| c == '>') {
                    i += 1 + rel + 1;
                    continue;
                }
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    clean_index_text(&out, max_len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_html_tags_keeps_inner_text() {
        assert_eq!(to_index_plain_text("<b>bold</b> text", 100), "bold text");
        assert_eq!(
            to_index_plain_text("<script>alert(1)</script>x", 100),
            "alert(1)x"
        );
        assert_eq!(
            to_index_plain_text("中文<b>标签</b>内容", 100),
            "中文标签内容"
        );
        assert_eq!(to_index_plain_text("a<div class='x'>b</div>c", 100), "abc");
    }

    #[test]
    fn keeps_non_tag_angle_operators() {
        assert_eq!(to_index_plain_text("x < y 且 z > 0", 100), "x < y 且 z > 0");
        assert_eq!(to_index_plain_text("<3 爱心", 100), "<3 爱心");
        assert_eq!(to_index_plain_text("成本 ≤100 元", 100), "成本 ≤100 元");
    }

    #[test]
    fn truncates_on_char_boundary_and_cleans_whitespace() {
        let s = "中文测试文本".repeat(20);
        let out = to_index_plain_text(&s, 10);
        assert_eq!(out.chars().count(), 10);
        assert!(s.starts_with(&out));

        assert_eq!(to_index_plain_text("a\tb\nc", 100), "a b c");
        assert_eq!(to_index_plain_text("", 100), "");
        assert_eq!(to_index_plain_text("anything", 0), "");
    }

    #[test]
    fn result_passes_vet_gate() {
        use crate::search::vet_index_text;
        // 转换后不得残留 HTML（P0 门）：标签剥离后是纯文本。
        let converted = to_index_plain_text("<div>安全</div><script>x</script>", 100);
        assert!(!converted.contains('<'), "标签必须被剥离");
        assert_eq!(converted, "安全x");
        assert!(vet_index_text(&converted).is_ok());

        // canary：公开投影中若出现受限特征串文字，vet 仍拒绝（防受限内容混入）。
        let leaky = to_index_plain_text("<b>restricted_html</b>", 100);
        assert_eq!(leaky, "restricted_html");
        assert!(
            vet_index_text(&leaky).is_err(),
            "受限特征串文字必须被 vet 拒绝（canary）"
        );
    }
}
