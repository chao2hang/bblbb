//! HLS playlist 解析限制（M10-VIDEO-05/10）。
//!
//! Master/media playlist 必须在白名单来源（上游策略已校验）；递归 playlist
//! 深度、分片数量、单片大小、总时长/总字节、Key URI、Map URI 与重定向均受
//! 限。默认禁止外部 HLS `EXT-X-KEY`/`EXT-X-MAP` 或跨域分片；禁止把服务端当
//! 开放 HLS 代理。本模块是纯文本解析（不发起网络），网络取回由
//! [`crate::video::egress::FetchClient`] 负责，测试直接驱动解析函数。

use url::Url;

use crate::video::classify::{hash_hex, validate_url_shape, ClassifyError};

/// HLS 限制（由 `video_provider_policies` 派生）。
#[derive(Debug, Clone, Copy)]
pub struct HlsLimits {
    /// 递归 playlist 深度（master→variant→…）。
    pub max_depth: usize,
    /// 分片总数上限（跨所有 variant 累计）。
    pub max_segments: usize,
    /// 单 playlist 文本大小上限。
    pub max_playlist_bytes: usize,
    /// 总时长上限（毫秒，#EXTINF 累计）。
    pub max_duration_ms: i64,
    /// 分片 URI 是否允许跨源（默认 false：必须与 playlist 同 scheme+host+port）。
    pub allow_cross_origin: bool,
}

impl Default for HlsLimits {
    fn default() -> Self {
        HlsLimits {
            max_depth: 5,
            max_segments: 200,
            max_playlist_bytes: 2 * 1024 * 1024,
            max_duration_ms: 3_600_000,
            allow_cross_origin: false,
        }
    }
}

/// HLS 解析稳定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HlsError {
    /// 不是合法 HLS playlist（缺 #EXTM3U）。
    InvalidPlaylist,
    /// 解析失败（行/URI 形态）。
    InvalidLine(String),
    /// `EXT-X-KEY` 非 NONE（外部密钥一律拒绝，密钥 URI 不落库不转发）。
    KeyNotAllowed,
    /// `EXT-X-MAP`（跨域或策略禁止）。
    MapNotAllowed,
    /// 分片/子 playlist URI 跨源且策略不允许。
    CrossOriginSegment,
    /// 递归深度超限。
    DepthExceeded,
    /// 分片总数超限。
    SegmentCountExceeded,
    /// #EXTINF 累计时长超限。
    DurationExceeded,
    /// 分片 URI 带签名/凭证参数。
    SignedUri,
    /// 分片 URI 形状非法（scheme/host/port/userinfo/私网）。
    InvalidUri(String),
}

impl HlsError {
    /// 稳定错误类（写入 video_embeds.error_class）。
    pub fn class(&self) -> &'static str {
        match self {
            HlsError::InvalidPlaylist => "video_hls_invalid",
            HlsError::InvalidLine(_) => "video_hls_invalid",
            HlsError::KeyNotAllowed => "video_hls_key_not_allowed",
            HlsError::MapNotAllowed => "video_hls_map_not_allowed",
            HlsError::CrossOriginSegment => "video_hls_cross_origin_segment",
            HlsError::DepthExceeded => "video_hls_depth_exceeded",
            HlsError::SegmentCountExceeded => "video_hls_segment_count_exceeded",
            HlsError::DurationExceeded => "video_hls_duration_exceeded",
            HlsError::SignedUri => "video_hls_signed_uri",
            HlsError::InvalidUri(_) => "video_hls_invalid",
        }
    }
}

/// 单个 playlist 的解析结果（累计预算由调用方跨 playlist 维护）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedPlaylist {
    /// 是否为 master playlist（含 #EXT-X-STREAM-INF）。
    pub is_master: bool,
    /// 子 playlist URI（相对主 URL 解析后；数量计入深度预算）。
    pub variant_urls: Vec<String>,
    /// 分片 URI（相对 playlist URL 解析后；已通过来源策略）。
    pub segments: Vec<String>,
    /// 累计时长（毫秒）。
    pub duration_ms: i64,
}

/// 跨 playlist 共享的累计预算。
#[derive(Debug, Clone, Copy, Default)]
pub struct HlsBudget {
    pub segments_left: usize,
    pub duration_left_ms: i64,
    pub depth_left: usize,
}

/// 解析一个 HLS playlist 文本。`playlist_url` 是取回该文本的规范化 URL
/// （分片/子 playlist 相对解析与同源判定基准）。`budget` 为共享预算（可变，
/// 解析消耗后回写）。返回分片与子 playlist 列表。
///
/// 安全约束（docs/VIDEO-PLUGIN.md §3）：
/// - 首行必须 `#EXTM3U`；
/// - 每个分片/子 playlist URI 重新过 [`validate_url_shape`]（https、无
///   userinfo、端口 443、非私网字面量、无签名）；
/// - `EXT-X-KEY`（非 NONE）与跨域 `EXT-X-MAP` 拒绝；
/// - 跨源分片默认拒绝；
/// - 深度/分片数/时长计入共享预算。
pub fn parse_playlist(
    text: &str,
    playlist_url: &str,
    limits: &HlsLimits,
    budget: &mut HlsBudget,
) -> Result<ParsedPlaylist, HlsError> {
    if text.len() > limits.max_playlist_bytes {
        return Err(HlsError::InvalidLine("playlist too large".into()));
    }
    if budget.depth_left == 0 {
        return Err(HlsError::DepthExceeded);
    }
    if budget.segments_left == 0 {
        return Err(HlsError::SegmentCountExceeded);
    }
    let mut lines = text.lines();
    let first = lines.next().unwrap_or("").trim();
    if first != "#EXTM3U" {
        return Err(HlsError::InvalidPlaylist);
    }

    let base =
        Url::parse(playlist_url).map_err(|e| HlsError::InvalidLine(format!("base url: {e}")))?;
    let base_origin =
        origin_of(&base).ok_or_else(|| HlsError::InvalidLine("base url has no host".into()))?;

    let mut out = ParsedPlaylist {
        is_master: false,
        variant_urls: Vec::new(),
        segments: Vec::new(),
        duration_ms: 0,
    };
    let mut expect_after_stream_inf = false;

    for line in lines {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with("#EXT-X-KEY") {
            // METHOD="NONE" 允许；其余（外部密钥）一律拒绝。
            if !line.contains("METHOD=\"NONE\"") && !line.contains("METHOD=NONE") {
                return Err(HlsError::KeyNotAllowed);
            }
            continue;
        }
        if line.starts_with("#EXT-X-MAP") {
            // 默认禁止（解析 EXT-X-MAP 的 URI 有泄露签名/内部地址风险）。
            if limits.allow_cross_origin {
                continue;
            }
            return Err(HlsError::MapNotAllowed);
        }
        if line.starts_with("#EXT-X-SESSION-KEY") {
            // 会话级密钥 = 密钥泄漏面。
            return Err(HlsError::KeyNotAllowed);
        }
        if line.starts_with("#EXT-X-STREAM-INF") {
            expect_after_stream_inf = true;
            out.is_master = true;
            continue;
        }
        if line.starts_with("#EXT-X-TARGETDURATION") {
            continue;
        }
        if let Some(dur) = line.strip_prefix("#EXTINF:") {
            // 取 "9.009," 形态的时长。
            let raw = dur.trim_end_matches(',');
            let raw = raw.split(',').next().unwrap_or("").trim();
            let secs: f64 = raw.parse().unwrap_or(0.0);
            let ms = (secs * 1000.0).round() as i64;
            if ms > 0 {
                out.duration_ms += ms;
            }
            if out.duration_ms > budget.duration_left_ms {
                return Err(HlsError::DurationExceeded);
            }
            continue;
        }
        if line.starts_with('#') {
            // 其余 tag（EXT-X-VERSION、EXT-X-DISCONTINUITY、EXT-X-ENDLIST 等）。
            if line.starts_with("#EXT-X-ENDLIST") {
                continue;
            }
            continue;
        }

        // 数据行：分片或子 playlist URI。
        let resolved = resolve_uri(&base, line).map_err(HlsError::InvalidUri)?;
        let normalized = validate_url_shape(&resolved).map_err(|e| match e {
            ClassifyError::SignedUrl => HlsError::SignedUri,
            other => HlsError::InvalidUri(other.code().to_string()),
        })?;
        if (normalized.host != base_origin.host || normalized.url.port() != base_origin.port)
            && !limits.allow_cross_origin
        {
            return Err(HlsError::CrossOriginSegment);
        }
        if has_signed_query(&normalized.url) {
            return Err(HlsError::SignedUri);
        }

        if expect_after_stream_inf {
            if budget.depth_left == 0 {
                return Err(HlsError::DepthExceeded);
            }
            out.variant_urls.push(resolved);
            budget.depth_left -= 1;
            expect_after_stream_inf = false;
        } else {
            if budget.segments_left == 0 {
                return Err(HlsError::SegmentCountExceeded);
            }
            out.segments.push(resolved);
            budget.segments_left -= 1;
        }
    }

    Ok(out)
}

/// 解析后的汇总报告（refresh 成功后写入 embed 元数据）。
#[derive(Debug, Clone)]
pub struct HlsReport {
    pub total_segments: usize,
    pub total_duration_ms: i64,
    pub playlists_checked: usize,
    pub playlist_url: String,
    pub content_hash: String,
}

/// 构建初始预算（供 refresh 使用）。
pub fn initial_budget(limits: &HlsLimits) -> HlsBudget {
    HlsBudget {
        segments_left: limits.max_segments,
        duration_left_ms: limits.max_duration_ms,
        depth_left: limits.max_depth,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Origin {
    host: String,
    port: Option<u16>,
}

fn origin_of(url: &Url) -> Option<Origin> {
    Some(Origin {
        host: url.host_str()?.to_lowercase(),
        port: url.port(),
    })
}

/// 相对 URI 解析（RFC 3986；非法返回错误信息）。
fn resolve_uri(base: &Url, raw: &str) -> Result<String, String> {
    base.join(raw)
        .map(|u| u.to_string())
        .map_err(|e| format!("bad uri '{raw}': {e}"))
}

/// 分片/子 playlist URI 的签名/凭证参数检测（复用 classify 的保守集合）。
fn has_signed_query(url: &Url) -> bool {
    let Some(query) = url.query() else {
        return false;
    };
    query
        .split('&')
        .map(|pair| pair.split('=').next().unwrap_or("").to_ascii_lowercase())
        .any(|key| {
            matches!(
                key.as_str(),
                "x-amz-signature"
                    | "x-amz-credential"
                    | "x-amz-security-token"
                    | "x-goog-signature"
                    | "x-goog-credential"
                    | "signature"
                    | "sig"
                    | "token"
                    | "access_token"
                    | "expires"
                    | "policy"
                    | "key-pair-id"
            )
        })
}

/// 统计校验辅助：超过分片上限即返回错误（供 refresh 汇总时复核）。
pub fn check_totals(
    total_segments: usize,
    total_duration_ms: i64,
    limits: &HlsLimits,
) -> Result<(), HlsError> {
    if total_segments > limits.max_segments {
        return Err(HlsError::SegmentCountExceeded);
    }
    if total_duration_ms > limits.max_duration_ms {
        return Err(HlsError::DurationExceeded);
    }
    Ok(())
}

/// 汇总摘要（供 refresh 落库：仅计数/哈希，不保存分片 URI 或密钥）。
pub fn summarize(
    playlist_url: &str,
    total_segments: usize,
    total_duration_ms: i64,
    playlists_checked: usize,
    content: &str,
) -> HlsReport {
    HlsReport {
        total_segments,
        total_duration_ms,
        playlists_checked,
        playlist_url: playlist_url.to_string(),
        content_hash: hash_hex(content),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> HlsLimits {
        HlsLimits {
            max_depth: 5,
            max_segments: 200,
            max_playlist_bytes: 64 * 1024,
            max_duration_ms: 3_600_000,
            allow_cross_origin: false,
        }
    }

    fn base_url() -> &'static str {
        "https://cdn.example.com/live/index.m3u8"
    }

    #[test]
    fn rejects_non_hls() {
        let mut b = initial_budget(&limits());
        assert_eq!(
            parse_playlist("#EXT-X-VERSION:3\nseg1.ts\n", base_url(), &limits(), &mut b),
            Err(HlsError::InvalidPlaylist)
        );
    }

    #[test]
    fn parses_media_playlist_and_bounds_segments() {
        let mut b = initial_budget(&limits());
        let text = "#EXTM3U\n#EXT-X-VERSION:3\n#EXTINF:10.0,\nseg1.ts\n#EXTINF:9.0,\nseg2.ts\n#EXT-X-ENDLIST\n";
        let parsed = parse_playlist(text, base_url(), &limits(), &mut b).unwrap();
        assert!(!parsed.is_master);
        assert_eq!(parsed.segments.len(), 2);
        assert_eq!(parsed.duration_ms, 19_000);
        assert_eq!(b.segments_left, limits().max_segments - 2);
    }

    #[test]
    fn segment_count_exceeds_budget() {
        let small = HlsLimits {
            max_segments: 2,
            ..limits()
        };
        let mut b = initial_budget(&small);
        let text = "#EXTM3U\n#EXTINF:1,\nseg1.ts\n#EXTINF:1,\nseg2.ts\n#EXTINF:1,\nseg3.ts\n";
        assert!(matches!(
            parse_playlist(text, base_url(), &small, &mut b),
            Err(HlsError::SegmentCountExceeded)
        ));
    }

    #[test]
    fn duration_exceeds_budget() {
        let small = HlsLimits {
            max_duration_ms: 100,
            ..limits()
        };
        let mut b = initial_budget(&small);
        let text = "#EXTM3U\n#EXTINF:5.0,\nseg1.ts\n#EXTINF:5.0,\nseg2.ts\n";
        assert!(matches!(
            parse_playlist(text, base_url(), &small, &mut b),
            Err(HlsError::DurationExceeded)
        ));
    }

    #[test]
    fn external_key_and_map_rejected() {
        let mut b = initial_budget(&limits());
        let key = "#EXTM3U\n#EXT-X-KEY:METHOD=AES-128,URI=\"https://keys.example.com/k\"\n#EXTINF:1,\ns.ts\n";
        assert_eq!(
            parse_playlist(key, base_url(), &limits(), &mut b),
            Err(HlsError::KeyNotAllowed)
        );
        let map = "#EXTM3U\n#EXT-X-MAP:URI=\"init.mp4\"\n#EXTINF:1,\ns.ts\n";
        assert_eq!(
            parse_playlist(map, base_url(), &limits(), &mut b),
            Err(HlsError::MapNotAllowed)
        );
        let session_key = "#EXTM3U\n#EXT-X-SESSION-KEY:METHOD=AES-128,URI=\"https://k.example.com\"\n#EXTINF:1,\ns.ts\n";
        assert_eq!(
            parse_playlist(session_key, base_url(), &limits(), &mut b),
            Err(HlsError::KeyNotAllowed)
        );
        // METHOD=NONE 允许（明文化，无密钥泄漏）。
        let none = "#EXTM3U\n#EXT-X-KEY:METHOD=NONE\n#EXTINF:1,\ns.ts\n";
        assert!(parse_playlist(none, base_url(), &limits(), &mut b).is_ok());
    }

    #[test]
    fn cross_origin_segment_rejected_by_default() {
        let mut b = initial_budget(&limits());
        let text = "#EXTM3U\n#EXTINF:1,\nhttps://evil.example.com/seg.ts\n";
        assert_eq!(
            parse_playlist(text, base_url(), &limits(), &mut b),
            Err(HlsError::CrossOriginSegment)
        );
        // 允许跨源时通过。
        let loose = HlsLimits {
            allow_cross_origin: true,
            ..limits()
        };
        let mut b2 = initial_budget(&loose);
        assert!(parse_playlist(text, base_url(), &loose, &mut b2).is_ok());
    }

    #[test]
    fn signed_segment_uri_rejected() {
        let mut b = initial_budget(&limits());
        let text = "#EXTM3U\n#EXTINF:1,\nseg.ts?token=abc\n";
        assert_eq!(
            parse_playlist(text, base_url(), &limits(), &mut b),
            Err(HlsError::SignedUri)
        );
    }

    #[test]
    fn private_ip_segment_uri_rejected() {
        let mut b = initial_budget(&limits());
        let text = "#EXTM3U\n#EXTINF:1,\nhttps://127.0.0.1/seg.ts\n";
        assert!(matches!(
            parse_playlist(text, base_url(), &limits(), &mut b),
            Err(HlsError::InvalidUri(_))
        ));
    }

    #[test]
    fn master_playlist_bounds_variants_by_depth() {
        let small = HlsLimits {
            max_depth: 1,
            ..limits()
        };
        let mut b = initial_budget(&small);
        let text = "#EXTM3U\n#EXT-X-STREAM-INF:BANDWIDTH=1000000\nchunk0.m3u8\n";
        assert!(parse_playlist(text, base_url(), &small, &mut b).is_ok());
        let too_deep = HlsLimits {
            max_depth: 0,
            ..limits()
        };
        let mut b2 = initial_budget(&too_deep);
        assert_eq!(
            parse_playlist(text, base_url(), &too_deep, &mut b2),
            Err(HlsError::DepthExceeded)
        );
    }

    #[test]
    fn summarize_never_contains_segment_uris() {
        let mut b = initial_budget(&limits());
        let text = "#EXTM3U\n#EXTINF:1,\nseg1.ts\n";
        let parsed = parse_playlist(text, base_url(), &limits(), &mut b).unwrap();
        let report = summarize(
            base_url(),
            parsed.segments.len(),
            parsed.duration_ms,
            1,
            text,
        );
        assert_eq!(report.total_segments, 1);
        assert_eq!(report.total_duration_ms, 1000);
        assert_eq!(report.playlists_checked, 1);
        assert!(!report.content_hash.is_empty());
    }
}
