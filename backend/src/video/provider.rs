//! VideoProvider Adapter trait 与 Direct/Hls/Xigua 内置适配器（M10-VIDEO-01）。
//!
//! 领域层不依赖具体 Provider SDK：网络访问只经
//! [`crate::video::egress::FetchClient`] 抽象；所有适配器随应用编译、由管理员
//! 启停（`video_provider_policies`），不属于运行时上传代码，也不能直接访问
//! 网络之外的资源。适配器失效时核心服务降级为外链，不影响帖子正文事务。

use std::future::Future;
use std::pin::Pin;

use crate::video::classify::{classify, Classified, ClassifyError};
use crate::video::egress::{
    egress_validate, EgressLimits, FetchClient, FetchError, FetchRequest, FetchedResponse,
};
use crate::video::hls::{self, HlsLimits};
use crate::video::policy::VideoPolicy;
use crate::video::VideoError;

/// Provider refresh 输入（owned，便于异步闭包借用）。
#[derive(Debug, Clone)]
pub struct RefreshInput {
    pub source: String,
    /// 分类阶段的预期 MIME（direct/hls；最终以探测为准）。
    pub media_type: Option<String>,
    pub policy: VideoPolicy,
}

/// Provider refresh 结果（只含非敏感元数据；分片/密钥 URI 不返回）。
#[derive(Debug, Clone, Default)]
pub struct RefreshOutcome {
    pub title: Option<String>,
}

/// Provider Adapter trait（Direct/Hls/Xigua）。
///
/// - [`VideoProvider::classify`]：纯函数，无网络；
/// - [`VideoProvider::refresh`]：受限异步探测（经注入的 [`FetchClient`]；
///   失败返回稳定 [`VideoError`]，由状态机降级外链）。
pub trait VideoProvider: Send + Sync {
    fn name(&self) -> &'static str;

    fn classify(&self, raw: &str) -> Result<Classified, ClassifyError>;

    fn refresh<'a>(
        &'a self,
        input: &'a RefreshInput,
        client: &'a dyn FetchClient,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshOutcome, VideoError>> + Send + 'a>>;
}

/// 从策略派生出站限制。
pub fn egress_limits(policy: &VideoPolicy) -> EgressLimits {
    EgressLimits {
        max_redirects: policy.max_redirects,
        max_response_bytes: policy.max_response_bytes,
        timeout_ms: policy.timeout_ms(),
    }
}

/// 传输层错误 → VideoError（稳定 class 映射）。
pub fn fetch_error_to_video(e: FetchError) -> VideoError {
    match e {
        FetchError::Timeout => VideoError::EgressTimeout,
        FetchError::TooLarge(n) => VideoError::EgressTooLarge(n),
        FetchError::TooManyRedirects => VideoError::EgressTooManyRedirects,
        FetchError::Transport(_) => VideoError::ProviderUnavailable("transport failure".into()),
        FetchError::EgressUnavailable => VideoError::EgressUnavailable,
    }
}

/// 响应状态 → 结果（2xx 通过；404/410 下架；429 限流；其余 4xx 无嵌入权限；
/// 5xx Provider 故障；未跟随的 3xx = 重定向循环）。
fn status_outcome(status: u16) -> Result<(), VideoError> {
    match status {
        200 | 203 | 206 => Ok(()),
        301..=399 => Err(VideoError::EgressTooManyRedirects),
        404 | 410 => Err(VideoError::Takedown),
        429 => Err(VideoError::ProviderRatelimited),
        400..=499 => Err(VideoError::NoEmbedPermission),
        _ => Err(VideoError::ProviderUnavailable(format!(
            "http status {status}"
        ))),
    }
}

/// 直接媒体 MIME 判定（MIME 欺骗防线）：`video/*` 与明确的未知二进制接受；
/// 已知非视频类型（text/html、image/*、application/pdf 等）拒绝；其余
/// fail-closed。
fn check_media_type(expected: Option<&str>, actual: Option<&str>) -> Result<(), VideoError> {
    let Some(actual) = actual else {
        return Ok(());
    };
    let essence = actual
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    if essence.starts_with("video/")
        || matches!(
            essence.as_str(),
            "application/octet-stream" | "application/vnd.apple.mpegurl" | "application/x-mpegurl"
        )
    {
        return Ok(());
    }
    // 明确非视频类型（MIME 欺骗）→ 拒绝。
    if essence.starts_with("text/")
        || essence.starts_with("image/")
        || essence.starts_with("multipart/")
        || matches!(
            essence.as_str(),
            "application/pdf"
                | "application/json"
                | "application/xml"
                | "application/javascript"
                | "application/x-javascript"
                | "application/zip"
                | "application/x-www-form-urlencoded"
                | "application/xhtml+xml"
                | "application/x-shockwave-flash"
        )
    {
        return Err(VideoError::MimeMismatch(essence));
    }
    // 未知类型 → fail closed（期望的 direct 媒体必须是已知视频类型）。
    let _ = expected;
    Err(VideoError::MimeMismatch(essence))
}

/// 受限探测请求（direct/hls/xigua 共用；direct 带 Range，避免下载任意大文件）。
fn build_request(url: &str, policy: &VideoPolicy, range: bool) -> FetchRequest {
    FetchRequest {
        url: url.to_string(),
        method: "GET",
        range: if range {
            Some("bytes=0-0".into())
        } else {
            None
        },
        max_bytes: policy.max_response_bytes,
        timeout_ms: policy.timeout_ms(),
    }
}

/// Direct 适配器：受限 Range 探测 + MIME 判定。
#[derive(Debug, Clone, Copy, Default)]
pub struct DirectProvider;

impl VideoProvider for DirectProvider {
    fn name(&self) -> &'static str {
        "direct"
    }

    fn classify(&self, raw: &str) -> Result<Classified, ClassifyError> {
        classify(raw)
    }

    fn refresh<'a>(
        &'a self,
        input: &'a RefreshInput,
        client: &'a dyn FetchClient,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshOutcome, VideoError>> + Send + 'a>> {
        Box::pin(async move {
            let limits = egress_limits(&input.policy);
            let req = build_request(&input.source, &input.policy, true);
            let resp = client.fetch(req).await.map_err(fetch_error_to_video)?;
            egress_validate(&limits, &resp)
                .map_err(|e| VideoError::Internal(e.class().to_string()))?;
            status_outcome(resp.status)?;
            check_media_type(input.media_type.as_deref(), resp.content_type.as_deref())?;
            Ok(RefreshOutcome::default())
        })
    }
}

/// HLS 适配器：master/media playlist 受限探测 + 解析预算。
#[derive(Debug, Clone, Copy, Default)]
pub struct HlsProvider;

impl VideoProvider for HlsProvider {
    fn name(&self) -> &'static str {
        "hls"
    }

    fn classify(&self, raw: &str) -> Result<Classified, ClassifyError> {
        classify(raw)
    }

    fn refresh<'a>(
        &'a self,
        input: &'a RefreshInput,
        client: &'a dyn FetchClient,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshOutcome, VideoError>> + Send + 'a>> {
        Box::pin(async move {
            let limits = egress_limits(&input.policy);
            let hls_limits = HlsLimits {
                max_depth: input.policy.max_playlist_depth,
                max_segments: input.policy.max_segments,
                max_playlist_bytes: input.policy.max_response_bytes as usize,
                max_duration_ms: input.policy.max_duration_ms,
                allow_cross_origin: input.policy.hls_allow_cross_origin(),
            };
            let mut budget = hls::initial_budget(&hls_limits);
            let body = fetch_playlist(client, &input.source, &limits).await?;
            let report = parse_hls_tree(
                client,
                &input.source,
                &body,
                &limits,
                &hls_limits,
                &mut budget,
            )
            .await?;
            hls::check_totals(report.total_segments, report.total_duration_ms, &hls_limits)
                .map_err(VideoError::Hls)?;
            Ok(RefreshOutcome::default())
        })
    }
}

/// 取回单个 playlist（状态 + egress 双防线；HLS 内容合法性由解析器校验，
/// 不依赖 MIME——部分合法服务器以 text/plain 提供 .m3u8）。
async fn fetch_playlist(
    client: &dyn FetchClient,
    url: &str,
    limits: &EgressLimits,
) -> Result<Vec<u8>, VideoError> {
    let req = FetchRequest {
        url: url.to_string(),
        method: "GET",
        range: None,
        max_bytes: limits.max_response_bytes,
        timeout_ms: limits.timeout_ms,
    };
    let resp: FetchedResponse = client.fetch(req).await.map_err(fetch_error_to_video)?;
    egress_validate(limits, &resp).map_err(|e| VideoError::Internal(e.class().to_string()))?;
    status_outcome(resp.status)?;
    Ok(resp.body)
}

/// 解析 master 树：递归取回 variant playlist，累计分片/时长/深度预算。
async fn parse_hls_tree(
    client: &dyn FetchClient,
    url: &str,
    body: &[u8],
    limits: &EgressLimits,
    hls_limits: &HlsLimits,
    budget: &mut hls::HlsBudget,
) -> Result<hls::HlsReport, VideoError> {
    let text = String::from_utf8_lossy(body);
    let parsed = hls::parse_playlist(&text, url, hls_limits, budget).map_err(VideoError::Hls)?;
    let mut total_segments = parsed.segments.len();
    let mut total_duration = parsed.duration_ms;
    let mut playlists_checked = 1usize;
    for variant in &parsed.variant_urls {
        let variant_body = fetch_playlist(client, variant, limits).await?;
        let report = Box::pin(parse_hls_tree(
            client,
            variant,
            &variant_body,
            limits,
            hls_limits,
            budget,
        ))
        .await?;
        total_segments += report.total_segments;
        total_duration += report.total_duration_ms;
        playlists_checked += report.playlists_checked;
    }
    Ok(hls::summarize(
        url,
        total_segments,
        total_duration,
        playlists_checked,
        &text,
    ))
}

/// Xigua 适配器：官方公开页面只做受限存活探测，不抓取播放地址。
#[derive(Debug, Clone, Copy, Default)]
pub struct XiguaProvider;

impl VideoProvider for XiguaProvider {
    fn name(&self) -> &'static str {
        "xigua"
    }

    fn classify(&self, raw: &str) -> Result<Classified, ClassifyError> {
        classify(raw)
    }

    fn refresh<'a>(
        &'a self,
        input: &'a RefreshInput,
        client: &'a dyn FetchClient,
    ) -> Pin<Box<dyn Future<Output = Result<RefreshOutcome, VideoError>> + Send + 'a>> {
        Box::pin(async move {
            let limits = egress_limits(&input.policy);
            let req = build_request(&input.source, &input.policy, false);
            let resp = client.fetch(req).await.map_err(fetch_error_to_video)?;
            egress_validate(&limits, &resp)
                .map_err(|e| VideoError::Internal(e.class().to_string()))?;
            status_outcome(resp.status)?;
            // 不解析页面正文（不抓取播放地址/元数据）。
            Ok(RefreshOutcome { title: None })
        })
    }
}

/// 内置 Provider 注册表（随应用编译，非运行时上传）。
pub struct ProviderRegistry {
    providers: Vec<Box<dyn VideoProvider>>,
}

impl ProviderRegistry {
    pub fn builtin() -> Self {
        ProviderRegistry {
            providers: vec![
                Box::new(DirectProvider),
                Box::new(HlsProvider),
                Box::new(XiguaProvider),
            ],
        }
    }

    pub fn get(&self, name: &str) -> Option<&dyn VideoProvider> {
        self.providers
            .iter()
            .find(|p| p.name() == name)
            .map(|b| b.as_ref())
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mime_check_rejects_spoof_and_accepts_video() {
        assert!(check_media_type(Some("video/mp4"), Some("video/mp4")).is_ok());
        assert!(check_media_type(Some("video/mp4"), Some("video/mp4; charset=utf-8")).is_ok());
        assert!(check_media_type(Some("video/mp4"), Some("application/octet-stream")).is_ok());
        // MIME 欺骗：.mp4 由 text/html 提供。
        assert!(matches!(
            check_media_type(Some("video/mp4"), Some("text/html")),
            Err(VideoError::MimeMismatch(_))
        ));
        assert!(matches!(
            check_media_type(Some("video/mp4"), Some("image/gif")),
            Err(VideoError::MimeMismatch(_))
        ));
        assert!(matches!(
            check_media_type(Some("video/mp4"), Some("application/pdf")),
            Err(VideoError::MimeMismatch(_))
        ));
        // 未知类型 fail-closed。
        assert!(matches!(
            check_media_type(Some("video/mp4"), Some("application/x-evil")),
            Err(VideoError::MimeMismatch(_))
        ));
    }

    #[test]
    fn status_outcome_maps_takedown_and_ratelimit() {
        assert!(status_outcome(200).is_ok());
        assert!(status_outcome(206).is_ok());
        assert!(matches!(status_outcome(404), Err(VideoError::Takedown)));
        assert!(matches!(status_outcome(410), Err(VideoError::Takedown)));
        assert!(matches!(
            status_outcome(429),
            Err(VideoError::ProviderRatelimited)
        ));
        assert!(matches!(
            status_outcome(403),
            Err(VideoError::NoEmbedPermission)
        ));
        assert!(matches!(
            status_outcome(503),
            Err(VideoError::ProviderUnavailable(_))
        ));
        assert!(matches!(
            status_outcome(302),
            Err(VideoError::EgressTooManyRedirects)
        ));
    }

    #[test]
    fn builtin_registry_has_three_providers() {
        let registry = ProviderRegistry::builtin();
        assert_eq!(registry.get("direct").unwrap().name(), "direct");
        assert_eq!(registry.get("hls").unwrap().name(), "hls");
        assert_eq!(registry.get("xigua").unwrap().name(), "xigua");
        assert!(registry.get("youtube").is_none());
    }

    #[test]
    fn providers_classify_consistently() {
        let direct = DirectProvider;
        let hls = HlsProvider;
        let xigua = XiguaProvider;
        assert_eq!(
            direct
                .classify("https://cdn.example.com/a.mp4")
                .unwrap()
                .provider,
            crate::video::Provider::Direct
        );
        assert_eq!(
            hls.classify("https://cdn.example.com/a.m3u8")
                .unwrap()
                .provider,
            crate::video::Provider::Hls
        );
        assert_eq!(
            xigua
                .classify("https://www.ixigua.com/video/7301234567890123456")
                .unwrap()
                .provider,
            crate::video::Provider::Xigua
        );
    }
}
