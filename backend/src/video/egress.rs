//! 受控出站边界（M10-VIDEO-04/10）：重定向、DNS 重绑定、响应大小与超时。
//!
//! 网络访问一律经 [`FetchClient`] 抽象（与 M09-GATEWAY 的 `ProviderClient`
//! 同模式）；测试用 mock 注入，不发起真实外部请求。真实部署注入由平台提供
//! 的 egress 实现（HTTPS + host allowlist + 端口 + 每次连接重新校验 IP +
//! 连接/读取/总耗时 + 响应头/正文大小限制）。
//!
//! [`egress_validate`] 是纯决策层（响应到达后的二次防线）：重定向计数超限、
//! DNS 重绑定 IP 复核（任一解析 IP 私网即拒）、响应体超限都在此裁决，测试
//! 直接驱动该函数（MIME 欺骗/Range/超时/超大响应/开放重定向/DNS rebinding）。

use std::net::IpAddr;

use crate::ai::gateway::is_private_ip;

/// 出站限制（每 Provider 策略派生）。
#[derive(Debug, Clone)]
pub struct EgressLimits {
    pub max_redirects: u32,
    pub max_response_bytes: i64,
    pub timeout_ms: u64,
}

/// 出站请求（已通过 URL 形状校验的目标 + 用途元数据）。
#[derive(Debug, Clone)]
pub struct FetchRequest {
    pub url: String,
    /// "GET" | "HEAD"
    pub method: &'static str,
    /// 受限探测：`bytes=0-0`（避免下载任意大文件）。
    pub range: Option<String>,
    pub max_bytes: i64,
    pub timeout_ms: u64,
}

/// 出站响应（传输层已按策略完成超时/大小/重定向校验）。
#[derive(Debug, Clone)]
pub struct FetchedResponse {
    pub status: u16,
    /// 跟随重定向后的最终 URL（不得与请求目标跨出策略范围）。
    pub final_url: String,
    /// 连接过程中观察到的解析 IP（DNS 重绑定复核数据）。
    pub resolved_ips: Vec<IpAddr>,
    pub content_type: Option<String>,
    pub content_length: Option<i64>,
    /// 已受 `max_bytes` 约束的正文。
    pub body: Vec<u8>,
    /// 已跟随的重定向跳数。
    pub hop_count: u32,
}

/// 传输层稳定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchError {
    Timeout,
    TooLarge(i64),
    TooManyRedirects,
    Transport(String),
    EgressUnavailable,
}

impl FetchError {
    /// 稳定错误类（写入 video_embeds.error_class）。
    pub fn class(&self) -> &'static str {
        match self {
            FetchError::Timeout => "video_egress_timeout",
            FetchError::TooLarge(_) => "video_egress_too_large",
            FetchError::TooManyRedirects => "video_egress_too_many_redirects",
            FetchError::Transport(_) => "video_provider_unavailable",
            FetchError::EgressUnavailable => "video_egress_unavailable",
        }
    }
}

/// 响应二次防线错误（纯决策，测试直接驱动）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EgressError {
    TooManyRedirects,
    PrivateIp(String),
    ResponseTooLarge(i64),
}

impl EgressError {
    /// 稳定错误类（写入 video_embeds.error_class）。
    pub fn class(&self) -> &'static str {
        match self {
            EgressError::TooManyRedirects => "video_egress_too_many_redirects",
            EgressError::PrivateIp(_) => "video_egress_private_ip",
            EgressError::ResponseTooLarge(_) => "video_egress_too_large",
        }
    }
}

/// 默认客户端：egress 未配置时安全拒绝（resolve/create 不受影响，refresh 降级
/// 为外链；与 M09-GATEWAY 真实 reqwest 客户端由部署注入同模式）。
pub struct UnavailableClient;

impl FetchClient for UnavailableClient {
    fn fetch(
        &self,
        _req: FetchRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<FetchedResponse, FetchError>> + Send + '_>,
    > {
        Box::pin(async { Err(FetchError::EgressUnavailable) })
    }
}

/// 出站客户端抽象（真实实现由部署注入，测试用 mock）。
pub trait FetchClient: Send + Sync {
    fn fetch(
        &self,
        req: FetchRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<FetchedResponse, FetchError>> + Send + '_>,
    >;
}

/// 响应二次防线（纯决策，测试直接驱动）：
/// - 重定向跳数超限 → `video_egress_too_many_redirects`；
/// - DNS 重绑定：任一解析 IP 为私网/回环/链路本地 → `video_egress_private_ip`；
/// - 响应体超限 → `video_egress_too_large`。
pub fn egress_validate(limits: &EgressLimits, resp: &FetchedResponse) -> Result<(), EgressError> {
    if resp.hop_count >= limits.max_redirects {
        return Err(EgressError::TooManyRedirects);
    }
    if let Some(ip) = resp.resolved_ips.iter().find(|ip| is_private_ip(ip)) {
        return Err(EgressError::PrivateIp(ip.to_string()));
    }
    let size = resp.body.len() as i64;
    if size > limits.max_response_bytes {
        return Err(EgressError::ResponseTooLarge(size));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> EgressLimits {
        EgressLimits {
            max_redirects: 3,
            max_response_bytes: 1024,
            timeout_ms: 5000,
        }
    }

    fn base() -> FetchedResponse {
        FetchedResponse {
            status: 200,
            final_url: "https://cdn.example.com/a.mp4".into(),
            resolved_ips: vec!["93.184.216.34".parse().unwrap()],
            content_type: Some("video/mp4".into()),
            content_length: None,
            body: vec![0u8; 10],
            hop_count: 0,
        }
    }

    #[test]
    fn validates_redirect_count() {
        assert!(egress_validate(&limits(), &base()).is_ok());
        let looped = FetchedResponse {
            hop_count: 3,
            ..base()
        };
        assert_eq!(
            egress_validate(&limits(), &looped),
            Err(EgressError::TooManyRedirects)
        );
    }

    #[test]
    fn rejects_dns_rebinding_ips() {
        let rebound = FetchedResponse {
            resolved_ips: vec!["8.8.8.8".parse().unwrap(), "10.0.0.1".parse().unwrap()],
            ..base()
        };
        assert!(matches!(
            egress_validate(&limits(), &rebound),
            Err(EgressError::PrivateIp(_))
        ));
        let ipv6_mapped = FetchedResponse {
            resolved_ips: vec!["::ffff:127.0.0.1".parse().unwrap()],
            ..base()
        };
        assert!(matches!(
            egress_validate(&limits(), &ipv6_mapped),
            Err(EgressError::PrivateIp(_))
        ));
    }

    #[test]
    fn rejects_oversized_response() {
        let big = FetchedResponse {
            body: vec![0u8; 2048],
            ..base()
        };
        assert_eq!(
            egress_validate(&limits(), &big),
            Err(EgressError::ResponseTooLarge(2048))
        );
    }

    #[tokio::test]
    async fn unavailable_client_is_safe_default() {
        let client = UnavailableClient;
        let err = client
            .fetch(FetchRequest {
                url: "https://cdn.example.com/a.mp4".into(),
                method: "GET",
                range: Some("bytes=0-0".into()),
                max_bytes: 1024,
                timeout_ms: 5000,
            })
            .await;
        assert!(matches!(err, Err(FetchError::EgressUnavailable)));
    }
}
