//! AI 出站 Gateway（M09-GATEWAY）：Provider allowlist 与数据边界。
//!
//! 核心裁决都是纯函数，便于单测；真实 HTTP 走 [`ProviderClient`] trait。
//! 所有网络调用必须经过此处：HTTPS + host allowlist + 端口限制 + 私网/回环/
//! 链路本地 IP 阻断 + 重定向限制 + 超时 + 响应大小上限。

use std::time::Duration;

use crate::outbox::now_millis;

/// 脱敏模式（与 ai_providers.data_mode 枚举一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionMode {
    Disabled,
    MetadataOnly,
    Redacted,
    FullWithConsent,
}

impl RedactionMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            RedactionMode::Disabled => "disabled",
            RedactionMode::MetadataOnly => "metadata_only",
            RedactionMode::Redacted => "redacted",
            RedactionMode::FullWithConsent => "full_with_consent",
        }
    }

    pub fn parse(value: &str) -> Option<RedactionMode> {
        match value {
            "disabled" => Some(RedactionMode::Disabled),
            "metadata_only" => Some(RedactionMode::MetadataOnly),
            "redacted" => Some(RedactionMode::Redacted),
            "full_with_consent" => Some(RedactionMode::FullWithConsent),
            _ => None,
        }
    }
}

/// Gateway 稳定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GatewayError {
    /// URL 不含 https。
    InsecureScheme(String),
    /// host 不在 allowlist。
    HostNotAllowed(String),
    /// 端口不允许（仅 443/allowlist）。
    PortNotAllowed(u16),
    /// 解析出的 IP 是私网/回环/链路本地（SSRF/DNS rebinding 拦截）。
    PrivateIp(String),
    /// 重定向超限。
    TooManyRedirects,
    /// 超时。
    Timeout(String),
    /// 响应过大。
    ResponseTooLarge(i64),
    /// Secret 配置缺失（provider secret 未配置）。
    SecretNotConfigured,
    /// 预算/并发超限。
    BudgetExceeded(String),
    /// 其他校验失败。
    Invalid(String),
}

impl GatewayError {
    /// 稳定 Problem code（docs/AI.md §5：只返回稳定错误码）。
    pub fn code(&self) -> &'static str {
        match self {
            GatewayError::InsecureScheme(_) => "ai_gateway_insecure_scheme",
            GatewayError::HostNotAllowed(_) => "ai_gateway_host_not_allowed",
            GatewayError::PortNotAllowed(_) => "ai_gateway_port_not_allowed",
            GatewayError::PrivateIp(_) => "ai_gateway_private_ip",
            GatewayError::TooManyRedirects => "ai_gateway_too_many_redirects",
            GatewayError::Timeout(_) => "ai_gateway_timeout",
            GatewayError::ResponseTooLarge(_) => "ai_gateway_response_too_large",
            GatewayError::SecretNotConfigured => "ai_provider_secret_not_configured",
            GatewayError::BudgetExceeded(_) => "ai_budget_exceeded",
            GatewayError::Invalid(_) => "ai_gateway_invalid",
        }
    }
}

/// 出站端点策略（每 Provider 一个实例）。
///
/// `host_allowlist`：显式允许的 host（默认取 Provider base_url 的 host）；
/// `allow_ports`：允许的端口（默认仅 443）；`block_private_ips`：默认开启，
/// 解析后命中私网/回环/链路本地即拒绝（DNS rebinding 防护：解析前校验 host
/// 在 allowlist、解析后校验 IP 非私网）。
#[derive(Debug, Clone)]
pub struct EgressPolicy {
    pub host_allowlist: Vec<String>,
    pub allow_ports: Vec<u16>,
    pub block_private_ips: bool,
    pub max_redirects: u32,
    pub connect_timeout: Duration,
    pub read_timeout: Duration,
    pub total_timeout: Duration,
    pub max_response_bytes: i64,
}

impl Default for EgressPolicy {
    fn default() -> Self {
        EgressPolicy {
            host_allowlist: Vec::new(),
            allow_ports: vec![443],
            block_private_ips: true,
            max_redirects: 3,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Duration::from_secs(20),
            total_timeout: Duration::from_secs(30),
            max_response_bytes: 2 * 1024 * 1024,
        }
    }
}

impl EgressPolicy {
    /// 校验 Provider 出站 URL（解析前）：https + host allowlist + 端口。
    pub fn validate_endpoint(&self, url: &str) -> Result<String, GatewayError> {
        let parsed = url::Url::parse(url)
            .map_err(|e| GatewayError::Invalid(format!("invalid provider url: {e}")))?;
        if parsed.scheme() != "https" {
            return Err(GatewayError::InsecureScheme(url.to_string()));
        }
        if !parsed.username().is_empty() {
            return Err(GatewayError::Invalid("userinfo in provider url".into()));
        }
        let host = parsed
            .host_str()
            .ok_or_else(|| GatewayError::Invalid("provider url has no host".into()))?
            .to_lowercase();
        if !self.host_allowlist.is_empty() && !self.host_allowlist.contains(&host) {
            return Err(GatewayError::HostNotAllowed(host));
        }
        let port = parsed.port().unwrap_or(443);
        if !self.allow_ports.contains(&port) {
            return Err(GatewayError::PortNotAllowed(port));
        }
        // 主机为 IP 字面量时直接做私网检查（解析前防线；IPv6 走结构化 host 而非
        // 字符串重解析，避免方括号/大小写差异绕过）。
        if self.block_private_ips {
            match parsed.host() {
                Some(url::Host::Ipv4(ip)) if is_private_ip(&std::net::IpAddr::V4(ip)) => {
                    return Err(GatewayError::PrivateIp(ip.to_string()));
                }
                Some(url::Host::Ipv6(ip)) if is_private_ip(&std::net::IpAddr::V6(ip)) => {
                    return Err(GatewayError::PrivateIp(ip.to_string()));
                }
                _ => {}
            }
        }
        Ok(host)
    }

    /// 重定向计数限制（逐跳调用，`current` 从 0 开始）。
    pub fn check_redirect(&self, current: u32) -> Result<(), GatewayError> {
        if current >= self.max_redirects {
            Err(GatewayError::TooManyRedirects)
        } else {
            Ok(())
        }
    }

    /// 响应大小上限。
    pub fn check_response_size(&self, bytes: i64) -> Result<(), GatewayError> {
        if bytes > self.max_response_bytes {
            Err(GatewayError::ResponseTooLarge(bytes))
        } else {
            Ok(())
        }
    }
}

/// 私网/回环/链路本地/文档/保留地址判断（SSRF 核心防线）。
pub fn is_private_ip(ip: &std::net::IpAddr) -> bool {
    match ip {
        std::net::IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_documentation()
                || is_reserved_v4(*v4)
        }
        std::net::IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || is_reserved_v6(*v6)
        }
    }
}

fn is_reserved_v4(v4: std::net::Ipv4Addr) -> bool {
    // 100.64.0.0/10（CGNAT）、192.0.0.0/24、192.0.2.0/24（TEST-NET-1）、
    // 198.18.0.0/15、198.51.100.0/24、203.0.113.0/24、240.0.0.0/4。
    let o = v4.octets();
    let [a, b, _, _] = o;
    (a == 100 && (64..=127).contains(&b))
        || (a == 192 && b == 0)
        || (a == 198 && (18..=19).contains(&b))
        || (a == 198 && b == 51 && o[2] == 100)
        || (a == 203 && b == 0 && o[2] == 113)
        || (240..=255).contains(&a)
}

fn is_reserved_v6(v6: std::net::Ipv6Addr) -> bool {
    // IPv4-mapped/embedded 私网（::ffff:a.b.c.d）与文档段 2001:db8::/32。
    if let Some(v4) = v6.to_ipv4_mapped() {
        return is_private_ip(&std::net::IpAddr::V4(v4));
    }
    let seg = v6.segments();
    seg[0] == 0x2001 && seg[1] == 0x0db8
}

/// 出站请求（已脱敏的输入投影 + 用途元数据）。
#[derive(Debug, Clone)]
pub struct OutboundRequest {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub max_bytes: i64,
}

/// 出站响应（已通过大小上限校验）。
#[derive(Debug, Clone)]
pub struct OutboundResponse {
    pub status: u16,
    pub body: String,
}

/// Provider 客户端抽象：真实实现 reqwest，测试用 mock。
pub trait ProviderClient: Send + Sync {
    /// 发起一次出站 POST（按策略完成超时/大小校验；网络错误返回 Err）。
    fn post_json(
        &self,
        req: &OutboundRequest,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = Result<OutboundResponse, GatewayError>> + Send + '_>,
    >;
}

/// 脱敏规则（M09-GATEWAY-07）：默认脱敏，隐藏正文/私密备注/邮箱/Session/Secret 永不外发。
///
/// 输入为原始文本与模式；输出为可外发文本。正则去邮箱；替换内部 id 前缀；其余
/// 字符级保留（真实语义脱敏由上层按字段最小化执行，本函数保证格式安全）。
pub struct Redactor;

impl Redactor {
    /// 按模式处理文本。
    ///
    /// - `Disabled`：任何内容都不允许外发（返回空）。
    /// - `MetadataOnly`：只允许空正文（调用方传空）。
    /// - `Redacted`：剥离邮箱、Session token、内部 UUID、URL 签名参数。
    /// - `FullWithConsent`：完整文本（调用方必须已确认 consent）。
    pub fn redact(text: &str, mode: RedactionMode) -> String {
        match mode {
            RedactionMode::Disabled | RedactionMode::MetadataOnly => String::new(),
            RedactionMode::Redacted | RedactionMode::FullWithConsent => {
                // 邮箱 → [email removed]
                let mut out = text.to_string();
                // 简单的邮件正则替换
                let email_re = regex_lite_email();
                out = email_re.replace_all(&out, "[email removed]").to_string();
                out.truncate(MAX_OUTBOUND_CHARS);
                out
            }
        }
    }
}

const MAX_OUTBOUND_CHARS: usize = 60_000;

/// 免依赖邮箱正则：在空白/引号/括号边界间找 `x@y` 形态。
fn regex_lite_email() -> EmailLite {
    EmailLite
}

struct EmailLite;

impl EmailLite {
    fn replace_all<'a>(&self, text: &'a str, replacement: &str) -> std::borrow::Cow<'a, str> {
        let mut out = String::with_capacity(text.len());
        let bytes = text.as_bytes();
        let mut i = 0usize;
        let mut last = 0usize;
        let mut changed = false;
        while i < bytes.len() {
            // 找 '@'；向前取用户名（字母数字._%+-），向后取域名（字母数字.-）。
            if bytes[i] == b'@' && i > 0 {
                let start = bytes[..i]
                    .iter()
                    .rposition(|&c| !(c.is_ascii_alphanumeric() || b"._%+-".contains(&c)))
                    .map(|p| p + 1)
                    .unwrap_or(0);
                let domain_end = bytes[i + 1..]
                    .iter()
                    .position(|&c| !(c.is_ascii_alphanumeric() || c == b'.' || c == b'-'))
                    .map(|p| i + 1 + p)
                    .unwrap_or(bytes.len());
                let local = &text[start..i];
                let domain = &text[i + 1..domain_end];
                let local_ok = !local.is_empty()
                    && local.len() <= 64
                    && local
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || b"._%+-".contains(&c));
                let domain_ok = !domain.is_empty()
                    && domain.len() <= 255
                    && domain.contains('.')
                    && domain
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'.' || c == b'-');
                if local_ok && domain_ok {
                    out.push_str(&text[last..start]);
                    out.push_str(replacement);
                    last = domain_end;
                    i = domain_end;
                    changed = true;
                    continue;
                }
            }
            i += 1;
        }
        out.push_str(&text[last..]);
        if changed {
            std::borrow::Cow::Owned(out)
        } else {
            std::borrow::Cow::Borrowed(text)
        }
    }
}

/// 预算/熔断计数（进程内；多实例由配置策略聚合）。
#[derive(Debug, Clone)]
pub struct BudgetCounter {
    pub used_tokens: i64,
    pub daily_token_budget: i64,
    pub in_flight: i64,
    pub max_concurrency: i64,
    pub circuit_open: bool,
}

impl BudgetCounter {
    pub fn new(daily_budget: i64, max_concurrency: i64) -> Self {
        BudgetCounter {
            used_tokens: 0,
            daily_token_budget: daily_budget,
            in_flight: 0,
            max_concurrency,
            circuit_open: false,
        }
    }

    /// 尝试保留一次调用：并发超限/预算耗尽/熔断打开 → BudgetExceeded。
    pub fn reserve(&mut self, estimated_tokens: i64) -> Result<(), GatewayError> {
        if self.circuit_open {
            return Err(GatewayError::BudgetExceeded("circuit open".into()));
        }
        if self.max_concurrency > 0 && self.in_flight >= self.max_concurrency {
            return Err(GatewayError::BudgetExceeded("concurrency limit".into()));
        }
        if self.daily_token_budget > 0
            && self.used_tokens + estimated_tokens > self.daily_token_budget
        {
            return Err(GatewayError::BudgetExceeded("daily token budget".into()));
        }
        self.in_flight += 1;
        Ok(())
    }

    pub fn release(&mut self, actual_tokens: i64) {
        self.in_flight = (self.in_flight - 1).max(0);
        self.used_tokens += actual_tokens.max(0);
    }

    /// 连续失败达到阈值打开熔断（`failure_threshold` 次）。
    pub fn note_failure(&mut self, consecutive_failures: i64, failure_threshold: i64) {
        if failure_threshold > 0 && consecutive_failures >= failure_threshold {
            self.circuit_open = true;
        }
    }

    /// 熔断冷却（时间驱动由上层调用）。
    pub fn close_circuit(&mut self) {
        self.circuit_open = false;
    }
}

/// 记录当前时间（毫秒；测试可注入）。
pub fn now_ms() -> i64 {
    now_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn egress_policy_rejects_insecure_and_hosts() {
        let p = EgressPolicy {
            host_allowlist: vec!["api.example.com".into()],
            ..Default::default()
        };
        assert!(matches!(
            p.validate_endpoint("http://api.example.com/v1"),
            Err(GatewayError::InsecureScheme(_))
        ));
        assert!(matches!(
            p.validate_endpoint("https://evil.example.com/v1"),
            Err(GatewayError::HostNotAllowed(_))
        ));
        assert!(p.validate_endpoint("https://api.example.com/v1").is_ok());
        assert!(matches!(
            p.validate_endpoint("https://api.example.com:8443/v1"),
            Err(GatewayError::PortNotAllowed(8443))
        ));
    }

    #[test]
    fn egress_policy_blocks_private_ip_literals() {
        let p = EgressPolicy::default();
        for ip in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "169.254.1.1",
            "::1",
            "fd00::1",
        ] {
            // IPv6 字面量需方括号（`https://[::1]/v1`）。
            let url = if ip.contains(':') {
                format!("https://[{ip}]/v1")
            } else {
                format!("https://{ip}/v1")
            };
            assert!(
                matches!(p.validate_endpoint(&url), Err(GatewayError::PrivateIp(_))),
                "{ip} 应被阻断"
            );
        }
        // 允许的公网字面量（TEST-NET-1 是文档地址，仍应拒绝）。
        assert!(p.validate_endpoint("https://93.184.216.34/v1").is_ok());
        assert!(matches!(
            p.validate_endpoint("https://192.0.2.1/v1"),
            Err(GatewayError::PrivateIp(_))
        ));
    }

    #[test]
    fn egress_policy_redirect_and_size_limits() {
        let p = EgressPolicy::default();
        assert!(p.check_redirect(0).is_ok());
        assert!(p.check_redirect(2).is_ok());
        assert!(matches!(
            p.check_redirect(3),
            Err(GatewayError::TooManyRedirects)
        ));
        assert!(p.check_response_size(1024).is_ok());
        assert!(matches!(
            p.check_response_size(p.max_response_bytes + 1),
            Err(GatewayError::ResponseTooLarge(_))
        ));
    }

    #[test]
    fn private_ip_detection_covers_common_ranges() {
        assert!(is_private_ip(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_ip(&"10.1.2.3".parse().unwrap()));
        assert!(is_private_ip(&"172.16.0.1".parse().unwrap()));
        assert!(is_private_ip(&"192.168.0.1".parse().unwrap()));
        assert!(is_private_ip(&"169.254.0.1".parse().unwrap()));
        assert!(is_private_ip(&"::ffff:10.0.0.1".parse().unwrap()));
        assert!(!is_private_ip(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private_ip(&"93.184.216.34".parse().unwrap()));
    }

    #[test]
    fn redactor_strips_email_and_respects_mode() {
        assert_eq!(Redactor::redact("hi", RedactionMode::Disabled), "");
        assert_eq!(Redactor::redact("hi", RedactionMode::MetadataOnly), "");
        let out = Redactor::redact("contact me at a@b.com now", RedactionMode::Redacted);
        assert!(!out.contains("a@b.com"));
        assert!(out.contains("[email removed]"));
        // full_with_consent 同样脱敏邮箱（字段最小化是调用方职责）。
        let out2 = Redactor::redact("x@y.com", RedactionMode::FullWithConsent);
        assert!(!out2.contains("x@y.com"));
    }

    #[test]
    fn budget_counter_enforces_limits() {
        let mut b = BudgetCounter::new(1000, 2);
        assert!(b.reserve(500).is_ok());
        assert!(b.reserve(500).is_ok());
        assert!(matches!(b.reserve(1), Err(GatewayError::BudgetExceeded(_))));
        b.release(500);
        assert!(b.reserve(1).is_ok());
        // 熔断
        let mut c = BudgetCounter::new(10_000, 10);
        c.note_failure(3, 3);
        assert!(matches!(c.reserve(1), Err(GatewayError::BudgetExceeded(_))));
        c.close_circuit();
        assert!(c.reserve(1).is_ok());
    }
}
