//! `video_embeds` 状态机与 Video Service（M10-VIDEO-08/09/11）。
//!
//! 状态机：`pending`（创建后、异步 refresh 前）→ `ready`（解析通过）|
//! `error`（Provider 故障/下架/限流/无嵌入权限/策略变更——保留官方外链降级）|
//! `blocked`（侵权/平台通知，渲染 none，保留审计）| `removed`（删除）。
//!
//! 发布/阅读时必须重新校验（docs/VIDEO-PLUGIN.md §5）：create 与 get 都重读
//! 当前 Provider 策略与目标帖可见性；历史引用在策略变更后经
//! [`recheck_references`] 重检（继续嵌入或降级外链，不阻塞发帖）。
//!
//! 本模块使用 `&crate::db::DatabasePool`（sqlite/mysql Either 分支投影），
//! 不依赖 axum。

use serde_json::Value;
use sqlx::{Either, Row};

use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::outbox::now_millis;
use crate::video::classify::is_allowed_host;
use crate::video::csp::render_for;
use crate::video::egress::FetchClient;
use crate::video::policy::{
    validate_config, validate_host_list, VideoPolicy, MAX_DURATION_MS_LIMIT,
    MAX_PLAYLIST_DEPTH_LIMIT, MAX_REDIRECTS_LIMIT, MAX_RESPONSE_BYTES_LIMIT, MAX_SEGMENTS_LIMIT,
    MIN_RESPONSE_BYTES,
};
use crate::video::provider::ProviderRegistry;
use crate::video::resolution::{consume_resolution, issue_resolution};
use crate::video::xigua::{extract_video_id, is_xigua_host};
use crate::video::{classify, Provider, RenderPolicy, VideoError};

/// 解析目标（post/comment）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VideoTarget {
    pub target_type: String,
    pub target_id: String,
}

/// resolve 响应视图（只含非敏感元数据；不回显原始 source）。
#[derive(Debug, Clone)]
pub struct ResolvedView {
    pub resolution_id: String,
    pub provider: String,
    pub media_type: Option<String>,
    pub official_url: String,
    pub source_host: String,
    pub title: Option<String>,
    pub policy_version: i64,
    pub policy_enabled: bool,
    pub embeddable: bool,
    pub expires_at: i64,
    pub target: VideoTarget,
}

/// 单个 embed 的可见投影（含动态渲染策略）。
#[derive(Debug, Clone)]
pub struct EmbedView {
    pub id: String,
    pub user_id: String,
    pub provider: String,
    pub status: String,
    pub target: VideoTarget,
    pub title: Option<String>,
    pub poster_attachment_id: Option<String>,
    /// 安全官方 URL（render mode=none 时省略，不向隐藏/封禁内容泄漏）。
    pub official_url: Option<String>,
    pub source_host: Option<String>,
    pub media_type: Option<String>,
    pub external_id: Option<String>,
    pub error_class: Option<String>,
    pub policy_version: i64,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
    pub render: RenderPolicy,
}

/// 行模型（`video_embeds` 投影）。
#[derive(Debug, Clone)]
struct EmbedRow {
    id: String,
    user_id: String,
    provider: Provider,
    status: String,
    target_type: String,
    target_id: String,
    title: Option<String>,
    poster_attachment_id: Option<String>,
    official_url: Option<String>,
    error_class: Option<String>,
    policy_version: i64,
    version: i64,
    created_at: i64,
    updated_at: i64,
}

impl EmbedRow {
    fn host(&self) -> String {
        host_of(self.official_url.as_deref())
    }

    fn media_type(&self) -> Option<String> {
        media_type_for_url(self.official_url.as_deref(), self.provider)
    }
}

fn host_of(url: Option<&str>) -> String {
    url.and_then(|u| url::Url::parse(u).ok())
        .and_then(|u| u.host_str().map(|h| h.to_lowercase()))
        .unwrap_or_default()
}

fn media_type_for_url(url: Option<&str>, provider: Provider) -> Option<String> {
    let url = url?;
    let path = url.split(['?', '#']).next().unwrap_or("").to_lowercase();
    if provider == Provider::Hls && (path.ends_with(".m3u8") || path.ends_with(".m3u")) {
        Some("application/vnd.apple.mpegurl".to_string())
    } else if provider == Provider::Direct {
        if path.ends_with(".mp4") {
            Some("video/mp4".to_string())
        } else if path.ends_with(".webm") {
            Some("video/webm".to_string())
        } else if path.ends_with(".ogv") || path.ends_with(".ogg") {
            Some("video/ogg".to_string())
        } else if path.ends_with(".mov") {
            Some("video/quicktime".to_string())
        } else {
            None
        }
    } else {
        None
    }
}

// ─────────────────────────── 策略加载 ───────────────────────────

/// 读取 Provider 策略（缺省 = 关闭，与迁移列默认一致）。
pub async fn load_policy(
    pool: &crate::db::DatabasePool,
    provider: Provider,
) -> Result<VideoPolicy, sqlx::Error> {
    let sql = "SELECT provider, enabled, allow_hosts_json, max_redirects, max_response_bytes,
               max_playlist_depth, max_segments, max_duration_ms, config_json, version, updated_at
               FROM video_provider_policies WHERE provider = ?";
    let provider_str = provider.as_str();
    let row = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(provider_str)
            .fetch_optional(p)
            .await?
            .map(|r| policy_row_sqlite(&r)),
        Either::Right(p) => sqlx::query(sql)
            .bind(provider_str)
            .fetch_optional(p)
            .await?
            .map(|r| policy_row_mysql(&r)),
    };
    Ok(row.unwrap_or_else(|| VideoPolicy::default_for(provider)))
}

fn policy_row_sqlite(r: &sqlx::sqlite::SqliteRow) -> VideoPolicy {
    policy_row_values(
        Provider::parse(&r.get::<String, _>("provider")).unwrap_or(Provider::Direct),
        r.get::<i64, _>("enabled") != 0,
        r.get::<Option<String>, _>("allow_hosts_json").as_deref(),
        r.get::<i64, _>("max_redirects") as u32,
        r.get::<i64, _>("max_response_bytes"),
        r.get::<i64, _>("max_playlist_depth") as usize,
        r.get::<i64, _>("max_segments") as usize,
        r.get::<i64, _>("max_duration_ms"),
        r.get::<Option<String>, _>("config_json").as_deref(),
        r.get::<i64, _>("version"),
        r.get::<i64, _>("updated_at"),
    )
}

fn policy_row_mysql(r: &sqlx::mysql::MySqlRow) -> VideoPolicy {
    policy_row_values(
        Provider::parse(&r.get::<String, _>("provider")).unwrap_or(Provider::Direct),
        r.get::<i64, _>("enabled") != 0,
        r.get::<Option<String>, _>("allow_hosts_json").as_deref(),
        r.get::<i64, _>("max_redirects") as u32,
        r.get::<i64, _>("max_response_bytes"),
        r.get::<i64, _>("max_playlist_depth") as usize,
        r.get::<i64, _>("max_segments") as usize,
        r.get::<i64, _>("max_duration_ms"),
        r.get::<Option<String>, _>("config_json").as_deref(),
        r.get::<i64, _>("version"),
        r.get::<i64, _>("updated_at"),
    )
}

#[allow(clippy::too_many_arguments)]
fn policy_row_values(
    provider: Provider,
    enabled: bool,
    allow_hosts_json: Option<&str>,
    max_redirects: u32,
    max_response_bytes: i64,
    max_playlist_depth: usize,
    max_segments: usize,
    max_duration_ms: i64,
    config_json: Option<&str>,
    version: i64,
    updated_at: i64,
) -> VideoPolicy {
    let allow_hosts = allow_hosts_json
        .and_then(|s| serde_json::from_str::<Vec<String>>(s).ok())
        .unwrap_or_default();
    let config = config_json
        .and_then(|s| serde_json::from_str::<Value>(s).ok())
        .unwrap_or_else(|| Value::Object(Default::default()));
    VideoPolicy {
        provider,
        enabled,
        allow_hosts,
        max_redirects,
        max_response_bytes,
        max_playlist_depth,
        max_segments,
        max_duration_ms,
        config,
        version,
        updated_at,
    }
}

/// 管理端更新 Provider 策略（If-Match 乐观并发；写入后触发历史引用重检）。
///
/// 返回 `(新策略, 被降级的引用数)`。
pub async fn update_provider_policy(
    pool: &crate::db::DatabasePool,
    provider: Provider,
    body: &Value,
    expected_version: i64,
    now: i64,
) -> Result<(VideoPolicy, u64), VideoError> {
    let current = load_policy(pool, provider)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?;
    if expected_version != current.version {
        return Err(VideoError::VersionConflict {
            expected: expected_version,
            current: current.version,
        });
    }

    let enabled = body
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(current.enabled);
    let allow_hosts = match body.get("allow_hosts") {
        Some(Value::Array(items)) => {
            let raw: Vec<String> = items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect();
            validate_host_list(&raw).map_err(VideoError::Classify)?
        }
        Some(_) => return Err(VideoError::Invalid("allow_hosts 必须是字符串数组".into())),
        None => current.allow_hosts.clone(),
    };
    let max_redirects = body
        .get("max_redirects")
        .and_then(Value::as_u64)
        .map(|v| v as u32)
        .unwrap_or(current.max_redirects);
    let max_response_bytes = body
        .get("max_response_bytes")
        .and_then(Value::as_i64)
        .unwrap_or(current.max_response_bytes);
    let max_playlist_depth = body
        .get("max_playlist_depth")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(current.max_playlist_depth);
    let max_segments = body
        .get("max_segments")
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(current.max_segments);
    let max_duration_ms = body
        .get("max_duration_ms")
        .and_then(Value::as_i64)
        .unwrap_or(current.max_duration_ms);
    let config = match body.get("config") {
        Some(v) => validate_config(v).map_err(VideoError::Invalid)?,
        None => current.config.clone(),
    };

    // 数值上限（保守约束）。
    if max_redirects > MAX_REDIRECTS_LIMIT {
        return Err(VideoError::Invalid("max_redirects 超限".into()));
    }
    if !(MIN_RESPONSE_BYTES..=MAX_RESPONSE_BYTES_LIMIT).contains(&max_response_bytes) {
        return Err(VideoError::Invalid("max_response_bytes 超出范围".into()));
    }
    if max_playlist_depth == 0 || max_playlist_depth > MAX_PLAYLIST_DEPTH_LIMIT {
        return Err(VideoError::Invalid("max_playlist_depth 超出范围".into()));
    }
    if max_segments == 0 || max_segments > MAX_SEGMENTS_LIMIT {
        return Err(VideoError::Invalid("max_segments 超出范围".into()));
    }
    if !(1..=MAX_DURATION_MS_LIMIT).contains(&max_duration_ms) {
        return Err(VideoError::Invalid("max_duration_ms 超出范围".into()));
    }

    let new_version = current.version + 1;
    let allow_hosts_json = serde_json::to_string(&allow_hosts).unwrap_or_else(|_| "[]".into());
    let config_json = serde_json::to_string(&config).unwrap_or_else(|_| "{}".into());

    // 行是否存在（缺省策略与真实行 version 同为 1，需显式区分）。
    let existing: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT version FROM video_provider_policies WHERE provider = ?")
                .bind(provider.as_str())
                .fetch_optional(p)
                .await
                .map_err(|e| VideoError::Db(e.to_string()))?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT version FROM video_provider_policies WHERE provider = ?")
                .bind(provider.as_str())
                .fetch_optional(p)
                .await
                .map_err(|e| VideoError::Db(e.to_string()))?
        }
    };

    match existing {
        None => {
            if expected_version != 1 {
                return Err(VideoError::VersionConflict {
                    expected: expected_version,
                    current: 1,
                });
            }
            match pool {
                Either::Left(p) => {
                    sqlx::query(
                        "INSERT INTO video_provider_policies
                         (provider, enabled, allow_hosts_json, max_redirects, max_response_bytes,
                          max_playlist_depth, max_segments, max_duration_ms, config_json, version,
                          updated_by, updated_at)
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'system', ?)",
                    )
                    .bind(provider.as_str())
                    .bind(enabled)
                    .bind(&allow_hosts_json)
                    .bind(max_redirects as i64)
                    .bind(max_response_bytes)
                    .bind(max_playlist_depth as i64)
                    .bind(max_segments as i64)
                    .bind(max_duration_ms)
                    .bind(&config_json)
                    .bind(new_version)
                    .bind(now)
                    .execute(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?;
                }
                Either::Right(p) => {
                    sqlx::query(
                        "INSERT INTO video_provider_policies
                         (provider, enabled, allow_hosts_json, max_redirects, max_response_bytes,
                          max_playlist_depth, max_segments, max_duration_ms, config_json, version,
                          updated_by, updated_at)
                         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'system', ?)",
                    )
                    .bind(provider.as_str())
                    .bind(enabled)
                    .bind(&allow_hosts_json)
                    .bind(max_redirects as i64)
                    .bind(max_response_bytes)
                    .bind(max_playlist_depth as i64)
                    .bind(max_segments as i64)
                    .bind(max_duration_ms)
                    .bind(&config_json)
                    .bind(new_version)
                    .bind(now)
                    .execute(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?;
                }
            }
        }
        Some(current_version) => {
            if expected_version != current_version {
                return Err(VideoError::VersionConflict {
                    expected: expected_version,
                    current: current_version,
                });
            }
            let affected: u64 = match pool {
                Either::Left(p) => sqlx::query(
                    "UPDATE video_provider_policies SET enabled = ?, allow_hosts_json = ?,
                         max_redirects = ?, max_response_bytes = ?, max_playlist_depth = ?,
                         max_segments = ?, max_duration_ms = ?, config_json = ?, version = ?,
                         updated_at = ? WHERE provider = ? AND version = ?",
                )
                .bind(enabled)
                .bind(&allow_hosts_json)
                .bind(max_redirects as i64)
                .bind(max_response_bytes)
                .bind(max_playlist_depth as i64)
                .bind(max_segments as i64)
                .bind(max_duration_ms)
                .bind(&config_json)
                .bind(new_version)
                .bind(now)
                .bind(provider.as_str())
                .bind(expected_version)
                .execute(p)
                .await
                .map_err(|e| VideoError::Db(e.to_string()))?
                .rows_affected(),
                Either::Right(p) => sqlx::query(
                    "UPDATE video_provider_policies SET enabled = ?, allow_hosts_json = ?,
                         max_redirects = ?, max_response_bytes = ?, max_playlist_depth = ?,
                         max_segments = ?, max_duration_ms = ?, config_json = ?, version = ?,
                         updated_at = ? WHERE provider = ? AND version = ?",
                )
                .bind(enabled)
                .bind(&allow_hosts_json)
                .bind(max_redirects as i64)
                .bind(max_response_bytes)
                .bind(max_playlist_depth as i64)
                .bind(max_segments as i64)
                .bind(max_duration_ms)
                .bind(&config_json)
                .bind(new_version)
                .bind(now)
                .bind(provider.as_str())
                .bind(expected_version)
                .execute(p)
                .await
                .map_err(|e| VideoError::Db(e.to_string()))?
                .rows_affected(),
            };
            if affected == 0 {
                return Err(VideoError::VersionConflict {
                    expected: expected_version,
                    current: current_version,
                });
            }
        }
    }

    let policy = load_policy(pool, provider)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?;
    let downgraded = recheck_references(pool, provider, now).await?;
    Ok((policy, downgraded))
}

// ─────────────────────────── resolve / create ───────────────────────────

/// POST /video-embeds/resolve：离线分类 + 策略门 + 签发短效 resolution_id。
///
/// 不发起网络；签名 URL / iframe HTML / Key 在分类阶段拒绝，绝不回显。
#[allow(clippy::too_many_arguments)]
pub async fn resolve_source(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    source_url: &str,
    target: &VideoTarget,
    now: i64,
) -> Result<ResolvedView, VideoError> {
    let classified = classify(source_url)?;
    let policy = load_policy(pool, classified.provider)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?;
    if !policy.enabled {
        return Err(VideoError::ProviderDisabled);
    }
    if !is_allowed_host(&classified.host, &policy.allow_hosts) {
        return Err(VideoError::HostNotAllowed(classified.host.clone()));
    }

    let resolution_id = issue_resolution(
        user_id,
        classified.provider,
        classified.source.clone(),
        classified.source_hash.clone(),
        classified.official_url.clone(),
        classified.host.clone(),
        classified.media_type.clone(),
        classified.external_id.clone(),
        classified.title.clone(),
        classified.embeddable,
        policy.version,
        now,
    );

    Ok(ResolvedView {
        resolution_id,
        provider: classified.provider.as_str().to_string(),
        media_type: classified.media_type,
        official_url: classified.official_url,
        source_host: classified.host,
        title: classified.title,
        policy_version: policy.version,
        policy_enabled: policy.enabled,
        embeddable: classified.embeddable,
        expires_at: now + crate::video::RESOLUTION_TTL_MS,
        target: target.clone(),
    })
}

/// POST /video-embeds：消费 resolution_id 并绑定目标（发布/创建时重新校验
/// 当前策略与目标所有权）。
pub async fn create_embed(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    resolution_id: &str,
    target: &VideoTarget,
    expected_policy_version: i64,
    now: i64,
) -> Result<EmbedView, VideoError> {
    let rec =
        consume_resolution(user_id, resolution_id, now).ok_or(VideoError::ResolutionExpired)?;
    if rec.policy_version != expected_policy_version {
        return Err(VideoError::PolicyVersionConflict {
            expected: expected_policy_version,
            current: rec.policy_version,
        });
    }
    // 创建时必须重新校验当前策略（策略可能在 resolve 后变更）。
    let policy = load_policy(pool, rec.provider)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?;
    if !policy.enabled {
        return Err(VideoError::ProviderDisabled);
    }
    if !is_allowed_host(&rec.host, &policy.allow_hosts) {
        return Err(VideoError::HostNotAllowed(rec.host.clone()));
    }
    check_target_owner(pool, user_id, target).await?;

    let id = uuid::Uuid::now_v7().to_string();
    let insert_sql = "INSERT INTO video_embeds
        (id, user_id, resolution_id, source, source_hash, provider, status, target_type,
         target_id, title, poster_attachment_id, official_url, error_class, policy_version,
         version, created_at, updated_at)
        VALUES (?, ?, ?, ?, ?, ?, 'pending', ?, ?, ?, NULL, ?, NULL, ?, 1, ?, ?)";
    let insert_result = match pool {
        Either::Left(p) => sqlx::query(insert_sql)
            .bind(&id)
            .bind(user_id)
            .bind(resolution_id)
            .bind(&rec.source)
            .bind(&rec.source_hash)
            .bind(rec.provider.as_str())
            .bind(&target.target_type)
            .bind(&target.target_id)
            .bind(&rec.title)
            .bind(&rec.official_url)
            .bind(rec.policy_version)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(insert_sql)
            .bind(&id)
            .bind(user_id)
            .bind(resolution_id)
            .bind(&rec.source)
            .bind(&rec.source_hash)
            .bind(rec.provider.as_str())
            .bind(&target.target_type)
            .bind(&target.target_id)
            .bind(&rec.title)
            .bind(&rec.official_url)
            .bind(rec.policy_version)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ()),
    };
    if let Err(e) = insert_result {
        if is_unique_violation(&e) {
            return Err(VideoError::TargetConflict);
        }
        return Err(VideoError::Db(e.to_string()));
    }

    let row = EmbedRow {
        id,
        user_id: user_id.to_string(),
        provider: rec.provider,
        status: "pending".to_string(),
        target_type: target.target_type.clone(),
        target_id: target.target_id.clone(),
        title: rec.title.clone(),
        poster_attachment_id: None,
        official_url: Some(rec.official_url.clone()),
        error_class: None,
        policy_version: rec.policy_version,
        version: 1,
        created_at: now,
        updated_at: now,
    };
    build_view(pool, &row, user_id, now).await
}

/// 目标所有权/权限门：作者本人或管理员。
async fn check_target_owner(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    target: &VideoTarget,
) -> Result<(), VideoError> {
    let owner: Option<String> = match target.target_type.as_str() {
        "post" => {
            let sql = "SELECT author_id FROM posts WHERE id = ?";
            match pool {
                Either::Left(p) => sqlx::query_scalar(sql)
                    .bind(&target.target_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?,
                Either::Right(p) => sqlx::query_scalar(sql)
                    .bind(&target.target_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?,
            }
        }
        "comment" => {
            let sql = "SELECT author_id FROM comments WHERE id = ?";
            match pool {
                Either::Left(p) => sqlx::query_scalar(sql)
                    .bind(&target.target_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?,
                Either::Right(p) => sqlx::query_scalar(sql)
                    .bind(&target.target_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?,
            }
        }
        _ => {
            return Err(VideoError::Invalid(
                "target_type 必须是 post 或 comment".into(),
            ))
        }
    };
    match owner {
        None => Err(VideoError::TargetNotFound),
        Some(owner) if owner != user_id => {
            if is_admin(pool, user_id).await? {
                Ok(())
            } else {
                Err(VideoError::TargetForbidden)
            }
        }
        Some(_) => Ok(()),
    }
}

async fn is_admin(pool: &crate::db::DatabasePool, user_id: &str) -> Result<bool, VideoError> {
    let decision = authorize_action(pool, user_id, "admin.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(VideoError::Internal)?;
    Ok(decision.is_allowed())
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(
        e,
        sqlx::Error::Database(db) if db.message().to_lowercase().contains("unique")
    )
}

// ─────────────────────────── 读取 / 更新 / 删除 ───────────────────────────

/// GET /video-embeds/{id}：当前请求方可见投影（owner/admin；removed → 404）。
pub async fn get_embed(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    id: &str,
    _now: i64,
) -> Result<EmbedView, VideoError> {
    let row = fetch_embed(pool, id)
        .await?
        .ok_or(VideoError::EmbedNotFound)?;
    if row.status == "removed" {
        return Err(VideoError::EmbedNotFound);
    }
    if row.user_id != user_id && !is_admin(pool, user_id).await? {
        return Err(VideoError::EmbedNotFound);
    }
    build_view(pool, &row, user_id, row.updated_at).await
}

/// PATCH /video-embeds/{id}：用户可编辑字段（标题/封面附件；If-Match）。
pub async fn update_embed(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    id: &str,
    title_override: Option<Option<String>>,
    poster_override: Option<Option<String>>,
    expected_version: i64,
    now: i64,
) -> Result<EmbedView, VideoError> {
    let row = fetch_embed(pool, id)
        .await?
        .ok_or(VideoError::EmbedNotFound)?;
    if row.status == "removed" {
        return Err(VideoError::EmbedNotFound);
    }
    if row.user_id != user_id && !is_admin(pool, user_id).await? {
        return Err(VideoError::EmbedNotFound);
    }
    if expected_version != row.version {
        return Err(VideoError::VersionConflict {
            expected: expected_version,
            current: row.version,
        });
    }

    // 封面附件必须属于当前用户。
    if let Some(Some(attachment_id)) = &poster_override {
        let owner: Option<String> = match pool {
            Either::Left(p) => sqlx::query_scalar("SELECT owner_id FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_optional(p)
                .await
                .map_err(|e| VideoError::Db(e.to_string()))?,
            Either::Right(p) => sqlx::query_scalar("SELECT owner_id FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_optional(p)
                .await
                .map_err(|e| VideoError::Db(e.to_string()))?,
        };
        if owner.as_deref() != Some(user_id) {
            return Err(VideoError::PosterAttachmentInvalid);
        }
    }

    let title = title_override.unwrap_or(row.title.clone());
    let poster = poster_override.unwrap_or(row.poster_attachment_id.clone());
    let affected: u64 = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE video_embeds SET title = ?, poster_attachment_id = ?,
             version = version + 1, updated_at = ? WHERE id = ? AND version = ?",
        )
        .bind(&title)
        .bind(&poster)
        .bind(now)
        .bind(id)
        .bind(expected_version)
        .execute(p)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE video_embeds SET title = ?, poster_attachment_id = ?,
             version = version + 1, updated_at = ? WHERE id = ? AND version = ?",
        )
        .bind(&title)
        .bind(&poster)
        .bind(now)
        .bind(id)
        .bind(expected_version)
        .execute(p)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?
        .rows_affected(),
    };
    if affected == 0 {
        return Err(VideoError::VersionConflict {
            expected: expected_version,
            current: row.version,
        });
    }
    let updated = fetch_embed(pool, id)
        .await?
        .ok_or(VideoError::EmbedNotFound)?;
    build_view(pool, &updated, user_id, now).await
}

/// DELETE /video-embeds/{id}：仅删除未引用引用（已发布/已隐藏目标 → 409）。
pub async fn delete_embed(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    id: &str,
    now: i64,
) -> Result<(), VideoError> {
    let row = fetch_embed(pool, id)
        .await?
        .ok_or(VideoError::EmbedNotFound)?;
    if row.status == "removed" {
        return Err(VideoError::EmbedNotFound);
    }
    if row.user_id != user_id && !is_admin(pool, user_id).await? {
        return Err(VideoError::EmbedNotFound);
    }
    if target_referenced(pool, &row).await? {
        return Err(VideoError::EmbedReferenced);
    }
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE video_embeds SET status = 'removed', version = version + 1, updated_at = ?
                 WHERE id = ?",
            )
            .bind(now)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| VideoError::Db(e.to_string()))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE video_embeds SET status = 'removed', version = version + 1, updated_at = ?
                 WHERE id = ?",
            )
            .bind(now)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| VideoError::Db(e.to_string()))?;
        }
    }
    Ok(())
}

/// 目标是否已被发布内容引用（published/hidden 帖或 published 评论）。
async fn target_referenced(
    pool: &crate::db::DatabasePool,
    row: &EmbedRow,
) -> Result<bool, VideoError> {
    match row.target_type.as_str() {
        "post" => {
            let status: Option<String> = match pool {
                Either::Left(p) => sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
                    .bind(&row.target_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?,
                Either::Right(p) => sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
                    .bind(&row.target_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?,
            };
            Ok(matches!(
                status.as_deref(),
                Some("published") | Some("hidden")
            ))
        }
        "comment" => {
            let status: Option<String> = match pool {
                Either::Left(p) => sqlx::query_scalar("SELECT status FROM comments WHERE id = ?")
                    .bind(&row.target_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?,
                Either::Right(p) => sqlx::query_scalar("SELECT status FROM comments WHERE id = ?")
                    .bind(&row.target_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?,
            };
            Ok(status.as_deref() == Some("published"))
        }
        _ => Ok(false),
    }
}

/// 目标可见性（隐藏/审核中/删除 → 不加载第三方播放器）。
async fn target_visible(pool: &crate::db::DatabasePool, row: &EmbedRow) -> bool {
    match row.target_type.as_str() {
        "post" => {
            let status: Option<String> = match pool {
                Either::Left(p) => sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
                    .bind(&row.target_id)
                    .fetch_optional(p)
                    .await
                    .ok()
                    .flatten(),
                Either::Right(p) => sqlx::query_scalar("SELECT status FROM posts WHERE id = ?")
                    .bind(&row.target_id)
                    .fetch_optional(p)
                    .await
                    .ok()
                    .flatten(),
            };
            !matches!(
                status.as_deref(),
                Some("hidden") | Some("deleted") | Some("rejected") | Some("pending_review")
            )
        }
        "comment" => {
            let status: Option<String> = match pool {
                Either::Left(p) => sqlx::query_scalar("SELECT status FROM comments WHERE id = ?")
                    .bind(&row.target_id)
                    .fetch_optional(p)
                    .await
                    .ok()
                    .flatten(),
                Either::Right(p) => sqlx::query_scalar("SELECT status FROM comments WHERE id = ?")
                    .bind(&row.target_id)
                    .fetch_optional(p)
                    .await
                    .ok()
                    .flatten(),
            };
            status.as_deref() == Some("published")
        }
        _ => false,
    }
}

/// 组装可见投影（策略重校验 + 渲染决策）。
async fn build_view(
    pool: &crate::db::DatabasePool,
    row: &EmbedRow,
    _user_id: &str,
    _now: i64,
) -> Result<EmbedView, VideoError> {
    let policy = load_policy(pool, row.provider)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?;
    let visible = target_visible(pool, row).await;
    let external_id = if row.provider == Provider::Xigua {
        row.official_url
            .as_deref()
            .and_then(|u| url::Url::parse(u).ok())
            .and_then(|u| extract_video_id(u.path()))
    } else {
        None
    };
    let host = row.host();
    let render = render_for(
        &row.status,
        &row.provider,
        (!host.is_empty()).then_some(host.as_str()),
        external_id.as_deref(),
        row.provider == Provider::Xigua && is_xigua_host(&host),
        policy.xigua_allow_embed(),
        visible,
    );
    // 隐藏/封禁内容不渲染视频 URL。
    let official_url = if render.mode == "none" {
        None
    } else {
        row.official_url.clone()
    };
    Ok(EmbedView {
        id: row.id.clone(),
        user_id: row.user_id.clone(),
        provider: row.provider.as_str().to_string(),
        status: row.status.clone(),
        target: VideoTarget {
            target_type: row.target_type.clone(),
            target_id: row.target_id.clone(),
        },
        title: row.title.clone(),
        poster_attachment_id: row.poster_attachment_id.clone(),
        official_url,
        source_host: if host.is_empty() { None } else { Some(host) },
        media_type: row.media_type(),
        external_id,
        error_class: row.error_class.clone(),
        policy_version: row.policy_version,
        version: row.version,
        created_at: row.created_at,
        updated_at: row.updated_at,
        render,
    })
}

// ─────────────────────────── refresh / recheck ───────────────────────────

/// POST /video-embeds/{id}/refresh：按当前策略异步重新解析（同步服务函数；
/// 路由层负责 202 + spawn）。失败时保留安全外链（status=error），不阻塞发帖。
pub async fn refresh_embed(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    id: &str,
    client: &dyn FetchClient,
    now: i64,
) -> Result<(), VideoError> {
    let row = fetch_embed(pool, id)
        .await?
        .ok_or(VideoError::EmbedNotFound)?;
    if row.status == "removed" {
        return Err(VideoError::EmbedNotFound);
    }
    if row.user_id != user_id && !is_admin(pool, user_id).await? {
        return Err(VideoError::EmbedNotFound);
    }
    // blocked（侵权/平台通知）不自动刷新。
    if row.status == "blocked" {
        return Ok(());
    }

    let policy = load_policy(pool, row.provider)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?;
    let host = row.host();
    if !policy.allows_host(&host) {
        let class = if policy.enabled {
            "video_policy_changed"
        } else {
            "video_provider_disabled"
        };
        set_error(pool, &row.id, class, now).await?;
        return Ok(());
    }

    let registry = ProviderRegistry::builtin();
    let provider = registry
        .get(row.provider.as_str())
        .ok_or_else(|| VideoError::Internal("unknown provider".into()))?;
    let input = crate::video::provider::RefreshInput {
        source: row.official_url.clone().unwrap_or_default(),
        media_type: row.media_type(),
        policy,
    };
    match provider.refresh(&input, client).await {
        Ok(_) => {
            let current_policy = load_policy(pool, row.provider)
                .await
                .map_err(|e| VideoError::Db(e.to_string()))?;
            match pool {
                Either::Left(p) => {
                    sqlx::query(
                        "UPDATE video_embeds SET status = 'ready', error_class = NULL,
                         policy_version = ?, version = version + 1, updated_at = ?
                         WHERE id = ?",
                    )
                    .bind(current_policy.version)
                    .bind(now)
                    .bind(&row.id)
                    .execute(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?;
                }
                Either::Right(p) => {
                    sqlx::query(
                        "UPDATE video_embeds SET status = 'ready', error_class = NULL,
                         policy_version = ?, version = version + 1, updated_at = ?
                         WHERE id = ?",
                    )
                    .bind(current_policy.version)
                    .bind(now)
                    .bind(&row.id)
                    .execute(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?;
                }
            }
            Ok(())
        }
        Err(e) => {
            set_error(pool, &row.id, e.code(), now).await?;
            Ok(())
        }
    }
}

/// 写入 error 状态（保留 official_url → 渲染外链卡片；blocked 除外）。
async fn set_error(
    pool: &crate::db::DatabasePool,
    id: &str,
    error_class: &str,
    now: i64,
) -> Result<(), VideoError> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE video_embeds SET status = 'error', error_class = ?,
                 version = version + 1, updated_at = ? WHERE id = ? AND status != 'removed'",
            )
            .bind(error_class)
            .bind(now)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| VideoError::Db(e.to_string()))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE video_embeds SET status = 'error', error_class = ?,
                 version = version + 1, updated_at = ? WHERE id = ? AND status != 'removed'",
            )
            .bind(error_class)
            .bind(now)
            .bind(id)
            .execute(p)
            .await
            .map_err(|e| VideoError::Db(e.to_string()))?;
        }
    }
    Ok(())
}

/// 历史引用重检查：策略变更后对目标 Provider 的引用重新裁决——
/// 继续嵌入（同步 policy_version）或降级外链（error + 稳定 error_class）。
/// 返回被降级/同步的引用数。
pub async fn recheck_references(
    pool: &crate::db::DatabasePool,
    provider: Provider,
    now: i64,
) -> Result<u64, VideoError> {
    let policy = load_policy(pool, provider)
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?;
    let rows = fetch_all_by_provider(pool, provider.as_str())
        .await
        .map_err(|e| VideoError::Db(e.to_string()))?;
    let mut changed = 0u64;
    for row in rows {
        if !matches!(row.status.as_str(), "pending" | "ready" | "error") {
            continue;
        }
        let host = row.host();
        if !policy.allows_host(&host) {
            let class = if policy.enabled {
                "video_policy_changed"
            } else {
                "video_provider_disabled"
            };
            set_error(pool, &row.id, class, now).await?;
            changed += 1;
        } else if row.policy_version != policy.version {
            // 继续嵌入：仅同步策略版本（下次 refresh 走新策略）。
            match pool {
                Either::Left(p) => {
                    sqlx::query(
                        "UPDATE video_embeds SET policy_version = ?, version = version + 1,
                         updated_at = ? WHERE id = ?",
                    )
                    .bind(policy.version)
                    .bind(now)
                    .bind(&row.id)
                    .execute(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?;
                }
                Either::Right(p) => {
                    sqlx::query(
                        "UPDATE video_embeds SET policy_version = ?, version = version + 1,
                         updated_at = ? WHERE id = ?",
                    )
                    .bind(policy.version)
                    .bind(now)
                    .bind(&row.id)
                    .execute(p)
                    .await
                    .map_err(|e| VideoError::Db(e.to_string()))?;
                }
            }
            changed += 1;
        }
    }
    Ok(changed)
}

// ─────────────────────────── 行读取 ───────────────────────────

async fn fetch_embed(
    pool: &crate::db::DatabasePool,
    id: &str,
) -> Result<Option<EmbedRow>, sqlx::Error> {
    let sql = "SELECT id, user_id, provider, status, target_type, target_id, title,
               poster_attachment_id, official_url, error_class, policy_version, version,
               created_at, updated_at FROM video_embeds WHERE id = ?";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(id)
            .fetch_optional(p)
            .await
            .map(|r| r.map(|row| embed_row_sqlite(&row))),
        Either::Right(p) => sqlx::query(sql)
            .bind(id)
            .fetch_optional(p)
            .await
            .map(|r| r.map(|row| embed_row_mysql(&row))),
    }
}

async fn fetch_all_by_provider(
    pool: &crate::db::DatabasePool,
    provider: &str,
) -> Result<Vec<EmbedRow>, sqlx::Error> {
    let sql = "SELECT id, user_id, provider, status, target_type, target_id, title,
               poster_attachment_id, official_url, error_class, policy_version, version,
               created_at, updated_at FROM video_embeds WHERE provider = ?";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(provider)
            .fetch_all(p)
            .await
            .map(|rows| rows.iter().map(embed_row_sqlite).collect()),
        Either::Right(p) => sqlx::query(sql)
            .bind(provider)
            .fetch_all(p)
            .await
            .map(|rows| rows.iter().map(embed_row_mysql).collect()),
    }
}

fn embed_row_sqlite(r: &sqlx::sqlite::SqliteRow) -> EmbedRow {
    embed_row_values(
        &r.get::<String, _>("id"),
        &r.get::<String, _>("user_id"),
        &r.get::<String, _>("provider"),
        &r.get::<String, _>("status"),
        &r.get::<String, _>("target_type"),
        &r.get::<String, _>("target_id"),
        r.get::<Option<String>, _>("title"),
        r.get::<Option<String>, _>("poster_attachment_id"),
        r.get::<Option<String>, _>("official_url"),
        r.get::<Option<String>, _>("error_class"),
        r.get::<i64, _>("policy_version"),
        r.get::<i64, _>("version"),
        r.get::<i64, _>("created_at"),
        r.get::<i64, _>("updated_at"),
    )
}

fn embed_row_mysql(r: &sqlx::mysql::MySqlRow) -> EmbedRow {
    embed_row_values(
        &r.get::<String, _>("id"),
        &r.get::<String, _>("user_id"),
        &r.get::<String, _>("provider"),
        &r.get::<String, _>("status"),
        &r.get::<String, _>("target_type"),
        &r.get::<String, _>("target_id"),
        r.get::<Option<String>, _>("title"),
        r.get::<Option<String>, _>("poster_attachment_id"),
        r.get::<Option<String>, _>("official_url"),
        r.get::<Option<String>, _>("error_class"),
        r.get::<i64, _>("policy_version"),
        r.get::<i64, _>("version"),
        r.get::<i64, _>("created_at"),
        r.get::<i64, _>("updated_at"),
    )
}

#[allow(clippy::too_many_arguments)]
fn embed_row_values(
    id: &str,
    user_id: &str,
    provider: &str,
    status: &str,
    target_type: &str,
    target_id: &str,
    title: Option<String>,
    poster_attachment_id: Option<String>,
    official_url: Option<String>,
    error_class: Option<String>,
    policy_version: i64,
    version: i64,
    created_at: i64,
    updated_at: i64,
) -> EmbedRow {
    EmbedRow {
        id: id.to_string(),
        user_id: user_id.to_string(),
        provider: Provider::parse(provider).unwrap_or(Provider::Direct),
        status: status.to_string(),
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
        title,
        poster_attachment_id,
        official_url,
        error_class,
        policy_version,
        version,
        created_at,
        updated_at,
    }
}

/// 当前时间（毫秒）。
pub fn now() -> i64 {
    now_millis()
}

/// 供路由层做轻量校验：target_type 是否合法。
pub fn valid_target_type(t: &str) -> bool {
    t == "post" || t == "comment"
}
