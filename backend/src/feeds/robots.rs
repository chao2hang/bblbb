//! robots.txt / `X-Robots-Tag` / meta noindex（M08-FEEDS-04）。
//!
//! - [`render_robots`]：动态 robots.txt——默认允许明确公开的路径，管理/私有
//!   API 一律 Disallow；**AI 训练爬虫默认拒绝**（GPTBot/CCBot/Google-Extended/
//!   ClaudeBot，CRAWLER-POLICY §2）；文件末尾声明 robots 只是爬虫声明层，
//!   不是安全边界（服务端鉴权/授权始终独立执行）；
//! - [`x_robots_tag`] / [`meta_robots`]：按 index policy 输出
//!   `X-Robots-Tag`/`<meta name="robots">` 决策——不明确允许索引的内容一律
//!   `noindex, nofollow, noarchive`。

/// 默认拒绝的 AI 训练爬虫（CRAWLER-POLICY §2 初始名单；后台可配置）。
pub const AI_TRAINING_CRAWLERS: &[&str] = &[
    "GPTBot",
    "CCBot",
    "Google-Extended",
    "ClaudeBot",
    "PerplexityBot",
];

/// 渲染动态 robots.txt。
pub fn render_robots() -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("# BBLBB robots.txt（动态生成，M08-FEEDS-04）\n");
    out.push_str("# robots.txt 是爬虫声明层，不是安全边界；服务端授权始终独立执行。\n\n");

    out.push_str("User-agent: *\n");
    out.push_str("Allow: /\n");
    out.push_str("Disallow: /api/\n");
    out.push_str("Disallow: /admin/\n");
    out.push_str("Disallow: /search?\n");
    out.push_str("Disallow: /*/revisions\n");
    out.push_str("Disallow: /*/revisions/\n\n");

    for bot in AI_TRAINING_CRAWLERS {
        out.push_str(&format!("User-agent: {bot}\n"));
        out.push_str("Disallow: /\n\n");
    }
    out.push_str("# 其余未知/伪装机器人按普通访问处理，并参与行为风控（M08-CRAWL）。\n");
    out
}

/// `X-Robots-Tag` 值：允许索引 → `all`；否则 `noindex, nofollow, noarchive`。
pub fn x_robots_tag(index_allowed: bool) -> &'static str {
    if index_allowed {
        "all"
    } else {
        "noindex, nofollow, noarchive"
    }
}

/// `<meta name="robots">` content 值（前端 SSR 用；与 `X-Robots-Tag` 同源）。
pub fn meta_robots(index_allowed: bool) -> &'static str {
    if index_allowed {
        "index, follow"
    } else {
        "noindex, nofollow, noarchive"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn robots_disallows_api_admin_search_and_ai_crawlers() {
        let text = render_robots();
        assert!(text.contains("User-agent: *"));
        assert!(text.contains("Disallow: /api/"));
        assert!(text.contains("Disallow: /admin/"));
        assert!(text.contains("Disallow: /search?"));
        for bot in AI_TRAINING_CRAWLERS {
            assert!(
                text.contains(&format!("User-agent: {bot}")),
                "AI 训练爬虫 {bot} 必须默认拒绝"
            );
        }
        assert!(text.contains("不是安全边界"));
    }

    #[test]
    fn robots_tag_matches_meta() {
        assert_eq!(x_robots_tag(true), "all");
        assert_eq!(x_robots_tag(false), "noindex, nofollow, noarchive");
        assert_eq!(meta_robots(true), "index, follow");
        assert_eq!(meta_robots(false), "noindex, nofollow, noarchive");
    }
}
