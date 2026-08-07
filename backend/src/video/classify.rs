//! URL 解析与分类（M10-VIDEO-02/03/04）。
//!
//! 所有 resolve 入口必须先经过 [`validate_url_shape`]：标准 URL parser + 精确
//! scheme/host/port/userinfo/IPv4-IPv6 私网字面量/Unicode-IDN/签名 URL 限制，
//! 禁止字符串前缀判断、混淆 Host、userinfo、IDN 绕过与开放重定向。随后按
//! Host/扩展名分类到 Direct/HLS/Xigua Provider。
//!
//! 扩展名不是可信依据（docs/VIDEO-PLUGIN.md §1）：分类只做初判，最终 MIME
//! 由异步 refresh 的受限探测确认；这里绝不下载任意大文件。

use std::net::IpAddr;

use sha2::{Digest, Sha256};
use url::Url;

use crate::ai::gateway::is_private_ip;
use crate::video::xigua::{extract_video_id, is_xigua_host, is_xigua_page_path};
use crate::video::Provider;

/// 分类稳定错误（与 Problem `code` 一一对应；detail 由路由层给出，不回显源 URL）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClassifyError {
    /// URL 无法解析。
    InvalidUrl(String),
    /// 只允许 https。
    InsecureScheme,
    /// URL 包含 userinfo（username/password）。
    UserinfoNotAllowed,
    /// 端口不在允许集（默认仅 443）。
    PortNotAllowed(u16),
    /// host 是私网/回环/链路本地 IP 字面量，或疑似数字混淆 IP。
    PrivateIp(String),
    /// host 带签名查询参数（AWS/Google 签名 URL、token、signature 等）。
    SignedUrl,
    /// URL 带 fragment。
    FragmentNotAllowed,
    /// host 形态非法（localhost、单标签、纯数字/0x 混淆、控制字符）。
    HostInvalid(String),
    /// 不是支持的视频 URL 形态。
    UnsupportedType,
    /// 西瓜域名下的页面不是公开视频页。
    NotAVideoPage,
}

impl ClassifyError {
    pub fn code(&self) -> &'static str {
        match self {
            ClassifyError::InvalidUrl(_) => "video_invalid_url",
            ClassifyError::InsecureScheme => "video_insecure_scheme",
            ClassifyError::UserinfoNotAllowed => "video_userinfo_not_allowed",
            ClassifyError::PortNotAllowed(_) => "video_port_not_allowed",
            ClassifyError::PrivateIp(_) => "video_private_ip",
            ClassifyError::SignedUrl => "video_signed_url",
            ClassifyError::FragmentNotAllowed => "video_fragment_not_allowed",
            ClassifyError::HostInvalid(_) => "video_host_invalid",
            ClassifyError::UnsupportedType => "video_unsupported_type",
            ClassifyError::NotAVideoPage => "video_not_video_page",
        }
    }
}

/// 规范化后的 URL 与 host（后续出站/渲染只使用该结果，不再解析原始输入）。
#[derive(Debug, Clone)]
pub struct NormalizedUrl {
    pub url: Url,
    /// 小写、punycode、去尾点（Unicode/IDN 统一到此形态做 allowlist 匹配）。
    pub host: String,
}

/// 分类结果（只含非敏感安全元数据；签名/Key/iframe HTML 不落库、不回显）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Classified {
    pub provider: Provider,
    /// 直接媒体的预期 MIME（按扩展名初判；最终以 refresh 探测为准）。
    pub media_type: Option<String>,
    /// 西瓜公开页面的平台视频 id（非敏感）。
    pub external_id: Option<String>,
    /// 规范化官方 URL：direct/hls = 播放/源 URL，xigua = 规范化公开页面。
    pub official_url: String,
    /// 源 URL（规范化后；敏感 query 已在分类阶段拒绝）。
    pub source: String,
    /// source 的 SHA-256 hex（去重/审计，不回显源）。
    pub source_hash: String,
    /// 规范化 host（小写、punycode）。
    pub host: String,
    /// 是否可用官方嵌入（西瓜：仅精确官方嵌入 Host）。
    pub embeddable: bool,
    /// 公开元数据标题（当前不抓取第三方元数据，恒为 None；预留字段）。
    pub title: Option<String>,
}

/// 签名/访问凭证类 query key（大小写不敏感匹配）。命中即拒绝——不保存、不转发。
const SIGNATURE_QUERY_KEYS: &[&str] = &[
    "x-amz-algorithm",
    "x-amz-credential",
    "x-amz-date",
    "x-amz-expires",
    "x-amz-signedheaders",
    "x-amz-signature",
    "x-amz-security-token",
    "x-goog-algorithm",
    "x-goog-credential",
    "x-goog-date",
    "x-goog-expires",
    "x-goog-signedheaders",
    "x-goog-signature",
    "googleaccessid",
    "signature",
    "_signature",
    "sig",
    "sigv4",
    "signatureversion",
    "awsaccesskeyid",
    "key-pair-id",
    "policy",
    "expires",
    "token",
    "access_token",
    "x-signature",
    "x-saas",
    "signed",
];

/// 校验 URL 形状（解析前后双防线；host 为 IP 字面量时直接做私网检查）。
pub fn validate_url_shape(raw: &str) -> Result<NormalizedUrl, ClassifyError> {
    if raw.len() > 2048 {
        return Err(ClassifyError::InvalidUrl("url too long".into()));
    }
    let mut url = Url::parse(raw).map_err(|e| ClassifyError::InvalidUrl(e.to_string()))?;
    if url.scheme() != "https" {
        return Err(ClassifyError::InsecureScheme);
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(ClassifyError::UserinfoNotAllowed);
    }
    let port = url.port().unwrap_or(443);
    if port != 443 {
        return Err(ClassifyError::PortNotAllowed(port));
    }
    if url.fragment().is_some() {
        return Err(ClassifyError::FragmentNotAllowed);
    }
    // 签名/访问凭证检测（覆盖 query key 与整个 URL 字符串中的签名标记）。
    if has_signature_credentials(&url) {
        return Err(ClassifyError::SignedUrl);
    }

    // host 归一化：小写 + 去尾点（合法 FQDN 与 allowlist 匹配的统一形态）。
    let mut host = url
        .host_str()
        .ok_or_else(|| ClassifyError::HostInvalid("missing host".into()))?
        .to_lowercase();
    while host.ends_with('.') && host.len() > 1 {
        host.pop();
    }

    // host 为 IP 字面量时直接做私网检查（解析前防线；IPv6 走结构化 host，
    // 避免方括号/大小写差异绕过）。
    match url.host() {
        Some(url::Host::Ipv4(ip)) => {
            if is_private_ip(&IpAddr::V4(ip)) {
                return Err(ClassifyError::PrivateIp(ip.to_string()));
            }
        }
        Some(url::Host::Ipv6(ip)) => {
            if is_private_ip(&IpAddr::V6(ip)) {
                return Err(ClassifyError::PrivateIp(ip.to_string()));
            }
        }
        Some(url::Host::Domain(_)) => {}
        None => return Err(ClassifyError::HostInvalid("missing host".into())),
    }

    // 域名字面量形态限制：拒绝 localhost、单标签、纯数字/0x 混淆 host。
    validate_domain_shape(&host)?;

    url.set_fragment(None);
    Ok(NormalizedUrl { url, host })
}

/// 域名形态限制：localhost / 单标签 / 纯数字或 0x 混淆 / 控制字符全部拒绝。
fn validate_domain_shape(host: &str) -> Result<(), ClassifyError> {
    if host.is_empty() {
        return Err(ClassifyError::HostInvalid("empty host".into()));
    }
    if host.bytes().any(|b| b < 0x20 || b == 0x7f) {
        return Err(ClassifyError::HostInvalid(
            "control character in host".into(),
        ));
    }
    if host == "localhost" || host.ends_with(".localhost") {
        return Err(ClassifyError::HostInvalid("localhost".into()));
    }
    if !host.contains('.') {
        return Err(ClassifyError::HostInvalid("single-label host".into()));
    }
    if looks_like_numeric_ip(host) {
        return Err(ClassifyError::PrivateIp(host.into()));
    }
    Ok(())
}

/// 纯数字/0x 混淆 IP 判定（如 `2130706433`、`0x7f.0.0.1`；解析器可能把它当
/// 域名，但 DNS 会解析到私网——统一按疑似私网拒绝）。
fn looks_like_numeric_ip(host: &str) -> bool {
    let lower = host.to_ascii_lowercase();
    let compact: String = lower.chars().filter(|c| *c != '.').collect();
    if compact.is_empty() {
        return false;
    }
    if compact.bytes().all(|b| b.is_ascii_digit()) {
        return true;
    }
    if lower.starts_with("0x") && compact.bytes().all(|b| b.is_ascii_hexdigit()) {
        return true;
    }
    false
}

/// 签名/访问凭证检测：命中任何签名 key 或 URL 字符串中的签名标记即拒绝。
fn has_signature_credentials(url: &Url) -> bool {
    if let Some(query) = url.query() {
        for pair in query.split('&') {
            let key = pair.split('=').next().unwrap_or("").to_ascii_lowercase();
            if SIGNATURE_QUERY_KEYS.contains(&key.as_str()) {
                return true;
            }
        }
    }
    // 路径/query 中出现签名标记（如 `X-Amz-Signature=...` 或以 `,` 拼接）。
    let haystack = format!("{}?{}", url.path(), url.query().unwrap_or("")).to_ascii_lowercase();
    [
        "x-amz-signature",
        "x-amz-credential",
        "x-goog-signature",
        "x-goog-credential",
        "signature=",
        "sig=",
        "awsauth",
        "key-pair-id",
        "policy=",
    ]
    .iter()
    .any(|marker| haystack.contains(marker))
}

/// 分类主入口：校验 URL 形状后按扩展名/西瓜 Host 路由到对应 Provider。
pub fn classify(raw: &str) -> Result<Classified, ClassifyError> {
    let normalized = validate_url_shape(raw)?;
    let path = normalized.url.path().to_lowercase();

    // 西瓜：只允许官方公开页面/嵌入 Host，且必须是公开视频页形态。
    if is_xigua_host(&normalized.host) {
        if !is_xigua_page_path(&path) {
            return Err(ClassifyError::NotAVideoPage);
        }
        let video_id = extract_video_id(&path).ok_or(ClassifyError::NotAVideoPage)?;
        let canonical = format!("https://{}/video/{}", normalized.host, video_id);
        let source = canonical.clone();
        let source_hash = hash_hex(&source);
        let embeddable = normalized.host == xigua::XiguaHosts::EMBED_HOST;
        return Ok(Classified {
            provider: Provider::Xigua,
            media_type: None,
            external_id: Some(video_id),
            official_url: canonical,
            source,
            source_hash,
            host: normalized.host,
            embeddable,
            title: None,
        });
    }

    // Direct / HLS：按扩展名初判（扩展名不是可信依据，最终 MIME 由探测确认）。
    let (provider, media_type) = if path.ends_with(".m3u8") || path.ends_with(".m3u") {
        (
            Provider::Hls,
            Some("application/vnd.apple.mpegurl".to_string()),
        )
    } else if path.ends_with(".mp4") {
        (Provider::Direct, Some("video/mp4".to_string()))
    } else if path.ends_with(".webm") {
        (Provider::Direct, Some("video/webm".to_string()))
    } else if path.ends_with(".ogv") || path.ends_with(".ogg") {
        (Provider::Direct, Some("video/ogg".to_string()))
    } else if path.ends_with(".mov") {
        (Provider::Direct, Some("video/quicktime".to_string()))
    } else {
        return Err(ClassifyError::UnsupportedType);
    };

    let source = normalized.url.to_string();
    let source_hash = hash_hex(&source);
    Ok(Classified {
        provider,
        media_type,
        external_id: None,
        official_url: source.clone(),
        source,
        source_hash,
        host: normalized.host,
        embeddable: true,
        title: None,
    })
}

/// SHA-256 hex（source 摘要，审计/去重用；不存原始敏感 query）。
pub fn hash_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

/// host 是否命中 allowlist（精确匹配或子域后缀匹配；入参必须是规范化形态）。
///
/// - allowlist 为空 → 放行任意合法 host（仍受 egress 私网/形态限制约束）；
/// - 条目以 `*.` 开头 → 子域匹配；
/// - 其余条目 → 精确匹配或 `.entry` 后缀（子域）。
pub fn is_allowed_host(host: &str, allowlist: &[String]) -> bool {
    if allowlist.is_empty() {
        return true;
    }
    let host = host.trim_end_matches('.').to_lowercase();
    allowlist.iter().any(|entry| {
        let entry = entry.trim_end_matches('.').to_lowercase();
        if let Some(suffix) = entry.strip_prefix("*.") {
            host == suffix || host.ends_with(&format!(".{suffix}"))
        } else {
            host == entry || host.ends_with(&format!(".{entry}"))
        }
    })
}

use crate::video::xigua;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_insecure_and_exotic_schemes() {
        assert_eq!(
            classify("http://example.com/a.mp4"),
            Err(ClassifyError::InsecureScheme)
        );
        assert_eq!(
            classify("javascript:alert(1)"),
            Err(ClassifyError::InsecureScheme)
        );
        assert_eq!(
            classify("data:text/html;base64,xx"),
            Err(ClassifyError::InsecureScheme)
        );
        assert_eq!(
            classify("blob:https://x/y"),
            Err(ClassifyError::InsecureScheme)
        );
        assert_eq!(
            classify("file:///etc/passwd"),
            Err(ClassifyError::InsecureScheme)
        );
    }

    #[test]
    fn rejects_userinfo_and_non_standard_ports() {
        assert!(matches!(
            classify("https://user:pass@example.com/a.mp4"),
            Err(ClassifyError::UserinfoNotAllowed)
        ));
        assert!(matches!(
            classify("https://example.com:8443/a.mp4"),
            Err(ClassifyError::PortNotAllowed(8443))
        ));
        // 显式 443 与省略端口等价，允许。
        assert!(classify("https://example.com:443/a.mp4").is_ok());
    }

    #[test]
    fn rejects_private_and_numeric_hosts() {
        for bad in [
            "https://127.0.0.1/a.mp4",
            "https://10.0.0.5/a.mp4",
            "https://192.168.1.1/a.mp4",
            "https://169.254.1.1/a.mp4",
            "https://[::1]/a.mp4",
            "https://[fd00::1]/a.mp4",
            "https://[::ffff:10.0.0.1]/a.mp4",
        ] {
            assert!(
                matches!(classify(bad), Err(ClassifyError::PrivateIp(_))),
                "{bad} 应被阻断"
            );
        }
        // 数字混淆 IP（DNS 可解析到私网）。
        assert!(matches!(
            classify("https://2130706433/a.mp4"),
            Err(ClassifyError::PrivateIp(_))
        ));
        assert!(matches!(
            classify("https://0x7f.0.0.1/a.mp4"),
            Err(ClassifyError::PrivateIp(_))
        ));
        // 文档地址 TEST-NET-1 也应拒绝。
        assert!(matches!(
            classify("https://192.0.2.1/a.mp4"),
            Err(ClassifyError::PrivateIp(_))
        ));
    }

    #[test]
    fn rejects_localhost_single_label_and_control_hosts() {
        assert!(matches!(
            classify("https://localhost/a.mp4"),
            Err(ClassifyError::HostInvalid(_))
        ));
        assert!(matches!(
            classify("https://intranet/a.mp4"),
            Err(ClassifyError::HostInvalid(_))
        ));
        assert!(classify("https://cdn.example.com/a.mp4").is_ok());
    }

    #[test]
    fn rejects_signed_urls_never_echoed() {
        for bad in [
            "https://cdn.example.com/a.mp4?X-Amz-Signature=deadbeef&X-Amz-Credential=k",
            "https://cdn.example.com/a.mp4?signature=abc",
            "https://cdn.example.com/a.mp4?token=abc",
            "https://cdn.example.com/a.mp4?Expires=123&Signature=xyz",
        ] {
            assert!(
                matches!(classify(bad), Err(ClassifyError::SignedUrl)),
                "{bad} 应被拒绝"
            );
        }
        assert!(matches!(
            classify("https://cdn.example.com/a.mp4#frag"),
            Err(ClassifyError::FragmentNotAllowed)
        ));
    }

    #[test]
    fn classifies_media_extensions() {
        let mp4 = classify("https://cdn.example.com/vid/hello.mp4").unwrap();
        assert_eq!(mp4.provider, Provider::Direct);
        assert_eq!(mp4.media_type.as_deref(), Some("video/mp4"));
        assert_eq!(mp4.host, "cdn.example.com");

        let webm = classify("https://cdn.example.com/x.webm?drm=0").unwrap();
        assert_eq!(webm.provider, Provider::Direct);
        assert_eq!(webm.media_type.as_deref(), Some("video/webm"));

        let ogv = classify("https://cdn.example.com/x.ogv").unwrap();
        assert_eq!(ogv.provider, Provider::Direct);
        assert_eq!(ogv.media_type.as_deref(), Some("video/ogg"));

        let mov = classify("https://cdn.example.com/x.mov").unwrap();
        assert_eq!(mov.provider, Provider::Direct);
        assert_eq!(mov.media_type.as_deref(), Some("video/quicktime"));

        let hls = classify("https://cdn.example.com/live/index.m3u8").unwrap();
        assert_eq!(hls.provider, Provider::Hls);
        assert_eq!(
            hls.media_type.as_deref(),
            Some("application/vnd.apple.mpegurl")
        );

        assert_eq!(
            classify("https://cdn.example.com/evil.exe"),
            Err(ClassifyError::UnsupportedType)
        );
        assert_eq!(
            classify("https://cdn.example.com/photo.jpg"),
            Err(ClassifyError::UnsupportedType)
        );
    }

    #[test]
    fn classifies_xigua_official_pages_only() {
        let page = classify("https://www.ixigua.com/video/7301234567890123456").unwrap();
        assert_eq!(page.provider, Provider::Xigua);
        assert_eq!(page.external_id.as_deref(), Some("7301234567890123456"));
        assert_eq!(
            page.official_url,
            "https://www.ixigua.com/video/7301234567890123456"
        );
        assert!(page.embeddable);

        // item 形态同样接受。
        let item = classify("https://m.ixigua.com/item/7301234567890123456").unwrap();
        assert_eq!(item.provider, Provider::Xigua);
        assert_eq!(item.external_id.as_deref(), Some("7301234567890123456"));
        // m. 不是官方嵌入 Host。
        assert!(!item.embeddable);

        // 非官方 Host / 非视频页拒绝。
        assert!(matches!(
            classify("https://evil.example.com/video/7301234567890123456"),
            Err(ClassifyError::UnsupportedType)
        ));
        assert!(matches!(
            classify("https://www.ixigua.com/"),
            Err(ClassifyError::NotAVideoPage)
        ));
        assert!(matches!(
            classify("https://www.ixigua.com/channel/1"),
            Err(ClassifyError::NotAVideoPage)
        ));
        assert!(matches!(
            classify("https://www.ixigua.com/video/not-an-id"),
            Err(ClassifyError::NotAVideoPage)
        ));
    }

    #[test]
    fn idn_hosts_normalize_to_punycode() {
        let url = validate_url_shape("https://例え.jp/休憩/小.mp4").unwrap();
        assert!(
            url.host.contains("xn--"),
            "IDN 必须归一化为 punycode: {}",
            url.host
        );
        let c = classify("https://例え.jp/休憩/小.mp4").unwrap();
        assert_eq!(c.host, url.host);
    }

    #[test]
    fn allowlist_matching_supports_subdomains() {
        let list = vec!["example.com".to_string(), "*.cdn.example.net".to_string()];
        assert!(is_allowed_host("example.com", &list));
        assert!(is_allowed_host("sub.example.com", &list));
        assert!(!is_allowed_host("evil.com", &list));
        assert!(is_allowed_host("a.cdn.example.net", &list));
        assert!(is_allowed_host("cdn.example.net", &list));
        assert!(!is_allowed_host("example.net", &list));
        assert!(is_allowed_host("anything.example.com", &[]));
    }
}
