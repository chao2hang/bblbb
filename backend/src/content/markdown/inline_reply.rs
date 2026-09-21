//! 帖子正文内嵌「回复可见」区块解析与渲染。
//!
//! 支持以下标记语法（支持单行或跨多行，标签大小写不敏感）：
//! - Markdown 指令块：`:::reply\n...\n:::` 或 `:::hide\n...\n:::`
//! - BBCode 标签：`[reply]...[/reply]` 或 `[hide]...[/hide]`
//!
//! 安全约定：
//! - 未解锁状态下，隐藏正文绝不渲染进 HTML，替换为内联受限占位卡片；
//! - 已解锁状态下，隐藏正文经完整渲染清洗管线，置于专属解锁卡片中；
//! - 公开摘要生成（`render_public_excerpt`）与索引输入必须通过 `strip_inline_reply`
//!   剔除所有受限文本，严防摘要泄漏。

/// 正文切片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownSegment<'a> {
    Public(&'a str),
    ReplyHidden(&'a str),
}

/// 快速检测 Markdown 原文中是否可能包含内嵌回复可见标记。
pub fn has_inline_reply(markdown: &str) -> bool {
    let lower = markdown.to_ascii_lowercase();
    if !lower.contains(":::reply")
        && !lower.contains(":::hide")
        && !lower.contains("[reply]")
        && !lower.contains("[hide]")
    {
        return false;
    }
    split_inline_reply(markdown)
        .iter()
        .any(|s| matches!(s, MarkdownSegment::ReplyHidden(_)))
}

/// 将 Markdown 文本切分为公开段与回复可见段。
pub fn split_inline_reply(markdown: &str) -> Vec<MarkdownSegment<'_>> {
    let mut segments = Vec::new();
    let mut cursor = 0;
    let len = markdown.len();

    while cursor < len {
        let remaining = &markdown[cursor..];

        // 寻找最近的合法区块起点
        let mut best_match: Option<(usize, usize, usize, usize)> = None;
        // (block_start, content_start, content_end, block_end)

        // 1. 扫描 `:::reply` 或 `:::hide`
        let mut search_pos = 0;
        while let Some(rel_idx) = remaining[search_pos..].find(":::") {
            let abs_rel = search_pos + rel_idx;
            let is_line_start = abs_rel == 0 || remaining.as_bytes()[abs_rel - 1] == b'\n';
            if is_line_start {
                let tag_rest = &remaining[abs_rel + 3..];
                let trimmed = tag_rest.trim_start_matches([' ', '\t']);
                let is_reply = trimmed
                    .strip_prefix("reply")
                    .or_else(|| trimmed.strip_prefix("REPLY"))
                    .or_else(|| trimmed.strip_prefix("Reply"));
                let is_hide = if is_reply.is_none() {
                    trimmed
                        .strip_prefix("hide")
                        .or_else(|| trimmed.strip_prefix("HIDE"))
                        .or_else(|| trimmed.strip_prefix("Hide"))
                } else {
                    None
                };

                if let Some(after_tag) = is_reply.or(is_hide) {
                    if let Some(newline_pos) = after_tag.find('\n') {
                        let line_end = &after_tag[..newline_pos];
                        if line_end.trim().is_empty() {
                            let content_start =
                                abs_rel + 3 + (tag_rest.len() - after_tag.len()) + newline_pos + 1;
                            let inner_rest = &remaining[content_start..];
                            let mut close_pos = 0;
                            while let Some(c_idx) = inner_rest[close_pos..].find(":::") {
                                let c_abs = close_pos + c_idx;
                                let c_line_start =
                                    c_abs == 0 || inner_rest.as_bytes()[c_abs - 1] == b'\n';
                                if c_line_start {
                                    let after_close = &inner_rest[c_abs + 3..];
                                    let close_len = if let Some(close_nl) = after_close.find('\n') {
                                        if after_close[..close_nl].trim().is_empty() {
                                            Some(c_abs + 3 + close_nl + 1)
                                        } else {
                                            None
                                        }
                                    } else if after_close.trim().is_empty() {
                                        Some(inner_rest.len())
                                    } else {
                                        None
                                    };

                                    if let Some(total_close_end) = close_len {
                                        let mut content_end = content_start + c_abs;
                                        if content_end > content_start
                                            && remaining.as_bytes()[content_end - 1] == b'\n'
                                        {
                                            if content_end - 1 > content_start
                                                && remaining.as_bytes()[content_end - 2] == b'\r'
                                            {
                                                content_end -= 2;
                                            } else {
                                                content_end -= 1;
                                            }
                                        }
                                        let block_end = content_start + total_close_end;
                                        best_match =
                                            Some((abs_rel, content_start, content_end, block_end));
                                        break;
                                    }
                                }
                                close_pos = c_abs + 3;
                            }
                            if best_match.is_some() {
                                break;
                            }
                        }
                    }
                }
            }
            search_pos = abs_rel + 3;
        }

        // 2. 扫描 `[reply]` 或 `[hide]`
        let mut bbcode_search = 0;
        while let Some(b_idx) = remaining[bbcode_search..].find('[') {
            let b_abs = bbcode_search + b_idx;
            let after_bracket = &remaining[b_abs + 1..];
            let tag_name = if after_bracket.to_ascii_lowercase().starts_with("reply]") {
                Some(("reply", b_abs + 1 + 6))
            } else if after_bracket.to_ascii_lowercase().starts_with("hide]") {
                Some(("hide", b_abs + 1 + 5))
            } else {
                None
            };

            if let Some((tag, content_start)) = tag_name {
                let close_tag = format!("[/{tag}]");
                let inner_rest = &remaining[content_start..];
                let lower_inner = inner_rest.to_ascii_lowercase();
                if let Some(close_idx) = lower_inner.find(&close_tag) {
                    let content_end = content_start + close_idx;
                    let block_end = content_end + close_tag.len();

                    if best_match.is_none() || b_abs < best_match.unwrap().0 {
                        best_match = Some((b_abs, content_start, content_end, block_end));
                    }
                    break;
                }
            }
            bbcode_search = b_abs + 1;
        }

        if let Some((block_start, content_start, content_end, block_end)) = best_match {
            if block_start > 0 {
                segments.push(MarkdownSegment::Public(&remaining[..block_start]));
            }
            segments.push(MarkdownSegment::ReplyHidden(
                &remaining[content_start..content_end],
            ));
            cursor += block_end;
        } else {
            segments.push(MarkdownSegment::Public(remaining));
            break;
        }
    }

    segments
}

/// 剥离所有回复可见区块，仅保留公开段（用于公开摘要和搜索索引安全清洗）。
pub fn strip_inline_reply(markdown: &str) -> String {
    let segments = split_inline_reply(markdown);
    let mut out = String::new();
    for seg in segments {
        if let MarkdownSegment::Public(s) = seg {
            out.push_str(s);
        }
    }
    out
}

/// 渲染包含内嵌回复可见区块的完整正文。
///
/// - `unlocked == true`：隐藏内容渲染进解锁卡片；
/// - `unlocked == false`：隐藏内容彻底剥离，输出锁定提示卡片。
pub fn render_with_inline_reply(markdown: &str, unlocked: bool) -> String {
    let segments = split_inline_reply(markdown);
    let mut out = String::new();

    for seg in segments {
        match seg {
            MarkdownSegment::Public(s) => {
                if !s.is_empty() {
                    out.push_str(&super::render_and_sanitize(s));
                }
            }
            MarkdownSegment::ReplyHidden(inner) => {
                if unlocked {
                    let rendered_inner = super::render_and_sanitize(inner.trim());
                    out.push_str(&format!(
                        r##"<div class="topic-unlocked topic-unlocked--inline restricted-unlocked" role="region" aria-label="回复可见内容已解锁"><div class="topic-unlocked__header"><span class="topic-unlocked__icon"><svg class="icon icon-unlock" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><rect width="18" height="11" x="3" y="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 9.9-1"></path></svg></span><div class="topic-unlocked__meta"><span class="topic-unlocked__title">回复可见内容已解锁</span><span class="topic-unlocked__desc">以下为解锁后的隐藏内容</span></div><span class="topic-unlocked__badge"><svg class="icon icon-check" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><path d="M20 6 9 17l-5-5"></path></svg><span>已解锁</span></span></div><div class="topic-unlocked__divider"></div><div class="prose topic-unlocked__prose">{rendered_inner}</div></div>"##
                    ));
                } else {
                    out.push_str(
                        r##"<aside class="topic-restricted topic-restricted--inline" role="note" aria-label="此处内容回复后可见"><span class="topic-restricted__icon"><svg class="icon icon-lock" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true" focusable="false"><rect width="18" height="11" x="3" y="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg></span><div class="topic-restricted__body"><h3 style="margin:0;font-size:14px;font-weight:600;color:var(--color-text-primary);">此处内容回复后可见</h3><p class="text-secondary" style="margin:4px 0 0;font-size:13px;">请参与本帖回复后查看隐藏内容。</p><div style="margin-top:8px;"><a href="#comment-input" class="btn primary sm" style="text-decoration:none;display:inline-flex;align-items:center;gap:4px;padding:4px 12px;font-size:12px;border-radius:var(--radius-sm);background:var(--color-brand);color:var(--on-brand,#fff);">去回复</a></div></div></aside>"##,
                    );
                }
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_directive_syntax() {
        let md = "前文\n\n:::reply\n隐藏提取码：123456\n:::\n\n后文";
        let segs = split_inline_reply(md);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], MarkdownSegment::Public("前文\n\n"));
        assert_eq!(segs[1], MarkdownSegment::ReplyHidden("隐藏提取码：123456"));
        assert_eq!(segs[2], MarkdownSegment::Public("\n后文"));
    }

    #[test]
    fn test_split_bbcode_syntax() {
        let md = "说明 [hide]这是提取码：ABC[/hide] 补充";
        let segs = split_inline_reply(md);
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0], MarkdownSegment::Public("说明 "));
        assert_eq!(segs[1], MarkdownSegment::ReplyHidden("这是提取码：ABC"));
        assert_eq!(segs[2], MarkdownSegment::Public(" 补充"));
    }

    #[test]
    fn test_strip_inline_reply() {
        let md = "前文\n\n:::reply\n秘密\n:::\n\n后文";
        let stripped = strip_inline_reply(md);
        assert_eq!(stripped, "前文\n\n\n后文");
        assert!(!stripped.contains("秘密"));
    }

    #[test]
    fn test_render_locked_hides_secret() {
        let md = "公开段落\n\n:::reply\nSECRET_CODE_777\n:::\n\n结尾段落";
        let html = render_with_inline_reply(md, false);
        assert!(!html.contains("SECRET_CODE_777"));
        assert!(html.contains("公开段落"));
        assert!(html.contains("结尾段落"));
        assert!(html.contains("此处内容回复后可见"));
        assert!(html.contains("topic-restricted--inline"));
    }

    #[test]
    fn test_render_unlocked_shows_secret_with_style() {
        let md = "公开段落\n\n:::reply\nSECRET_CODE_777\n:::\n\n结尾段落";
        let html = render_with_inline_reply(md, true);
        assert!(html.contains("SECRET_CODE_777"));
        assert!(html.contains("公开段落"));
        assert!(html.contains("结尾段落"));
        assert!(html.contains("topic-unlocked--inline"));
        assert!(html.contains("回复可见内容已解锁"));
    }

    #[test]
    fn test_multiple_inline_blocks() {
        let md = "第一段\n\n:::reply\n密码1\n:::\n\n第二段\n\n[hide]密码2[/hide]\n\n第三段";
        let locked = render_with_inline_reply(md, false);
        assert!(!locked.contains("密码1"));
        assert!(!locked.contains("密码2"));
        assert!(locked.contains("第一段"));
        assert!(locked.contains("第二段"));
        assert!(locked.contains("第三段"));

        let unlocked = render_with_inline_reply(md, true);
        assert!(unlocked.contains("密码1"));
        assert!(unlocked.contains("密码2"));
        assert!(unlocked.contains("第一段"));
        assert!(unlocked.contains("第二段"));
        assert!(unlocked.contains("第三段"));
    }
}
