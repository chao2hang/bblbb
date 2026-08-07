//! 反爬/行为检测（M08-CRAWL）：行为信号 → 分级处置。
//!
//! 处置阶梯（docs/CRAWLER-POLICY.md §4）：observe → throttle → 429 → challenge
//! → 临时封禁 → 人工复核。
//!
//! 实现要点：
//! - [`resolve_client_ip`]：只信任可信代理链（XFF 最右跳必须是可信代理），
//!   不信任客户端伪造的 `X-Forwarded-For` / `X-Real-IP`；
//! - [`Bucket`]：匿名/登录/搜索/RSS/sitemap/公开文章/管理独立限流桶；
//! - throttle 只增加延迟，不改变安全授权与内容结果；
//! - 429 带 `Retry-After` 与 `RateLimit-*` 头（复用 OpenAPI `RateLimited`）；
//! - 挑战为无路由实现：响应 `X-BBLBB-Challenge` 一次性 token，重试带 token
//!   头验证；验证失败计入并触发临时封禁；
//! - AI 训练爬虫默认拒绝（UA 名单配置化）；普通爬虫按行为参与风控；
//! - 临时封禁写审计、不泄漏检测规则；告警与人工复核查询进程内保留，
//!   日志最小化（只记录 IP 段与稳定类别）。

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::Duration;

use axum::{
    body::Body,
    extract::State,
    http::{header, HeaderMap, HeaderValue, Request as HttpRequest, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};
use base64::Engine as _;
use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

use crate::app::AppState;
use crate::error::AppError;
use crate::middleware::request_id::RequestId;
use crate::outbox::now_millis;
use crate::ratelimit::RateLimiter;
use uuid::Uuid;

type HmacSha256 = Hmac<Sha256>;

/// 爬虫类别（UA 名单配置化；UA 不单独作为可信依据，只用于分类）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BotClass {
    SearchEngine,
    AiTrainingCrawler,
    SocialPreview,
    Unknown,
}

/// 限流桶（docs/CRAWLER-POLICY.md §5：搜索/RSS/sitemap/公开文章独立桶）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket {
    Anonymous,
    Authenticated,
    Login,
    Search,
    Rss,
    Sitemap,
    PublicArticle,
    Admin,
}

impl Bucket {
    pub fn as_str(&self) -> &'static str {
        match self {
            Bucket::Anonymous => "anonymous",
            Bucket::Authenticated => "authenticated",
            Bucket::Login => "login",
            Bucket::Search => "search",
            Bucket::Rss => "rss",
            Bucket::Sitemap => "sitemap",
            Bucket::PublicArticle => "public_article",
            Bucket::Admin => "admin",
        }
    }

    /// 路径 → 桶（None = 不参与风控，如健康检查/契约端点）。
    pub fn for_path(path: &str, method: &str, authenticated: bool) -> Option<Bucket> {
        if path == "/healthz" || path == "/readyz" || path == "/api/v1/openapi.json" {
            return None;
        }
        if path == "/api/v1/search" {
            return Some(Bucket::Search);
        }
        if path == "/api/v1/rss" || path == "/api/v1/atom" {
            return Some(Bucket::Rss);
        }
        if path == "/api/v1/sitemap.xml" || path == "/robots.txt" {
            return Some(Bucket::Sitemap);
        }
        if path.starts_with("/api/v1/admin/") {
            return Some(Bucket::Admin);
        }
        if path.starts_with("/api/v1/auth/") {
            return Some(Bucket::Login);
        }
        if method == "GET" && path.starts_with("/api/v1/posts") {
            return Some(Bucket::PublicArticle);
        }
        if authenticated {
            Some(Bucket::Authenticated)
        } else {
            Some(Bucket::Anonymous)
        }
    }
}

/// 反爬配置（默认值来自 docs/CRAWLER-POLICY.md；生产可用环境变量覆盖）。
#[derive(Debug, Clone)]
pub struct AntibotConfig {
    pub enabled: bool,
    /// 可信代理 IP（仅这些地址作为 XFF 最右跳时信任代理头；默认回环）。
    pub trusted_proxies: Vec<std::net::IpAddr>,
    /// (次数, 窗口毫秒) 按桶。
    pub bucket_limits: HashMap<&'static str, (u32, i64)>,
    /// 触发降速的剩余额度比例（剩余 <= limit * ratio 时加延迟）。
    pub throttle_ratio: f32,
    /// 降速延迟（毫秒），只加延迟不改内容/授权。
    pub throttle_delay_ms: u64,
    /// 429 后是否签发挑战 token。
    pub challenge_enabled: bool,
    /// 挑战 token 有效期（毫秒）。
    pub challenge_ttl_ms: i64,
    /// 挑战失败多少次触发临时封禁。
    pub challenge_fail_ban_threshold: u32,
    /// 临时封禁时长（毫秒）。
    pub temp_ban_ms: i64,
    /// AI 训练爬虫 UA 名单（默认拒绝）。
    pub ai_crawler_uas: Vec<String>,
    /// 普通搜索引擎 UA 名单（允许索引明确公开内容）。
    pub search_engine_uas: Vec<String>,
    /// 社交预览抓取器名单。
    pub social_preview_uas: Vec<String>,
}

impl Default for AntibotConfig {
    fn default() -> Self {
        AntibotConfig {
            enabled: true,
            trusted_proxies: vec![
                std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST),
            ],
            bucket_limits: [
                ("anonymous", (120, 60_000)),
                ("authenticated", (300, 60_000)),
                ("login", (30, 60_000)),
                ("search", (30, 60_000)),
                ("rss", (30, 600_000)),
                ("sitemap", (60, 600_000)),
                ("public_article", (120, 60_000)),
                ("admin", (120, 60_000)),
            ]
            .into_iter()
            .collect(),
            throttle_ratio: 0.15,
            throttle_delay_ms: 150,
            challenge_enabled: true,
            challenge_ttl_ms: 300_000,
            challenge_fail_ban_threshold: 3,
            temp_ban_ms: 600_000,
            ai_crawler_uas: vec![
                "GPTBot".to_string(),
                "CCBot".to_string(),
                "Google-Extended".to_string(),
                "ClaudeBot".to_string(),
                "anthropic-ai".to_string(),
                "Bytespider".to_string(),
                "PerplexityBot".to_string(),
                "Amazonbot".to_string(),
            ],
            search_engine_uas: vec![
                "Googlebot".to_string(),
                "Bingbot".to_string(),
                "DuckDuckBot".to_string(),
                "YandexBot".to_string(),
                "Baiduspider".to_string(),
            ],
            social_preview_uas: vec![
                "facebookexternalhit".to_string(),
                "Twitterbot".to_string(),
                "Slackbot".to_string(),
                "LinkedInBot".to_string(),
            ],
        }
    }
}

impl AntibotConfig {
    pub fn limit_for(&self, bucket: Bucket) -> (u32, i64) {
        self.bucket_limits
            .get(bucket.as_str())
            .copied()
            .unwrap_or((120, 60_000))
    }
}

/// 反爬告警（人工复核用；隐私最小化——只记 IP 段与稳定类别，不记完整 UA/路径）。
#[derive(Debug, Clone)]
pub struct AntibotAlert {
    pub at_ms: i64,
    pub kind: &'static str,
    /// IPv4 前三段 / IPv6 前四组（保留前缀便于复核，不存完整地址）。
    pub ip_segment: String,
    pub bucket: &'static str,
    pub reason: String,
}

#[derive(Debug, Clone, Copy)]
struct BanState {
    until_ms: i64,
    review: bool,
}

/// 行为检测引擎（进程内；AppState 持有 Arc 共享）。
pub struct AntibotEngine {
    config: AntibotConfig,
    limiter: RateLimiter,
    secret: [u8; 32],
    bans: Mutex<HashMap<String, BanState>>,
    /// 已用挑战 token → 过期时间（惰性清理）。
    challenge_used: Mutex<HashMap<String, i64>>,
    challenge_fails: Mutex<HashMap<String, u32>>,
    alerts: Mutex<VecDeque<AntibotAlert>>,
}

impl Default for AntibotEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl AntibotEngine {
    pub fn new() -> Self {
        // 每进程随机 secret：由 uuid v7（fast-rng）与时钟派生，无需额外熵依赖。
        let mut secret = [0u8; 32];
        let mut h = Sha256::new();
        h.update(Uuid::now_v7().as_bytes());
        h.update(Uuid::now_v7().as_bytes());
        h.update(now_millis().to_be_bytes());
        let digest = h.finalize();
        secret.copy_from_slice(&digest);
        AntibotEngine {
            config: AntibotConfig::default(),
            limiter: RateLimiter::new(),
            secret,
            bans: Mutex::new(HashMap::new()),
            challenge_used: Mutex::new(HashMap::new()),
            challenge_fails: Mutex::new(HashMap::new()),
            alerts: Mutex::new(VecDeque::new()),
        }
    }

    /// 测试/管理用：自定义配置。
    pub fn with_config(config: AntibotConfig) -> Self {
        let mut secret = [0u8; 32];
        // 测试注入固定密钥以便验证 token；生产用 `new()`。
        for (i, b) in b"antibot-test-secret-0123456789ab".iter().enumerate() {
            secret[i] = *b;
        }
        AntibotEngine {
            config,
            limiter: RateLimiter::new(),
            secret,
            bans: Mutex::new(HashMap::new()),
            challenge_used: Mutex::new(HashMap::new()),
            challenge_fails: Mutex::new(HashMap::new()),
            alerts: Mutex::new(VecDeque::new()),
        }
    }

    pub fn config(&self) -> &AntibotConfig {
        &self.config
    }

    /// 当前是否封禁（含到期清理）。
    pub fn check_ban(&self, ip: &str, now: i64) -> Option<bool> {
        let mut bans = self.bans.lock().expect("antibot bans poisoned");
        match bans.get(ip) {
            Some(b) if b.until_ms > now => Some(b.review),
            Some(_) => {
                bans.remove(ip);
                None
            }
            None => None,
        }
    }

    /// 临时封禁（写审计由调用方/Middleware 执行；此处只记状态与告警）。
    pub fn temp_ban(&self, ip: &str, reason: &str, now: i64) {
        let review = ip_has_suspicious_shape(ip);
        {
            let mut bans = self.bans.lock().expect("antibot bans poisoned");
            bans.insert(
                ip.to_string(),
                BanState {
                    until_ms: now + self.config.temp_ban_ms,
                    review,
                },
            );
        }
        self.push_alert(now, "temp_ban", ip, reason);
        tracing::warn!(target: "antibot", ip_segment = %segment(ip), reason, "temporary ban issued");
    }

    /// 签发一次性挑战 token（HMAC(secret, ip|bucket|nonce|expiry)）。
    pub fn issue_challenge(&self, ip: &str, bucket: Bucket, now: i64) -> String {
        let nonce = uuid::Uuid::now_v7().to_string();
        let expiry = now + self.config.challenge_ttl_ms;
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("hmac key");
        mac.update(ip.as_bytes());
        mac.update(b"|");
        mac.update(bucket.as_str().as_bytes());
        mac.update(b"|");
        mac.update(nonce.as_bytes());
        mac.update(b"|");
        mac.update(expiry.to_be_bytes().as_slice());
        let tag = mac.finalize().into_bytes();
        let mut payload = Vec::with_capacity(nonce.len() + 8 + tag.len());
        payload.extend_from_slice(nonce.as_bytes());
        payload.extend_from_slice(&expiry.to_be_bytes());
        payload.extend_from_slice(&tag);
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload)
    }

    /// 验证一次性挑战 token；成功标记已用（防重放）。返回是否通过。
    pub fn verify_challenge(&self, token: &str, ip: &str, bucket: Bucket, now: i64) -> bool {
        let Ok(payload) = base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(token.as_bytes())
        else {
            self.note_challenge_failure(ip, now);
            return false;
        };
        if payload.len() < 32 {
            self.note_challenge_failure(ip, now);
            return false;
        }
        let expiry = i64::from_be_bytes(payload[36..44].try_into().expect("fixed slice len"));
        let nonce = &payload[..36];
        let tag = &payload[44..];
        let mut mac = HmacSha256::new_from_slice(&self.secret).expect("hmac key");
        mac.update(ip.as_bytes());
        mac.update(b"|");
        mac.update(bucket.as_str().as_bytes());
        mac.update(b"|");
        mac.update(nonce);
        mac.update(b"|");
        mac.update(expiry.to_be_bytes().as_slice());
        let expected = mac.finalize().into_bytes();
        if tag != &expected[..] {
            self.note_challenge_failure(ip, now);
            return false;
        }
        if expiry <= now {
            self.note_challenge_failure(ip, now);
            return false;
        }
        let key = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(nonce);
        {
            let mut used = self
                .challenge_used
                .lock()
                .expect("antibot challenges poisoned");
            // 惰性清理过期条目，防无限增长。
            used.retain(|_, expires_at| *expires_at > now);
            if used.contains_key(&key) {
                return false;
            }
            used.insert(key, expiry);
        }
        // 通过挑战 → 重置失败计数。
        self.challenge_fails
            .lock()
            .expect("antibot challenge_fails poisoned")
            .remove(ip);
        true
    }

    fn note_challenge_failure(&self, ip: &str, now: i64) {
        let threshold = self.config.challenge_fail_ban_threshold;
        let fails = {
            let mut map = self
                .challenge_fails
                .lock()
                .expect("antibot challenge_fails poisoned");
            let n = map.entry(ip.to_string()).or_insert(0);
            *n += 1;
            *n
        };
        if threshold > 0 && fails >= threshold {
            self.temp_ban(ip, "challenge_failed_threshold", now);
            self.challenge_fails
                .lock()
                .expect("antibot challenge_fails poisoned")
                .remove(ip);
        }
    }

    fn push_alert(&self, now: i64, kind: &'static str, ip: &str, reason: &str) {
        let alert = AntibotAlert {
            at_ms: now,
            kind,
            ip_segment: segment(ip),
            bucket: "global",
            reason: reason.to_string(),
        };
        let mut alerts = self.alerts.lock().expect("antibot alerts poisoned");
        alerts.push_back(alert);
        while alerts.len() > 256 {
            alerts.pop_front();
        }
    }

    /// 人工复核查询（进程内最近告警）。
    pub fn review_query(&self) -> Vec<AntibotAlert> {
        let alerts = self.alerts.lock().expect("antibot alerts poisoned");
        alerts.iter().rev().cloned().collect()
    }

    /// 手动解封（人工复核后）。
    pub fn unban(&self, ip: &str) -> bool {
        self.bans
            .lock()
            .expect("antibot bans poisoned")
            .remove(ip)
            .is_some()
    }
}

/// IP 是否具有可疑形态（进入人工复核的启发式；不泄漏检测规则本身）。
fn ip_has_suspicious_shape(ip: &str) -> bool {
    // 非标准形态（含非数字/点）或端口附带 → 复核。
    let v4 = ip.split('.').count() == 4 && ip.bytes().all(|b| b.is_ascii_digit() || b == b'.');
    !v4 && !ip.contains(':')
}

/// 隐私最小化 IP 段（IPv4 前 3 段 / IPv6 前 4 组）。
pub fn segment(ip: &str) -> String {
    if let Ok(v4) = ip.parse::<std::net::Ipv4Addr>() {
        let o = v4.octets();
        format!("{}.{}.{}.0/24", o[0], o[1], o[2])
    } else if ip.contains(':') {
        let groups: Vec<&str> = ip.split(':').take(4).collect();
        format!("{}:x:x:x", groups.join(":"))
    } else {
        "unknown".to_string()
    }
}

/// UA 分类（名单匹配大小写不敏感；缺省 Unknown 参与行为风控）。
pub fn classify_ua(ua: &str, config: &AntibotConfig) -> BotClass {
    let lower = ua.to_lowercase();
    if config
        .ai_crawler_uas
        .iter()
        .any(|b| lower.contains(&b.to_lowercase()))
    {
        return BotClass::AiTrainingCrawler;
    }
    if config
        .search_engine_uas
        .iter()
        .any(|b| lower.contains(&b.to_lowercase()))
    {
        return BotClass::SearchEngine;
    }
    if config
        .social_preview_uas
        .iter()
        .any(|b| lower.contains(&b.to_lowercase()))
    {
        return BotClass::SocialPreview;
    }
    BotClass::Unknown
}

/// 可信代理链解析客户端 IP（docs/CRAWLER-POLICY.md §5）。
///
/// 规则：
/// - 只有 XFF 最右跳是可信代理时才信任该头（否则视为客户端伪造并忽略）；
/// - 客户端取链中最左的合法 IP；
/// - `x-real-ip` 仅在它等于 XFF 首跳（代理转发一致）时信任；
/// - 全部失败回退 `"unknown"`（共享桶天然限流）。
pub fn resolve_client_ip(headers: &HeaderMap, trusted: &[std::net::IpAddr]) -> String {
    let xff = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let hops: Vec<&str> = xff
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();

    let trusted_chain = !hops.is_empty()
        && hops
            .last()
            .and_then(|hop| hop.parse::<std::net::IpAddr>().ok())
            .map(|ip| trusted.contains(&ip))
            .unwrap_or(false);

    if trusted_chain {
        // 最左合法 IP = 客户端。
        for hop in hops.iter() {
            if hop.parse::<std::net::IpAddr>().is_ok() {
                return (*hop).to_string();
            }
        }
    }

    // x-real-ip：仅当代理链一致时信任。
    if let Some(real) = headers.get("x-real-ip").and_then(|v| v.to_str().ok()) {
        let real = real.trim();
        if let Ok(ip) = real.parse::<std::net::IpAddr>() {
            let consistent = hops.first().map(|h| *h == real).unwrap_or(false);
            if consistent && trusted_chain {
                return ip.to_string();
            }
        }
    }
    "unknown".to_string()
}

/// 从扩展读取 request_id（由 request_id 中间件先注入）。
fn req_id(request: &HttpRequest<Body>) -> String {
    request
        .extensions()
        .get::<RequestId>()
        .map(|r| r.0.clone())
        .unwrap_or_else(|| "unknown".to_owned())
}

/// 风控中间件（M08-CRAWL）：observe → throttle → 429 → challenge → ban。
pub async fn antibot_guard(
    State(state): State<AppState>,
    request: HttpRequest<Body>,
    next: Next,
) -> Response {
    let engine = state.antibot;
    let config = engine.config();
    if !config.enabled {
        return next.run(request).await;
    }

    let now = now_millis();
    let path = request.uri().path().to_string();
    let method = request.method().as_str().to_string();
    let Some(bucket) = Bucket::for_path(
        &path,
        &method,
        request.headers().contains_key(header::COOKIE),
    ) else {
        return next.run(request).await;
    };

    let ua = request
        .headers()
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    // 默认拒绝 AI 训练爬虫（CRAWL-08）；robots 与 HTTP 授权同时执行（CRAWL-09）。
    if classify_ua(&ua, config) == BotClass::AiTrainingCrawler {
        return AppError::with_code(
            StatusCode::FORBIDDEN,
            "crawler_denied",
            "Forbidden",
            "this crawler category is not allowed",
            req_id(&request),
        )
        .into_response();
    }

    let ip = resolve_client_ip(request.headers(), &config.trusted_proxies);

    // 临时封禁（CRAWL-07）。
    if let Some(_review) = engine.check_ban(&ip, now) {
        let mut response = AppError::with_code(
            StatusCode::FORBIDDEN,
            "temporarily_banned",
            "Forbidden",
            "request blocked by rate policy; try again later",
            req_id(&request),
        )
        .into_response();
        if let Some(retry) = retry_after_secs(&engine, now) {
            response.headers_mut().insert(
                header::RETRY_AFTER,
                HeaderValue::from_str(&retry.to_string()).unwrap_or(HeaderValue::from_static("60")),
            );
        }
        return response;
    }

    // 挑战验证（CRAWL-06）：带 token 重试先验证再继续。
    let mut challenge_ok = false;
    if config.challenge_enabled {
        if let Some(token) = request
            .headers()
            .get("x-bblbb-challenge")
            .and_then(|v| v.to_str().ok())
        {
            if engine.verify_challenge(token, &ip, bucket, now) {
                challenge_ok = true;
            } else {
                return AppError::with_code(
                    StatusCode::FORBIDDEN,
                    "challenge_required",
                    "Forbidden",
                    "challenge token missing, expired, or already used",
                    req_id(&request),
                )
                .into_response();
            }
        }
    }

    // 限流（CRAWL-03/05）。
    let (limit, window_ms) = config.limit_for(bucket);
    let status = engine.limiter.check(
        &format!("antibot:{}:{}", bucket.as_str(), ip),
        limit,
        window_ms,
        now,
    );

    if !status.allowed {
        if challenge_ok {
            // 已通过一次性挑战 → 放行本次请求。
        } else if config.challenge_enabled {
            // 优先挑战而非硬 429（CRAWL-06 无障碍替代路径）。
            let token = engine.issue_challenge(&ip, bucket, now);
            let mut response = AppError::with_code(
                StatusCode::FORBIDDEN,
                "challenge_required",
                "Forbidden",
                "rate limit reached; complete the challenge to continue",
                req_id(&request),
            )
            .into_response();
            response.headers_mut().insert(
                "x-bblbb-challenge",
                HeaderValue::from_str(&token).unwrap_or(HeaderValue::from_static("")),
            );
            response.headers_mut().insert(
                header::RETRY_AFTER,
                HeaderValue::from_str(&status.retry_after_secs.to_string())
                    .unwrap_or(HeaderValue::from_static("60")),
            );
            return response;
        } else {
            return AppError::rate_limited(
                "too many requests; slow down and retry",
                req_id(&request),
                status.retry_after_secs,
                limit,
                status.remaining,
                status.reset_at_ms.div_euclid(1000),
            )
            .into_response();
        }
    }

    // observe → throttle（CRAWL-04）：只加延迟，不改内容/授权。
    if !challenge_ok
        && config.throttle_delay_ms > 0
        && status.remaining <= (limit as f32 * config.throttle_ratio) as u32
    {
        tokio::time::sleep(Duration::from_millis(config.throttle_delay_ms)).await;
    }

    next.run(request).await
}

fn retry_after_secs(engine: &AntibotEngine, now: i64) -> Option<u64> {
    // 封禁剩余时间（秒，至少 1）。
    let bans = engine.bans.lock().expect("antibot bans poisoned");
    let remaining_ms = bans.values().map(|b| b.until_ms - now).max().unwrap_or(0);
    if remaining_ms > 0 {
        Some(((remaining_ms + 999) / 1000).max(1) as u64)
    } else {
        None
    }
}

/// 请求哈希（告警/日志去重用；不存完整路径）。
pub fn hash_path(path: &str) -> String {
    let mut h = Sha256::new();
    h.update(path.as_bytes());
    hex::encode(h.finalize())[..12].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    fn test_config() -> AntibotConfig {
        let mut c = AntibotConfig::default();
        c.bucket_limits.insert("search", (3, 60_000));
        c.bucket_limits.insert("anonymous", (3, 60_000));
        c.challenge_fail_ban_threshold = 2;
        c
    }

    #[test]
    fn bucket_for_path_maps_public_and_admin() {
        let cfg_bucket = |p: &str, m: &str, a: bool| Bucket::for_path(p, m, a);
        assert_eq!(
            cfg_bucket("/api/v1/search", "GET", false),
            Some(Bucket::Search)
        );
        assert_eq!(cfg_bucket("/api/v1/rss", "GET", false), Some(Bucket::Rss));
        assert_eq!(cfg_bucket("/api/v1/atom", "GET", false), Some(Bucket::Rss));
        assert_eq!(
            cfg_bucket("/api/v1/sitemap.xml", "GET", false),
            Some(Bucket::Sitemap)
        );
        assert_eq!(
            cfg_bucket("/robots.txt", "GET", false),
            Some(Bucket::Sitemap)
        );
        assert_eq!(
            cfg_bucket("/api/v1/posts/abc", "GET", false),
            Some(Bucket::PublicArticle)
        );
        assert_eq!(
            cfg_bucket("/api/v1/admin/users", "GET", false),
            Some(Bucket::Admin)
        );
        assert_eq!(
            cfg_bucket("/api/v1/auth/login", "POST", false),
            Some(Bucket::Login)
        );
        assert_eq!(cfg_bucket("/healthz", "GET", false), None);
        assert_eq!(cfg_bucket("/readyz", "GET", false), None);
        assert_eq!(cfg_bucket("/api/v1/openapi.json", "GET", false), None);
        assert_eq!(
            cfg_bucket("/api/v1/users/me", "GET", false),
            Some(Bucket::Anonymous)
        );
        assert_eq!(
            cfg_bucket("/api/v1/users/me", "GET", true),
            Some(Bucket::Authenticated)
        );
    }

    #[test]
    fn classify_ua_default_denies_ai_crawlers() {
        let config = AntibotConfig::default();
        assert_eq!(
            classify_ua("Mozilla/5.0 GPTBot/1.0", &config),
            BotClass::AiTrainingCrawler
        );
        assert_eq!(
            classify_ua("CCBot/2.0 (+http://commoncrawl.org)", &config),
            BotClass::AiTrainingCrawler
        );
        assert_eq!(
            classify_ua("ClaudeBot/1.0", &config),
            BotClass::AiTrainingCrawler
        );
        assert_eq!(
            classify_ua("Google-Extended", &config),
            BotClass::AiTrainingCrawler
        );
        assert_eq!(
            classify_ua("Mozilla/5.0 Googlebot/2.1", &config),
            BotClass::SearchEngine
        );
        assert_eq!(
            classify_ua("Twitterbot/1.0", &config),
            BotClass::SocialPreview
        );
        assert_eq!(
            classify_ua("Mozilla/5.0 (Macintosh; Intel Mac OS X)", &config),
            BotClass::Unknown
        );
    }

    #[test]
    fn resolve_ip_requires_trusted_chain() {
        let trusted = [std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)];
        let mut headers = HeaderMap::new();
        // 伪造：最右跳不是可信代理 → 忽略。
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.7, 203.0.113.9"),
        );
        assert_eq!(resolve_client_ip(&headers, &trusted), "unknown");
        // 合法：最右跳是可信代理 → 取最左。
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.7, 127.0.0.1"),
        );
        assert_eq!(resolve_client_ip(&headers, &trusted), "198.51.100.7");
        // 链最左非法 → 取下一个合法。
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("garbage, 198.51.100.8, 127.0.0.1"),
        );
        assert_eq!(resolve_client_ip(&headers, &trusted), "198.51.100.8");
        // 无头 → unknown。
        let empty = HeaderMap::new();
        assert_eq!(resolve_client_ip(&empty, &trusted), "unknown");
    }

    #[test]
    fn resolve_ip_trusts_real_ip_only_when_consistent() {
        let trusted = [std::net::IpAddr::V4(std::net::Ipv4Addr::LOCALHOST)];
        // 链一致 → 信任 x-real-ip。
        let mut h1 = HeaderMap::new();
        h1.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.7, 127.0.0.1"),
        );
        h1.insert("x-real-ip", HeaderValue::from_static("198.51.100.7"));
        assert_eq!(resolve_client_ip(&h1, &trusted), "198.51.100.7");
        // 链不一致 → 忽略 x-real-ip，取链首跳。
        h1.insert("x-real-ip", HeaderValue::from_static("203.0.113.99"));
        assert_eq!(resolve_client_ip(&h1, &trusted), "198.51.100.7");
    }

    #[test]
    fn challenge_token_roundtrip_and_single_use() {
        let engine = AntibotEngine::with_config(test_config());
        let now = 1_700_000_000_000i64;
        let token = engine.issue_challenge("198.51.100.7", Bucket::Search, now);
        assert!(engine.verify_challenge(&token, "198.51.100.7", Bucket::Search, now));
        // 一次性：重放拒绝。
        assert!(!engine.verify_challenge(&token, "198.51.100.7", Bucket::Search, now));
        // 过期拒绝。
        let expired = engine.issue_challenge("198.51.100.7", Bucket::Search, now - 400_000);
        assert!(!engine.verify_challenge(&expired, "198.51.100.7", Bucket::Search, now));
        // 换 IP / 换桶拒绝。
        let token2 = engine.issue_challenge("198.51.100.7", Bucket::Search, now);
        assert!(!engine.verify_challenge(&token2, "198.51.100.8", Bucket::Search, now));
        let token3 = engine.issue_challenge("198.51.100.7", Bucket::Search, now);
        assert!(!engine.verify_challenge(&token3, "198.51.100.7", Bucket::Rss, now));
    }

    #[test]
    fn challenge_failures_trigger_temp_ban() {
        let engine = AntibotEngine::with_config(test_config());
        let now = 1_700_000_000_000i64;
        assert!(engine.check_ban("198.51.100.7", now).is_none());
        let _ = engine.verify_challenge("bogus", "198.51.100.7", Bucket::Search, now);
        assert!(engine.check_ban("198.51.100.7", now).is_none());
        let _ = engine.verify_challenge("bogus", "198.51.100.7", Bucket::Search, now);
        assert!(engine.check_ban("198.51.100.7", now).is_some());
        // 到期解封。
        assert!(engine
            .check_ban("198.51.100.7", now + engine.config().temp_ban_ms + 1)
            .is_none());
    }

    #[test]
    fn temp_ban_records_alert_and_unban_works() {
        let engine = AntibotEngine::with_config(test_config());
        let now = 1_700_000_000_000i64;
        engine.temp_ban("203.0.113.9", "rate_limit", now);
        assert!(engine.check_ban("203.0.113.9", now).is_some());
        let alerts = engine.review_query();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, "temp_ban");
        assert!(engine.unban("203.0.113.9"));
        assert!(engine.check_ban("203.0.113.9", now).is_none());
    }

    #[test]
    fn segment_minimizes_ip() {
        assert_eq!(segment("198.51.100.7"), "198.51.100.0/24");
        assert!(segment("2001:db8::1").contains("2001:db8"));
        assert_eq!(segment("not-an-ip"), "unknown");
    }
}
