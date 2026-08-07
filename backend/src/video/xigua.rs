//! 西瓜官方公开页面/嵌入 Host 白名单与视频 id 提取（M10-VIDEO-06）。
//!
//! 只允许精确的西瓜 HTTPS 主域名和官方嵌入 Host；拒绝抓取、转存、破解和绕过
//! 鉴权。视频页形态：`/video/{id}`、`/item/{id}`。适配器只处理公开页面引用
//! 与官方嵌入；无稳定嵌入协议时降级安全外链（不抓播放地址、不绕过登录/签名/
//! 地域/DRM）。

/// 西瓜官方公开页面 Host（精确匹配，文档见 docs/VIDEO-PLUGIN.md §1）。
pub struct XiguaHosts;

impl XiguaHosts {
    /// 官方公开页面 Host（只允许这些精确域名）。
    pub const PAGE_HOSTS: &'static [&'static str] =
        &["www.ixigua.com", "m.ixigua.com", "www.xigua.com"];
    /// 官方嵌入 Host（iframe 只允许此精确 Origin）。
    pub const EMBED_HOST: &'static str = "www.ixigua.com";
    /// 官方 iframe 路径前缀。
    pub const EMBED_PATH: &'static str = "/iframe/";
}

/// host（规范化小写形态）是否属于西瓜官方公开页面 Host。
pub fn is_xigua_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_lowercase();
    XiguaHosts::PAGE_HOSTS.contains(&host.as_str())
}

/// 是否为西瓜公开视频页路径（`/video/{id}` 或 `/item/{id}`）。
pub fn is_xigua_page_path(path: &str) -> bool {
    extract_video_id(path).is_some()
}

/// 从路径提取平台视频 id（非敏感公开 id；8-64 位字母数字）。
pub fn extract_video_id(path: &str) -> Option<String> {
    let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    if segments.len() == 2 && (segments[0] == "video" || segments[0] == "item") {
        let id = segments[1];
        if (8..=64).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return Some(id.to_string());
        }
    }
    None
}

/// 官方 iframe URL（仅当嵌入允许且 host 命中官方嵌入 Host 时使用）。
pub fn iframe_url(video_id: &str) -> String {
    format!(
        "https://{}{}{}",
        XiguaHosts::EMBED_HOST,
        XiguaHosts::EMBED_PATH,
        video_id
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn official_hosts_only() {
        assert!(is_xigua_host("www.ixigua.com"));
        assert!(is_xigua_host("m.ixigua.com"));
        assert!(is_xigua_host("www.xigua.com"));
        assert!(!is_xigua_host("ixigua.com"));
        assert!(!is_xigua_host("evil-ixigua.com"));
        assert!(!is_xigua_host("www.ixigua.com.evil.com"));
        assert!(!is_xigua_host("static.ixigua.com"));
    }

    #[test]
    fn extracts_video_ids() {
        assert_eq!(
            extract_video_id("/video/7301234567890123456"),
            Some("7301234567890123456".to_string())
        );
        assert_eq!(
            extract_video_id("/item/7301234567890123456"),
            Some("7301234567890123456".to_string())
        );
        assert_eq!(
            extract_video_id("/video/7301234567890123456/"),
            Some("7301234567890123456".to_string())
        );
        assert_eq!(extract_video_id("/"), None);
        assert_eq!(extract_video_id("/channel/1"), None);
        assert_eq!(extract_video_id("/video/abc"), None); // 太短
        assert_eq!(extract_video_id("/video/../../.."), None);
        assert_eq!(extract_video_id("/subscribe/abc"), None);
    }

    #[test]
    fn iframe_built_from_official_embed_host() {
        let url = iframe_url("7301234567890123456");
        assert_eq!(url, "https://www.ixigua.com/iframe/7301234567890123456");
        assert!(url.starts_with("https://www.ixigua.com"));
    }
}
