//! 进程内固定窗口限流器（M02-IDENTITY-06）。
//!
//! 策略依据（docs/SECURITY.md §16）：
//! - 单机使用进程内限流 + 数据库账号锁定；多实例再引入 Redis；
//! - 代理 IP 只信任 Caddy 注入且来自 loopback/配置的可信代理；
//! - 429 返回 `Retry-After`（OpenAPI `RateLimited` 响应，错误码 `rate_limited`），
//!   并按 docs/API.md §17 附 `RateLimit-Limit/Remaining/Reset` 头。
//!
//! 实现为固定窗口（fixed window）：`now_ms - now_ms % window_ms` 确定窗口起点，
//! 窗口内计数达上限即拒绝。注册场景低频（每小时 3 次），窗口边界突发可接受，
//! 换取 O(1) 与无后台清理线程（条目随窗口滚动自然重置；上限由 key 空间决定）。

use std::collections::HashMap;
use std::sync::Mutex;

use axum::http::HeaderMap;

/// 注册限流：每 IP 3 次 / 小时（docs/SECURITY.md §16 建议初始值）。
pub const REGISTER_IP_LIMIT: u32 = 3;
/// 注册限流：每账号（规范化邮箱）3 次 / 小时。
pub const REGISTER_ACCOUNT_LIMIT: u32 = 3;
/// 注册限流窗口：1 小时（Unix 毫秒）。
pub const REGISTER_WINDOW_MS: i64 = 60 * 60 * 1000;

/// 线程安全限流器（进程内）。`AppState` 持有 `Arc<RateLimiter>` 全进程共享。
#[derive(Debug, Default)]
pub struct RateLimiter {
    inner: Mutex<HashMap<String, Window>>,
}

#[derive(Debug, Clone, Copy)]
struct Window {
    window_start: i64,
    count: u32,
}

/// 一次限流检查的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateLimitStatus {
    /// 是否放行（未超限）。
    pub allowed: bool,
    /// 窗口额度。
    pub limit: u32,
    /// 本窗口剩余额度。
    pub remaining: u32,
    /// 窗口重置的 Unix 毫秒时间戳。
    pub reset_at_ms: i64,
    /// 需要等待的秒数（`allowed == false` 时供 `Retry-After` 使用；≥ 1）。
    pub retry_after_secs: u64,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(HashMap::new()),
        }
    }

    /// 检查并消费 `key` 的窗口额度：`limit` 次 / `window_ms` 毫秒。
    ///
    /// 放行时计数 +1；拒绝时计数不变（后续请求仍会被拒绝直到窗口滚动）。
    /// 锁内仅做常数级哈希表操作，无任何 await，可安全在 async 上下文调用。
    pub fn check(&self, key: &str, limit: u32, window_ms: i64, now_ms: i64) -> RateLimitStatus {
        let window_ms = window_ms.max(1);
        let limit = limit.max(1);
        let window_start = now_ms - now_ms.rem_euclid(window_ms);

        let mut inner = self.inner.lock().expect("ratelimit mutex poisoned");
        let entry = inner.entry(key.to_string()).or_insert(Window {
            window_start,
            count: 0,
        });
        if entry.window_start != window_start {
            entry.window_start = window_start;
            entry.count = 0;
        }

        let allowed = entry.count < limit;
        if allowed {
            entry.count += 1;
        }

        let reset_at_ms = window_start + window_ms;
        let remaining = limit.saturating_sub(entry.count);
        let retry_after_secs = if allowed {
            0
        } else {
            // ceil(delta/1000)，避免依赖 unstable int_roundings
            let delta = (reset_at_ms - now_ms).max(0);
            (delta.div_euclid(1000) + if delta.rem_euclid(1000) != 0 { 1 } else { 0 }).max(1) as u64
        };

        RateLimitStatus {
            allowed,
            limit,
            remaining,
            reset_at_ms,
            retry_after_secs,
        }
    }
}

/// 从请求头提取客户端 IP（M02-IDENTITY-06）。
///
/// 只信任反向代理（Caddy）注入的 `x-real-ip` / `x-forwarded-for` 首跳；
/// 均缺失时回退为 `"unknown"`（所有未知来源共享同一桶，天然限流）。
/// 生产按 docs/SECURITY.md §16：代理注入必须来自 loopback/可信代理。
pub fn client_ip(headers: &HeaderMap) -> String {
    if let Some(real) = headers
        .get("x-real-ip")
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        return real.to_owned();
    }
    if let Some(fwd) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(first) = fwd
            .split(',')
            .next()
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            return first.to_owned();
        }
    }
    "unknown".to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::HeaderValue;

    #[test]
    fn allows_within_limit_and_rejects_after() {
        let limiter = RateLimiter::new();
        let t0 = 1_700_000_000_000i64;
        for i in 0..3 {
            let status = limiter.check("register:ip:1.2.3.4", 3, 60_000, t0 + i * 100);
            assert!(status.allowed, "第 {} 次应放行", i + 1);
            assert_eq!(status.remaining, 3 - (i as u32) - 1);
        }
        let rejected = limiter.check("register:ip:1.2.3.4", 3, 60_000, t0 + 300);
        assert!(!rejected.allowed);
        assert_eq!(rejected.remaining, 0);
        assert!(rejected.retry_after_secs >= 1);
    }

    #[test]
    fn window_rolls_over_and_resets_count() {
        let limiter = RateLimiter::new();
        let window_ms = 60_000;
        let t0 = 1_700_000_000_000i64;
        for i in 0..3 {
            assert!(limiter.check("k", 3, window_ms, t0 + i).allowed);
        }
        assert!(!limiter.check("k", 3, window_ms, t0 + 10_000).allowed);

        // 下一个窗口起点 → 计数重置
        let next = t0 + window_ms;
        assert!(limiter.check("k", 3, window_ms, next).allowed);
        assert_eq!(limiter.check("k", 3, window_ms, next + 1).remaining, 1);
    }

    #[test]
    fn keys_are_isolated() {
        let limiter = RateLimiter::new();
        let t0 = 1_700_000_000_000i64;
        assert!(limiter.check("ip:a", 1, 60_000, t0).allowed);
        assert!(!limiter.check("ip:a", 1, 60_000, t0 + 1).allowed);
        assert!(
            limiter.check("ip:b", 1, 60_000, t0).allowed,
            "不同 key 独立"
        );
        assert!(limiter.check("account:x", 3, 60_000, t0).allowed);
    }

    #[test]
    fn reset_and_retry_after_are_reported() {
        let limiter = RateLimiter::new();
        // 对齐到窗口边界，保证 t0..t0+59s 属于同一窗口
        let t0 = (1_700_000_000_000i64 / 60_000) * 60_000;
        let window_ms = 60_000;
        for _ in 0..2 {
            limiter.check("k", 2, window_ms, t0);
        }
        let status = limiter.check("k", 2, window_ms, t0 + 59_000);
        assert!(!status.allowed);
        // 距窗口重置还有 1 秒（ceil），retry_after 至少 1
        assert_eq!(status.reset_at_ms, t0 + window_ms);
        assert_eq!(status.retry_after_secs, 1);
    }

    #[test]
    fn client_ip_trusts_real_ip_then_forwarded_for() {
        let mut headers = HeaderMap::new();
        assert_eq!(client_ip(&headers), "unknown");

        headers.insert("x-real-ip", HeaderValue::from_static("203.0.113.9"));
        assert_eq!(client_ip(&headers), "203.0.113.9");

        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.7, 10.0.0.1"),
        );
        // x-real-ip 优先
        assert_eq!(client_ip(&headers), "203.0.113.9");
    }

    #[test]
    fn client_ip_falls_back_to_forwarded_for_first_hop() {
        let mut headers = HeaderMap::new();
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_static("198.51.100.7, 10.0.0.1, 10.0.0.2"),
        );
        assert_eq!(client_ip(&headers), "198.51.100.7");
    }
}
