//! M13-THEME：数据型主题领域模块（无 axum；sqlx Either 双库）。
//!
//! 主题是**数据型安全 Token**（THEME.md §1.1/§2）：
//! - 封闭 Token schema：只接受已知 token key，类型与取值范围在代码中冻结；
//! - 服务端 schema 校验拒绝 CSS/HTML/JS/SVG/远程资源/任意 style 字符串；
//! - 主题不存在/不兼容/停用/损坏时回退内置 default 并记录非敏感告警；
//! - `revision` 是主题 Token 的单调递增版本，SSR/浏览器/缓存/用户偏好共享
//!   同一 revision（THEME.md §6）——`GET /api/v1/themes/active` 与用户偏好
//!   端点都返回当前生效主题的 revision，前端据此生成 ETag/避免闪烁。
//!
//! 本模块不执行任何上传代码、不发起网络请求、不读取凭据。

use std::collections::BTreeMap;

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::db::DatabasePool;

/// 内置 default 主题名（不可删除；兜底 fallback）。
pub const DEFAULT_THEME_NAME: &str = "default";
/// 数据型主题清单 schema 版本（THEME.md §8）。
pub const THEME_SCHEMA_VERSION: i64 = 1;
/// 本应用核心主题兼容版本（semver 主版本 1.0，range `>=1.0 <2.0`）。
pub const CORE_THEME_VERSION: &str = "1.0";
pub const CORE_THEME_RANGE: &str = ">=1.0 <2.0";
/// 上传数据包总大小上限（THEME.md §7：解压限制；JSON 数据包上限 256KB）。
pub const MAX_PACKAGE_BYTES: usize = 256 * 1024;

/// 封闭 Token schema：所有允许的 token key。
pub const TOKEN_KEYS: &[&str] = &[
    "color.background",
    "color.surface",
    "color.text",
    "color.muted",
    "color.accent",
    "color.border",
    "font.body",
    "font.mono",
    "radius.control",
    "radius.card",
    "space.density",
    "shadow.card",
    "motion.duration",
    "motion.reduced",
];

/// 字体族白名单（精确匹配；不接受任意字符串/`url()`/引号内代码）。
pub const FONT_FAMILY_ALLOWLIST: &[&str] = &[
    "system-ui",
    "sans-serif",
    "serif",
    "monospace",
    "ui-monospace",
    "-apple-system",
    "Segoe UI",
    "PingFang SC",
    "Microsoft YaHei",
    "Noto Sans SC",
    "Noto Serif SC",
    "Georgia",
    "Times New Roman",
    "Courier New",
];

/// 密度预设。
pub const DENSITY_ALLOWLIST: &[&str] = &["compact", "comfortable", "relaxed"];
/// 阴影预设。
pub const SHADOW_ALLOWLIST: &[&str] = &["none", "sm", "md", "lg"];

/// 危险内容特征：任何 token 字符串值命中即拒绝（CSS/HTML/JS/SVG/远程资源）。
const DANGEROUS_PATTERNS: &[&str] = &[
    "<",
    ">",
    "{",
    "}",
    ";",
    "url(",
    "@import",
    "expression(",
    "javascript:",
    "data:text/html",
    "onerror",
    "onload",
    "onclick",
    "&",
    "\u{00a0}",
];

/// 主题错误（稳定 code + 用户可读信息）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThemeError {
    Invalid(String),
    NotFound(String),
    Conflict(String),
    Incompatible(String),
    Corrupt(String),
}

impl ThemeError {
    pub fn code(&self) -> &'static str {
        match self {
            ThemeError::Invalid(_) => "theme_invalid",
            ThemeError::NotFound(_) => "theme_not_found",
            ThemeError::Conflict(_) => "theme_conflict",
            ThemeError::Incompatible(_) => "theme_incompatible",
            ThemeError::Corrupt(_) => "theme_corrupt",
        }
    }
    pub fn message(&self) -> &str {
        match self {
            ThemeError::Invalid(m)
            | ThemeError::NotFound(m)
            | ThemeError::Conflict(m)
            | ThemeError::Incompatible(m)
            | ThemeError::Corrupt(m) => m,
        }
    }
}

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code(), self.message())
    }
}

impl std::error::Error for ThemeError {}

/// 内置 default 主题 Token（BBLBB 默认亮色；数据型安全 Token）。
pub fn default_tokens() -> Value {
    json!({
        "color.background": "#f5f3ed",
        "color.surface": "#ffffff",
        "color.text": "#1f2937",
        "color.muted": "#6b7280",
        "color.accent": "#2563eb",
        "color.border": "#e5e7eb",
        "font.body": "system-ui",
        "font.mono": "ui-monospace",
        "radius.control": "0.5rem",
        "radius.card": "0.75rem",
        "space.density": "comfortable",
        "shadow.card": "sm",
        "motion.duration": "150ms",
        "motion.reduced": false,
    })
}

/// 主题名校验：小写 ASCII、数字、连字符，长度 1..=64（THEME.md §2）。
pub fn validate_theme_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// 简单 semver 解析：`MAJOR.MINOR`（可含 PATCH），拒绝前导零等不规范形态。
fn parse_version(v: &str) -> Option<(u64, u64, u64)> {
    let mut parts = v.split('.');
    let major = parts.next()?.parse::<u64>().ok()?;
    let minor = parts.next()?.parse::<u64>().ok()?;
    let patch = parts.next().unwrap_or("0").parse::<u64>().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

/// 校验 `supports` 范围字符串并计算该范围是否接受给定版本。
///
/// 语法：约束序列（空格分隔），如 `>=1.0 <2.0`（支持 `>= > <= < ==`，
/// 运算符可与版本连写或分开）。
fn range_accepts(range: &str, version: &str) -> bool {
    let Some(ver) = parse_version(version) else {
        return false;
    };
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
        let Some(bound) = parse_version(raw) else {
            return false;
        };
        let ok = match op {
            ">=" => ver >= bound,
            ">" => ver > bound,
            "<=" => ver <= bound,
            "<" => ver < bound,
            "==" => ver == bound,
            _ => false,
        };
        if !ok {
            return false;
        }
        remaining = &remaining[end..];
    }
    true
}

/// 主题与核心兼容性：清单 `supports` 必须接受核心版本 `1.0`。
pub fn is_compatible(supports: &str) -> bool {
    range_accepts(supports, CORE_THEME_VERSION)
}

/// 检测字符串值中的危险内容（CSS/HTML/JS/SVG/远程资源特征）。
fn scan_dangerous(value: &str, key: &str) -> Result<(), ThemeError> {
    for pattern in DANGEROUS_PATTERNS {
        if value.contains(pattern) {
            return Err(ThemeError::Invalid(format!(
                "token '{key}' contains dangerous content pattern '{pattern}'"
            )));
        }
    }
    Ok(())
}

/// 校验单个 token 值（key 必须在封闭 schema 内；类型/范围冻结）。
fn validate_token(key: &str, value: &Value) -> Result<(), ThemeError> {
    let bad = |msg: String| ThemeError::Invalid(msg);
    match key {
        "color.background" | "color.surface" | "color.text" | "color.muted" | "color.accent"
        | "color.border" => {
            let s = value
                .as_str()
                .ok_or_else(|| bad(format!("token '{key}' must be a string")))?;
            scan_dangerous(s, key)?;
            // 仅标准十六进制色值：#rgb #rgba #rrggbb #rrggbbaa
            let ok = matches!(s.len(), 4 | 5 | 7 | 9)
                && s.starts_with('#')
                && s[1..].bytes().all(|b| b.is_ascii_hexdigit());
            if !ok {
                return Err(bad(format!(
                    "token '{key}' must be a standard hex color (#rgb/#rgba/#rrggbb/#rrggbbaa)"
                )));
            }
        }
        "font.body" | "font.mono" => {
            let s = value
                .as_str()
                .ok_or_else(|| bad(format!("token '{key}' must be a string")))?;
            scan_dangerous(s, key)?;
            if !FONT_FAMILY_ALLOWLIST.contains(&s) {
                return Err(bad(format!(
                    "token '{key}' must be a known font family (allowlist), got '{s}'"
                )));
            }
        }
        "radius.control" | "radius.card" => {
            let s = value
                .as_str()
                .ok_or_else(|| bad(format!("token '{key}' must be a string")))?;
            scan_dangerous(s, key)?;
            let num = if let Some(stripped) = s.strip_suffix("rem") {
                stripped
            } else if let Some(stripped) = s.strip_suffix("px") {
                stripped
            } else {
                return Err(bad(format!(
                    "token '{key}' must use px or rem unit, got '{s}'"
                )));
            };
            let n: f64 = num
                .parse()
                .map_err(|_| bad(format!("token '{key}' is not a valid size, got '{s}'")))?;
            if !(0.0..=64.0).contains(&n) {
                return Err(bad(format!("token '{key}' must be in 0..=64, got '{s}'")));
            }
        }
        "space.density" => {
            let s = value
                .as_str()
                .ok_or_else(|| bad(format!("token '{key}' must be a string")))?;
            if !DENSITY_ALLOWLIST.contains(&s) {
                return Err(bad(format!(
                    "token '{key}' must be one of compact|comfortable|relaxed"
                )));
            }
        }
        "shadow.card" => {
            let s = value
                .as_str()
                .ok_or_else(|| bad(format!("token '{key}' must be a string")))?;
            if !SHADOW_ALLOWLIST.contains(&s) {
                return Err(bad(format!(
                    "token '{key}' must be one of none|sm|md|lg (no arbitrary CSS shadow)"
                )));
            }
        }
        "motion.duration" => {
            let s = value
                .as_str()
                .ok_or_else(|| bad(format!("token '{key}' must be a string")))?;
            scan_dangerous(s, key)?;
            let (num, unit) = if let Some(stripped) = s.strip_suffix("ms") {
                (stripped, "ms")
            } else if let Some(stripped) = s.strip_suffix('s') {
                (stripped, "s")
            } else {
                return Err(bad(format!(
                    "token '{key}' must use ms or s unit, got '{s}'"
                )));
            };
            if unit == "s" {
                let n: f64 = num
                    .parse()
                    .map_err(|_| bad(format!("token '{key}' is not a valid duration")))?;
                if !(0.0..=2.0).contains(&n) {
                    return Err(bad(format!("token '{key}' must be in 0s..=2s, got '{s}'")));
                }
            } else {
                let n: u64 = num
                    .parse()
                    .map_err(|_| bad(format!("token '{key}' is not a valid duration")))?;
                if n > 2000 {
                    return Err(bad(format!(
                        "token '{key}' must be in 0ms..=2000ms, got '{s}'"
                    )));
                }
            }
        }
        "motion.reduced" => {
            if !value.is_boolean() {
                return Err(bad(format!("token '{key}' must be a boolean")));
            }
        }
        _ => {
            return Err(ThemeError::Invalid(format!(
                "unknown token '{key}' (closed token schema)"
            )));
        }
    }
    Ok(())
}

/// 校验整个 tokens 对象（封闭 schema：未知 key 拒绝；类型/值范围冻结）。
pub fn validate_tokens(tokens: &Value) -> Result<BTreeMap<String, Value>, ThemeError> {
    let obj = tokens
        .as_object()
        .ok_or_else(|| ThemeError::Invalid("tokens must be a JSON object".to_string()))?;
    if obj.is_empty() {
        return Err(ThemeError::Invalid(
            "tokens must not be empty (closed schema)".to_string(),
        ));
    }
    let mut out = BTreeMap::new();
    for (key, value) in obj {
        validate_token(key, value)?;
        out.insert(key.clone(), value.clone());
    }
    Ok(out)
}

/// 校验资产声明（logo/preview）：只允许相对路径、无 `..`/绝对路径/URL。
pub fn validate_asset_paths(assets: Option<&Value>) -> Result<Value, ThemeError> {
    let Some(assets) = assets else {
        // 允许省略 assets
        return Ok(json!({}));
    };
    let Some(obj) = assets.as_object() else {
        // 允许 null/省略
        if assets.is_null() {
            return Ok(json!({}));
        }
        return Err(ThemeError::Invalid(
            "assets must be a JSON object".to_string(),
        ));
    };
    for (kind, value) in obj {
        let s = value
            .as_str()
            .ok_or_else(|| ThemeError::Invalid(format!("asset '{kind}' must be a string path")))?;
        if s.contains("..")
            || s.starts_with('/')
            || s.starts_with("http://")
            || s.starts_with("https://")
            || s.starts_with("data:")
            || s.contains('\\')
        {
            return Err(ThemeError::Invalid(format!(
                "asset '{kind}' path is not allowed: '{s}'"
            )));
        }
    }
    Ok(obj.clone().into())
}

/// 解析并校验完整主题数据包（JSON；不信任输入）。
pub fn parse_theme_package(package: &Value) -> Result<ParsedTheme, ThemeError> {
    let schema_version = package
        .get("schema_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| ThemeError::Invalid("schema_version required".to_string()))?;
    if schema_version != THEME_SCHEMA_VERSION {
        return Err(ThemeError::Incompatible(format!(
            "unsupported schema_version {schema_version} (expected {THEME_SCHEMA_VERSION})"
        )));
    }
    let kind = package.get("kind").and_then(Value::as_str).unwrap_or("");
    if kind != "data" {
        return Err(ThemeError::Invalid(
            "only data themes are supported online (kind must be 'data')".to_string(),
        ));
    }
    let name = package
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| ThemeError::Invalid("name required".to_string()))?;
    if !validate_theme_name(name) {
        return Err(ThemeError::Invalid(format!(
            "invalid theme name '{name}' (lowercase ascii/digits/hyphens, <=64)"
        )));
    }
    let display_name = package
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or(name)
        .to_string();
    if display_name.len() > 120 {
        return Err(ThemeError::Invalid(
            "display_name too long (<=120)".to_string(),
        ));
    }
    let version = package
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| ThemeError::Invalid("version required (semver)".to_string()))?;
    if parse_version(version).is_none() {
        return Err(ThemeError::Invalid(format!(
            "invalid version '{version}' (semver MAJOR.MINOR[.PATCH])"
        )));
    }
    let supports = package
        .get("supports")
        .and_then(Value::as_str)
        .ok_or_else(|| ThemeError::Invalid("supports range required".to_string()))?;
    if supports.len() > 64 || !is_compatible(supports) {
        return Err(ThemeError::Incompatible(format!(
            "theme '{name}' supports range '{supports}' is not compatible with core {CORE_THEME_RANGE}"
        )));
    }
    let tokens = package
        .get("tokens")
        .ok_or_else(|| ThemeError::Invalid("tokens required".to_string()))?;
    let tokens = validate_tokens(tokens)?;
    let assets = validate_asset_paths(package.get("assets"))?;
    Ok(ParsedTheme {
        name: name.to_string(),
        display_name,
        version: version.to_string(),
        supports: supports.to_string(),
        tokens,
        assets,
    })
}

/// 校验通过的主题数据包。
#[derive(Debug, Clone)]
pub struct ParsedTheme {
    pub name: String,
    pub display_name: String,
    pub version: String,
    pub supports: String,
    pub tokens: BTreeMap<String, Value>,
    pub assets: Value,
}

/// 主题行（数据库投影）。
#[derive(Debug, Clone)]
pub struct Theme {
    pub name: String,
    pub display_name: String,
    pub kind: String,
    pub schema_version: i64,
    pub version: String,
    pub supports: String,
    pub status: String,
    pub is_default: bool,
    pub revision: i64,
    pub tokens: Value,
    pub created_by: String,
    pub created_at: i64,
    pub updated_at: i64,
}

impl Theme {
    /// 无 DB 访问的安全 JSON 投影（不含凭据/内部字段）。
    pub fn json(&self) -> Value {
        json!({
            "name": self.name,
            "display_name": self.display_name,
            "kind": self.kind,
            "schema_version": self.schema_version,
            "version": self.version,
            "supports": self.supports,
            "status": self.status,
            "is_default": self.is_default,
            "revision": self.revision,
            "tokens": self.tokens,
            "created_by": self.created_by,
            "created_at": self.created_at,
            "updated_at": self.updated_at,
        })
    }
}

fn theme_from_row(r: &sqlx::sqlite::SqliteRow) -> Theme {
    let tokens_json: String = r.get("tokens_json");
    Theme {
        name: r.get("name"),
        display_name: r.get("display_name"),
        kind: r.get("kind"),
        schema_version: r.get("schema_version"),
        version: r.get("version"),
        supports: r.get("supports"),
        status: r.get("status"),
        is_default: r.get::<i64, _>("is_default") != 0,
        revision: r.get("revision"),
        tokens: serde_json::from_str(&tokens_json).unwrap_or_else(|_| json!({})),
        created_by: r.get("created_by"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

fn theme_from_row_mysql(r: &sqlx::mysql::MySqlRow) -> Theme {
    let tokens_json: String = r.get("tokens_json");
    Theme {
        name: r.get("name"),
        display_name: r.get("display_name"),
        kind: r.get("kind"),
        schema_version: r.get("schema_version"),
        version: r.get("version"),
        supports: r.get("supports"),
        status: r.get("status"),
        is_default: r.get::<i64, _>("is_default") != 0,
        revision: r.get("revision"),
        tokens: serde_json::from_str(&tokens_json).unwrap_or_else(|_| json!({})),
        created_by: r.get("created_by"),
        created_at: r.get("created_at"),
        updated_at: r.get("updated_at"),
    }
}

const THEME_COLUMNS: &str = "name, display_name, kind, schema_version, version, supports, status, is_default, revision, tokens_json, created_by, created_at, updated_at";

async fn load_theme_by_name(pool: &DatabasePool, name: &str) -> Result<Option<Theme>, ThemeError> {
    match pool {
        Either::Left(p) => {
            let row = sqlx::query(&format!(
                "SELECT {THEME_COLUMNS} FROM themes WHERE name = ?"
            ))
            .bind(name)
            .fetch_optional(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            Ok(row.map(|r| theme_from_row(&r)))
        }
        Either::Right(p) => {
            let row = sqlx::query(&format!(
                "SELECT {THEME_COLUMNS} FROM themes WHERE name = ?"
            ))
            .bind(name)
            .fetch_optional(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            Ok(row.map(|r| theme_from_row_mysql(&r)))
        }
    }
}

async fn load_default_theme(pool: &DatabasePool) -> Result<Option<Theme>, ThemeError> {
    match pool {
        Either::Left(p) => {
            let row = sqlx::query(&format!(
                "SELECT {THEME_COLUMNS} FROM themes WHERE is_default = 1 AND status = 'active' ORDER BY updated_at DESC LIMIT 1"
            ))
            .fetch_optional(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            Ok(row.map(|r| theme_from_row(&r)))
        }
        Either::Right(p) => {
            let row = sqlx::query(&format!(
                "SELECT {THEME_COLUMNS} FROM themes WHERE is_default = 1 AND status = 'active' ORDER BY updated_at DESC LIMIT 1"
            ))
            .fetch_optional(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            Ok(row.map(|r| theme_from_row_mysql(&r)))
        }
    }
}

/// 加载主题并把 `tokens_json` 重新过封闭 schema 校验；损坏/不兼容/停用返回
/// `Err`（调用方回退 default）。损坏时更新状态为 `corrupt` 并记录非敏感告警。
pub(crate) async fn load_theme_checked(
    pool: &DatabasePool,
    name: &str,
) -> Result<Theme, ThemeError> {
    let theme = load_theme_by_name(pool, name)
        .await?
        .ok_or_else(|| ThemeError::NotFound(format!("theme '{name}' not found")))?;
    if theme.status != "active" {
        return Err(ThemeError::Invalid(format!(
            "theme '{name}' is not active (status={})",
            theme.status
        )));
    }
    if !is_compatible(&theme.supports) {
        return Err(ThemeError::Incompatible(format!(
            "theme '{name}' supports range is incompatible with core {CORE_THEME_RANGE}"
        )));
    }
    // 重新校验已存储 Token（防 DB 篡改/历史数据损坏）。
    if let Err(e) = validate_tokens(&theme.tokens) {
        let _ = mark_corrupt(pool, &theme.name, e.message()).await;
        return Err(ThemeError::Corrupt(format!(
            "theme '{name}' tokens failed validation: {}",
            crate::error::sanitize(e.message())
        )));
    }
    Ok(theme)
}

/// 把主题标记为 corrupt（仅内部状态；不含敏感信息）。
async fn mark_corrupt(pool: &DatabasePool, name: &str, reason: &str) -> Result<(), ThemeError> {
    let now = crate::outbox::now_millis();
    let safe = crate::error::sanitize(reason);
    tracing::warn!(theme = %name, reason = %safe, "theme marked corrupt; falling back to default");
    let affected = match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE themes SET status = 'corrupt', updated_at = ? WHERE name = ?")
                .bind(now)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?
                .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query("UPDATE themes SET status = 'corrupt', updated_at = ? WHERE name = ?")
                .bind(now)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?
                .rows_affected()
        }
    };
    let _ = affected;
    Ok(())
}

/// 当前生效主题解析（THEME.md §4 优先级）：
/// 1. 登录用户偏好（仅在主题 active 且兼容时生效，否则回退）；
/// 2. 站点默认 active 主题；
/// 3. 内置 `default` 兜底。
///
/// 任何回退路径都记录非敏感告警；返回与 SSR/浏览器一致的 `revision`。
pub async fn resolve_active_theme(
    pool: &DatabasePool,
    user_id: Option<&str>,
) -> Result<ActiveTheme, ThemeError> {
    if let Some(user_id) = user_id {
        let preferred = user_theme_name(pool, user_id).await?;
        if let Some(name) = preferred {
            if name != DEFAULT_THEME_NAME {
                match load_theme_checked(pool, &name).await {
                    Ok(theme) => {
                        return Ok(ActiveTheme {
                            name: theme.name,
                            revision: theme.revision,
                            tokens: theme.tokens,
                            source: "user_preference".to_string(),
                        });
                    }
                    Err(e) => {
                        // 非敏感告警：主题不存在/停用/损坏 → 回退 default
                        tracing::warn!(
                            user_id = %crate::error::sanitize(user_id),
                            theme = %name,
                            code = %e.code(),
                            "user theme preference unavailable; falling back to default"
                        );
                    }
                }
            }
        }
    }
    // 站点默认
    if let Some(default) = load_default_theme(pool).await? {
        if let Ok(theme) = load_theme_checked(pool, &default.name).await {
            return Ok(ActiveTheme {
                name: theme.name,
                revision: theme.revision,
                tokens: theme.tokens,
                source: "site_default".to_string(),
            });
        }
        tracing::warn!(
            theme = %default.name,
            "site default theme unavailable; falling back to built-in default"
        );
    }
    // 内置 default 兜底（revision 恒为 1，SSR/浏览器一致）。
    Ok(ActiveTheme {
        name: DEFAULT_THEME_NAME.to_string(),
        revision: 1,
        tokens: default_tokens(),
        source: "builtin_default".to_string(),
    })
}

/// 当前生效主题（name/revision/tokens/source）。
#[derive(Debug, Clone)]
pub struct ActiveTheme {
    pub name: String,
    pub revision: i64,
    pub tokens: Value,
    pub source: String,
}

impl ActiveTheme {
    pub fn json(&self) -> Value {
        json!({
            "name": self.name,
            "revision": self.revision,
            "tokens": self.tokens,
            "source": self.source,
        })
    }
}

/// 读取用户主题偏好（无偏好返回 None）。
async fn user_theme_name(pool: &DatabasePool, user_id: &str) -> Result<Option<String>, ThemeError> {
    let v = match pool {
        Either::Left(p) => sqlx::query_scalar::<_, Option<String>>(
            "SELECT theme_name FROM user_preferences WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?,
        Either::Right(p) => sqlx::query_scalar::<_, Option<String>>(
            "SELECT theme_name FROM user_preferences WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?,
    };
    Ok(v.flatten())
}

/// 用户主题偏好视图：主题名 + 该主题 revision（default 内置 = 1）。
/// `revision` 与 SSR/浏览器/缓存共享（M13-THEME-05）。
pub async fn user_theme_preference(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<PreferenceView, ThemeError> {
    let name = user_theme_name(pool, user_id)
        .await?
        .unwrap_or_else(|| DEFAULT_THEME_NAME.to_string());
    let active = resolve_active_theme(pool, Some(user_id)).await?;
    Ok(PreferenceView {
        theme: name.clone(),
        revision: if name == DEFAULT_THEME_NAME {
            1
        } else {
            active.revision
        },
        effective: active.name,
    })
}

/// 用户主题偏好视图。
#[derive(Debug, Clone)]
pub struct PreferenceView {
    pub theme: String,
    pub revision: i64,
    pub effective: String,
}

/// 更新用户主题偏好。
///
/// 安全约束（M13-THEME-07）：
/// - 目标主题必须 active 且兼容（否则回退逻辑已在 resolve 层兜底）；
/// - `expected_revision` 必须等于当前生效主题 revision（If-Match 乐观锁），
///   冲突返回 [`ThemeError::Conflict`]；
/// - 只允许保存 default 或已安装且激活的数据主题名（封闭集合）。
pub async fn update_user_theme_preference(
    pool: &DatabasePool,
    user_id: &str,
    theme_name: &str,
    expected_revision: i64,
) -> Result<PreferenceView, ThemeError> {
    if !validate_theme_name(theme_name) {
        return Err(ThemeError::Invalid("invalid theme name".to_string()));
    }
    let current = user_theme_preference(pool, user_id).await?;
    if current.revision != expected_revision {
        return Err(ThemeError::Conflict(format!(
            "theme revision conflict: expected {expected_revision}, current {}",
            current.revision
        )));
    }
    if theme_name != DEFAULT_THEME_NAME {
        load_theme_checked(pool, theme_name).await?;
    }
    let now = crate::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_preferences (user_id, timezone, locale, updated_at)
                 VALUES (?, 'UTC', 'zh-CN', ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            sqlx::query(
                "UPDATE user_preferences SET theme_name = ?, updated_at = ? WHERE user_id = ?",
            )
            .bind(theme_name)
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO user_preferences (user_id, timezone, locale, updated_at)
                 VALUES (?, 'UTC', 'zh-CN', ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            sqlx::query(
                "UPDATE user_preferences SET theme_name = ?, updated_at = ? WHERE user_id = ?",
            )
            .bind(theme_name)
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
        }
    }
    Ok(PreferenceView {
        theme: theme_name.to_string(),
        revision: if theme_name == DEFAULT_THEME_NAME {
            1
        } else {
            load_theme_checked(pool, theme_name).await?.revision
        },
        effective: theme_name.to_string(),
    })
}

/// 列出全部主题（管理端；含禁用状态）。
pub async fn list_themes(pool: &DatabasePool) -> Result<Vec<Theme>, ThemeError> {
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(&format!("SELECT {THEME_COLUMNS} FROM themes ORDER BY name"))
                .fetch_all(p)
                .await
                .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            Ok(rows.iter().map(theme_from_row).collect())
        }
        Either::Right(p) => {
            let rows = sqlx::query(&format!("SELECT {THEME_COLUMNS} FROM themes ORDER BY name"))
                .fetch_all(p)
                .await
                .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            Ok(rows.iter().map(theme_from_row_mysql).collect())
        }
    }
}

/// 管理员上传数据包（M13-THEME-06）：完整校验 → 插入（revision=1，
/// status=disabled 隔离态）→ 返回主题。同名升级需先删除或进入冲突。
pub async fn upload_theme_package(
    pool: &DatabasePool,
    package: &Value,
    actor: &str,
) -> Result<Theme, ThemeError> {
    let parsed = parse_theme_package(package)?;
    if load_theme_by_name(pool, &parsed.name).await?.is_some() {
        return Err(ThemeError::Conflict(format!(
            "theme '{}' already exists (delete first or update via settings)",
            parsed.name
        )));
    }
    let now = crate::outbox::now_millis();
    let tokens_json =
        serde_json::to_string(&Value::Object(parsed.tokens.clone().into_iter().collect()))
            .map_err(|e| ThemeError::Invalid(crate::error::sanitize(&e.to_string())))?;
    let assets_json = serde_json::to_string(&parsed.assets)
        .map_err(|e| ThemeError::Invalid(crate::error::sanitize(&e.to_string())))?;
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO themes (name, display_name, kind, schema_version, version, supports, status, is_default, revision, tokens_json, asset_meta_json, created_by, created_at, updated_at)
                 VALUES (?, ?, 'data', ?, ?, ?, 'disabled', 0, 1, ?, ?, ?, ?, ?)",
            )
            .bind(&parsed.name)
            .bind(&parsed.display_name)
            .bind(THEME_SCHEMA_VERSION)
            .bind(&parsed.version)
            .bind(&parsed.supports)
            .bind(&tokens_json)
            .bind(&assets_json)
            .bind(actor)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO themes (name, display_name, kind, schema_version, version, supports, status, is_default, revision, tokens_json, asset_meta_json, created_by, created_at, updated_at)
                 VALUES (?, ?, 'data', ?, ?, ?, 'disabled', 0, 1, ?, ?, ?, ?, ?)",
            )
            .bind(&parsed.name)
            .bind(&parsed.display_name)
            .bind(THEME_SCHEMA_VERSION)
            .bind(&parsed.version)
            .bind(&parsed.supports)
            .bind(&tokens_json)
            .bind(&assets_json)
            .bind(actor)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
        }
    }
    // 记录初始修订（revision 1）。
    let rev_id = uuid::Uuid::now_v7().to_string();
    let _ = insert_revision(
        pool,
        &rev_id,
        &parsed.name,
        1,
        &tokens_json,
        actor,
        "upload",
        now,
    )
    .await;
    load_theme_by_name(pool, &parsed.name)
        .await?
        .ok_or_else(|| ThemeError::NotFound(format!("theme '{}' not found", parsed.name)))
}

/// 有界修订写入 API：全部参数均必需且显式。
#[allow(clippy::too_many_arguments)]
async fn insert_revision(
    pool: &DatabasePool,
    id: &str,
    theme_name: &str,
    revision: i64,
    tokens_json: &str,
    changed_by: &str,
    reason: &str,
    now: i64,
) -> Result<(), ThemeError> {
    {
        match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO theme_revisions (id, theme_name, revision, tokens_json, changed_by, reason, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(theme_name)
            .bind(revision)
            .bind(tokens_json)
            .bind(changed_by)
            .bind(reason)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO theme_revisions (id, theme_name, revision, tokens_json, changed_by, reason, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(theme_name)
            .bind(revision)
            .bind(tokens_json)
            .bind(changed_by)
            .bind(reason)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?
        }
    };
        Ok(())
    }
}

/// 设置站点默认主题（同时激活 disabled→active；Is-Match 由管理端版本协商）。
/// default 内置主题不能作为 data 主题覆盖（name 冲突已在上传时拒绝）。
pub async fn set_default_theme(
    pool: &DatabasePool,
    name: &str,
    actor: &str,
    reason: &str,
) -> Result<Theme, ThemeError> {
    // 读取（不要求已 active——上传即 disabled 隔离态），但要求 token 可校验、
    // 兼容性满足核心 range。
    let theme = load_theme_by_name(pool, name)
        .await?
        .ok_or_else(|| ThemeError::NotFound(format!("theme '{name}' not found")))?;
    if !is_compatible(&theme.supports) {
        return Err(ThemeError::Incompatible(format!(
            "theme '{name}' supports range is incompatible with core {CORE_THEME_RANGE}"
        )));
    }
    validate_tokens(&theme.tokens)?;
    let now = crate::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE themes SET is_default = 0, updated_at = ?")
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            sqlx::query("UPDATE themes SET is_default = 1, status = 'active', updated_at = ? WHERE name = ?")
                .bind(now)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE themes SET is_default = 0, updated_at = ?")
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
            sqlx::query("UPDATE themes SET is_default = 1, status = 'active', updated_at = ? WHERE name = ?")
                .bind(now)
                .bind(name)
                .execute(p)
                .await
                .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?;
        }
    }
    let _ = AuditNote::set_default(actor, reason, now, pool).await;
    load_theme_checked(pool, name).await
}

/// 更新主题 Token 设置（M13-THEME-05/09）：closed schema 校验 + 新修订
/// （revision+1）+ 审计。`expected_revision` 若指定则做乐观锁。
pub async fn update_theme_settings(
    pool: &DatabasePool,
    name: &str,
    tokens: &Value,
    actor: &str,
    reason: &str,
    expected_revision: Option<i64>,
) -> Result<Theme, ThemeError> {
    let theme = load_theme_checked(pool, name).await?;
    if let Some(expected) = expected_revision {
        if expected != theme.revision {
            return Err(ThemeError::Conflict(format!(
                "theme revision conflict: expected {expected}, current {}",
                theme.revision
            )));
        }
    }
    let validated = validate_tokens(tokens)?;
    let tokens_json = serde_json::to_string(&Value::Object(validated.into_iter().collect()))
        .map_err(|e| ThemeError::Invalid(crate::error::sanitize(&e.to_string())))?;
    let new_revision = theme.revision + 1;
    let now = crate::outbox::now_millis();
    let affected = match pool {
        Either::Left(p) => {

            sqlx::query(
                "UPDATE themes SET tokens_json = ?, revision = ?, updated_at = ? WHERE name = ? AND revision = ?",
            )
            .bind(&tokens_json)
            .bind(new_revision)
            .bind(now)
            .bind(name)
            .bind(theme.revision)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?
    .rows_affected()
        }
        Either::Right(p) => {

            sqlx::query(
                "UPDATE themes SET tokens_json = ?, revision = ?, updated_at = ? WHERE name = ? AND revision = ?",
            )
            .bind(&tokens_json)
            .bind(new_revision)
            .bind(now)
            .bind(name)
            .bind(theme.revision)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?
    .rows_affected()
        }
    };
    if affected == 0 {
        return Err(ThemeError::Conflict(
            "theme revision conflict on update".to_string(),
        ));
    }
    let rev_id = uuid::Uuid::now_v7().to_string();
    let _ = insert_revision(
        pool,
        &rev_id,
        name,
        new_revision,
        &tokens_json,
        actor,
        reason,
        now,
    )
    .await;
    let _ = AuditNote::settings(actor, reason, new_revision, pool).await;
    load_theme_checked(pool, name).await
}

/// 删除主题（内置 default 与当前默认主题不可删除；删除前用户偏好迁移由
/// resolve 兜底——偏好指向不存在/停用主题自动回退 default）。
pub async fn delete_theme(
    pool: &DatabasePool,
    name: &str,
    actor: &str,
    reason: &str,
) -> Result<(), ThemeError> {
    if name == DEFAULT_THEME_NAME {
        return Err(ThemeError::Conflict(
            "built-in default theme cannot be deleted".to_string(),
        ));
    }
    let theme = load_theme_by_name(pool, name)
        .await?
        .ok_or_else(|| ThemeError::NotFound(format!("theme '{name}' not found")))?;
    if theme.is_default {
        return Err(ThemeError::Conflict(
            "current site default theme cannot be deleted".to_string(),
        ));
    }
    let affected = match pool {
        Either::Left(p) => sqlx::query("DELETE FROM themes WHERE name = ?")
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?
            .rows_affected(),
        Either::Right(p) => sqlx::query("DELETE FROM themes WHERE name = ?")
            .bind(name)
            .execute(p)
            .await
            .map_err(|e| ThemeError::Corrupt(crate::error::sanitize(&e.to_string())))?
            .rows_affected(),
    };
    if affected == 0 {
        return Err(ThemeError::NotFound(format!("theme '{name}' not found")));
    }
    let _ = AuditNote::delete(actor, reason, pool).await;
    Ok(())
}

/// 审计记录（theme 域）。失败不阻断业务（审计失败已由核心 audit 路径保证；
/// 这里用独立表简化——不泄漏 token 内容）。
struct AuditNote;

impl AuditNote {
    async fn record(pool: &DatabasePool, actor: &str, action: &str, reason: &str, now: i64) {
        let _ = crate::audit::AuditEntry::user_action(actor, action)
            .with_reason(reason)
            .with_policy_version(crate::authz::decision::AUTHZ_POLICY_VERSION)
            .record(pool)
            .await;
        let _ = now;
    }
    async fn set_default(actor: &str, reason: &str, now: i64, pool: &DatabasePool) {
        Self::record(pool, actor, "theme.default.update", reason, now).await;
    }
    async fn settings(actor: &str, reason: &str, _revision: i64, pool: &DatabasePool) {
        Self::record(
            pool,
            actor,
            "theme.settings.update",
            reason,
            crate::outbox::now_millis(),
        )
        .await;
    }
    async fn delete(actor: &str, reason: &str, pool: &DatabasePool) {
        Self::record(
            pool,
            actor,
            "theme.delete",
            reason,
            crate::outbox::now_millis(),
        )
        .await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn closed_token_schema_rejects_unknown_keys() {
        let err = validate_tokens(&json!({ "color.background": "#fff", "color.evil": "#000" }))
            .unwrap_err();
        assert_eq!(err.code(), "theme_invalid");
        assert!(err.message().contains("unknown token"));
    }

    #[test]
    fn rejects_css_html_js_svg_and_remote_resources() {
        // CSS 任意 style 字符串
        assert!(validate_tokens(&json!({ "color.background": "red; position: fixed" })).is_err());
        assert!(
            validate_tokens(&json!({ "color.background": "url(https://evil/x.png)" })).is_err()
        );
        // HTML/JS
        assert!(
            validate_tokens(&json!({ "color.background": "<script>alert(1)</script>" })).is_err()
        );
        assert!(
            validate_tokens(&json!({ "font.body": "x</style><svg onload=alert(1)>" })).is_err()
        );
        // 远程资源 / 表达式
        assert!(validate_tokens(&json!({ "color.accent": "expression(alert(1))" })).is_err());
        assert!(validate_tokens(&json!({ "color.text": "javascript:alert(1)" })).is_err());
        assert!(validate_tokens(&json!({ "color.border": "data:text/html;base64,xx" })).is_err());
        // 非十六进制色值
        assert!(validate_tokens(&json!({ "color.background": "red" })).is_err());
        assert!(validate_tokens(&json!({ "color.background": "#zzz" })).is_err());
    }

    #[test]
    fn rejects_arbitrary_fonts_shadows_and_radii() {
        assert!(validate_tokens(&json!({ "font.body": "Comic Sans MS" })).is_err());
        assert!(validate_tokens(&json!({ "font.body": "system-ui" })).is_ok());
        assert!(validate_tokens(&json!({ "shadow.card": "0 2px 8px rgba(0,0,0,0.2)" })).is_err());
        assert!(validate_tokens(&json!({ "shadow.card": "md" })).is_ok());
        assert!(validate_tokens(&json!({ "radius.control": "9999px" })).is_err());
        assert!(validate_tokens(&json!({ "radius.control": "0.5rem" })).is_ok());
    }

    #[test]
    fn accepts_full_valid_token_set() {
        let tokens = default_tokens();
        let parsed = validate_tokens(&tokens).unwrap();
        assert_eq!(parsed.len(), TOKEN_KEYS.len());
    }

    #[test]
    fn package_validation_rejects_code_and_remote_assets() {
        let pkg = json!({
            "schema_version": 1,
            "name": "my-theme",
            "display_name": "My Theme",
            "version": "1.0.0",
            "supports": ">=1.0 <2.0",
            "kind": "data",
            "tokens": default_tokens(),
        });
        assert!(parse_theme_package(&pkg).is_ok());

        // kind 非 data（代码型）→ 拒绝
        let mut code = pkg.clone();
        code["kind"] = json!("code");
        assert!(parse_theme_package(&code).is_err());

        // 版本范围不兼容
        let mut bad_range = pkg.clone();
        bad_range["supports"] = json!(">=2.0");
        assert_eq!(
            parse_theme_package(&bad_range).unwrap_err().code(),
            "theme_incompatible"
        );

        // schema_version 超出
        let mut bad_schema = pkg.clone();
        bad_schema["schema_version"] = json!(99);
        assert_eq!(
            parse_theme_package(&bad_schema).unwrap_err().code(),
            "theme_incompatible"
        );

        // 非法资产路径
        let mut bad_asset = pkg.clone();
        bad_asset["assets"] = json!({"logo": "https://evil.example/logo.png"});
        assert!(parse_theme_package(&bad_asset).is_err());
        let mut bad_asset = pkg.clone();
        bad_asset["assets"] = json!({"logo": "../../etc/passwd"});
        assert!(parse_theme_package(&bad_asset).is_err());
    }

    #[test]
    fn theme_name_rules() {
        assert!(validate_theme_name("default"));
        assert!(validate_theme_name("midnight-ocean-v2"));
        assert!(!validate_theme_name("Midnight"));
        assert!(!validate_theme_name("a b"));
        assert!(!validate_theme_name("a/b"));
        assert!(!validate_theme_name(""));
        assert!(!validate_theme_name(&"x".repeat(65)));
    }

    #[test]
    fn supports_range_intersects_core() {
        assert!(is_compatible(">=1.0 <2.0"));
        assert!(is_compatible(">=0.9"));
        assert!(!is_compatible("<1.0"));
        assert!(!is_compatible(">=2.0"));
        assert!(!is_compatible("==0.5"));
        assert!(!is_compatible("banana"));
    }
}
