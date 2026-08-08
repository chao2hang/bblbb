//! M15-OBSERVE-01/03：结构化 JSON 日志输出与脱敏集成测试。
//!
//! - JSON 事件字段契约：timestamp / service / level / request_id / route / message；
//! - 敏感字段（password/email/token 等）在日志层整体替换为 `[REDACTED]`；
//! - `BBLBB__LOG_FORMAT` 解析：`text` / `json` / 未知值降级 json。

use std::io::Write;
use std::sync::{Arc, Mutex};

use bblbb_backend::observability::{JsonLogLayer, LogFormat};
use tracing_subscriber::prelude::*;

/// 捕获日志输出的 writer。
#[derive(Clone)]
struct CaptureWriter {
    sink: Arc<Mutex<Vec<u8>>>,
}

impl Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.sink.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[test]
fn log_format_parse_accepts_text_and_json() {
    assert_eq!(LogFormat::parse("text"), LogFormat::Text);
    assert_eq!(LogFormat::parse("JSON"), LogFormat::Json);
    assert_eq!(LogFormat::parse(" json "), LogFormat::Json);
    // 未知值降级 json（生产安全默认）
    assert_eq!(LogFormat::parse("bogus"), LogFormat::Json);
}

#[test]
fn json_layer_emits_structured_fields_with_request_context() {
    use tracing::{info, span, Level};

    let capture = CaptureWriter {
        sink: Arc::new(Mutex::new(Vec::new())),
    };
    let layer = JsonLogLayer::with_writer(Box::new(capture.clone()));
    let dispatcher = tracing::dispatcher::Dispatch::new(tracing_subscriber::registry().with(layer));

    tracing::dispatcher::with_default(&dispatcher, || {
        let _span = span!(
            Level::INFO,
            "http_request",
            method = "GET",
            route = "/api/v1/posts",
            request_id = "req-abc",
        )
        .entered();
        info!(message = "post listed", status = 200);
    });

    let output = String::from_utf8(capture.sink.lock().unwrap().clone()).unwrap();
    let line = output.lines().next().expect("至少一行 JSON");
    let parsed: serde_json::Value = serde_json::from_str(line).unwrap();

    assert_eq!(parsed["service"], "bblbb-backend");
    assert_eq!(parsed["level"], "INFO");
    assert_eq!(parsed["request_id"], "req-abc");
    assert_eq!(parsed["route"], "/api/v1/posts");
    assert_eq!(parsed["method"], "GET");
    assert_eq!(parsed["message"], "post listed");
    assert_eq!(parsed["fields"]["status"], 200);
    assert!(
        parsed["timestamp"].as_str().unwrap().len() >= 19,
        "timestamp 必须为 RFC3339"
    );
}

#[test]
fn json_layer_redacts_forbidden_values() {
    use tracing::warn;

    let capture = CaptureWriter {
        sink: Arc::new(Mutex::new(Vec::new())),
    };
    let layer = JsonLogLayer::with_writer(Box::new(capture.clone()));
    let dispatcher = tracing::dispatcher::Dispatch::new(tracing_subscriber::registry().with(layer));

    tracing::dispatcher::with_default(&dispatcher, || {
        warn!(
            message = "login attempt",
            email = "victim@example.com",
            authorization = "Bearer eyJhbGciOiJIUzI1NiJ9.eyJzdWIiOiIxIn0.sig",
            password = "hunter2",
            route = "/api/v1/auth/login",
        );
    });

    let output = String::from_utf8(capture.sink.lock().unwrap().clone()).unwrap();
    assert!(!output.contains("victim@example.com"), "不得出现完整邮箱");
    assert!(!output.contains("hunter2"), "不得出现密码");
    assert!(
        !output.contains("eyJhbGciOiJIUzI1NiJ9"),
        "不得出现 JWT/Bearer"
    );
    let parsed: serde_json::Value = serde_json::from_str(output.lines().next().unwrap()).unwrap();
    assert_eq!(parsed["fields"]["email"], "[REDACTED]");
    assert_eq!(parsed["fields"]["authorization"], "[REDACTED]");
    assert_eq!(parsed["fields"]["password"], "[REDACTED]");
}
