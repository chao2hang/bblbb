//! 观测模块（M15-OBSERVE-01/02/03）：结构化 JSON 日志、敏感字段脱敏与指标注册表。
//!
//! 日志输出字段契约（M15-OBSERVE-01）：
//!
//! ```json
//! {"timestamp":"...","service":"bblbb-backend","level":"INFO","target":"...",
//!  "request_id":"...","route":"...","method":"...","message":"...","fields":{...}}
//! ```
//!
//! - `timestamp`：RFC3339 毫秒精度本地时间；
//! - `service`：固定 `bblbb-backend`；
//! - `level`/`target`：tracing 元数据；
//! - `request_id`/`route`/`method`：来自 HTTP span（`app.rs` 的 TraceLayer 注入，
//!   经 `on_new_span`/`on_record` 捕获进 span extensions）；
//! - 其余事件字段进入 `fields`。
//!
//! 脱敏契约（M15-OBSERVE-02/03）：
//!
//! - 敏感字段名（Cookie、Authorization、OAuth code/token、密码、完整邮箱、
//!   隐藏正文、Prompt、签名 URL 等）整体替换为 `[REDACTED]`；
//! - 值级规则：Bearer 头、JWT、私钥块、长 hex/base64、完整邮箱等常见泄密
//!   形态即使出现在非敏感字段名中也会被掩码；
//! - 应用层已通过 `redact_dsn`（连接串）与 `sanitize_log`（邮件）进一步降噪。

pub mod metrics;

use std::io::{self, Write};
use std::sync::Mutex;
use std::time::SystemTime;

use serde_json::json;
use tracing::field::{Field, Visit};
use tracing::span;
use tracing::Subscriber;
use tracing_subscriber::layer::Context;
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::Layer;

/// 日志输出格式（`BBLBB__LOG_FORMAT`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// 人类可读文本（开发/本地）。
    Text,
    /// 结构化 JSON（生产；每行一个事件）。
    Json,
}

impl LogFormat {
    /// 解析 `BBLBB__LOG_FORMAT` 值；未知值按 `Json` 处理并告警。
    pub fn parse(value: &str) -> LogFormat {
        match value.trim().to_ascii_lowercase().as_str() {
            "text" => LogFormat::Text,
            "json" => LogFormat::Json,
            other => {
                tracing::warn!(
                    format = %other,
                    "unknown BBLBB__LOG_FORMAT value; defaulting to json"
                );
                LogFormat::Json
            }
        }
    }
}

/// 初始化全局 tracing：文本或 JSON 输出 + `EnvFilter`。
pub fn init(filter: &str, format: LogFormat) {
    let env_filter = EnvFilter::new(filter);
    match format {
        LogFormat::Text => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(env_filter)
                .with_target(true)
                .try_init();
        }
        LogFormat::Json => {
            let _ = tracing_subscriber::registry()
                .with(env_filter)
                .with(JsonLogLayer::new())
                .try_init();
        }
    }
}

/// 敏感字段名（M15-OBSERVE-02）：命中即整体替换为 `[REDACTED]`。
const SENSITIVE_FIELD_NAMES: &[&str] = &[
    "cookie",
    "set_cookie",
    "authorization",
    "oauth_code",
    "oauth_token",
    "access_token",
    "refresh_token",
    "id_token",
    "password",
    "password_hash",
    "token",
    "token_hash",
    "csrf_token",
    "session_token",
    "secret",
    "private_key",
    "ciphertext",
    "webhook_secret",
    "mfa_encryption_key",
    "oidc_key_encryption_key",
    "marketplace_webhook_encryption_key",
    "prompt",
    "signed_url",
    "signature",
    "email",
    "body",
    "request_body",
    "hidden_body",
    "dsn",
    "database_url",
    "s3_access_key_id",
    "s3_secret_access_key",
];

/// 字段名是否敏感（精确匹配或包含敏感子串）。
fn is_sensitive_field_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    SENSITIVE_FIELD_NAMES
        .iter()
        .any(|marker| lower.contains(marker))
}

/// 值级脱敏（M15-OBSERVE-03）：字段名不敏感但值形态泄密时仍掩码。
fn mask_sensitive_value(name: &str, value: &str) -> String {
    if is_sensitive_field_name(name) {
        return "[REDACTED]".to_owned();
    }
    if value_contains_secret(value) {
        return "[REDACTED]".to_owned();
    }
    value.to_owned()
}

/// 判断字符串是否包含常见泄密形态：
/// - 完整邮箱（`local@domain.tld`）；
/// - `Bearer <token>`；
/// - JWT（`eyJ…` 三段式）；
/// - 私钥块（`-----BEGIN …PRIVATE KEY-----`）；
/// - `password=`/`secret=`/`token=` 键值对；
/// - 长连续 hex/base64（≥48 字符，UUID 等短 id 不命中）。
fn value_contains_secret(value: &str) -> bool {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();

    if lower.starts_with("-----begin") || lower.contains("private key-----") {
        return true;
    }
    if lower.contains("bearer ") {
        return true;
    }
    // JWT 三段式
    if lower.starts_with("eyj") && trimmed.matches('.').count() >= 2 && trimmed.len() >= 40 {
        return true;
    }
    // 完整邮箱：按空白切分，剥离尾部标点后逐 token 判定
    for token in trimmed.split_whitespace() {
        let cleaned = token.trim_matches(|c: char| {
            matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '>')
        });
        if is_email(cleaned) {
            return true;
        }
    }
    // 键值对泄漏：password=/secret=/token= 后跟非占位内容
    for marker in ["password=", "secret=", "token=", "apikey=", "api_key="] {
        if lower.contains(marker) {
            let after = &trimmed[lower.find(marker).unwrap() + marker.len()..];
            if !after.is_empty() && !after.starts_with("***") && !after.starts_with("[redacted]") {
                return true;
            }
        }
    }
    // 长连续 hex/base64（≥48 字符、无分隔符）：UUID 等短 id 不命中
    let alnum_len = trimmed
        .chars()
        .filter(|c| c.is_ascii_hexdigit() || "+/=".contains(*c))
        .count();
    if trimmed.len() >= 48 && alnum_len >= 48 {
        return true;
    }
    false
}

/// `local@domain.tld` 形态判定（domain 必须含点）。
fn is_email(candidate: &str) -> bool {
    let Some(at) = candidate.rfind('@') else {
        return false;
    };
    if at == 0 {
        return false;
    }
    let (local, domain) = candidate.split_at(at);
    let domain = &domain[1..];
    let local_ok = local
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "._%+-".contains(c));
    let domain_ok = domain.contains('.')
        && domain
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-".contains(c));
    local_ok && domain_ok
}

/// 事件字段收集器（自动脱敏）。
#[derive(Default)]
struct RedactingVisitor {
    fields: Vec<(String, serde_json::Value)>,
}

impl RedactingVisitor {
    fn record_value(&mut self, field: &Field, value: &str) {
        let masked = mask_sensitive_value(field.name(), value);
        self.fields.push((field.name().to_owned(), json!(masked)));
    }
}

impl Visit for RedactingVisitor {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.record_value(field, value);
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields.push((field.name().to_owned(), json!(value)));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields.push((field.name().to_owned(), json!(value)));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields.push((field.name().to_owned(), json!(value)));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields.push((field.name().to_owned(), json!(value)));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        let rendered = format!("{value:?}");
        self.record_value(field, &rendered);
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        let rendered = value.to_string();
        self.record_value(field, &rendered);
    }
}

/// span 字段值收集器（仅记录名称 + 字符串渲染，供 request_id/route/method）。
struct SpanFieldCapture(Vec<(String, String)>);

impl Visit for SpanFieldCapture {
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.push((field.name().to_owned(), value.to_owned()));
    }
    fn record_bool(&mut self, field: &Field, value: bool) {
        self.0.push((field.name().to_owned(), value.to_string()));
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.push((field.name().to_owned(), value.to_string()));
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.push((field.name().to_owned(), value.to_string()));
    }
    fn record_f64(&mut self, field: &Field, value: f64) {
        self.0.push((field.name().to_owned(), value.to_string()));
    }
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0.push((field.name().to_owned(), format!("{value:?}")));
    }
    fn record_error(&mut self, field: &Field, value: &(dyn std::error::Error + 'static)) {
        self.0.push((field.name().to_owned(), value.to_string()));
    }
}

/// 结构化 JSON 日志层（M15-OBSERVE-01）。
///
/// 每个事件输出一行 JSON；字段经 [`mask_sensitive_value`] 脱敏；span 字段在
/// `on_new_span`/`on_record` 时捕获进 span extensions，事件输出时从当前
/// span 作用域提取 `request_id`/`route`/`method`。
pub struct JsonLogLayer {
    writer: Mutex<Box<dyn Write + Send>>,
    service: String,
}

impl JsonLogLayer {
    pub fn new() -> Self {
        Self {
            writer: Mutex::new(Box::new(io::stdout())),
            service: "bblbb-backend".to_owned(),
        }
    }

    /// 注入自定义 writer（测试用）。
    pub fn with_writer(writer: Box<dyn Write + Send>) -> Self {
        Self {
            writer: Mutex::new(writer),
            service: "bblbb-backend".to_owned(),
        }
    }
}

impl Default for JsonLogLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Layer<S> for JsonLogLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let mut capture = SpanFieldCapture(Vec::new());
        attrs.record(&mut capture);
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(capture);
        }
    }

    fn on_record(&self, id: &span::Id, values: &span::Record<'_>, ctx: Context<'_, S>) {
        if let Some(span) = ctx.span(id) {
            let mut capture = SpanFieldCapture(Vec::new());
            values.record(&mut capture);
            if let Some(fields) = span.extensions_mut().get_mut::<SpanFieldCapture>() {
                fields.0.extend(capture.0);
            } else {
                span.extensions_mut().insert(capture);
            }
        }
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let mut visitor = RedactingVisitor::default();
        event.record(&mut visitor);

        let mut fields = serde_json::Map::new();
        let mut message = serde_json::Value::Null;
        for (name, value) in visitor.fields {
            if name == "message" {
                message = value;
            } else {
                fields.insert(name, value);
            }
        }

        let mut request_id = "unknown".to_owned();
        let mut route = String::new();
        let mut method = String::new();
        if let Some(span) = ctx.event_span(event) {
            for ancestor in span.scope().from_root() {
                if let Some(captured) = ancestor.extensions().get::<SpanFieldCapture>() {
                    for (name, value) in &captured.0 {
                        match name.as_str() {
                            "request_id" if request_id == "unknown" => request_id = value.clone(),
                            "route" if route.is_empty() => route = value.clone(),
                            "method" if method.is_empty() => method = value.clone(),
                            _ => {}
                        }
                    }
                }
            }
        }

        let timestamp = rfc3339_millis();
        let level = event.metadata().level().as_str().to_owned();
        let target = event.metadata().target();

        let record = json!({
            "timestamp": timestamp,
            "service": self.service,
            "level": level,
            "target": target,
            "request_id": request_id,
            "route": route,
            "method": method,
            "message": message,
            "fields": fields,
        });

        if let Ok(mut writer) = self.writer.lock() {
            let line = format!("{record}\n");
            let _ = writer.write_all(line.as_bytes());
            let _ = writer.flush();
        }
    }
}

/// RFC3339 毫秒时间戳（本地时间，无时区后缀——部署统一 UTC，见 SCHEMA §2.2）。
fn rfc3339_millis() -> String {
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs() as i64;
    let millis = duration.subsec_millis();
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let (hour, min, sec) = (
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60,
    );
    let (year, month, day) = civil_from_days(days);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{min:02}:{sec:02}.{millis:03}Z")
}

/// 天数 → 公历日期（Howard Hinnant 的 civil_from_days 算法）。
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sensitive_field_names_are_redacted() {
        for name in [
            "cookie",
            "authorization",
            "oauth_code",
            "access_token",
            "password",
            "token",
            "csrf_token",
            "webhook_secret",
            "email",
            "signed_url",
            "body",
            "prompt",
        ] {
            assert!(is_sensitive_field_name(name), "{name} 应被识别为敏感字段名");
            assert_eq!(mask_sensitive_value(name, "anything"), "[REDACTED]");
        }
    }

    #[test]
    fn full_email_is_redacted_even_in_innocent_field() {
        assert_eq!(
            mask_sensitive_value("note", "user@example.com"),
            "[REDACTED]"
        );
        assert_eq!(
            mask_sensitive_value("message", "contact bob@example.org please"),
            "[REDACTED]"
        );
    }

    #[test]
    fn bearer_jwt_and_private_key_are_redacted() {
        assert_eq!(
            mask_sensitive_value("header", "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig"),
            "[REDACTED]"
        );
        assert_eq!(
            mask_sensitive_value("detail", "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAA"),
            "[REDACTED]"
        );
        assert!(
            value_contains_secret("eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.eyJzdWIiOiIxIn0.x"),
            "JWT 形态应被识别"
        );
    }

    #[test]
    fn long_hex_and_base64_are_redacted() {
        let hex64 = "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4";
        assert!(value_contains_secret(hex64), "长 hex 应被识别");
        assert_eq!(mask_sensitive_value("data", hex64), "[REDACTED]");
    }

    #[test]
    fn key_value_leaks_are_redacted() {
        assert!(value_contains_secret("password=hunter2"));
        assert!(value_contains_secret("token=abc123def456"));
    }

    #[test]
    fn benign_values_pass_through() {
        assert_eq!(mask_sensitive_value("job_id", "j-01911fd5"), "j-01911fd5");
        assert_eq!(
            mask_sensitive_value("route", "/api/v1/posts"),
            "/api/v1/posts"
        );
        assert_eq!(
            mask_sensitive_value("user_id", "01911fd5-0047-0000-0000-000000000001"),
            "01911fd5-0047-0000-0000-000000000001"
        );
        assert!(!value_contains_secret("2026-08-07T10:00:00.000Z"));
        assert!(!value_contains_secret("applied 4 migration(s)"));
    }

    #[test]
    fn json_layer_emits_service_and_redacts_fields() {
        use tracing::{info, span, Level};

        let shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let sink = shared.clone();
        let layer = JsonLogLayer::with_writer(Box::new(TestWriter::new(sink)));

        let dispatcher =
            tracing::dispatcher::Dispatch::new(tracing_subscriber::registry().with(layer));
        tracing::dispatcher::with_default(&dispatcher, || {
            let _span = span!(
                Level::INFO,
                "http_request",
                request_id = "req-123",
                route = "/healthz"
            )
            .entered();
            info!(message = "hello world", status = 200);
        });

        let content = String::from_utf8(shared.lock().unwrap().clone()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content.lines().next().unwrap())
            .expect("JSON 日志必须是合法 JSON");
        assert_eq!(parsed["service"], "bblbb-backend");
        assert_eq!(parsed["level"], "INFO");
        assert_eq!(parsed["request_id"], "req-123");
        assert_eq!(parsed["route"], "/healthz");
        assert_eq!(parsed["message"], "hello world");
        assert_eq!(parsed["fields"]["status"], 200);
        assert!(parsed["timestamp"].as_str().unwrap().len() >= 23);
    }

    #[test]
    fn json_layer_redacts_sensitive_event_field() {
        use tracing::warn;

        let shared = std::sync::Arc::new(std::sync::Mutex::new(Vec::<u8>::new()));
        let sink = shared.clone();
        let layer = JsonLogLayer::with_writer(Box::new(TestWriter::new(sink)));

        let dispatcher =
            tracing::dispatcher::Dispatch::new(tracing_subscriber::registry().with(layer));
        tracing::dispatcher::with_default(&dispatcher, || {
            warn!(
                message = "auth failed",
                password = "hunter2",
                email = "bob@example.com"
            );
        });

        let content = String::from_utf8(shared.lock().unwrap().clone()).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(content.lines().next().unwrap())
            .expect("JSON 日志必须是合法 JSON");
        assert_eq!(parsed["fields"]["password"], "[REDACTED]");
        assert_eq!(parsed["fields"]["email"], "[REDACTED]");
        assert!(!content.contains("hunter2"), "日志不得包含明文密码");
        assert!(!content.contains("bob@example.com"), "日志不得包含完整邮箱");
    }

    #[test]
    fn civil_from_days_round_trips_epoch() {
        // 1970-01-01
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        // 2026-08-07：Unix 秒 1786060800 / 86400 = 20672 天
        assert_eq!(civil_from_days(20_672), (2026, 8, 7));
    }

    /// 测试辅助 writer：把字节写入共享缓冲区。
    struct TestWriter {
        sink: std::sync::Arc<std::sync::Mutex<Vec<u8>>>,
    }

    impl TestWriter {
        fn new(sink: std::sync::Arc<std::sync::Mutex<Vec<u8>>>) -> Self {
            Self { sink }
        }
    }

    impl Write for TestWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.sink.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }
}
