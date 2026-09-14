//! 站内附件引用的读取时占位（M06-QUOTA-09 展示端约定）。
//!
//! 帖子正文/回复的 Markdown 里以 `/api/v1/attachments/{id}/content` 同源
//! 相对路径引用附件（编辑器上传产物，见前端 `attachmentContentUrl`）。
//! 附件删除后进入 30 天保留期（软删 `status='deleted'`），内容端点对
//! 非 `ready` 附件一律 404——若仍按落库 HTML 原样渲染，正文里会留下
//! 死链/碎图。本模块在**读取时**把引用了不可用附件的 `<img>`/`<a>`
//! 整元素替换为「附件已删除」占位（落库 HTML 与 Markdown 原文不变，
//! 与 @提及链接化同为"清洗后 HTML 的确定性后处理"）。
//!
//! 约定：
//!
//! - **不可用判定与内容端点一致**：附件行缺失，或 `status != 'ready'`
//!   （含 pending/processing/quarantined/deleted）。即：凡 content 端点
//!   会 404 的引用，都渲染占位；owner 也不例外（端点对非 ready 同样 404）。
//! - **替换作用于标签级**：只重写 `<img src=...>` 整标签与
//!   `<a href=...>…</a>` 整元素；正文代码块/行内代码中的 URL 文本
//!   （`<code>/<pre>` 内）不是标签属性，不受影响。
//! - **占位 HTML 由本模块构造**，不含用户可控字符，注入于清洗之后
//!   （与 [`crate::content::mentions`] 生成的锚点同一信任级别）。
//! - **渲染管线保持纯函数**：本模块的 HTML 改写不访问 DB；DB 状态查询
//!   由调用方（路由/投影层）完成后传入（[`replace_unavailable_attachments`]）。

use sqlx::Either;

use crate::content::markdown::sanitize::ATTACHMENT_CONTENT_PREFIX;
use crate::db::DatabasePool;

/// 引用不可用附件时的占位 HTML。
pub const UNAVAILABLE_PLACEHOLDER: &str =
    "<span class=\"attachment-unavailable\">附件已删除</span>";

/// 单次占位判定最多查询的附件引用数（防御性上限；正文长度本身有渲染上限）。
const MAX_ATTACHMENT_REFS: usize = 64;

/// 从文本（Markdown 原文或渲染后 HTML）提取站内附件内容端点引用的附件 id
/// （保持出现顺序、去重、限量 [`MAX_ATTACHMENT_REFS`]）。
///
/// 形态：`ATTACHMENT_CONTENT_PREFIX + {id} + /content`。id 为 UUID（小写
/// hex + `-`）；这里放宽为"不含分隔符的 URL 段"，随后用 `/content` 后缀
/// 收口——拼错的 id 查不到行，同样归入不可用（与内容端点 404 行为一致）。
pub fn extract_attachment_content_ids(text: &str) -> Vec<String> {
    const SUFFIX: &str = "/content";
    let mut out: Vec<String> = Vec::new();
    if !text.contains(ATTACHMENT_CONTENT_PREFIX) {
        return out;
    }
    let bytes = text.as_bytes();
    let mut cursor = 0usize;
    while let Some(pos) = text[cursor..].find(ATTACHMENT_CONTENT_PREFIX) {
        let id_start = cursor + pos + ATTACHMENT_CONTENT_PREFIX.len();
        let mut id_end = id_start;
        while id_end < bytes.len() {
            let b = bytes[id_end];
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' {
                id_end += 1;
            } else {
                break;
            }
        }
        cursor = id_end.max(id_start + 1);
        if id_end == id_start {
            continue; // 空段（如 `attachments//content`）——跳过
        }
        if !bytes[id_end..].starts_with(SUFFIX.as_bytes()) {
            continue; // 非 content 端点（元数据/下载端点等）不参与占位判定
        }
        let id = text[id_start..id_end].to_string();
        if !out.contains(&id) {
            out.push(id);
            if out.len() >= MAX_ATTACHMENT_REFS {
                break;
            }
        }
    }
    out
}

/// URL 属性值（`img[src]` / `a[href]`）→ 附件内容端点 id。
///
/// 放行编辑器产出的同源相对路径与绝对 URL（scheme+host 剥离后按 path
/// 匹配：同一后端可能经多个 origin 暴露，清洗层对绝对 http(s) 值也不限
/// 主机）；query/hash 忽略。其余形态返回 `None`。
pub fn attachment_id_of_url(value: &str) -> Option<String> {
    let raw = value.split(['?', '#']).next()?;
    let path = if let Some(rest) = raw
        .strip_prefix("http://")
        .or_else(|| raw.strip_prefix("https://"))
    {
        let idx = rest.find('/')?;
        &rest[idx..]
    } else {
        raw
    };
    let id_and_rest = path.strip_prefix(ATTACHMENT_CONTENT_PREFIX)?;
    let (id, rest) = id_and_rest.split_once('/')?;
    if id.is_empty() || rest != "content" {
        return None;
    }
    Some(id.to_string())
}

/// 查询候选附件中**当前不可用**（行缺失或 `status != 'ready'`）的 id 子集。
///
/// 判定口径与 `GET /api/v1/attachments/{id}/content` 一致：非 ready 一律
/// 404（deleted 进入保留期、pending/processing 未就绪、quarantined 隔离）。
pub async fn load_unavailable_attachment_ids(
    pool: &DatabasePool,
    attachment_ids: &[String],
) -> Result<Vec<String>, sqlx::Error> {
    if attachment_ids.is_empty() {
        return Ok(Vec::new());
    }
    let placeholders = vec!["?"; attachment_ids.len()].join(", ");
    let sql = format!("SELECT id, status FROM attachments WHERE id IN ({placeholders})");
    let rows: Vec<(String, String)> = match pool {
        Either::Left(p) => {
            let mut q = sqlx::query_as::<_, (String, String)>(&sql);
            for id in attachment_ids {
                q = q.bind(id);
            }
            q.fetch_all(p).await?
        }
        Either::Right(p) => {
            let mut q = sqlx::query_as::<_, (String, String)>(&sql);
            for id in attachment_ids {
                q = q.bind(id);
            }
            q.fetch_all(p).await?
        }
    };
    let mut unavailable = Vec::new();
    for id in attachment_ids {
        let known_ready = rows
            .iter()
            .any(|(rid, status)| rid == id && status.eq_ignore_ascii_case("ready"));
        if !known_ready {
            unavailable.push(id.clone());
        }
    }
    Ok(unavailable)
}

/// 把清洗后 HTML 中引用 [`attachment_id_of_url`] 命中且 id 属于
/// `unavailable` 的 `<img>` 标签 / `<a href>` 元素整体替换为
/// [`UNAVAILABLE_PLACEHOLDER`]。输入必须是本管线 [`crate::content::
/// markdown::sanitize_html`] 的产物（well-formed，属性值双引号）；
/// `unavailable` 为空或正文不含附件端点前缀时原样返回（零开销）。
pub fn replace_unavailable_attachments(html: &str, unavailable: &[String]) -> String {
    if unavailable.is_empty() || !html.contains(ATTACHMENT_CONTENT_PREFIX) {
        return html.to_string();
    }
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            let next = bytes[i..]
                .iter()
                .position(|&b| b == b'<')
                .map(|p| i + p)
                .unwrap_or(bytes.len());
            out.push_str(&html[i..next]);
            i = next;
            continue;
        }
        // 标签区域：整体扫描（引号属性值内可能出现 '>'，与 mentions 扫描器一致）
        let tag_start = i;
        let mut j = i + 1;
        let mut quote: Option<u8> = None;
        while j < bytes.len() {
            let b = bytes[j];
            if let Some(q) = quote {
                if b == q {
                    quote = None;
                }
            } else if b == b'"' || b == b'\'' {
                quote = Some(b);
            } else if b == b'>' {
                break;
            }
            j += 1;
        }
        let tag_end = (j + 1).min(bytes.len());
        let tag = &html[tag_start..tag_end];
        let lower = tag.to_ascii_lowercase();
        if !lower.starts_with("</") {
            let name = lower
                .trim_start_matches('<')
                .split(|c: char| !c.is_ascii_alphanumeric())
                .find(|s| !s.is_empty())
                .unwrap_or("");
            if name == "img" {
                if tag
                    .attr_value("src")
                    .and_then(attachment_id_of_url)
                    .is_some_and(|id| unavailable.iter().any(|u| u == &id))
                {
                    out.push_str(UNAVAILABLE_PLACEHOLDER);
                    i = tag_end;
                    continue;
                }
            } else if name == "a"
                && tag
                    .attr_value("href")
                    .and_then(attachment_id_of_url)
                    .is_some_and(|id| unavailable.iter().any(|u| u == &id))
            {
                // 引用不可用附件的链接：连同链接文本一起替换为占位。
                // （清洗产物中 <a> 不嵌套——CommonMark 不允许嵌套链接，
                // 原始 HTML 已剥离，提及链接化跳过 <a> 内部文本。）
                match find_close_anchor(html, tag_end) {
                    Some(close_end) => {
                        out.push_str(UNAVAILABLE_PLACEHOLDER);
                        i = close_end;
                        continue;
                    }
                    None => {
                        // 防御：找不到闭合标签时不改写，原样拷贝余下内容
                        out.push_str(&html[tag_start..]);
                        i = bytes.len();
                        continue;
                    }
                }
            }
        }
        out.push_str(tag);
        i = tag_end;
    }
    out
}

/// 从标签文本读取属性值（双/单引号；本管线产物为双引号，解析保持宽容）。
trait TagAttr {
    fn attr_value(&self, name: &str) -> Option<&str>;
}

impl TagAttr for str {
    fn attr_value(&self, name: &str) -> Option<&str> {
        let bytes = self.as_bytes();
        let mut i = 1usize; // 跳过 '<'
                            // 跳过标签名
        while i < bytes.len()
            && !bytes[i].is_ascii_whitespace()
            && bytes[i] != b'>'
            && bytes[i] != b'/'
        {
            i += 1;
        }
        while i < bytes.len() {
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i >= bytes.len() || bytes[i] == b'>' {
                return None;
            }
            let name_start = i;
            while i < bytes.len()
                && bytes[i] != b'='
                && !bytes[i].is_ascii_whitespace()
                && bytes[i] != b'>'
                && bytes[i] != b'/'
            {
                i += 1;
            }
            let attr_name = &self[name_start..i];
            while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                i += 1;
            }
            if i < bytes.len() && bytes[i] == b'=' {
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                let value_start;
                let value_end;
                if i < bytes.len() && (bytes[i] == b'"' || bytes[i] == b'\'') {
                    let q = bytes[i];
                    i += 1;
                    value_start = i;
                    while i < bytes.len() && bytes[i] != q {
                        i += 1;
                    }
                    value_end = i;
                    i += 1; // 闭合引号
                } else {
                    value_start = i;
                    while i < bytes.len() && !bytes[i].is_ascii_whitespace() && bytes[i] != b'>' {
                        i += 1;
                    }
                    value_end = i;
                }
                if attr_name.eq_ignore_ascii_case(name) {
                    return Some(&self[value_start..value_end]);
                }
            }
        }
        None
    }
}

/// 从 `from` 起找下一个 `</a>`（不区分大小写），返回其结束位置。
fn find_close_anchor(html: &str, from: usize) -> Option<usize> {
    let bytes = html.as_bytes();
    let mut i = from;
    while i + 4 <= bytes.len() {
        if bytes[i] == b'<'
            && bytes[i + 1] == b'/'
            && (bytes[i + 2] == b'a' || bytes[i + 2] == b'A')
            && bytes[i + 3] == b'>'
        {
            return Some(i + 4);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- extract_attachment_content_ids ----

    #[test]
    fn extract_ids_from_markdown_and_dedupe() {
        let md = "![a](/api/v1/attachments/01aa-2/content) 前文 \
                  [📎 附件: x](/api/v1/attachments/01aa-2/content) \
                  [y](/api/v1/attachments/01bb-3/content)";
        assert_eq!(extract_attachment_content_ids(md), vec!["01aa-2", "01bb-3"]);
    }

    #[test]
    fn extract_ignores_other_attachment_endpoints() {
        // 元数据/下载端点与无 /content 后缀的路径不参与
        assert!(extract_attachment_content_ids(
            "[meta](/api/v1/attachments/01aa-2) [dl](/api/v1/attachments/01aa-2/download)"
        )
        .is_empty());
    }

    #[test]
    fn extract_over_approximates_from_markdown_including_code_spans() {
        // Markdown 原文不做语法区分（代码段里的 URL 也会进候选集）：
        // 只影响一次 IN 查询的候选范围；实际改写在 HTML 标签级做，
        // <code>/<pre> 内文本不是标签属性，不会被替换。
        assert_eq!(
            extract_attachment_content_ids("看代码 `/api/v1/attachments/x1/content`"),
            vec!["x1"]
        );
        assert!(extract_attachment_content_ids("普通文本").is_empty());
    }

    #[test]
    fn extract_is_capped() {
        let ids: Vec<String> = (0..100).map(|i| format!("{i:032}")).collect();
        let md: String = ids
            .iter()
            .map(|id| format!("![x](/api/v1/attachments/{id}/content) "))
            .collect();
        assert_eq!(
            extract_attachment_content_ids(&md).len(),
            MAX_ATTACHMENT_REFS
        );
    }

    // ---- attachment_id_of_url ----

    #[test]
    fn url_id_matches_relative_and_absolute_forms() {
        assert_eq!(
            attachment_id_of_url("/api/v1/attachments/01aa-2/content"),
            Some("01aa-2".into())
        );
        assert_eq!(
            attachment_id_of_url("https://10.10.10.10:5173/api/v1/attachments/01aa-2/content"),
            Some("01aa-2".into())
        );
        assert_eq!(
            attachment_id_of_url("/api/v1/attachments/01aa-2/content?v=1"),
            Some("01aa-2".into())
        );
        // 非内容端点 / 空段。绝对 URL 按路径匹配（同一后端可能经多个
        // origin 暴露：localhost / 局域网 IP:端口 / 域名，清洗层对
        // a[href]/img[src] 的绝对 http(s) 值也不限主机）。
        assert_eq!(
            attachment_id_of_url("/api/v1/attachments/01aa-2/download"),
            None
        );
        assert_eq!(
            attachment_id_of_url("https://other.example/api/v1/attachments/01aa-2/content"),
            Some("01aa-2".into())
        );
        assert_eq!(attachment_id_of_url("/api/v1/attachments//content"), None);
        assert_eq!(attachment_id_of_url("https://host.example"), None);
    }

    // ---- replace_unavailable_attachments ----

    fn unavailable(id: &str) -> Vec<String> {
        vec![id.to_string()]
    }

    #[test]
    fn replaces_deleted_img_and_link() {
        let html = "<p><img src=\"/api/v1/attachments/att-1/content\" alt=\"下载.png\" /></p>";
        let out = replace_unavailable_attachments(html, &unavailable("att-1"));
        assert_eq!(
            out,
            format!("<p>{UNAVAILABLE_PLACEHOLDER}</p>"),
            "img 整标签替换: {out}"
        );

        let html = "<p><a href=\"/api/v1/attachments/att-1/content\" target=\"_blank\" rel=\"nofollow noopener noreferrer\">📎 附件: 下载.png</a></p>";
        let out = replace_unavailable_attachments(html, &unavailable("att-1"));
        assert_eq!(
            out,
            format!("<p>{UNAVAILABLE_PLACEHOLDER}</p>"),
            "a 整元素替换: {out}"
        );
    }

    #[test]
    fn keeps_ready_attachments_and_unrelated_urls() {
        let html = "<p><img src=\"/api/v1/attachments/att-1/content\" alt=\"ok\" /><img src=\"/api/v1/attachments/att-2/content\" alt=\"keep\" /></p>";
        let out = replace_unavailable_attachments(html, &unavailable("att-1"));
        assert!(out.contains("att-2"), "可用附件保留: {out}");
        assert!(!out.contains("att-1"), "不可用附件替换: {out}");
        assert!(out.contains(UNAVAILABLE_PLACEHOLDER));

        let html = "<p><a href=\"https://ok.example/a.png\">外链</a></p>";
        assert_eq!(
            replace_unavailable_attachments(html, &unavailable("att-1")),
            html,
            "外链不受影响"
        );
    }

    #[test]
    fn absolute_same_origin_attachment_url_is_replaced() {
        let html = "<p><img src=\"http://10.10.10.10:5173/api/v1/attachments/att-1/content\" alt=\"x\" /></p>";
        let out = replace_unavailable_attachments(html, &unavailable("att-1"));
        assert_eq!(out, format!("<p>{UNAVAILABLE_PLACEHOLDER}</p>"), "{out}");
    }

    #[test]
    fn code_span_urls_are_untouched() {
        let html = "<p><code>/api/v1/attachments/att-1/content</code></p>";
        assert_eq!(
            replace_unavailable_attachments(html, &unavailable("att-1")),
            html,
            "代码文本不是标签属性，不改写"
        );
    }

    #[test]
    fn nested_img_inside_deleted_link_collapses_to_one_placeholder() {
        let html = "<p><a href=\"/api/v1/attachments/att-1/content\" target=\"_blank\" rel=\"nofollow noopener noreferrer\"><img src=\"/api/v1/attachments/att-1/content\" alt=\"x\" /></a></p>";
        let out = replace_unavailable_attachments(html, &unavailable("att-1"));
        assert_eq!(out, format!("<p>{UNAVAILABLE_PLACEHOLDER}</p>"), "{out}");
    }

    #[test]
    fn multiple_occurrences_replaced_and_order_kept() {
        let html = "<p><img src=\"/api/v1/attachments/a1/content\" alt=\"1\" />中间文本<img src=\"/api/v1/attachments/a2/content\" alt=\"2\" /></p>";
        let unavailable = vec!["a1".to_string(), "a2".to_string()];
        let out = replace_unavailable_attachments(html, &unavailable);
        assert_eq!(
            out,
            format!("<p>{UNAVAILABLE_PLACEHOLDER}中间文本{UNAVAILABLE_PLACEHOLDER}</p>"),
            "{out}"
        );
    }

    #[test]
    fn empty_unavailable_returns_input_unchanged() {
        let html = "<p><img src=\"/api/v1/attachments/a1/content\" alt=\"1\" /></p>";
        assert_eq!(replace_unavailable_attachments(html, &[]), html);
    }

    #[test]
    fn unclosed_anchor_falls_back_to_passthrough() {
        let html = "<p><a href=\"/api/v1/attachments/att-1/content\">没闭合";
        assert_eq!(
            replace_unavailable_attachments(html, &unavailable("att-1")),
            html,
            "找不到 </a> 时不改写"
        );
    }

    // ---- 端到端：渲染管线产物上的占位（render + sanitize + 替换） ----

    #[test]
    fn end_to_end_pipeline_renders_placeholder_for_deleted_image() {
        let markdown = "![下载.png](/api/v1/attachments/att-1/content)\n";
        let html = crate::content::markdown::sanitize::sanitize_html(
            &crate::content::markdown::render::render_to_html(markdown),
        );
        assert!(
            html.contains("src=\"/api/v1/attachments/att-1/content\""),
            "清洗产物保留附件 src: {html}"
        );
        let out = replace_unavailable_attachments(&html, &unavailable("att-1"));
        assert!(
            out.contains(UNAVAILABLE_PLACEHOLDER) && !out.contains("att-1"),
            "读取时替换为占位: {out}"
        );
    }
}
