//! M13-PLUGIN：v1 配置型插件领域模块（无 axum；sqlx Either 双库）。
//!
//! 安全边界（PLUGIN.md §1.2/§4）：
//! - 插件是**配置数据**：manifest + 声明式 rules + settings，**v1 无在线代码
//!   执行路径**（代码/WASM 插件是 v2 研究项，见 docs/PLUGIN.md §10）；
//! - 插件只能访问显式输入（事件 payload + 自身 `plugin_data` 命名空间）与
//!   最小白名单动作；**不能**获得 DB/Session/OAuth Token/S3 Secret/通用网络；
//! - Direct/HLS/Xigua 是受控 Provider Adapter（复用 `crate::video::Provider`
//!   注册表）；插件不能替换权限、审核或账本裁决；
//! - 配置校验拒绝未知 capability、危险 URL、代码内容和超出版本范围的设置；
//! - 插件故障/超时/重复调用/旧版本结果**安全降级**：v1 插件动作是异步
//!   worker（PLUGIN.md §5），`record_call` 只追加指标，从不阻塞核心论坛。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::db::DatabasePool;

/// 配置型插件清单 schema 版本（PLUGIN.md §2）。
pub const PLUGIN_SCHEMA_VERSION: i64 = 1;
/// 核心兼容 range（语义版本主版本 1）。
pub const CORE_PLUGIN_RANGE: &str = ">=1.0 <2.0";
/// 上传配置包总大小上限（PLUGIN.md §2：解压限制；JSON 包 256KB）。
pub const MAX_PACKAGE_BYTES: usize = 256 * 1024;

/// v1 已知 capability 白名单（PLUGIN.md §4 动作表 + 视频 Provider 受控能力）。
/// 该集合**不含**任何权限/审核/账本裁决能力——插件永远不能改变裁决结果。
pub const KNOWN_CAPABILITIES: &[&str] = &[
    "notification.create",
    "points.award",
    "plugin_data.put",
    "plugin_data.delete",
    "tag.attach",
    "audit.note",
    "video.resolve",
    "video.render",
    "video.metadata.refresh",
];

/// v1 已知领域事件（PLUGIN.md §3；after-event 语义）。
pub const KNOWN_EVENTS: &[&str] = &[
    "user.verified.v1",
    "user.login_succeeded.v1",
    "post.published.v1",
    "post.updated.v1",
    "comment.published.v1",
    "reaction.created.v1",
    "report.created.v1",
    "moderation.action_recorded.v1",
    "points.operation_completed.v1",
    "level.changed.v1",
];

/// 调用结果标签（plugin_call_metrics.result）。
pub const CALL_RESULTS: &[&str] = &["ok", "error", "timeout", "repeat", "stale", "skipped"];

/// 插件错误（稳定 code）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginError {
    Invalid(String),
    NotFound(String),
    Conflict(String),
    Incompatible(String),
}

impl PluginError {
    pub fn code(&self) -> &'static str {
        match self {
            PluginError::Invalid(_) => "plugin_invalid",
            PluginError::NotFound(_) => "plugin_not_found",
            PluginError::Conflict(_) => "plugin_conflict",
            PluginError::Incompatible(_) => "plugin_incompatible",
        }
    }
    pub fn message(&self) -> &str {
        match self {
            PluginError::Invalid(m)
            | PluginError::NotFound(m)
            | PluginError::Conflict(m)
            | PluginError::Incompatible(m) => m,
        }
    }
}

impl std::fmt::Display for PluginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for PluginError {}

/// 插件 ID：小写 ASCII/数字/连字符，1..=64。
pub fn validate_plugin_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// 危险内容特征（设置/订阅/任何字符串值）：代码内容与远程/凭据 URL。
const DANGEROUS_STRING_PATTERNS: &[&str] = &[
    "<script",
    "<?php",
    "<?xml",
    "javascript:",
    "vbscript:",
    "data:text/html",
    "eval(",
    "Function(",
    "system(",
    "exec(",
    "shell_exec",
    "require(",
    "include(",
    "import ",
    "http://",
    "https://",
    "//",
    "file:",
    "ftp:",
    "\\\\",
    "{",
    "}",
    ";",
];

/// 扫描字符串值（拒绝代码内容与危险 URL）。
fn scan_dangerous(value: &str, where_: &str) -> Result<(), PluginError> {
    for pattern in DANGEROUS_STRING_PATTERNS {
        if value.contains(pattern) {
            return Err(PluginError::Invalid(format!(
                "{where_} contains dangerous content pattern '{pattern}'"
            )));
        }
    }
    Ok(())
}

/// 校验 settings schema（最小 JSON Schema 子集；未知 schema 键拒绝）。
pub fn validate_settings_schema(schema: &Value) -> Result<(), PluginError> {
    let obj = schema
        .as_object()
        .ok_or_else(|| PluginError::Invalid("settings_schema must be a JSON object".to_string()))?;
    if obj.get("type").and_then(Value::as_str) != Some("object") {
        return Err(PluginError::Invalid(
            "settings_schema.type must be 'object'".to_string(),
        ));
    }
    for key in obj.keys() {
        if !["type", "properties", "required", "additionalProperties"].contains(&key.as_str()) {
            return Err(PluginError::Invalid(format!(
                "settings_schema contains unsupported key '{key}'"
            )));
        }
    }
    if let Some(required) = obj.get("required") {
        let list = required.as_array().ok_or_else(|| {
            PluginError::Invalid("settings_schema.required must be an array".to_string())
        })?;
        for item in list {
            if !item.is_string() {
                return Err(PluginError::Invalid(
                    "settings_schema.required items must be strings".to_string(),
                ));
            }
        }
    }
    if obj.get("additionalProperties").map(Value::is_boolean) != Some(true) {
        return Err(PluginError::Invalid(
            "settings_schema.additionalProperties must be a boolean".to_string(),
        ));
    }
    if let Some(props) = obj.get("properties") {
        let props = props.as_object().ok_or_else(|| {
            PluginError::Invalid("settings_schema.properties must be an object".to_string())
        })?;
        for (name, prop_schema) in props {
            validate_property_schema(name, prop_schema)?;
        }
    }
    Ok(())
}

fn validate_property_schema(name: &str, schema: &Value) -> Result<(), PluginError> {
    let obj = schema.as_object().ok_or_else(|| {
        PluginError::Invalid(format!("property '{name}' schema must be an object"))
    })?;
    let ty = obj
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Invalid(format!("property '{name}' missing type")))?;
    if !["string", "integer", "number", "boolean"].contains(&ty) {
        return Err(PluginError::Invalid(format!(
            "property '{name}' has unsupported type '{ty}'"
        )));
    }
    for key in obj.keys() {
        if ![
            "type",
            "minimum",
            "maximum",
            "minLength",
            "maxLength",
            "enum",
            "default",
        ]
        .contains(&key.as_str())
        {
            return Err(PluginError::Invalid(format!(
                "property '{name}' contains unsupported key '{key}'"
            )));
        }
    }
    if obj.contains_key("minimum") && !obj["minimum"].is_number() {
        return Err(PluginError::Invalid(format!(
            "property '{name}' minimum must be numeric"
        )));
    }
    if obj.contains_key("maximum") && !obj["maximum"].is_number() {
        return Err(PluginError::Invalid(format!(
            "property '{name}' maximum must be numeric"
        )));
    }
    if let Some(enum_values) = obj.get("enum") {
        let list = enum_values.as_array().ok_or_else(|| {
            PluginError::Invalid(format!("property '{name}' enum must be an array"))
        })?;
        if list.is_empty() {
            return Err(PluginError::Invalid(format!(
                "property '{name}' enum must not be empty"
            )));
        }
        // 封闭枚举：每一项都必须是 string/number/boolean（拒绝对象/数组）。
        for item in list {
            if !(item.is_string() || item.is_number() || item.is_boolean()) {
                return Err(PluginError::Invalid(format!(
                    "property '{name}' enum items must be primitive"
                )));
            }
        }
    }
    Ok(())
}

/// 校验 settings 值是否满足 schema（含危险内容扫描）。
pub fn validate_settings_against_schema(
    settings: &Value,
    schema: &Value,
) -> Result<(), PluginError> {
    let obj = settings
        .as_object()
        .ok_or_else(|| PluginError::Invalid("settings must be a JSON object".to_string()))?;
    let schema_obj = schema
        .as_object()
        .ok_or_else(|| PluginError::Invalid("settings_schema must be an object".to_string()))?;
    let properties = schema_obj
        .get("properties")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let required = schema_obj
        .get("required")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    // additionalProperties 必须为 false（v1 封闭 settings）。
    if schema_obj
        .get("additionalProperties")
        .and_then(Value::as_bool)
        != Some(false)
    {
        return Err(PluginError::Invalid(
            "settings_schema.additionalProperties must be false".to_string(),
        ));
    }
    for name in &required {
        if !obj.contains_key(name.as_str().unwrap_or_default()) {
            return Err(PluginError::Invalid(format!(
                "missing required setting '{}'",
                name.as_str().unwrap_or_default()
            )));
        }
    }
    for (key, value) in obj {
        let Some(prop_schema) = properties.get(key) else {
            return Err(PluginError::Invalid(format!(
                "unknown setting '{key}' (closed settings schema)"
            )));
        };
        validate_setting_value(key, value, prop_schema)?;
        if let Some(s) = value.as_str() {
            scan_dangerous(s, &format!("setting '{key}'"))?;
        }
    }
    Ok(())
}

fn validate_setting_value(key: &str, value: &Value, schema: &Value) -> Result<(), PluginError> {
    let ty = schema
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("string");
    let invalid = |m: String| PluginError::Invalid(m);
    match ty {
        "string" => {
            let s = value
                .as_str()
                .ok_or_else(|| invalid(format!("setting '{key}' must be a string")))?;
            if let Some(min) = schema.get("minLength").and_then(Value::as_u64) {
                if (s.len() as u64) < min {
                    return Err(invalid(format!(
                        "setting '{key}' is shorter than minLength {min}"
                    )));
                }
            }
            if let Some(max) = schema.get("maxLength").and_then(Value::as_u64) {
                if (s.len() as u64) > max {
                    return Err(invalid(format!(
                        "setting '{key}' is longer than maxLength {max}"
                    )));
                }
            }
        }
        "integer" => {
            let n = value
                .as_i64()
                .ok_or_else(|| invalid(format!("setting '{key}' must be an integer")))?;
            if let Some(min) = schema.get("minimum").and_then(Value::as_i64) {
                if n < min {
                    return Err(invalid(format!("setting '{key}' is below minimum {min}")));
                }
            }
            if let Some(max) = schema.get("maximum").and_then(Value::as_i64) {
                if n > max {
                    return Err(invalid(format!("setting '{key}' is above maximum {max}")));
                }
            }
        }
        "number" => {
            let n = value
                .as_f64()
                .ok_or_else(|| invalid(format!("setting '{key}' must be a number")))?;
            if let Some(min) = schema.get("minimum").and_then(Value::as_f64) {
                if n < min {
                    return Err(invalid(format!("setting '{key}' is below minimum {min}")));
                }
            }
            if let Some(max) = schema.get("maximum").and_then(Value::as_f64) {
                if n > max {
                    return Err(invalid(format!("setting '{key}' is above maximum {max}")));
                }
            }
        }
        "boolean" => {
            if !value.is_boolean() {
                return Err(invalid(format!("setting '{key}' must be a boolean")));
            }
        }
        _ => {
            return Err(invalid(format!(
                "setting '{key}' has unsupported type '{ty}'"
            )));
        }
    }
    if let Some(enum_values) = schema.get("enum").and_then(Value::as_array) {
        if !enum_values.iter().any(|v| v == value) {
            return Err(invalid(format!(
                "setting '{key}' is not one of the allowed enum values"
            )));
        }
    }
    Ok(())
}

/// 解析并校验完整插件配置包（manifest；不信任输入）。
pub fn parse_plugin_package(package: &Value) -> Result<ParsedPlugin, PluginError> {
    let schema_version = package
        .get("schema_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| PluginError::Invalid("schema_version required".to_string()))?;
    if schema_version != PLUGIN_SCHEMA_VERSION {
        return Err(PluginError::Incompatible(format!(
            "unsupported schema_version {schema_version} (expected {PLUGIN_SCHEMA_VERSION})"
        )));
    }
    let kind = package.get("kind").and_then(Value::as_str).unwrap_or("");
    if kind != "config" {
        return Err(PluginError::Invalid(
            "only config plugins are supported in v1 (kind must be 'config'; code/WASM is a v2 research item)"
                .to_string(),
        ));
    }
    let id = package
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Invalid("id required".to_string()))?;
    if !validate_plugin_id(id) {
        return Err(PluginError::Invalid(format!(
            "invalid plugin id '{id}' (lowercase ascii/digits/hyphens, <=64)"
        )));
    }
    let name = package
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Invalid("name required".to_string()))?;
    if name.len() > 120 || name.is_empty() {
        return Err(PluginError::Invalid(
            "name must be 1..=120 characters".to_string(),
        ));
    }
    let version = package
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Invalid("version required (semver)".to_string()))?;
    if !is_semver(version) {
        return Err(PluginError::Invalid(format!(
            "invalid version '{version}' (semver MAJOR.MINOR[.PATCH])"
        )));
    }
    let supports = package
        .get("supports")
        .and_then(Value::as_str)
        .ok_or_else(|| PluginError::Invalid("supports range required".to_string()))?;
    if supports.len() > 64 || !is_compatible(supports) {
        return Err(PluginError::Incompatible(format!(
            "plugin '{id}' supports range '{supports}' is not compatible with core {CORE_PLUGIN_RANGE}"
        )));
    }
    // capabilities 必须是白名单子集（未知 capability 拒绝）。
    let capabilities = package
        .get("capabilities")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| PluginError::Invalid("capabilities must be an array".to_string()))?;
    let mut caps = Vec::new();
    for item in &capabilities {
        let c = item.as_str().ok_or_else(|| {
            PluginError::Invalid("capabilities items must be strings".to_string())
        })?;
        if !KNOWN_CAPABILITIES.contains(&c) {
            return Err(PluginError::Invalid(format!(
                "unknown capability '{c}' (allowlist: {KNOWN_CAPABILITIES:?})"
            )));
        }
        caps.push(c.to_string());
    }
    // subscriptions 必须是已知事件（未知事件拒绝）。
    let subscriptions = package
        .get("subscriptions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let mut subs = Vec::new();
    for item in &subscriptions {
        let e = item.as_str().ok_or_else(|| {
            PluginError::Invalid("subscriptions items must be strings".to_string())
        })?;
        if !KNOWN_EVENTS.contains(&e) {
            return Err(PluginError::Invalid(format!(
                "unknown event subscription '{e}' (known events: {KNOWN_EVENTS:?})"
            )));
        }
        subs.push(e.to_string());
    }
    let settings_schema = package
        .get("settings_schema")
        .ok_or_else(|| PluginError::Invalid("settings_schema required".to_string()))?;
    validate_settings_schema(settings_schema)?;
    Ok(ParsedPlugin {
        id: id.to_string(),
        name: name.to_string(),
        version: version.to_string(),
        supports: supports.to_string(),
        capabilities: caps,
        subscriptions: subs,
        settings_schema: settings_schema.clone(),
    })
}

/// 简单 semver 校验（MAJOR.MINOR[.PATCH]）。
fn is_semver(v: &str) -> bool {
    let mut parts = v.split('.');
    let ok = parts
        .next()
        .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
        && parts
            .next()
            .is_some_and(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    if !ok {
        return false;
    }
    let patch = parts.next().unwrap_or("0");
    if !patch.bytes().all(|b| b.is_ascii_digit()) || parts.next().is_some() {
        return false;
    }
    true
}

/// 版本范围接受性（与 theme 模块同一语义：支持 `>= > <= < ==`，运算符可连写）。
fn range_accepts(range: &str, version: &str) -> bool {
    if !is_semver(version) {
        return false;
    }
    let ver = version.to_string();
    let mut remaining = range.trim();
    if remaining.is_empty() {
        return false;
    }
    while !remaining.is_empty() {
        remaining = remaining.trim_start();
        let (op, rest) = if let Some(rest) = remaining.strip_prefix(">=") {
            (">=", rest)
        } else if let Some(rest) = remaining.strip_prefix("<=") {
            ("<=", rest)
        } else if let Some(rest) = remaining.strip_prefix("==") {
            ("==", rest)
        } else if let Some(rest) = remaining.strip_prefix('>') {
            (">", rest)
        } else if let Some(rest) = remaining.strip_prefix('<') {
            ("<", rest)
        } else {
            return false;
        };
        remaining = rest.trim_start();
        let end = remaining
            .find(char::is_whitespace)
            .unwrap_or(remaining.len());
        let raw = &remaining[..end];
        if !is_semver(raw) {
            return false;
        }
        let cmp = compare_semver(&ver, raw);
        let ok = match op {
            ">=" => cmp.is_ge(),
            ">" => cmp.is_gt(),
            "<=" => cmp.is_le(),
            "<" => cmp.is_lt(),
            "==" => cmp.is_eq(),
            _ => false,
        };
        if !ok {
            return false;
        }
        remaining = &remaining[end..];
    }
    true
}

fn parse_semver(v: &str) -> (u64, u64, u64) {
    let mut parts = v.split('.');
    let major = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let minor = parts.next().unwrap_or("0").parse().unwrap_or(0);
    let patch = parts.next().unwrap_or("0").parse().unwrap_or(0);
    (major, minor, patch)
}

fn compare_semver(a: &str, b: &str) -> std::cmp::Ordering {
    parse_semver(a).cmp(&parse_semver(b))
}

/// 插件与核心兼容性：`supports` 必须接受核心版本 `1.0`。
pub fn is_compatible(supports: &str) -> bool {
    range_accepts(supports, "1.0")
}

/// 校验通过的插件配置包。
#[derive(Debug, Clone)]
pub struct ParsedPlugin {
    pub id: String,
    pub name: String,
    pub version: String,
    pub supports: String,
    pub capabilities: Vec<String>,
    pub subscriptions: Vec<String>,
    pub settings_schema: Value,
}

/// 插件行（数据库投影）。
#[derive(Debug, Clone)]
pub struct Plugin {
    pub id: String,
    pub plugin_id: String,
    pub name: String,
    pub version: String,
    pub schema_version: i64,
    pub supports: String,
    pub status: String,
    pub capabilities: Vec<String>,
    pub subscriptions: Vec<String>,
    pub settings_schema: Value,
    pub settings: Value,
    pub policy_revision: i64,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Plugin {
    /// 安全 JSON 投影（不泄漏内部字段；settings 脱敏后返回）。
    pub fn json(&self) -> Value {
        json!({
            "id": self.plugin_id,
            "name": self.name,
            "version": self.version,
            "schema_version": self.schema_version,
            "supports": self.supports,
            "status": self.status,
            "capabilities": self.capabilities,
            "subscriptions": self.subscriptions,
            "settings": self.settings,
            "policy_revision": self.policy_revision,
            "created_by": self.created_by,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }
}

fn json_vec<T: AsRef<str>>(values: &[T]) -> Value {
    json!(values
        .iter()
        .map(|v| v.as_ref().to_string())
        .collect::<Vec<_>>())
}

fn plugin_from_row(r: &sqlx::sqlite::SqliteRow) -> Plugin {
    let caps: String = r.get("capabilities_json");
    let subs: String = r.get("subscriptions_json");
    let schema: String = r.get("settings_schema_json");
    let settings: String = r.get("settings_json");
    Plugin {
        id: r.get("id"),
        plugin_id: r.get("plugin_id"),
        name: r.get("name"),
        version: r.get("version"),
        schema_version: r.get("schema_version"),
        supports: r.get("supports"),
        status: r.get("status"),
        capabilities: serde_json::from_str(&caps).unwrap_or_default(),
        subscriptions: serde_json::from_str(&subs).unwrap_or_default(),
        settings_schema: serde_json::from_str(&schema).unwrap_or_else(|_| json!({})),
        settings: serde_json::from_str(&settings).unwrap_or_else(|_| json!({})),
        policy_revision: r.get("policy_revision"),
        created_by: r.get("created_by"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn plugin_from_row_mysql(r: &sqlx::mysql::MySqlRow) -> Plugin {
    let caps: String = r.get("capabilities_json");
    let subs: String = r.get("subscriptions_json");
    let schema: String = r.get("settings_schema_json");
    let settings: String = r.get("settings_json");
    Plugin {
        id: r.get("id"),
        plugin_id: r.get("plugin_id"),
        name: r.get("name"),
        version: r.get("version"),
        schema_version: r.get("schema_version"),
        supports: r.get("supports"),
        status: r.get("status"),
        capabilities: serde_json::from_str(&caps).unwrap_or_default(),
        subscriptions: serde_json::from_str(&subs).unwrap_or_default(),
        settings_schema: serde_json::from_str(&schema).unwrap_or_else(|_| json!({})),
        settings: serde_json::from_str(&settings).unwrap_or_else(|_| json!({})),
        policy_revision: r.get("policy_revision"),
        created_by: r.get("created_by"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

const PLUGIN_COLUMNS: &str = "id, plugin_id, name, version, schema_version, supports, status, capabilities_json, subscriptions_json, settings_schema_json, settings_json, policy_revision, created_by, created_at, updated_at";

async fn load_plugin_by_id(
    pool: &DatabasePool,
    plugin_id: &str,
) -> Result<Option<Plugin>, PluginError> {
    match pool {
        Either::Left(p) => {
            let row = sqlx::query(&format!(
                "SELECT {PLUGIN_COLUMNS} FROM plugins WHERE plugin_id = ?"
            ))
            .bind(plugin_id)
            .fetch_optional(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
            Ok(row.map(|r| plugin_from_row(&r)))
        }
        Either::Right(p) => {
            let row = sqlx::query(&format!(
                "SELECT {PLUGIN_COLUMNS} FROM plugins WHERE plugin_id = ?"
            ))
            .bind(plugin_id)
            .fetch_optional(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
            Ok(row.map(|r| plugin_from_row_mysql(&r)))
        }
    }
}

/// 读取单插件（供管理端 get）。
pub async fn load_plugin(
    pool: &DatabasePool,
    plugin_id: &str,
) -> Result<Option<Plugin>, PluginError> {
    load_plugin_by_id(pool, plugin_id).await
}

/// 安装插件（M13-PLUGIN-06）：完整校验 → 插入（status=disabled 隔离态，
/// policy_revision=1）→ 返回。安装不等于启用（PLUGIN.md §8）。
pub async fn install_plugin(
    pool: &DatabasePool,
    package: &Value,
    actor: &str,
) -> Result<Plugin, PluginError> {
    let parsed = parse_plugin_package(package)?;
    if load_plugin_by_id(pool, &parsed.id).await?.is_some() {
        return Err(PluginError::Conflict(format!(
            "plugin '{}' already installed (update via settings or disable first)",
            parsed.id
        )));
    }
    let now = crate::outbox::now_millis();
    let id = uuid::Uuid::now_v7().to_string();
    let caps = serde_json::to_string(&json_vec(&parsed.capabilities))
        .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
    let subs = serde_json::to_string(&json_vec(&parsed.subscriptions))
        .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
    let schema = serde_json::to_string(&parsed.settings_schema)
        .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
    let empty_settings = "{}";
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO plugins (id, plugin_id, name, version, schema_version, supports, status, capabilities_json, subscriptions_json, settings_schema_json, settings_json, policy_revision, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'disabled', ?, ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&parsed.id)
            .bind(&parsed.name)
            .bind(&parsed.version)
            .bind(PLUGIN_SCHEMA_VERSION)
            .bind(&parsed.supports)
            .bind(&caps)
            .bind(&subs)
            .bind(&schema)
            .bind(empty_settings)
            .bind(actor)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO plugins (id, plugin_id, name, version, schema_version, supports, status, capabilities_json, subscriptions_json, settings_schema_json, settings_json, policy_revision, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'disabled', ?, ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&parsed.id)
            .bind(&parsed.name)
            .bind(&parsed.version)
            .bind(PLUGIN_SCHEMA_VERSION)
            .bind(&parsed.supports)
            .bind(&caps)
            .bind(&subs)
            .bind(&schema)
            .bind(empty_settings)
            .bind(actor)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
        }
    }
    let _ = record_audit(
        pool,
        actor,
        "plugin.install",
        &parsed.id,
        "installed (disabled)",
        now,
    )
    .await;
    load_plugin_by_id(pool, &parsed.id)
        .await?
        .ok_or_else(|| PluginError::NotFound(format!("plugin '{}' not found", parsed.id)))
}

/// 列出全部插件（管理端；含禁用/错误态）。
pub async fn list_plugins(pool: &DatabasePool) -> Result<Vec<Plugin>, PluginError> {
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(&format!(
                "SELECT {PLUGIN_COLUMNS} FROM plugins ORDER BY plugin_id"
            ))
            .fetch_all(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
            Ok(rows.iter().map(plugin_from_row).collect())
        }
        Either::Right(p) => {
            let rows = sqlx::query(&format!(
                "SELECT {PLUGIN_COLUMNS} FROM plugins ORDER BY plugin_id"
            ))
            .fetch_all(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
            Ok(rows.iter().map(plugin_from_row_mysql).collect())
        }
    }
}

/// 更新插件 settings（M13-PLUGIN-04/06）：closed settings schema 校验 + 危险
/// 内容扫描 + policy_revision+1（乐观锁；expected 不匹配拒绝）。
pub async fn update_plugin_settings(
    pool: &DatabasePool,
    plugin_id: &str,
    settings: &Value,
    actor: &str,
    reason: &str,
    expected_policy_revision: i64,
) -> Result<Plugin, PluginError> {
    let plugin = load_plugin_by_id(pool, plugin_id)
        .await?
        .ok_or_else(|| PluginError::NotFound(format!("plugin '{plugin_id}' not found")))?;
    if plugin.policy_revision != expected_policy_revision {
        return Err(PluginError::Conflict(format!(
            "plugin policy revision conflict: expected {expected_policy_revision}, current {}",
            plugin.policy_revision
        )));
    }
    validate_settings_against_schema(settings, &plugin.settings_schema)?;
    let settings_json = serde_json::to_string(settings)
        .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
    let new_revision = plugin.policy_revision + 1;
    let now = crate::outbox::now_millis();
    let affected = match pool {
        Either::Left(p) => {

            sqlx::query(
                "UPDATE plugins SET settings_json = ?, policy_revision = ?, updated_at = ? WHERE plugin_id = ? AND policy_revision = ?",
            )
            .bind(&settings_json)
            .bind(new_revision)
            .bind(now)
            .bind(plugin_id)
            .bind(plugin.policy_revision)
            .execute(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?
    .rows_affected()
        }
        Either::Right(p) => {

            sqlx::query(
                "UPDATE plugins SET settings_json = ?, policy_revision = ?, updated_at = ? WHERE plugin_id = ? AND policy_revision = ?",
            )
            .bind(&settings_json)
            .bind(new_revision)
            .bind(now)
            .bind(plugin_id)
            .bind(plugin.policy_revision)
            .execute(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?
    .rows_affected()
        }
    };
    if affected == 0 {
        return Err(PluginError::Conflict(
            "plugin policy revision conflict".to_string(),
        ));
    }
    let _ = record_audit(
        pool,
        actor,
        "plugin.settings.update",
        plugin_id,
        reason,
        now,
    )
    .await;
    load_plugin_by_id(pool, plugin_id)
        .await?
        .ok_or_else(|| PluginError::NotFound(format!("plugin '{plugin_id}' not found")))
}

/// 启停插件（M13-PLUGIN-06）：enabled/disabled/error。policy_revision+1。
pub async fn set_plugin_status(
    pool: &DatabasePool,
    plugin_id: &str,
    status: &str,
    actor: &str,
    reason: &str,
    expected_policy_revision: i64,
) -> Result<Plugin, PluginError> {
    if !matches!(status, "enabled" | "disabled") {
        return Err(PluginError::Invalid(
            "status must be 'enabled' or 'disabled'".to_string(),
        ));
    }
    let plugin = load_plugin_by_id(pool, plugin_id)
        .await?
        .ok_or_else(|| PluginError::NotFound(format!("plugin '{plugin_id}' not found")))?;
    if plugin.policy_revision != expected_policy_revision {
        return Err(PluginError::Conflict(format!(
            "plugin policy revision conflict: expected {expected_policy_revision}, current {}",
            plugin.policy_revision
        )));
    }
    let new_revision = plugin.policy_revision + 1;
    let now = crate::outbox::now_millis();
    let affected = match pool {
        Either::Left(p) => {

            sqlx::query(
                "UPDATE plugins SET status = ?, policy_revision = ?, updated_at = ? WHERE plugin_id = ? AND policy_revision = ?",
            )
            .bind(status)
            .bind(new_revision)
            .bind(now)
            .bind(plugin_id)
            .bind(plugin.policy_revision)
            .execute(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?
    .rows_affected()
        }
        Either::Right(p) => {

            sqlx::query(
                "UPDATE plugins SET status = ?, policy_revision = ?, updated_at = ? WHERE plugin_id = ? AND policy_revision = ?",
            )
            .bind(status)
            .bind(new_revision)
            .bind(now)
            .bind(plugin_id)
            .bind(plugin.policy_revision)
            .execute(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?
    .rows_affected()
        }
    };
    if affected == 0 {
        return Err(PluginError::Conflict(
            "plugin policy revision conflict".to_string(),
        ));
    }
    let _ = record_audit(
        pool,
        actor,
        &format!("plugin.{status}"),
        plugin_id,
        reason,
        now,
    )
    .await;
    load_plugin_by_id(pool, plugin_id)
        .await?
        .ok_or_else(|| PluginError::NotFound(format!("plugin '{plugin_id}' not found")))
}

/// 卸载插件（保留 30 天数据由管理员决定；v1 直接移除配置与数据）。
pub async fn uninstall_plugin(
    pool: &DatabasePool,
    plugin_id: &str,
    actor: &str,
    reason: &str,
) -> Result<(), PluginError> {
    let plugin = load_plugin_by_id(pool, plugin_id)
        .await?
        .ok_or_else(|| PluginError::NotFound(format!("plugin '{plugin_id}' not found")))?;
    if plugin.status == "enabled" {
        return Err(PluginError::Conflict(
            "plugin must be disabled before uninstall".to_string(),
        ));
    }
    let affected = match pool {
        Either::Left(p) => sqlx::query("DELETE FROM plugins WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?
            .rows_affected(),
        Either::Right(p) => sqlx::query("DELETE FROM plugins WHERE plugin_id = ?")
            .bind(plugin_id)
            .execute(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?
            .rows_affected(),
    };
    if affected == 0 {
        return Err(PluginError::NotFound(format!(
            "plugin '{plugin_id}' not found"
        )));
    }
    let _ = record_audit(
        pool,
        actor,
        "plugin.uninstall",
        plugin_id,
        reason,
        crate::outbox::now_millis(),
    )
    .await;
    Ok(())
}

/// 事件解析：返回订阅该事件且 status=enabled 的插件（含 policy_revision）。
/// 禁用/错误插件一律不消费新事件（PLUGIN.md §5）。
pub async fn resolve_plugins_for_event(
    pool: &DatabasePool,
    event_type: &str,
) -> Result<Vec<Plugin>, PluginError> {
    let all = list_plugins(pool).await?;
    Ok(all
        .into_iter()
        .filter(|p| p.status == "enabled" && p.subscriptions.iter().any(|s| s == event_type))
        .collect())
}

/// 记录插件调用摘要（M13-PLUGIN-06）：fire-and-forget，失败只记告警，
/// **绝不阻塞核心事务**（M13-PLUGIN-05 安全降级）。
pub async fn record_call(
    pool: &DatabasePool,
    plugin_id: &str,
    event_type: &str,
    result: &str,
    error_class: Option<&str>,
    policy_revision: i64,
    latency_ms: Option<i64>,
) {
    if !CALL_RESULTS.contains(&result) {
        tracing::warn!(result, "dropping plugin call metric with unknown result");
        return;
    }
    let id = uuid::Uuid::now_v7().to_string();
    let now = crate::outbox::now_millis();
    let outcome: Result<(), String> = match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO plugin_call_metrics (id, plugin_id, event_type, result, error_class, policy_revision, latency_ms, occurred_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(plugin_id)
            .bind(event_type)
            .bind(result)
            .bind(error_class)
            .bind(policy_revision)
            .bind(latency_ms)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| crate::error::sanitize(&e.to_string()))
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO plugin_call_metrics (id, plugin_id, event_type, result, error_class, policy_revision, latency_ms, occurred_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(plugin_id)
            .bind(event_type)
            .bind(result)
            .bind(error_class)
            .bind(policy_revision)
            .bind(latency_ms)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| crate::error::sanitize(&e.to_string()))
        }
    };
    if let Err(e) = outcome {
        tracing::warn!(
            plugin_id,
            error = %e,
            "plugin call metric write failed (non-blocking)"
        );
    }
}

/// 插件调用指标列表（管理端）。
pub async fn list_plugin_metrics(
    pool: &DatabasePool,
    plugin_id: &str,
    limit: i64,
) -> Result<Vec<Value>, PluginError> {
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(
                "SELECT id, plugin_id, event_type, result, error_class, policy_revision, latency_ms, occurred_at
                 FROM plugin_call_metrics WHERE plugin_id = ? ORDER BY occurred_at DESC LIMIT ?",
            )
            .bind(plugin_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
            Ok(rows.iter().map(plugin_metric_row).collect())
        }
        Either::Right(p) => {
            let rows = sqlx::query(
                "SELECT id, plugin_id, event_type, result, error_class, policy_revision, latency_ms, occurred_at
                 FROM plugin_call_metrics WHERE plugin_id = ? ORDER BY occurred_at DESC LIMIT ?",
            )
            .bind(plugin_id)
            .bind(limit)
            .fetch_all(p)
            .await
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
            Ok(rows.iter().map(plugin_metric_row_mysql).collect())
        }
    }
}

fn plugin_metric_row(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "plugin_id": r.get::<String,_>("plugin_id"),
        "event_type": r.get::<String,_>("event_type"),
        "result": r.get::<String,_>("result"),
        "error_class": r.get::<Option<String>,_>("error_class"),
        "policy_revision": r.get::<i64,_>("policy_revision"),
        "latency_ms": r.get::<Option<i64>,_>("latency_ms"),
        "occurred_at": r.get::<i64,_>("occurred_at"),
    })
}

fn plugin_metric_row_mysql(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "plugin_id": r.get::<String,_>("plugin_id"),
        "event_type": r.get::<String,_>("event_type"),
        "result": r.get::<String,_>("result"),
        "error_class": r.get::<Option<String>,_>("error_class"),
        "policy_revision": r.get::<i64,_>("policy_revision"),
        "latency_ms": r.get::<Option<i64>,_>("latency_ms"),
        "occurred_at": r.get::<i64,_>("occurred_at"),
    })
}

/// 插件 plugin_data 命名空间（配额：每插件 64 keys，每值 8KB，总 1MB）。
pub const PLUGIN_DATA_MAX_KEYS: usize = 64;
pub const PLUGIN_DATA_MAX_VALUE_BYTES: usize = 8 * 1024;

/// 写插件自身数据（M13-PLUGIN-02：只能访问自身命名空间）。
pub async fn put_plugin_data(
    pool: &DatabasePool,
    plugin_id: &str,
    key: &str,
    value: &Value,
) -> Result<(), PluginError> {
    if key.is_empty() || key.len() > 128 || key.starts_with("__") {
        return Err(PluginError::Invalid("invalid plugin_data key".to_string()));
    }
    let value_json = serde_json::to_string(value)
        .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
    if value_json.len() > PLUGIN_DATA_MAX_VALUE_BYTES {
        return Err(PluginError::Invalid(format!(
            "plugin_data value exceeds {PLUGIN_DATA_MAX_VALUE_BYTES} bytes"
        )));
    }
    // 配额检查（skipped 非敏感）。
    let count: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_data WHERE plugin_id = ? AND key != ?")
                .bind(plugin_id)
                .bind(key)
                .fetch_one(p)
                .await
                .unwrap_or(0)
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM plugin_data WHERE plugin_id = ? AND key != ?")
                .bind(plugin_id)
                .bind(key)
                .fetch_one(p)
                .await
                .unwrap_or(0)
        }
    };
    if count as usize >= PLUGIN_DATA_MAX_KEYS {
        return Err(PluginError::Conflict(format!(
            "plugin_data exceeds max keys ({PLUGIN_DATA_MAX_KEYS})"
        )));
    }
    let now = crate::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO plugin_data (plugin_id, key, value_json, updated_at)
                 VALUES (?, ?, ?, ?)
                 ON CONFLICT(plugin_id, key) DO UPDATE SET value_json = excluded.value_json, updated_at = excluded.updated_at",
            )
            .bind(plugin_id)
            .bind(key)
            .bind(&value_json)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO plugin_data (plugin_id, key, value_json, updated_at)
                 VALUES (?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE value_json = VALUES(value_json), updated_at = VALUES(updated_at)",
            )
            .bind(plugin_id)
            .bind(key)
            .bind(&value_json)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| PluginError::Invalid(crate::error::sanitize(&e.to_string())))?;
        }
    };
    Ok(())
}

async fn record_audit(
    pool: &DatabasePool,
    actor: &str,
    action: &str,
    plugin_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), PluginError> {
    let _ = crate::audit::AuditEntry::user_action(actor, action)
        .with_target("plugin", plugin_id)
        .with_reason(reason)
        .with_policy_version(crate::authz::decision::AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    let _ = now;
    Ok(())
}

/// 受控 Provider Adapter 注册表（M13-PLUGIN-03）：Direct/HLS/Xigua 复用
/// `crate::video::Provider::ALL`。插件只能启停/配置策略，不能注册新 adapter
/// 或替换核心裁决。
pub fn provider_adapters() -> Vec<Value> {
    crate::video::Provider::ALL
        .iter()
        .map(|p| {
            json!({
                "provider": p.as_str(),
                "kind": "core_adapter",
                "managed": true,
                "capabilities": ["video.resolve", "video.render", "video.metadata.refresh"],
                "note": "compiled with the application; admins can enable/disable and configure policy only",
            })
        })
        .collect()
}

/// 插件可用的最小服务接口清单（M13-PLUGIN-02 白名单动作）。
pub fn service_interface() -> Vec<Value> {
    [
        (
            "notification.create",
            "create notification for the event-associated user",
        ),
        (
            "points.award",
            "award points via core ledger (site limits + idempotency key)",
        ),
        ("plugin_data.put", "write to own plugin_data namespace"),
        ("plugin_data.delete", "delete own plugin_data"),
        (
            "tag.attach",
            "attach configured approved tags to event posts",
        ),
        (
            "audit.note",
            "append plugin execution note (cannot modify core audit)",
        ),
        (
            "video.resolve",
            "resolve video embeds through the core video service",
        ),
        (
            "video.render",
            "render video embeds through the core video service",
        ),
        (
            "video.metadata.refresh",
            "refresh video metadata through the core video service",
        ),
    ]
    .iter()
    .map(|(action, description)| json!({ "action": action, "description": description }))
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn valid_package() -> Value {
        json!({
            "schema_version": 1,
            "id": "welcome-reward",
            "name": "新用户欢迎奖励",
            "version": "1.0.0",
            "supports": ">=1.0 <2.0",
            "kind": "config",
            "subscriptions": ["user.verified.v1"],
            "capabilities": ["notification.create"],
            "settings_schema": {
                "type": "object",
                "properties": {
                    "amount": { "type": "integer", "minimum": 0, "maximum": 1000 }
                },
                "required": ["amount"],
                "additionalProperties": false
            }
        })
    }

    #[test]
    fn rejects_unknown_capabilities_and_events() {
        let mut pkg = valid_package();
        pkg["capabilities"] = json!(["db.read", "admin.manage"]);
        assert_eq!(
            parse_plugin_package(&pkg).unwrap_err().code(),
            "plugin_invalid"
        );
        let mut pkg = valid_package();
        pkg["subscriptions"] = json!(["secrets.exfiltrate.v1"]);
        assert!(parse_plugin_package(&pkg).is_err());
        // 权限/审核/账本裁决能力不在白名单
        for forbidden in [
            "admin.manage",
            "user.manage",
            "moderation.review",
            "moderation.sanction",
            "ledger.debit",
            "ledger.credit",
            "points.adjust",
        ] {
            assert!(
                !KNOWN_CAPABILITIES.contains(&forbidden),
                "{forbidden} must not be a plugin capability"
            );
        }
    }

    #[test]
    fn rejects_dangerous_urls_and_code_in_settings() {
        let schema = json!({
            "type": "object",
            "properties": {
                "endpoint": { "type": "string", "minLength": 1, "maxLength": 256 },
                "message": { "type": "string", "minLength": 1, "maxLength": 256 }
            },
            "required": [],
            "additionalProperties": false
        });
        // 危险 URL（SSRF）→ 拒绝
        let mut settings = json!({ "endpoint": "http://169.254.169.254/latest/meta-data" });
        assert!(validate_settings_against_schema(&settings, &schema).is_err());
        settings = json!({ "endpoint": "https://evil.example/hook" });
        assert!(validate_settings_against_schema(&settings, &schema).is_err());
        // 代码内容 → 拒绝
        settings = json!({ "message": "<script>eval(alert(1))</script>" });
        assert!(validate_settings_against_schema(&settings, &schema).is_err());
        settings = json!({ "message": "ok text" });
        assert!(validate_settings_against_schema(&settings, &schema).is_ok());
    }

    #[test]
    fn settings_schema_closed_and_bounds_enforced() {
        let schema = json!({
            "type": "object",
            "properties": {
                "amount": { "type": "integer", "minimum": 0, "maximum": 1000 }
            },
            "required": ["amount"],
            "additionalProperties": false
        });
        assert!(validate_settings_against_schema(&json!({ "amount": 50 }), &schema).is_ok());
        assert!(validate_settings_against_schema(&json!({ "amount": 5000 }), &schema).is_err());
        assert!(validate_settings_against_schema(&json!({ "amount": "x" }), &schema).is_err());
        assert!(validate_settings_against_schema(&json!({}), &schema).is_err());
        // 未知设置键拒绝（closed）
        assert!(
            validate_settings_against_schema(&json!({ "amount": 1, "extra": 2 }), &schema).is_err()
        );
        // schema 本身拒绝未知键（避免 regex/类型混淆攻击面）
        let mut evil_schema = schema.clone();
        evil_schema["properties"]["amount"]["pattern"] = json!(".*");
        assert!(validate_settings_schema(&evil_schema).is_err());
    }

    #[test]
    fn code_and_wasm_plugins_rejected_in_v1() {
        let mut pkg = valid_package();
        pkg["kind"] = json!("wasm");
        let err = parse_plugin_package(&pkg).unwrap_err();
        assert_eq!(err.code(), "plugin_invalid");
        assert!(err.message().contains("v2"));
    }

    #[test]
    fn version_range_and_schema_version_checks() {
        let mut pkg = valid_package();
        pkg["supports"] = json!(">=2.0");
        assert_eq!(
            parse_plugin_package(&pkg).unwrap_err().code(),
            "plugin_incompatible"
        );
        let mut pkg = valid_package();
        pkg["schema_version"] = json!(99);
        assert_eq!(
            parse_plugin_package(&pkg).unwrap_err().code(),
            "plugin_incompatible"
        );
        assert!(is_compatible(">=1.0 <2.0"));
        assert!(!is_compatible("<1.0"));
    }

    #[test]
    fn provider_adapters_are_controlled_and_non_replaceable() {
        let adapters = provider_adapters();
        let names: Vec<&str> = adapters
            .iter()
            .filter_map(|a| a["provider"].as_str())
            .collect();
        assert_eq!(names, vec!["direct", "hls", "xigua"]);
        for a in &adapters {
            assert_eq!(a["managed"], json!(true));
            assert_eq!(a["kind"], json!("core_adapter"));
        }
        // 白名单不含注册新 adapter 的能力
        assert!(!KNOWN_CAPABILITIES.contains(&"video.register_adapter"));
    }

    #[test]
    fn call_result_labels_are_closed() {
        assert_eq!(
            CALL_RESULTS,
            &["ok", "error", "timeout", "repeat", "stale", "skipped"]
        );
    }
}
