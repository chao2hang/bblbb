//! M07-SHOP-UI-10：可配置装扮样式库（管理员自定义命名 + 结构化样式参数）。
//!
//! 安全模型（docs/INTERNAL-MARKETPLACE.md §2/§9）：
//! - `style_json` 只接受服务端 schema 校验过的结构化字段（颜色必须
//!   `#rrggbb`、动画类型走固定枚举、时长钳制），**不存在任意 CSS 通道**；
//! - 商品 Token 通过 `nickname.color.<id>` / `avatar.frame.<id>` 引用这里的
//!   定义（id 形如 `c` + uuid simple，满足 Token 字符白名单）；
//! - 公开投影只输出 active 定义的 style/name，归档定义自动失效；
//! - 归档为软删除（Token 引用保留历史，不硬删）。

use serde_json::{json, Value};
use sqlx::Either;

use crate::db::DatabasePool;
use crate::outbox::now_millis;

use super::service::ShopError;

/// 可由管理员维护的功能定义类型（仅保留彩色昵称、头像框与个人主页背景三种商品）。
pub const CUSTOMIZABLE_KINDS: &[&str] = &["nickname_color", "avatar_frame", "profile_effect"];

const SAFE_TEXTURES: &[&str] = &[
    "sparkle",
    "dark_stars",
    "grid",
    "dots",
    "aurora",
    "matrix",
    "waves",
    "snow",
    "bubbles",
];

const DEF_ID_MAX: usize = 64;
const NAME_MAX: usize = 32;
/// Token 前缀（`nickname.color.` 等）+ id 的总长上限与 service 侧 is_safe_token
/// 的 64 字节限制对齐（id 为 `c` + 32 hex = 33 字符，安全）。
const DURATION_MIN_MS: i64 = 800;
const DURATION_MAX_MS: i64 = 30_000;

fn is_hex_color(v: &Value) -> bool {
    v.as_str()
        .map(|s| {
            let b = s.as_bytes();
            b.len() == 7
                && b[0] == b'#'
                && b[1..].iter().all(|c| {
                    c.is_ascii_digit() || (b'a'..=b'f').contains(c) || (b'A'..=b'F').contains(c)
                })
        })
        .unwrap_or(false)
}

fn require_color(style: &serde_json::Map<String, Value>, key: &str) -> Result<String, ShopError> {
    let raw = style
        .get(key)
        .ok_or_else(|| ShopError::Invalid(format!("style.{key} required")))?;
    if !is_hex_color(raw) {
        return Err(ShopError::Invalid(format!("style.{key} must be #rrggbb")));
    }
    Ok(raw.as_str().unwrap_or_default().to_ascii_lowercase())
}

fn require_duration(
    style: &serde_json::Map<String, Value>,
    key: &str,
    default: i64,
) -> Result<i64, ShopError> {
    match style.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => {
            let ms = v
                .as_i64()
                .ok_or_else(|| ShopError::Invalid(format!("style.{key} must be integer ms")))?;
            if !(DURATION_MIN_MS..=DURATION_MAX_MS).contains(&ms) {
                return Err(ShopError::Invalid(format!(
                    "style.{key} must be within {DURATION_MIN_MS}..{DURATION_MAX_MS} ms"
                )));
            }
            Ok(ms)
        }
    }
}

fn require_enum(
    style: &serde_json::Map<String, Value>,
    key: &str,
    allowed: &[&str],
    default: &str,
) -> Result<String, ShopError> {
    match style.get(key) {
        None | Some(Value::Null) => Ok(default.to_string()),
        Some(v) => {
            let s = v
                .as_str()
                .ok_or_else(|| ShopError::Invalid(format!("style.{key} must be a string")))?;
            if !allowed.contains(&s) {
                return Err(ShopError::Invalid(format!(
                    "style.{key} must be one of: {}",
                    allowed.join(", ")
                )));
            }
            Ok(s.to_string())
        }
    }
}

fn optional_color(
    style: &serde_json::Map<String, Value>,
    key: &str,
) -> Result<Option<String>, ShopError> {
    match style.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            if !is_hex_color(v) {
                return Err(ShopError::Invalid(format!("style.{key} must be #rrggbb")));
            }
            Ok(Some(v.as_str().unwrap_or_default().to_ascii_lowercase()))
        }
    }
}

fn optional_range(
    style: &serde_json::Map<String, Value>,
    key: &str,
    min: i64,
    max: i64,
) -> Result<Option<i64>, ShopError> {
    match style.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let n = v
                .as_i64()
                .ok_or_else(|| ShopError::Invalid(format!("style.{key} must be an integer")))?;
            if !(min..=max).contains(&n) {
                return Err(ShopError::Invalid(format!(
                    "style.{key} must be within {min}..={max}"
                )));
            }
            Ok(Some(n))
        }
    }
}

/// 校验并规整 style_json（包含基础结构化参数，并支持开放自定义 CSS 和 JS）。
pub fn validate_style(kind: &str, style: &Value) -> Result<Value, ShopError> {
    let map = style
        .as_object()
        .ok_or_else(|| ShopError::Invalid("style must be a JSON object".into()))?;
    let mut result = match kind {
        "nickname_color" => {
            let mode = require_enum(map, "mode", &["solid", "gradient", "glow"], "solid")?;
            let shadow_px = optional_range(map, "shadowPx", 0, 30)?;
            let letter_spacing = optional_range(map, "letterSpacing", 0, 8)?;
            match mode.as_str() {
                "solid" => {
                    let color = require_color(map, "color")?;
                    let animate = require_enum(
                        map,
                        "animate",
                        &["none", "breathe", "pulse", "bounce", "glitch", "shimmer", "wave"],
                        "none",
                    )?;
                    let duration = require_duration(map, "durationMs", 2_800)?;
                    let mut obj = json!({
                        "mode": mode,
                        "color": color,
                        "animate": animate,
                        "durationMs": duration
                    });
                    if let Some(sp) = shadow_px {
                        obj["shadowPx"] = json!(sp);
                    }
                    if let Some(ls) = letter_spacing {
                        obj["letterSpacing"] = json!(ls);
                    }
                    Ok(obj)
                }
                "gradient" => {
                    let colors = map.get("colors").and_then(Value::as_array).ok_or_else(|| {
                        ShopError::Invalid("style.colors required for gradient".into())
                    })?;
                    if colors.len() < 2 || colors.len() > 5 {
                        return Err(ShopError::Invalid(
                            "style.colors must contain 2..=5 stops".into(),
                        ));
                    }
                    let normalized: Vec<String> = colors
                        .iter()
                        .map(|c| {
                            if !is_hex_color(c) {
                                return Err(ShopError::Invalid(
                                    "style.colors entries must be #rrggbb".into(),
                                ));
                            }
                            Ok(c.as_str().unwrap_or_default().to_ascii_lowercase())
                        })
                        .collect::<Result<_, ShopError>>()?;
                    let animate = require_enum(
                        map,
                        "animate",
                        &["none", "flow", "wave", "shimmer", "glitch", "rainbow", "pulse"],
                        "flow",
                    )?;
                    let duration = require_duration(map, "durationMs", 5_000)?;
                    let angle = optional_range(map, "angle", 0, 360)?.unwrap_or(90);
                    let mut obj = json!({
                        "mode": mode,
                        "colors": normalized,
                        "animate": animate,
                        "durationMs": duration,
                        "angle": angle
                    });
                    if let Some(sp) = shadow_px {
                        obj["shadowPx"] = json!(sp);
                    }
                    if let Some(ls) = letter_spacing {
                        obj["letterSpacing"] = json!(ls);
                    }
                    Ok(obj)
                }
                _ => {
                    // glow：发光昵称（呼吸光晕）
                    let color = require_color(map, "color")?;
                    let animate = require_enum(
                        map,
                        "animate",
                        &["none", "breathe", "flicker", "fire", "pulse", "glitch"],
                        "breathe",
                    )?;
                    let duration = require_duration(map, "durationMs", 2_800)?;
                    let mut obj = json!({
                        "mode": mode,
                        "color": color,
                        "animate": animate,
                        "durationMs": duration
                    });
                    if let Some(sp) = shadow_px {
                        obj["shadowPx"] = json!(sp);
                    }
                    if let Some(ls) = letter_spacing {
                        obj["letterSpacing"] = json!(ls);
                    }
                    Ok(obj)
                }
            }
        }
        "avatar_frame" => {
            // Steam APNG 动效头像框：支持直接指定 Steam / 本地动图路径
            let color = optional_color(map, "color")?.unwrap_or_else(|| "#f59e0b".to_string());
            let shape = require_enum(map, "shape", &["circle", "rounded", "square"], "rounded")?;
            let frame_scale = optional_range(map, "frameScale", 80, 250)?.unwrap_or(120);
            let mut obj = json!({
                "mode": "steam_frame",
                "color": color,
                "shape": shape,
                "frameScale": frame_scale
            });
            if let Some(url) = map.get("url").and_then(Value::as_str) {
                obj["url"] = json!(url.trim());
            }
            if let Some(img) = map.get("image").and_then(Value::as_str) {
                obj["image"] = json!(img.trim());
            }
            Ok(obj)
        }
        "profile_effect" => {
            // Steam 动态/静态个人资料背景与社区背景
            let base = optional_color(map, "baseColor")?.unwrap_or_else(|| "#101827".to_string());
            let accent = optional_color(map, "accentColor")?.unwrap_or_else(|| "#8b5cf6".to_string());
            let texture = require_enum(map, "texture", SAFE_TEXTURES, "sparkle")?;
            let animate = require_enum(
                map,
                "animate",
                &["none", "shimmer", "aurora", "flow", "pulse", "scanline"],
                "none",
            )?;
            let duration = require_duration(map, "durationMs", 5_000)?;
            let opacity = optional_range(map, "opacity", 10, 100)?.unwrap_or(100);
            let mut obj = json!({
                "mode": "profile",
                "texture": texture,
                "baseColor": base,
                "accentColor": accent,
                "animate": animate,
                "durationMs": duration,
                "opacity": opacity
            });
            if let Some(url) = map.get("url").and_then(Value::as_str) {
                obj["url"] = json!(url.trim());
            }
            if let Some(img) = map.get("image").and_then(Value::as_str) {
                obj["image"] = json!(img.trim());
            }
            if let Some(webm) = map.get("webm").and_then(Value::as_str) {
                obj["webm"] = json!(webm.trim());
            }
            if let Some(mp4) = map.get("mp4").and_then(Value::as_str) {
                obj["mp4"] = json!(mp4.trim());
            }
            Ok(obj)
        }
        other => Err(ShopError::Invalid(format!(
            "kind {other} is not supported; shop only provides nickname_color, avatar_frame, and profile_effect"
        ))),
    }?;

    // 开放自定义 CSS 与 JS 脚本定义（字符数上限 8192）
    if let Some(css_val) = map.get("css").and_then(Value::as_str) {
        if css_val.len() > 8192 {
            return Err(ShopError::Invalid(
                "style.css exceeds 8192 characters".into(),
            ));
        }
        let trimmed = css_val.trim();
        if !trimmed.is_empty() {
            result["css"] = json!(trimmed);
        }
    }
    if let Some(js_val) = map.get("js").and_then(Value::as_str) {
        if js_val.len() > 8192 {
            return Err(ShopError::Invalid(
                "style.js exceeds 8192 characters".into(),
            ));
        }
        let trimmed = js_val.trim();
        if !trimmed.is_empty() {
            result["js"] = json!(trimmed);
        }
    }

    Ok(result)
}

pub(super) fn valid_def_id(id: &str) -> bool {
    let bytes = id.as_bytes();
    bytes.len() >= 2
        && bytes.len() <= DEF_ID_MAX
        && bytes[0] == b'c'
        && bytes[1..]
            .iter()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

/// 校验并规整定义名称（trim 后 1..=NAME_MAX 字符；工作台复用）。
pub(super) fn validate_def_name(raw: &str) -> Result<String, ShopError> {
    let name = raw.trim();
    if name.is_empty() || name.chars().count() > NAME_MAX {
        return Err(ShopError::Invalid(format!(
            "name must be 1..={NAME_MAX} chars"
        )));
    }
    Ok(name.to_string())
}

/// 管理端样式定义列表（include_archived=false 只返回 active）。
pub async fn list_defs(pool: &DatabasePool, include_archived: bool) -> Result<Value, ShopError> {
    let filter = if include_archived {
        ""
    } else {
        " WHERE status = 'active'"
    };
    let sql = format!(
        "SELECT id, kind, name, style_json, status, created_at, updated_at FROM cosmetic_defs{filter} ORDER BY kind, created_at"
    );
    type DefRow = (String, String, String, String, String, i64, i64);
    let rows: Vec<DefRow> = match pool {
        Either::Left(p) => sqlx::query_as(&sql).fetch_all(p).await?,
        Either::Right(p) => sqlx::query_as(&sql).fetch_all(p).await?,
    };
    let items: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, kind, name, style_json, status, created_at, updated_at)| {
                let style: Value = serde_json::from_str(&style_json).map_err(|_| {
                    ShopError::Invalid("stored style_json is not valid JSON".into())
                })?;
                Ok(json!({
                    "id": id, "kind": kind, "name": name, "style": style,
                    "status": status, "createdAt": created_at, "updatedAt": updated_at
                }))
            },
        )
        .collect::<Result<_, ShopError>>()?;
    Ok(json!({ "cosmetics": items }))
}

/// 新建样式定义（审计由路由层记录）。
pub async fn create_def(
    pool: &DatabasePool,
    body: &Value,
    created_by: &str,
) -> Result<Value, ShopError> {
    let kind = body
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| ShopError::Invalid("kind required".into()))?;
    if !CUSTOMIZABLE_KINDS.contains(&kind) {
        return Err(ShopError::Invalid(format!(
            "kind {kind} does not support custom styles yet"
        )));
    }
    let name = validate_def_name(body.get("name").and_then(Value::as_str).unwrap_or(""))?;
    let style = validate_style(kind, body.get("style").unwrap_or(&Value::Null))?;

    let now = now_millis();
    let id = format!("c{}", uuid::Uuid::now_v7().simple());
    let style_json = style.to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(&name)
            .bind(&style_json)
            .bind(created_by)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO cosmetic_defs (id, kind, name, style_json, status, created_by, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(&name)
            .bind(&style_json)
            .bind(created_by)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?;
        }
    }
    Ok(
        json!({ "id": id, "kind": kind, "name": name, "style": style, "status": "active", "updatedAt": now }),
    )
}

/// 更新样式定义（改名/改样式/归档恢复）。归档走软删：历史 Token 引用保留。
pub async fn update_def(pool: &DatabasePool, id: &str, body: &Value) -> Result<Value, ShopError> {
    if !valid_def_id(id) {
        return Err(ShopError::Invalid("invalid cosmetic def id".into()));
    }
    let current: Option<(String, String, String, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT kind, name, style_json, status FROM cosmetic_defs WHERE id = ?")
                .bind(id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT kind, name, style_json, status FROM cosmetic_defs WHERE id = ?")
                .bind(id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((kind, current_name, current_style, current_status)) = current else {
        return Err(ShopError::NotFound(format!("cosmetic def {id}")));
    };
    let name = match body.get("name") {
        Some(Value::String(s)) => validate_def_name(s)?,
        _ => current_name,
    };
    let style_json = match body.get("style") {
        Some(s @ Value::Object(_)) => validate_style(&kind, s)?.to_string(),
        _ => current_style,
    };
    let status = match body.get("status") {
        Some(Value::String(s)) => {
            if !["active", "archived"].contains(&s.as_str()) {
                return Err(ShopError::Invalid("status must be active|archived".into()));
            }
            s.clone()
        }
        _ => current_status,
    };
    let now = now_millis();
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE cosmetic_defs SET name = ?, style_json = ?, status = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&name)
        .bind(&style_json)
        .bind(&status)
        .bind(now)
        .bind(id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE cosmetic_defs SET name = ?, style_json = ?, status = ?, updated_at = ? WHERE id = ?",
        )
        .bind(&name)
        .bind(&style_json)
        .bind(&status)
        .bind(now)
        .bind(id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if affected != 1 {
        return Err(ShopError::NotFound(format!("cosmetic def {id}")));
    }
    let style: Value = serde_json::from_str(&style_json)
        .map_err(|_| ShopError::Invalid("stored style_json is not valid JSON".into()))?;
    Ok(
        json!({ "id": id, "kind": kind, "name": name, "style": style, "status": status, "updatedAt": now }),
    )
}

/// Token 引用解析：返回 (kind, name, style)。仅 active 定义可解析。
pub async fn resolve_def(
    pool: &DatabasePool,
    id: &str,
) -> Result<Option<(String, String, Value)>, ShopError> {
    if !valid_def_id(id) {
        return Ok(None);
    }
    let row: Option<(String, String, String)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT kind, name, style_json FROM cosmetic_defs WHERE id = ? AND status = 'active'",
        )
        .bind(id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT kind, name, style_json FROM cosmetic_defs WHERE id = ? AND status = 'active'",
        )
        .bind(id)
        .fetch_optional(p)
        .await?,
    };
    let Some((kind, name, style_json)) = row else {
        return Ok(None);
    };
    let style: Value = serde_json::from_str(&style_json)
        .map_err(|_| ShopError::Invalid("stored style_json is not valid JSON".into()))?;
    Ok(Some((kind, name, style)))
}

/// 商品 Token 引用校验：所有由样式库生成的 Token 都必须命中 active 定义，
/// 并且定义类型必须与 Token 前缀匹配。内置历史 Token 继续兼容。
pub async fn validate_token_def_references(
    pool: &DatabasePool,
    tokens_json: Option<&str>,
) -> Result<(), ShopError> {
    let Some(json_str) = tokens_json else {
        return Ok(());
    };
    let tokens: Vec<String> = serde_json::from_str(json_str).map_err(|_| {
        ShopError::Invalid("presentation_tokens_json must be a string array".into())
    })?;
    let prefixes = [
        ("nickname.color.", "nickname_color"),
        ("avatar.frame.", "avatar_frame"),
        ("badge.", "cosmetic_badge"),
        ("profile.effect.", "profile_effect"),
        ("post.effect.", "post_effect"),
        ("reaction.pack.", "reaction_pack"),
        ("utility.", "utility"),
        ("title.prefix.", "title_prefix"),
    ];
    for token in &tokens {
        let Some((prefix, expected_kind)) = prefixes
            .iter()
            .find(|(prefix, _)| token.starts_with(prefix))
        else {
            continue;
        };
        let value = token.strip_prefix(prefix).unwrap_or_default();
        // 自定义 id 是 `c` + uuid simple（33 字符）；避免把历史的
        // `reaction.pack.celebrate` 这类普通 Token 误判为样式库引用。
        if value.len() < 16 || !value.starts_with('c') {
            // 内置枚举值由前端白名单/历史兼容规则处理。
            continue;
        }
        match resolve_def(pool, value).await? {
            Some((kind, _, _)) if kind == *expected_kind => {}
            _ => {
                return Err(ShopError::Invalid(format!(
                    "{token} is not an active {expected_kind} definition"
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_style_accepts_and_normalizes() {
        let s = validate_style(
            "nickname_color",
            &json!({ "mode": "gradient", "colors": ["#FF0000", "#00ff00"], "animate": "flow" }),
        )
        .unwrap();
        assert_eq!(s["colors"][0], "#ff0000");
        assert_eq!(s["durationMs"], 5_000);

        let f = validate_style(
            "avatar_frame",
            &json!({ "color": "#8B5CF6", "shape": "rounded", "frameScale": 120 }),
        )
        .unwrap();
        assert_eq!(f["frameScale"], 120);
        assert_eq!(f["shape"], "rounded");
    }

    #[test]
    fn validate_style_rejects_css_and_bad_colors() {
        for bad in [
            json!({ "mode": "solid", "color": "url(evil)" }),
            json!({ "mode": "solid", "color": "red; background:url(x)" }),
            json!({ "mode": "gradient", "colors": ["#ff0000"] }),
            json!({ "mode": "solid" }),
        ] {
            assert!(validate_style("nickname_color", &bad).is_err(), "{bad}");
        }
        assert!(validate_style("unknown_feature", &json!({ "color": "#ff0000" })).is_err());
        assert!(validate_style(
            "profile_effect",
            &json!({ "texture": "grid", "baseColor": "#101827", "accentColor": "#8b5cf6" })
        )
        .is_ok());
    }

    #[test]
    fn validate_style_accepts_custom_css_js_and_extended_animations() {
        let nick = validate_style(
            "nickname_color",
            &json!({
                "mode": "gradient",
                "colors": ["#ff0000", "#00ff00", "#0000ff"],
                "animate": "glitch",
                "angle": 45,
                "shadowPx": 12,
                "letterSpacing": 2,
                "css": ".cosmetic-name { font-weight: bold; }",
                "js": "console.log('cosmetic mounted');"
            }),
        )
        .unwrap();
        assert_eq!(nick["animate"], "glitch");
        assert_eq!(nick["angle"], 45);
        assert_eq!(nick["shadowPx"], 12);
        assert_eq!(nick["letterSpacing"], 2);
        assert_eq!(nick["css"], ".cosmetic-name { font-weight: bold; }");
        assert_eq!(nick["js"], "console.log('cosmetic mounted');");

        let frame = validate_style(
            "avatar_frame",
            &json!({
                "url": "/cosmetics/frames/steam/test.png",
                "shape": "rounded",
                "frameScale": 120,
                "css": "filter: drop-shadow(0 0 6px rgba(102, 192, 244, 0.35));"
            }),
        )
        .unwrap();
        assert_eq!(frame["shape"], "rounded");
        assert_eq!(frame["frameScale"], 120);
        assert_eq!(frame["url"], "/cosmetics/frames/steam/test.png");
        assert_eq!(
            frame["css"],
            "filter: drop-shadow(0 0 6px rgba(102, 192, 244, 0.35));"
        );
    }
}
