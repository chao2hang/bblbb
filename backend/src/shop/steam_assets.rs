//! Steam 装扮资产管理（M07-SHOP-ASSETS）：上架驱动下载 + 统一资产路由。
//!
//! 设计（替代"全量素材进前端仓库"）：
//! - 管理端「上架」Steam 头像框/全景背景时（studio publish），按装扮样式引用
//!   的文件名（`image`/`webm`/`mp4`，40 位内容哈希）从 Steam CDN 下载缺失
//!   文件，写入项目现有配置的存储后端：`BBLBB__STORAGE_BACKEND=local` →
//!   `storage_dir/steam-assets/…`；`s3` → bucket `steam-assets/…` 前缀。
//! - 前端统一通过 `GET /api/v1/steam-assets/{kind}/{file}` 读取：local 直接
//!   返回内容，S3 后端 302 到预签名下载 URL。文件名即内容哈希，响应按
//!   不可变资源缓存（`max-age=31536000, immutable`）。
//! - 样式内的 URL 在上架时统一改写为该稳定路由（前端不感知存储后端差异，
//!   切换 local/S3 无需回填历史数据）。
//! - 下载是 best-effort：失败仅告警不阻断上架（管理端可修复后重新发布）；
//!   样式缺 `appid`（目录外文件）时只复用已存在对象，不做下载。

use serde_json::Value;
use std::time::Duration;

use super::service::ShopError;
use crate::storage::StorageService;

pub const KIND_FRAMES: &str = "frames";
pub const KIND_BACKGROUNDS: &str = "backgrounds";
const KEY_PREFIX: &str = "steam-assets";
const CDN_BASE: &str = "https://shared.fastly.steamstatic.com/community_assets/images/items";

/// 文件名必须是 40 位小写十六进制内容哈希 + 受限扩展名（防路径穿越/任意读取）。
fn valid_filename(name: &str) -> bool {
    let Some((stem, ext)) = name.rsplit_once('.') else {
        return false;
    };
    if !matches!(ext, "png" | "jpg" | "jpeg" | "webm" | "mp4") {
        return false;
    }
    stem.len() == 40 && stem.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// URL/字段值 → 纯文件名（兼容裸文件名与完整 CDN URL 两种历史形态）。
fn basename_of(raw: &str) -> Option<String> {
    let name = raw.rsplit('/').next().unwrap_or(raw);
    valid_filename(name).then(|| name.to_string())
}

/// 前端渲染用的稳定资产路由。
pub fn asset_url(kind: &str, filename: &str) -> String {
    format!("/api/v1/{KEY_PREFIX}/{kind}/{filename}")
}

fn content_type_of(filename: &str) -> &'static str {
    match filename.rsplit_once('.').map(|(_, e)| e) {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("webm") => "video/webm",
        _ => "video/mp4",
    }
}

fn storage_err(e: impl std::fmt::Display) -> ShopError {
    ShopError::Invalid(format!("steam assets storage: {e}"))
}

/// 确保素材存在于「本地仓库 + 配置的默认后端」：
/// - 本地仓库（storage_dir/steam-assets/…）是公开路由的首选服务源
///   （读穿缓存，字节精确，不受对象存储网关缺陷影响）；
/// - 默认后端（local 或 S3）承载持久化/分发语义；
/// - 缺失且知道 appid 时从 Steam CDN 实时补拉；下载失败仅告警不阻断。
async fn ensure_object(
    storage: &StorageService,
    kind: &str,
    filename: &str,
    appid: Option<u64>,
) -> Result<(), ShopError> {
    if !valid_filename(filename) {
        return Ok(());
    }
    let key = format!("{KEY_PREFIX}/{kind}/{filename}");
    let content_type = content_type_of(filename);
    let local = storage.adapter(crate::storage::StorageBackend::Local).ok();
    let local_head = match &local {
        Some(l) => l.head_object(&key).await.ok(),
        None => None,
    };
    let local_missing = !local_head.as_ref().map(|h| h.exists).unwrap_or(false);
    let adapter = storage
        .adapter(storage.default_backend())
        .map_err(storage_err)?;
    let _default_is_local = matches!(adapter, crate::storage::adapter::DynamicAdapter::Local(_));
    let default_head = adapter.head_object(&key).await.ok();
    let default_missing = !default_head.as_ref().map(|h| h.exists).unwrap_or(false);
    if !local_missing && !default_missing {
        return Ok(());
    }

    let mut data: Option<Vec<u8>> = None;
    // 默认后端已有 → 拉回本地缓存（仅当返回长度与对象一致时可信）。
    if local_missing && !default_missing {
        if let Some(head) = &default_head {
            if let Ok(d) = adapter.read_object(&key).await {
                if d.len() as i64 == head.size_bytes {
                    data = Some(d);
                } else {
                    tracing::warn!(key = %key, expected = head.size_bytes, got = d.len(), "steam asset backend read length mismatch; falling back to CDN");
                }
            }
        }
    }
    // CDN 实时补拉（Steam CDN 无截断问题）。
    if data.is_none() && (default_missing || local_missing) {
        let Some(appid) = appid else {
            tracing::warn!(key = %key, "steam asset missing and style has no appid; skip download");
            return Ok(());
        };
        let url = format!("{CDN_BASE}/{appid}/{filename}");
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| ShopError::Invalid(format!("steam assets http client: {e}")))?;
        let resp = match client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
        {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!(key = %key, url = %url, error = %e, "steam asset download failed");
                return Ok(());
            }
        };
        if !resp.status().is_success() {
            tracing::warn!(key = %key, url = %url, status = %resp.status(), "steam asset download non-success");
            return Ok(());
        }
        match resp.bytes().await {
            Ok(b) if !b.is_empty() => data = Some(b.to_vec()),
            Ok(_) => return Ok(()),
            Err(e) => {
                tracing::warn!(key = %key, error = %e, "steam asset download body failed");
                return Ok(());
            }
        }
    }
    let Some(data) = data else {
        return Ok(());
    };
    if default_missing {
        adapter
            .write_object(&key, &data, Some(content_type))
            .await
            .map_err(storage_err)?;
    }
    if local_missing {
        if let Some(l) = &local {
            l.write_object(&key, &data, Some(content_type))
                .await
                .map_err(storage_err)?;
        }
    }
    tracing::info!(key = %key, bytes = data.len(), "steam asset ensured (local+backend)");
    Ok(())
}
/// 上架钩子：确保样式引用的 Steam 素材已入库，并把样式内 URL 统一改写为
/// 稳定资产路由。`storage = None`（存储未配置/配置关闭下载）时仅改写 URL。
pub async fn apply_to_cosmetic_style(
    storage: Option<&StorageService>,
    kind: &str,
    style: &mut Value,
) {
    let asset_kind = match kind {
        "profile_effect" => KIND_BACKGROUNDS,
        "avatar_frame" => KIND_FRAMES,
        _ => return,
    };
    let Some(obj) = style.as_object_mut() else {
        return;
    };
    let appid = obj.get("appid").and_then(Value::as_u64);

    let mut files: Vec<String> = Vec::new();
    if let Some(raw) = obj.get("image").and_then(Value::as_str) {
        if let Some(name) = basename_of(raw) {
            files.push(name);
        }
    }
    for field in ["webm", "mp4"] {
        if let Some(raw) = obj.get(field).and_then(Value::as_str) {
            if let Some(name) = basename_of(raw) {
                files.push(name);
            }
        }
    }
    if files.is_empty() {
        return;
    }

    if let Some(storage) = storage {
        for file in &files {
            if let Err(e) = ensure_object(storage, asset_kind, file, appid).await {
                tracing::warn!(kind = kind, file = %file, error = %e, "steam asset ensure failed");
            }
        }
    }

    if let Some(raw) = obj.get("image").and_then(Value::as_str) {
        if let Some(name) = basename_of(raw) {
            obj.insert("url".into(), Value::String(asset_url(asset_kind, &name)));
        }
    }
    for field in ["webm", "mp4"] {
        if let Some(raw) = obj.get(field).and_then(Value::as_str) {
            if raw.starts_with("http") {
                if let Some(name) = basename_of(raw) {
                    obj.insert(field.into(), Value::String(asset_url(asset_kind, &name)));
                }
            }
        }
    }
}

/// 资产读取结果：统一返回完整字节（local 直接读；S3 后端按 1MiB 分块
/// Range 读取后拼装——部分 S3 兼容网关对整段 GET 响应有 1MiB 上限，
/// 分块 Range 已实测可靠；待网关修复后可恢复 presign 直连）。
pub struct SteamAsset {
    pub content_type: &'static str,
    pub data: Vec<u8>,
}

/// 单次 Range 分块大小（实测部分 S3 兼容网关对 ≥1MiB 响应会截断，
/// 取 512KB 留足余量）。
const RANGE_CHUNK: u64 = 512 * 1024;
/// 单资产内存上限（当前 Steam 素材单文件 ≤ ~5MB；异常大对象直接拒绝）。
const MAX_ASSET_BYTES: u64 = 32 * 1024 * 1024;

/// 从 presentation token（如 `profile.effect.<def_id>`）提取装扮定义 ID。
fn def_id_from_token(token: &str) -> Option<String> {
    const PREFIXES: [&str; 8] = [
        "nickname.color.",
        "avatar.frame.",
        "badge.",
        "profile.effect.",
        "post.effect.",
        "reaction.pack.",
        "utility.",
        "title.prefix.",
    ];
    PREFIXES
        .iter()
        .find_map(|p| token.strip_prefix(p))
        .map(str::to_owned)
}

fn collect_def_ids(tokens_json: Option<String>, out: &mut std::collections::HashSet<String>) {
    let Ok(tokens) = serde_json::from_str::<Vec<String>>(tokens_json.as_deref().unwrap_or("[]"))
    else {
        return;
    };
    for token in tokens {
        if let Some(id) = def_id_from_token(&token) {
            out.insert(id);
        }
    }
}

/// 启动自愈：收集已上架（published）与已装备（equipped）商品引用的装扮
/// 定义，确保其样式引用的 Steam 素材存在于当前配置的存储后端。
/// 只做存储侧补齐（幂等），不改写数据库样式；失败仅记录告警。
pub async fn sync_referenced_assets(pool: crate::db::pool::DatabasePool, storage: StorageService) {
    use sqlx::Either;

    let mut def_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    let published: Vec<(Option<String>,)> = match pool.as_ref() {
        Either::Left(p) => sqlx::query_as(
            "SELECT presentation_tokens_json FROM shop_products WHERE status = 'published'",
        )
        .fetch_all(p)
        .await
        .unwrap_or_default(),
        Either::Right(p) => sqlx::query_as(
            "SELECT presentation_tokens_json FROM shop_products WHERE status = 'published'",
        )
        .fetch_all(p)
        .await
        .unwrap_or_default(),
    };
    for (tokens,) in published {
        collect_def_ids(tokens, &mut def_ids);
    }
    let equipped: Vec<(Option<String>,)> = match pool.as_ref() {
        Either::Left(p) => sqlx::query_as(
            "SELECT p.presentation_tokens_json FROM user_entitlements e
             JOIN shop_products p ON p.id = e.product_id WHERE e.status = 'equipped'",
        )
        .fetch_all(p)
        .await
        .unwrap_or_default(),
        Either::Right(p) => sqlx::query_as(
            "SELECT p.presentation_tokens_json FROM user_entitlements e
             JOIN shop_products p ON p.id = e.product_id WHERE e.status = 'equipped'",
        )
        .fetch_all(p)
        .await
        .unwrap_or_default(),
    };
    for (tokens,) in equipped {
        collect_def_ids(tokens, &mut def_ids);
    }
    if def_ids.is_empty() {
        return;
    }
    tracing::info!(count = def_ids.len(), "steam assets startup sync");

    for def_id in &def_ids {
        let row: Option<(String, String)> = match pool.as_ref() {
            Either::Left(p) => sqlx::query_as(
                "SELECT kind, style_json FROM cosmetic_defs WHERE id = ? AND status = 'active'",
            )
            .bind(def_id)
            .fetch_optional(p)
            .await
            .unwrap_or(None),
            Either::Right(p) => sqlx::query_as(
                "SELECT kind, style_json FROM cosmetic_defs WHERE id = ? AND status = 'active'",
            )
            .bind(def_id)
            .fetch_optional(p)
            .await
            .unwrap_or(None),
        };
        let Some((kind, style_json)) = row else {
            continue;
        };
        let Ok(mut style) = serde_json::from_str::<Value>(&style_json) else {
            continue;
        };
        apply_to_cosmetic_style(Some(&storage), &kind, &mut style).await;
    }
    tracing::info!("steam assets startup sync done");
}

/// 从当前存储拓扑读取素材字节：本地仓库（读穿缓存）优先，未命中回退
/// 默认后端（对象存储按 512KB Range 分块拼装，规避部分网关对整段 GET
/// 的 1MiB 响应截断）。
async fn read_served(storage: &StorageService, _kind: &str, key: &str) -> Option<Vec<u8>> {
    // 1) 本地仓库（读穿缓存）。
    if let Ok(local) = storage.adapter(crate::storage::StorageBackend::Local) {
        if let Ok(head) = local.head_object(key).await {
            if head.exists && head.size_bytes > 0 && (head.size_bytes as u64) <= MAX_ASSET_BYTES {
                if let Ok(data) = local.read_object(key).await {
                    return Some(data);
                }
            }
        }
    }
    // 2) 回退：当前配置的默认后端。
    let adapter = storage.adapter(storage.default_backend()).ok()?;
    let head = adapter.head_object(key).await.ok()?;
    if !head.exists || head.size_bytes <= 0 || (head.size_bytes as u64) > MAX_ASSET_BYTES {
        return None;
    }
    let size = head.size_bytes as u64;
    if matches!(adapter, crate::storage::adapter::DynamicAdapter::Local(_)) {
        return adapter.read_object(key).await.ok();
    }
    // 对象存储：实测部分网关整段 GET 在 1MiB 处截断、SDK 流式 Range 不稳，
    // 但「预签名 URL + 自发 HTTP Range 分段」可靠（已验证尾部字节吻合）。
    if let Some(data) = s3_read_ranged(&adapter, key, size).await {
        return Some(data);
    }
    // 兜底：SDK 整读（仅在返回长度与对象一致时可信）。
    match adapter.read_object(key).await {
        Ok(data) if data.len() as u64 == size => Some(data),
        Ok(data) => {
            tracing::warn!(key = %key, size, got = data.len(), "steam asset full read length mismatch");
            None
        }
        Err(e) => {
            tracing::warn!(key = %key, error = %e, "steam asset full read failed");
            None
        }
    }
}

/// 预签名 URL + 自发 HTTP 分段读取（每段 ≤512KB，网关偶发 5xx 重试一次）。
async fn s3_read_ranged(
    adapter: &crate::storage::adapter::DynamicAdapter,
    key: &str,
    size: u64,
) -> Option<Vec<u8>> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(60))
        .build()
        .ok()?;
    let mut data = Vec::with_capacity(size as usize);
    let mut offset: u64 = 0;
    while offset < size {
        let len = RANGE_CHUNK.min(size - offset);
        let mut chunk = None;
        for _ in 0..2 {
            let url = adapter.presign_download(key, 600).await.ok()?.url;
            let req = client
                .get(&url)
                .header("User-Agent", "Mozilla/5.0")
                .header("Range", format!("bytes={offset}-{}", offset + len - 1));
            match req.send().await {
                Ok(resp) => {
                    let status = resp.status();
                    match resp.bytes().await {
                        Ok(body) => {
                            let mut body = body.to_vec();
                            tracing::debug!(key = %key, offset, len, status = %status, got = body.len(), "steam asset ranged read");
                            // 网关忽略 Range 时会返回 200 整段（仍被 1MiB 截断）：
                            // 只保留请求段长度，长度不符的尾部由下一次分段补齐。
                            if body.len() as u64 > len {
                                body.truncate(len as usize);
                            }
                            if (body.len() as u64) == len {
                                chunk = Some(body);
                                break;
                            }
                            tracing::warn!(key = %key, offset, len, got = body.len(), "steam asset ranged read length mismatch");
                        }
                        Err(e) => {
                            tracing::warn!(key = %key, offset, len, error = %e, "steam asset ranged read body failed")
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(key = %key, offset, len, error = %e, "steam asset ranged read request failed")
                }
            }
        }
        let chunk = chunk?;
        data.extend_from_slice(&chunk);
        offset += len;
    }
    Some(data)
}

/// 公开路由读取：kind 仅限 backgrounds/frames。
///
/// 存储未命中时按 image 从目录表反查 appid，实时从 Steam CDN 补拉入库
/// （"实时拉取 Steam 已有"）：只要是 Steam 目录内的素材，渲染永远自愈。
pub async fn load_asset(
    pool: Option<&crate::db::pool::DatabasePool>,
    storage: &StorageService,
    kind: &str,
    filename: &str,
) -> Result<SteamAsset, ShopError> {
    if !matches!(kind, KIND_FRAMES | KIND_BACKGROUNDS) || !valid_filename(filename) {
        return Err(ShopError::NotFound(format!(
            "steam asset {kind}/{filename}"
        )));
    }
    let key = format!("{KEY_PREFIX}/{kind}/{filename}");
    let not_found = |_| ShopError::NotFound(format!("steam asset {kind}/{filename}"));

    if let Some(data) = read_served(storage, kind, &key).await {
        return Ok(SteamAsset {
            content_type: content_type_of(filename),
            data,
        });
    }

    // 实时补拉：目录反查 appid → Steam CDN → 当前存储后端 → 重读。
    if let Some(pool) = pool {
        if let Some(appid) = lookup_appid_by_image(pool, kind, filename).await {
            if let Err(e) = ensure_object(storage, kind, filename, Some(appid)).await {
                tracing::warn!(kind = kind, file = %filename, error = %e, "steam asset lazy fetch failed");
            }
            if let Some(data) = read_served(storage, kind, &key).await {
                return Ok(SteamAsset {
                    content_type: content_type_of(filename),
                    data,
                });
            }
        }
    }
    Err(not_found(()))
}
// ── Steam 目录实时拉取（M07-SHOP-ASSETS）─────────────────────────────
//
// steam_catalog_items 是 Steam 点数商店目录的镜像：管理端触发「同步」时
// 实时拉取 Steam 定义列表（class 14 = 头像框，class 13 = 迷你资料背景）
// 按唯一键 defid 合并；首次访问目录接口时若表为空，先用仓库内置的目录
// JSON 快照做基线种子，保证离线环境也有完整可选列表。
// 资产公开路由在存储未命中时按 image 反查 appid，实时补拉素材。

const STEAM_REWARDS_API: &str =
    "https://api.steampowered.com/ILoyaltyRewardsService/QueryRewardItems/v1/";
const STEAM_CLASS_FRAMES: u32 = 14;
const STEAM_CLASS_BACKGROUNDS: u32 = 13;
const STEAM_SYNC_LANGUAGE: &str = "schinese";
const STEAM_SYNC_MAX_PAGES: usize = 40;

const FRAMES_SNAPSHOT: &str =
    include_str!("../../data/steam-avatar-frames.json");
const BACKGROUNDS_SNAPSHOT: &str =
    include_str!("../../data/steam-profile-backgrounds.json");

#[derive(Debug, serde::Serialize)]
pub struct SteamCatalogItem {
    pub id: String,
    pub kind: &'static str,
    pub defid: i64,
    pub appid: i64,
    pub name: String,
    pub image: String,
    pub webm: Option<String>,
    pub mp4: Option<String>,
    pub cost: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<Value>,
}

#[derive(Debug, serde::Serialize)]
pub struct SteamCatalogSyncReport {
    pub frames: SteamCatalogSyncCounts,
    pub backgrounds: SteamCatalogSyncCounts,
}

#[derive(Debug, serde::Serialize)]
pub struct SteamCatalogSyncCounts {
    pub fetched: usize,
    pub added: usize,
    pub updated: usize,
}

fn snapshot_rows(kind: &'static str, raw: &str) -> Vec<(String, SteamCatalogItem)> {
    let Ok(Value::Array(items)) = serde_json::from_str::<Value>(raw) else {
        return Vec::new();
    };
    items
        .into_iter()
        .filter_map(|v| {
            let defid = v.get("defid")?.as_i64()?;
            let appid = v.get("appid")?.as_i64()?;
            let name = v.get("name")?.as_str()?.to_string();
            let image = v.get("image")?.as_str()?.to_string();
            let id = if kind == KIND_FRAMES {
                format!("steam_{defid}")
            } else {
                format!("steam_bg_{defid}")
            };
            let extra = match kind {
                KIND_FRAMES => {
                    let mut e = serde_json::Map::new();
                    if let Some(s) = v.get("shape") {
                        e.insert("shape".into(), s.clone());
                    }
                    if let Some(s) = v.get("scale") {
                        e.insert("scale".into(), s.clone());
                    }
                    if e.is_empty() {
                        None
                    } else {
                        Some(Value::Object(e))
                    }
                }
                _ => None,
            };
            let cost = v.get("cost").and_then(Value::as_i64).unwrap_or(2000);
            Some((
                id.clone(),
                SteamCatalogItem {
                    id,
                    kind,
                    defid,
                    appid,
                    name,
                    image,
                    webm: v.get("webm").and_then(Value::as_str).map(str::to_owned),
                    mp4: v.get("mp4").and_then(Value::as_str).map(str::to_owned),
                    cost,
                    extra,
                },
            ))
        })
        .collect()
}

async fn fetch_steam_class(class: u32) -> Result<Vec<(String, SteamCatalogItem)>, ShopError> {
    let kind = if class == STEAM_CLASS_FRAMES {
        KIND_FRAMES
    } else {
        KIND_BACKGROUNDS
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .build()
        .map_err(|e| ShopError::Invalid(format!("steam catalog http client: {e}")))?;
    let mut cursor = String::new();
    let mut seen = std::collections::HashSet::new();
    let mut out: Vec<(String, SteamCatalogItem)> = Vec::new();
    for _page in 0..STEAM_SYNC_MAX_PAGES {
        let url = format!(
            "{STEAM_REWARDS_API}?count=500&community_item_classes%5B0%5D={class}&language={STEAM_SYNC_LANGUAGE}&cursor={cursor}"
        );
        let resp = client
            .get(&url)
            .header("User-Agent", "Mozilla/5.0")
            .send()
            .await
            .map_err(|e| ShopError::Invalid(format!("steam catalog fetch failed: {e}")))?;
        if !resp.status().is_success() {
            return Err(ShopError::Invalid(format!(
                "steam catalog fetch status {}",
                resp.status()
            )));
        }
        let body: Value = resp
            .json()
            .await
            .map_err(|e| ShopError::Invalid(format!("steam catalog decode failed: {e}")))?;
        let defs = body
            .pointer("/response/definitions")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        if defs.is_empty() {
            break;
        }
        for def in defs {
            let defid = match def.get("defid").and_then(Value::as_i64) {
                Some(d) => d,
                None => continue,
            };
            if !seen.insert(defid) {
                continue;
            }
            let c = def
                .get("community_item_data")
                .cloned()
                .unwrap_or(Value::Null);
            let image = c
                .get("item_image_large")
                .or_else(|| c.get("item_image_small"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if image.is_empty() {
                continue;
            }
            let id = if kind == KIND_FRAMES {
                format!("steam_{defid}")
            } else {
                format!("steam_bg_{defid}")
            };
            let name = c
                .get("item_title")
                .or_else(|| c.get("item_name"))
                .and_then(Value::as_str)
                .unwrap_or("未命名")
                .to_string();
            out.push((
                id.clone(),
                SteamCatalogItem {
                    id,
                    kind,
                    defid,
                    appid: def.get("appid").and_then(Value::as_i64).unwrap_or_default(),
                    name,
                    image,
                    webm: c
                        .get("item_movie_webm")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    mp4: c
                        .get("item_movie_mp4")
                        .and_then(Value::as_str)
                        .map(str::to_owned),
                    cost: def
                        .get("point_cost")
                        .and_then(Value::as_str)
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(2000),
                    extra: None,
                },
            ));
        }
        cursor = body
            .pointer("/response/next_cursor")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if cursor.is_empty() {
            break;
        }
    }
    Ok(out)
}

async fn upsert_catalog_items(
    pool: &crate::db::pool::DatabasePool,
    items: &[(String, SteamCatalogItem)],
) -> Result<(usize, usize), ShopError> {
    let now = crate::storage::now_millis();
    let mut added = 0usize;
    let mut updated = 0usize;
    use sqlx::Either;
    for (id, item) in items {
        let extra = item.extra.as_ref().map(|v| v.to_string());
        let (added_count, updated_count): (i64, i64) = match pool {
            Either::Left(p) => {
                let exists: Option<i64> =
                    sqlx::query_scalar("SELECT 1 FROM steam_catalog_items WHERE defid = ?")
                        .bind(item.defid)
                        .fetch_optional(p)
                        .await
                        .map_err(|e| ShopError::Invalid(format!("steam catalog query: {e}")))?;
                sqlx::query(
                    "INSERT INTO steam_catalog_items (id, kind, defid, appid, name, image, webm, mp4, cost, extra_json, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON CONFLICT(defid) DO UPDATE SET appid = excluded.appid, name = excluded.name,
                       image = excluded.image, webm = excluded.webm, mp4 = excluded.mp4,
                       cost = excluded.cost, extra_json = excluded.extra_json, updated_at = excluded.updated_at",
                )
                .bind(id)
                .bind(item.kind)
                .bind(item.defid)
                .bind(item.appid)
                .bind(&item.name)
                .bind(&item.image)
                .bind(&item.webm)
                .bind(&item.mp4)
                .bind(item.cost)
                .bind(&extra)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| ShopError::Invalid(format!("steam catalog upsert: {e}")))?;
                (
                    if exists.is_some() { 0 } else { 1 },
                    if exists.is_some() { 1 } else { 0 },
                )
            }
            Either::Right(p) => {
                let res = sqlx::query(
                    "INSERT INTO steam_catalog_items (id, kind, defid, appid, name, image, webm, mp4, cost, extra_json, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                     ON DUPLICATE KEY UPDATE appid = VALUES(appid), name = VALUES(name),
                       image = VALUES(image), webm = VALUES(webm), mp4 = VALUES(mp4),
                       cost = VALUES(cost), extra_json = VALUES(extra_json), updated_at = VALUES(updated_at)",
                )
                .bind(id)
                .bind(item.kind)
                .bind(item.defid)
                .bind(item.appid)
                .bind(&item.name)
                .bind(&item.image)
                .bind(&item.webm)
                .bind(&item.mp4)
                .bind(item.cost)
                .bind(&extra)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| ShopError::Invalid(format!("steam catalog upsert: {e}")))?;
                // rows_affected: 1 = insert, 2 = update（MySQL 语义）。
                (
                    if res.rows_affected() == 1 { 1 } else { 0 },
                    if res.rows_affected() > 1 { 1 } else { 0 },
                )
            }
        };
        added += added_count as usize;
        updated += updated_count as usize;
    }
    Ok((added, updated))
}

/// 管理端「同步 Steam 目录」：实时拉取两个分类的定义列表并合并入库。
/// 内置 JSON 快照先作为基线合并（保证不丢既有富字段）。
pub async fn sync_catalog(
    pool: &crate::db::pool::DatabasePool,
) -> Result<SteamCatalogSyncReport, ShopError> {
    let mut baseline = snapshot_rows(KIND_FRAMES, FRAMES_SNAPSHOT);
    baseline.extend(snapshot_rows(KIND_BACKGROUNDS, BACKGROUNDS_SNAPSHOT));
    let (frames_added_base, frames_updated_base) = upsert_catalog_items(pool, &baseline).await?;

    let mut report = SteamCatalogSyncReport {
        frames: SteamCatalogSyncCounts {
            fetched: 0,
            added: frames_added_base,
            updated: frames_updated_base,
        },
        backgrounds: SteamCatalogSyncCounts {
            fetched: 0,
            added: 0,
            updated: 0,
        },
    };
    for (class, is_frames) in [(STEAM_CLASS_FRAMES, true), (STEAM_CLASS_BACKGROUNDS, false)] {
        let items = fetch_steam_class(class).await?;
        let (added, updated) = upsert_catalog_items(pool, &items).await?;
        let counts = if is_frames {
            &mut report.frames
        } else {
            &mut report.backgrounds
        };
        counts.fetched = items.len();
        counts.added += added;
        counts.updated += updated;
    }
    tracing::info!(
        frames = report.frames.fetched,
        backgrounds = report.backgrounds.fetched,
        "steam catalog synced"
    );
    Ok(report)
}

/// 确保目录表有基线数据（首次访问目录接口时用内置快照种子）。
async fn ensure_catalog_seeded(pool: &crate::db::pool::DatabasePool) -> Result<(), ShopError> {
    let total: i64 = match pool {
        sqlx::Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM steam_catalog_items")
                .fetch_one(p)
                .await
        }
        sqlx::Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM steam_catalog_items")
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| ShopError::Invalid(format!("steam catalog count: {e}")))?;
    if total > 0 {
        return Ok(());
    }
    let mut baseline = snapshot_rows(KIND_FRAMES, FRAMES_SNAPSHOT);
    baseline.extend(snapshot_rows(KIND_BACKGROUNDS, BACKGROUNDS_SNAPSHOT));
    upsert_catalog_items(pool, &baseline).await?;
    tracing::info!(
        count = baseline.len(),
        "steam catalog seeded from bundled snapshot"
    );
    Ok(())
}

/// 目录分页查询（公开只读；管理员选品与前端回退共用）。
pub async fn list_catalog(
    pool: &crate::db::pool::DatabasePool,
    kind: &str,
    page: i64,
    page_size: i64,
    query: Option<&str>,
) -> Result<(Vec<SteamCatalogItem>, i64), ShopError> {
    ensure_catalog_seeded(pool).await?;
    let page = page.max(1);
    let page_size = page_size.clamp(1, 3000);
    let offset = (page - 1) * page_size;
    let pattern = query
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| format!("%{}%", s.replace('%', "")));
    use sqlx::Either;
    let like_clause = pattern.is_some();
    let total: i64 = if like_clause {
        match pool {
            Either::Left(p) => {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM steam_catalog_items WHERE kind = ? AND name LIKE ?",
                )
                .bind(kind)
                .bind(&pattern)
                .fetch_one(p)
                .await
            }
            Either::Right(p) => {
                sqlx::query_scalar(
                    "SELECT COUNT(*) FROM steam_catalog_items WHERE kind = ? AND name LIKE ?",
                )
                .bind(kind)
                .bind(&pattern)
                .fetch_one(p)
                .await
            }
        }
    } else {
        match pool {
            Either::Left(p) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM steam_catalog_items WHERE kind = ?")
                    .bind(kind)
                    .fetch_one(p)
                    .await
            }
            Either::Right(p) => {
                sqlx::query_scalar("SELECT COUNT(*) FROM steam_catalog_items WHERE kind = ?")
                    .bind(kind)
                    .fetch_one(p)
                    .await
            }
        }
    }
    .map_err(|e| ShopError::Invalid(format!("steam catalog count: {e}")))?;

    let rows: Vec<CatalogRow> = if like_clause {
        match pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT id, defid, appid, name, image, webm, mp4, cost, extra_json FROM steam_catalog_items
                 WHERE kind = ? AND name LIKE ? ORDER BY defid DESC LIMIT ? OFFSET ?",
            ).bind(kind).bind(&pattern).bind(page_size).bind(offset).fetch_all(p).await,
            Either::Right(p) => sqlx::query_as(
                "SELECT id, defid, appid, name, image, webm, mp4, cost, extra_json FROM steam_catalog_items
                 WHERE kind = ? AND name LIKE ? ORDER BY defid DESC LIMIT ? OFFSET ?",
            ).bind(kind).bind(&pattern).bind(page_size).bind(offset).fetch_all(p).await,
        }
    } else {
        match pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT id, defid, appid, name, image, webm, mp4, cost, extra_json FROM steam_catalog_items
                 WHERE kind = ? ORDER BY defid DESC LIMIT ? OFFSET ?",
            ).bind(kind).bind(page_size).bind(offset).fetch_all(p).await,
            Either::Right(p) => sqlx::query_as(
                "SELECT id, defid, appid, name, image, webm, mp4, cost, extra_json FROM steam_catalog_items
                 WHERE kind = ? ORDER BY defid DESC LIMIT ? OFFSET ?",
            ).bind(kind).bind(page_size).bind(offset).fetch_all(p).await,
        }
    }
    .map_err(|e| ShopError::Invalid(format!("steam catalog list: {e}")))?;
    Ok((rows.into_iter().map(tuple_to_item).collect(), total))
}

#[allow(clippy::type_complexity)]
fn tuple_to_item(row: CatalogRow) -> SteamCatalogItem {
    let (id, defid, appid, name, image, webm, mp4, cost, extra_json) = row;
    let kind = if id.starts_with("steam_bg_") {
        KIND_BACKGROUNDS
    } else {
        KIND_FRAMES
    };
    SteamCatalogItem {
        id,
        kind,
        defid,
        appid,
        name,
        image,
        webm,
        mp4,
        cost,
        extra: extra_json.and_then(|s| serde_json::from_str(&s).ok()),
    }
}

/// 目录行元组（sqlx 查询投影，跨方言共用）。
type CatalogRow = (
    String,
    i64,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
);

/// 按 image 文件名反查 appid（公开路由未命中存储时实时补拉素材用）。
pub async fn lookup_appid_by_image(
    pool: &crate::db::pool::DatabasePool,
    kind: &str,
    filename: &str,
) -> Option<u64> {
    use sqlx::Either;
    let row: Option<(i64,)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT appid FROM steam_catalog_items WHERE kind = ? AND (image = ? OR webm = ? OR mp4 = ?)",
        )
        .bind(kind)
        .bind(filename)
        .bind(filename)
        .bind(filename)
        .fetch_optional(p)
        .await
        .ok()?,
        Either::Right(p) => sqlx::query_as(
            "SELECT appid FROM steam_catalog_items WHERE kind = ? AND (image = ? OR webm = ? OR mp4 = ?)",
        )
        .bind(kind)
        .bind(filename)
        .bind(filename)
        .bind(filename)
        .fetch_optional(p)
        .await
        .ok()?,
    };
    row.and_then(|(appid,)| u64::try_from(appid).ok())
}
