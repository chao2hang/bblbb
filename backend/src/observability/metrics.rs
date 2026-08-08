//! 进程内指标注册表与 Prometheus 文本输出（M15-OBSERVE-04/05）。
//!
//! - 全局单例 [`registry()`]：原子计数 + HTTP 延迟对数桶直方图
//!   （由桶上界近似 p50/p95/p99）；
//! - 指标名白名单 [`METRIC_HELP`]：未登记的名字直接 panic（防止拼写漂移）；
//! - DB 派生 gauge（连接池、队列深度、Outbox 堆积）由 `/metrics` handler
//!   在抓取时计算并 [`set_gauge`](MetricsRegistry::set_gauge)。
//!
//! 输出为 Prometheus 文本格式（`text/plain; version=0.0.4`）。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

/// HTTP 延迟对数桶上界（毫秒）。
pub const LATENCY_BUCKETS_MS: [u64; 12] = [
    5, 10, 25, 50, 100, 250, 500, 1_000, 2_500, 5_000, 10_000, 30_000,
];

/// 指标目录（name, help, type）。所有指标必须先在此登记。
pub const METRIC_HELP: &[(&str, &str, &str)] = &[
    // M15-OBSERVE-04：HTTP 与数据库基础设施
    ("bblbb_http_requests_total", "HTTP 请求总数", "counter"),
    ("bblbb_http_errors_total", "HTTP 5xx 响应数", "counter"),
    ("bblbb_http_429_total", "HTTP 429（限流）请求数", "counter"),
    (
        "bblbb_http_request_duration_seconds",
        "HTTP 请求耗时（p50/p95/p99/sum/count）",
        "summary",
    ),
    (
        "bblbb_db_connect_failures_total",
        "数据库连接池创建失败次数",
        "counter",
    ),
    (
        "bblbb_db_pool_size",
        "当前数据库连接池连接数（gauge，抓取时计算）",
        "gauge",
    ),
    (
        "bblbb_db_pool_idle",
        "当前数据库连接池空闲连接数（gauge，抓取时计算）",
        "gauge",
    ),
    (
        "bblbb_db_pool_max",
        "数据库连接池最大连接数（gauge，抓取时计算）",
        "gauge",
    ),
    (
        "bblbb_sqlite_busy_total",
        "SQLite busy/locked 指数退避累计次数",
        "counter",
    ),
    // M15-OBSERVE-05：身份/会话/经济/存储/任务领域指标
    (
        "bblbb_session_login_failures_total",
        "登录失败次数",
        "counter",
    ),
    ("bblbb_session_lockouts_total", "账号锁定次数", "counter"),
    (
        "bblbb_csrf_rejections_total",
        "CSRF 校验拒绝次数",
        "counter",
    ),
    ("bblbb_totp_failures_total", "TOTP 验证失败次数", "counter"),
    (
        "bblbb_oidc_token_errors_total",
        "OIDC Token 签发/校验错误次数",
        "counter",
    ),
    ("bblbb_uploads_failed_total", "附件上传失败次数", "counter"),
    (
        "bblbb_storage_errors_total",
        "存储适配器错误次数",
        "counter",
    ),
    ("bblbb_ledger_errors_total", "账本服务错误次数", "counter"),
    (
        "bblbb_jobs_dead_total",
        "任务 dead-letter 累计次数",
        "counter",
    ),
    (
        "bblbb_jobs_queued",
        "队列待处理任务数（gauge，抓取时计算）",
        "gauge",
    ),
    (
        "bblbb_jobs_running",
        "队列 running 任务数（gauge，抓取时计算）",
        "gauge",
    ),
    (
        "bblbb_jobs_dead",
        "队列 dead 任务数（gauge，抓取时计算）",
        "gauge",
    ),
    (
        "bblbb_outbox_pending",
        "Outbox 待处理事件数（gauge，抓取时计算）",
        "gauge",
    ),
    (
        "bblbb_outbox_failed",
        "Outbox 最终失败事件数（gauge，抓取时计算）",
        "gauge",
    ),
];

/// 全局注册表。
pub fn registry() -> &'static MetricsRegistry {
    static REGISTRY: OnceLock<MetricsRegistry> = OnceLock::new();
    REGISTRY.get_or_init(MetricsRegistry::new)
}

/// 进程内指标注册表。
pub struct MetricsRegistry {
    counters: Mutex<BTreeMap<String, i64>>,
    gauges: Mutex<BTreeMap<String, i64>>,
    latency: Mutex<[u64; LATENCY_BUCKETS_MS.len()]>,
    latency_count: AtomicI64,
    latency_sum_ms: AtomicI64,
}

impl MetricsRegistry {
    fn new() -> Self {
        Self {
            counters: Mutex::new(BTreeMap::new()),
            gauges: Mutex::new(BTreeMap::new()),
            latency: Mutex::new([0; LATENCY_BUCKETS_MS.len()]),
            latency_count: AtomicI64::new(0),
            latency_sum_ms: AtomicI64::new(0),
        }
    }

    /// 计数器 +delta；指标必须已在 [`METRIC_HELP`] 登记。
    pub fn counter_inc(&self, name: &str, delta: i64) {
        assert!(
            METRIC_HELP
                .iter()
                .any(|(n, _, t)| *n == name && *t == "counter"),
            "unknown counter metric: {name}"
        );
        let mut counters = self.counters.lock().unwrap();
        let entry = counters.entry(name.to_owned()).or_insert(0);
        *entry = entry.saturating_add(delta);
    }

    /// 读取计数器当前值。
    pub fn counter_get(&self, name: &str) -> i64 {
        self.counters
            .lock()
            .unwrap()
            .get(name)
            .copied()
            .unwrap_or(0)
    }

    /// 设置 gauge；指标必须已在 [`METRIC_HELP`] 登记。
    pub fn set_gauge(&self, name: &str, value: i64) {
        assert!(
            METRIC_HELP
                .iter()
                .any(|(n, _, t)| *n == name && *t == "gauge"),
            "unknown gauge metric: {name}"
        );
        self.gauges.lock().unwrap().insert(name.to_owned(), value);
    }

    /// 记录一次 HTTP 耗时（毫秒）进对数桶。
    pub fn observe_latency_ms(&self, ms: u64) {
        let mut buckets = self.latency.lock().unwrap();
        let idx = LATENCY_BUCKETS_MS
            .iter()
            .position(|upper| ms <= *upper)
            .unwrap_or(LATENCY_BUCKETS_MS.len() - 1);
        buckets[idx] = buckets[idx].saturating_add(1);
        drop(buckets);
        self.latency_count.fetch_add(1, Ordering::Relaxed);
        self.latency_sum_ms.fetch_add(ms as i64, Ordering::Relaxed);
    }

    /// 由桶分布近似分位数（毫秒）：p50 / p95 / p99。
    pub fn latency_quantiles_ms(&self) -> (u64, u64, u64) {
        let buckets = self.latency.lock().unwrap();
        let total: u64 = buckets.iter().sum();
        if total == 0 {
            return (0, 0, 0);
        }
        let quantile = |q: f64| -> u64 {
            let target = (total as f64 * q).ceil() as u64;
            let mut cumulative = 0u64;
            for (idx, count) in buckets.iter().enumerate() {
                cumulative += count;
                if cumulative >= target {
                    return LATENCY_BUCKETS_MS[idx];
                }
            }
            LATENCY_BUCKETS_MS[LATENCY_BUCKETS_MS.len() - 1]
        };
        (quantile(0.50), quantile(0.95), quantile(0.99))
    }

    /// Prometheus 文本输出。
    pub fn render_prometheus(&self) -> String {
        let mut out = String::new();
        let counters = self.counters.lock().unwrap();
        let gauges = self.gauges.lock().unwrap();
        let (p50, p95, p99) = self.latency_quantiles_ms();
        let count = self.latency_count.load(Ordering::Relaxed);
        let sum_ms = self.latency_sum_ms.load(Ordering::Relaxed);

        for (name, help, kind) in METRIC_HELP {
            out.push_str(&format!("# HELP {name} {help}\n"));
            out.push_str(&format!("# TYPE {name} {kind}\n"));
            match *kind {
                "counter" => {
                    let value = counters.get(*name).copied().unwrap_or(0);
                    out.push_str(&format!("{name} {value}\n"));
                }
                "gauge" => {
                    let value = gauges.get(*name).copied().unwrap_or(0);
                    out.push_str(&format!("{name} {value}\n"));
                }
                "summary" => {
                    out.push_str(&format!(
                        "{name}{{quantile=\"0.5\"}} {}\n",
                        p50 as f64 / 1000.0
                    ));
                    out.push_str(&format!(
                        "{name}{{quantile=\"0.95\"}} {}\n",
                        p95 as f64 / 1000.0
                    ));
                    out.push_str(&format!(
                        "{name}{{quantile=\"0.99\"}} {}\n",
                        p99 as f64 / 1000.0
                    ));
                    out.push_str(&format!("{name}_sum {}\n", sum_ms as f64 / 1000.0));
                    out.push_str(&format!("{name}_count {count}\n"));
                }
                _ => {}
            }
        }
        out
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counters_inc_and_read() {
        let reg = MetricsRegistry::new();
        assert_eq!(reg.counter_get("bblbb_csrf_rejections_total"), 0);
        reg.counter_inc("bblbb_csrf_rejections_total", 1);
        reg.counter_inc("bblbb_csrf_rejections_total", 2);
        assert_eq!(reg.counter_get("bblbb_csrf_rejections_total"), 3);
    }

    #[test]
    fn unknown_counter_panics() {
        let reg = MetricsRegistry::new();
        let result = std::panic::catch_unwind(|| {
            reg.counter_inc("bblbb_not_registered", 1);
        });
        assert!(result.is_err(), "未登记指标必须 panic 防止拼写漂移");
    }

    #[test]
    fn latency_quantiles_from_buckets() {
        let reg = MetricsRegistry::new();
        for _ in 0..100 {
            reg.observe_latency_ms(10); // 全部落在 10ms 桶
        }
        let (p50, p95, p99) = reg.latency_quantiles_ms();
        assert_eq!(p50, 10);
        assert_eq!(p95, 10);
        assert_eq!(p99, 10);
    }

    #[test]
    fn empty_registry_quantiles_are_zero() {
        let reg = MetricsRegistry::new();
        assert_eq!(reg.latency_quantiles_ms(), (0, 0, 0));
    }

    #[test]
    fn render_contains_help_and_quantiles() {
        let reg = MetricsRegistry::new();
        reg.observe_latency_ms(5);
        reg.counter_inc("bblbb_csrf_rejections_total", 1);
        reg.set_gauge("bblbb_db_pool_max", 8);
        let text = reg.render_prometheus();
        assert!(text.contains("# TYPE bblbb_http_requests_total counter"));
        assert!(text.contains("bblbb_http_request_duration_seconds{quantile=\"0.5\"}"));
        assert!(text.contains("bblbb_db_pool_max 8"));
        assert!(text.contains("bblbb_csrf_rejections_total 1"));
    }
}
