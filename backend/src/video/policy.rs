//! `video_provider_policies` 行模型与限制校验（M10-VIDEO-04/12）。
//!
//! 管理员修改立即影响新解析与新渲染；历史引用经
//! [`crate::video::state::recheck_references`] 重新检查后决定继续嵌入或降级
//! 外链。默认安全：缺省行为 = Provider 关闭（迁移列默认 `enabled=0`）。

use serde_json::Value;

use crate::video::classify::{is_allowed_host, validate_url_shape, ClassifyError};
use crate::video::Provider;

/// 每 Provider 出站/解析策略（`video_provider_policies` 投影）。
#[derive(Debug, Clone)]
pub struct VideoPolicy {
    pub provider: Provider,
    pub enabled: bool,
    /// host allowlist（JSON 数组；空 = 放行任意合法 host，仍受 egress 约束）。
    pub allow_hosts: Vec<String>,
    pub max_redirects: u32,
    pub max_response_bytes: i64,
    pub max_playlist_depth: usize,
    pub max_segments: usize,
    pub max_duration_ms: i64,
    /// 扩展配置（`config_json` JSON 对象；只接受已知键）。
    pub config: Value,
    pub version: i64,
    pub updated_at: i64,
}

impl VideoPolicy {
    /// 缺省策略（未配置 = Provider 关闭；列默认与迁移一致）。
    pub fn default_for(provider: Provider) -> VideoPolicy {
        VideoPolicy {
            provider,
            enabled: false,
            allow_hosts: Vec::new(),
            max_redirects: 3,
            max_response_bytes: 5 * 1024 * 1024,
            max_playlist_depth: 5,
            max_segments: 200,
            max_duration_ms: 3_600_000,
            config: Value::Object(Default::default()),
            version: 1,
            updated_at: 0,
        }
    }

    /// 出站超时（毫秒；config `timeout_ms`，缺省 15s）。
    pub fn timeout_ms(&self) -> u64 {
        self.config
            .get("timeout_ms")
            .and_then(Value::as_u64)
            .unwrap_or(15_000)
            .clamp(1_000, 60_000)
    }

    /// HLS 分片是否允许跨源（默认 false）。
    pub fn hls_allow_cross_origin(&self) -> bool {
        self.config
            .get("hls_allow_cross_origin")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    /// 西瓜是否允许官方 iframe 嵌入（默认 true；false = 仅外链降级）。
    pub fn xigua_allow_embed(&self) -> bool {
        self.config
            .get("xigua_allow_embed")
            .and_then(Value::as_bool)
            .unwrap_or(true)
    }

    /// host 是否在当前策略下允许（enable + allowlist 双门）。
    pub fn allows_host(&self, host: &str) -> bool {
        self.enabled && is_allowed_host(host, &self.allow_hosts)
    }
}

/// 校验 host allowlist 条目（纯形态校验；小写、可含子域，禁 IP/私网字面量）。
pub fn validate_host_list(raw: &[String]) -> Result<Vec<String>, ClassifyError> {
    let mut out = Vec::with_capacity(raw.len());
    for entry in raw {
        let mut entry = entry.trim();
        if entry.is_empty() {
            continue;
        }
        let wildcard = entry.starts_with("*.");
        if wildcard {
            entry = &entry[2..];
        }
        // 以规范化 URL 的形状校验为基准（scheme 无关）。
        let probe = format!("https://{entry}/");
        let normalized = validate_url_shape(&probe)?;
        let host = normalized.host;
        if host.contains(':') {
            return Err(ClassifyError::HostInvalid(host));
        }
        out.push(if wildcard { format!("*.{host}") } else { host });
    }
    Ok(out)
}

/// 数值上限（配置写入时的保守约束）。
pub const MAX_REDIRECTS_LIMIT: u32 = 10;
pub const MAX_RESPONSE_BYTES_LIMIT: i64 = 512 * 1024 * 1024;
pub const MIN_RESPONSE_BYTES: i64 = 64 * 1024;
pub const MAX_PLAYLIST_DEPTH_LIMIT: usize = 10;
pub const MAX_SEGMENTS_LIMIT: usize = 2000;
pub const MAX_DURATION_MS_LIMIT: i64 = 24 * 3600 * 1000;

/// 扩展配置允许的键。
const CONFIG_KEYS: &[&str] = &["timeout_ms", "hls_allow_cross_origin", "xigua_allow_embed"];

/// 校验扩展配置对象：必须是 JSON 对象且只含已知键、值类型正确。
pub fn validate_config(value: &Value) -> Result<Value, String> {
    let obj = value
        .as_object()
        .ok_or_else(|| "config 必须是 JSON 对象".to_string())?;
    for (key, v) in obj {
        if !CONFIG_KEYS.contains(&key.as_str()) {
            return Err(format!("config 未知键: {key}"));
        }
        match key.as_str() {
            "timeout_ms" => {
                let n = v
                    .as_u64()
                    .ok_or_else(|| "timeout_ms 必须是整数".to_string())?;
                if !(1_000..=60_000).contains(&n) {
                    return Err("timeout_ms 必须在 1000..=60000".to_string());
                }
            }
            "hls_allow_cross_origin" | "xigua_allow_embed" => {
                if !v.is_boolean() {
                    return Err(format!("{key} 必须是布尔值"));
                }
            }
            _ => unreachable!("CONFIG_KEYS 已覆盖"),
        }
    }
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_policy_is_disabled() {
        for provider in Provider::ALL {
            let p = VideoPolicy::default_for(provider);
            assert!(!p.enabled);
            assert!(!p.allows_host("cdn.example.com"));
        }
    }

    #[test]
    fn allows_host_respects_enable_and_allowlist() {
        let mut p = VideoPolicy::default_for(Provider::Direct);
        p.enabled = true;
        assert!(p.allows_host("any.example.com"));
        p.allow_hosts = vec!["example.com".to_string()];
        assert!(p.allows_host("example.com"));
        assert!(p.allows_host("sub.example.com"));
        assert!(!p.allows_host("evil.com"));
    }

    #[test]
    fn host_list_validation_rejects_ip_and_keeps_wildcards() {
        assert!(validate_host_list(&["10.0.0.1".to_string()]).is_err());
        assert!(validate_host_list(&["127.0.0.1".to_string()]).is_err());
        let ok = validate_host_list(&["example.com".to_string(), "*.cdn.net".to_string()]).unwrap();
        assert_eq!(ok, vec!["example.com", "*.cdn.net"]);
        assert!(validate_host_list(&["http://x.example.com".to_string()]).is_err());
        assert!(validate_host_list(&["example.com:8443".to_string()]).is_err());
    }

    #[test]
    fn config_validation_accepts_known_keys_only() {
        assert!(validate_config(&json!({"timeout_ms": 10000})).is_ok());
        assert!(validate_config(&json!({"hls_allow_cross_origin": true})).is_ok());
        assert!(validate_config(&json!({"xigua_allow_embed": false})).is_ok());
        assert!(validate_config(&json!({"evil": 1})).is_err());
        assert!(validate_config(&json!([])).is_err());
        assert!(validate_config(&json!({"timeout_ms": 100})).is_err());
        assert!(validate_config(&json!({"hls_allow_cross_origin": "yes"})).is_err());
    }

    #[test]
    fn config_accessors_with_defaults() {
        let p = VideoPolicy::default_for(Provider::Hls);
        assert_eq!(p.timeout_ms(), 15_000);
        assert!(!p.hls_allow_cross_origin());
        assert!(p.xigua_allow_embed());
        let mut p2 = p.clone();
        p2.config = json!({"timeout_ms": 20000, "hls_allow_cross_origin": true, "xigua_allow_embed": false});
        assert_eq!(p2.timeout_ms(), 20_000);
        assert!(p2.hls_allow_cross_origin());
        assert!(!p2.xigua_allow_embed());
    }
}
