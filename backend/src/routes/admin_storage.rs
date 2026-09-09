//! M06-QUOTA 管理路由：存储配置（脱敏）、测试连接与等级附件配额。
//!
//! OpenAPI 路由契约：
//! - `GET/PATCH /api/v1/admin/storage/config`（get/patch_admin_storage_config）
//! - `POST /api/v1/admin/storage/test`（post_admin_storage_test）
//! - `GET/PATCH /api/v1/admin/levels/{id}/attachment-quota`
//!
//! 权限门：`admin.manage`；PATCH/POST 额外要求 reason + recent-auth
//! （step-up，M02-MFA-07）+ 审计（with_reason + with_policy_version）。

use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{
    app::AppState,
    audit::AuditEntry,
    auth::session::{is_step_up_required_for_session, AuthSession, SESSION_COOKIE_NAME},
    authz::decision::AUTHZ_POLICY_VERSION,
    authz::enforce::authorize_action,
    config::AppConfig,
    error::AppError,
    storage::adapter::{S3Adapter, S3Config, StorageAdapter},
    storage::error::StorageError,
    storage::quota::{
        get_policy_for_level, get_policy_revisions, update_level_quota, PRESIGN_TTL_SECS,
    },
};

/// M06-QUOTA 管理路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/storage/config",
            get(get_storage_config).patch(update_storage_config),
        )
        .route("/api/v1/admin/storage/test", post(test_storage))
        .route(
            "/api/v1/admin/levels/{id}/attachment-quota",
            get(get_attachment_quota).patch(update_attachment_quota),
        )
}

/// 管理权限门（M03-AUTHZ-05）：admin.manage + 账号状态实时门。
async fn require_admin(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, "admin.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(crate::authz::enforce::deny_to_error(
            crate::authz::enforce::denied_reason(&decision)
                .unwrap_or(crate::authz::decision::DenyReason::DefaultDeny),
            request_id,
        ));
    }
    Ok(())
}

/// 高风险管理操作必填 reason。
#[allow(clippy::result_large_err)] // AppError 为统一错误类型（与 auth/session 同约定）
fn required_reason(body: &Value, request_id: &str) -> Result<String, AppError> {
    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin storage operation",
            request_id,
            None,
        ));
    }
    Ok(reason)
}

/// step-up 门（M02-MFA-07）：会话近期未重新认证 → `step_up_required`。
async fn require_step_up(
    pool: &crate::db::DatabasePool,
    headers: &HeaderMap,
    window_secs: u64,
    request_id: &str,
) -> Result<(), AppError> {
    let session_token = session_token_from_headers(headers)
        .ok_or_else(|| AppError::unauthorized("authentication required", request_id))?;
    let required = is_step_up_required_for_session(pool, &session_token, window_secs)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if required {
        return Err(AppError::step_up_required(request_id));
    }
    Ok(())
}

/// 从 Cookie 头提取会话 token（step-up 判定用；与 posts.rs 同一模式）。
fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get("cookie")?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        if k == SESSION_COOKIE_NAME {
            Some(v.to_string())
        } else {
            None
        }
    })
}

// ────────────────────────── 存储配置（脱敏）──────────────────────────────

#[derive(sqlx::FromRow, Clone)]
pub struct StorageSettingsRow {
    pub storage_backend: String,
    pub storage_local_path: String,
    pub storage_upload_max_bytes: i64,
    pub storage_s3_endpoint: String,
    pub storage_s3_region: String,
    pub storage_s3_bucket: String,
    pub storage_s3_access_key_id: String,
    pub storage_s3_secret_access_key: String,
    pub storage_s3_path_style: i64,
    pub storage_s3_public_base_url: String,
    pub storage_s3_signed_url_ttl: i64,
}

/// 全空行（等价 0066 迁移的列默认值）：DB 单行缺失时的回退基线。
impl Default for StorageSettingsRow {
    fn default() -> Self {
        Self {
            storage_backend: "local".to_string(),
            storage_local_path: String::new(),
            storage_upload_max_bytes: 20 * 1024 * 1024,
            storage_s3_endpoint: String::new(),
            storage_s3_region: "us-east-1".to_string(),
            storage_s3_bucket: String::new(),
            storage_s3_access_key_id: String::new(),
            storage_s3_secret_access_key: String::new(),
            storage_s3_path_style: 0,
            storage_s3_public_base_url: String::new(),
            storage_s3_signed_url_ttl: 300,
        }
    }
}

pub async fn load_storage_settings(
    pool: &crate::db::DatabasePool,
) -> Result<Option<StorageSettingsRow>, sqlx::Error> {
    let sql = "SELECT storage_backend, storage_local_path, storage_upload_max_bytes,
                      storage_s3_endpoint, storage_s3_region, storage_s3_bucket,
                      storage_s3_access_key_id, storage_s3_secret_access_key,
                      storage_s3_path_style, storage_s3_public_base_url,
                      storage_s3_signed_url_ttl
               FROM site_settings WHERE id = 'singleton'";
    match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, StorageSettingsRow>(sql)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, StorageSettingsRow>(sql)
                .fetch_optional(p)
                .await
        }
    }
}

/// 将数据库存储配置转为 StorageConfig
pub fn build_storage_config_from_db(
    app_config: &AppConfig,
    row: &StorageSettingsRow,
) -> crate::storage::StorageConfig {
    let s3 = if row.storage_backend == "s3" && !row.storage_s3_bucket.is_empty() {
        Some(crate::storage::S3Config {
            bucket: row.storage_s3_bucket.clone(),
            region: if row.storage_s3_region.is_empty() {
                "us-east-1".to_string()
            } else {
                row.storage_s3_region.clone()
            },
            endpoint: if row.storage_s3_endpoint.is_empty() {
                None
            } else {
                Some(row.storage_s3_endpoint.clone())
            },
            path_style: row.storage_s3_path_style != 0,
            access_key_id: if row.storage_s3_access_key_id.is_empty() {
                None
            } else {
                Some(row.storage_s3_access_key_id.clone())
            },
            secret_access_key: if row.storage_s3_secret_access_key.is_empty() {
                None
            } else {
                Some(row.storage_s3_secret_access_key.clone())
            },
            session_token: None,
        })
    } else {
        None
    };
    let local_root = if !row.storage_local_path.is_empty() {
        std::path::PathBuf::from(&row.storage_local_path)
    } else {
        app_config.storage_dir.clone()
    };
    crate::storage::StorageConfig { local_root, s3 }
}

/// GET /api/v1/admin/storage/config — 脱敏配置（backend/path_style/TTL；
/// **不返回 Secret**，M06-ADAPTER-03/09）。
async fn get_storage_config(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "getAdminStorageConfig";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let db_row = load_storage_settings(pool).await.unwrap_or(None);
    Ok(Json(storage_config_json(&state.config, db_row.as_ref())).into_response())
}

/// PATCH /api/v1/admin/storage/config — 在线保存存储配置并热重载（写入数据库即刻生效）。
async fn update_storage_config(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "patchAdminStorageConfig";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    require_admin(pool, &user.id, request_id).await?;
    // 契约要求 If-Match
    headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?;
    let reason = required_reason(&body, request_id)?;
    require_step_up(pool, &headers, state.config.step_up_window_secs, request_id).await?;

    let update: StorageConfigUpdate = serde_json::from_value(body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    validate_storage_config_update(&update, !state.config.is_production(), request_id)?;

    // 1. 读取当前数据库配置
    let current_row = load_storage_settings(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .unwrap_or_default();

    // 2. 合并更新
    let new_backend = update.backend.unwrap_or(current_row.storage_backend);
    let new_local_path = update.local_path.unwrap_or(current_row.storage_local_path);
    let new_max_bytes = update
        .upload_max_bytes
        .unwrap_or(current_row.storage_upload_max_bytes);
    let new_s3_endpoint = update
        .s3_endpoint
        .unwrap_or(current_row.storage_s3_endpoint);
    let new_s3_region = update.s3_region.unwrap_or(current_row.storage_s3_region);
    let new_s3_bucket = update.bucket.unwrap_or(current_row.storage_s3_bucket);
    let new_s3_ak = update
        .s3_access_key_id
        .filter(|s| !s.is_empty())
        .unwrap_or(current_row.storage_s3_access_key_id);
    let new_s3_sk = update
        .s3_secret_access_key
        .filter(|s| !s.is_empty())
        .unwrap_or(current_row.storage_s3_secret_access_key);
    let new_s3_path_style = update
        .path_style
        .map(|b| if b { 1i64 } else { 0i64 })
        .unwrap_or(current_row.storage_s3_path_style);
    let new_s3_public_url = update
        .s3_public_base_url
        .unwrap_or(current_row.storage_s3_public_base_url);
    let new_s3_ttl = update
        .signed_url_ttl_seconds
        .map(|t| t as i64)
        .unwrap_or(current_row.storage_s3_signed_url_ttl);

    let updated_row = StorageSettingsRow {
        storage_backend: new_backend.clone(),
        storage_local_path: new_local_path.clone(),
        storage_upload_max_bytes: new_max_bytes,
        storage_s3_endpoint: new_s3_endpoint.clone(),
        storage_s3_region: new_s3_region.clone(),
        storage_s3_bucket: new_s3_bucket.clone(),
        storage_s3_access_key_id: new_s3_ak.clone(),
        storage_s3_secret_access_key: new_s3_sk.clone(),
        storage_s3_path_style: new_s3_path_style,
        storage_s3_public_base_url: new_s3_public_url.clone(),
        storage_s3_signed_url_ttl: new_s3_ttl,
    };

    // 3. 持久化到 site_settings
    let update_sql = "UPDATE site_settings SET
        storage_backend = ?,
        storage_local_path = ?,
        storage_upload_max_bytes = ?,
        storage_s3_endpoint = ?,
        storage_s3_region = ?,
        storage_s3_bucket = ?,
        storage_s3_access_key_id = ?,
        storage_s3_secret_access_key = ?,
        storage_s3_path_style = ?,
        storage_s3_public_base_url = ?,
        storage_s3_signed_url_ttl = ?
        WHERE id = 'singleton'";

    match pool {
        Either::Left(p) => {
            sqlx::query(update_sql)
                .bind(&new_backend)
                .bind(&new_local_path)
                .bind(new_max_bytes)
                .bind(&new_s3_endpoint)
                .bind(&new_s3_region)
                .bind(&new_s3_bucket)
                .bind(&new_s3_ak)
                .bind(&new_s3_sk)
                .bind(new_s3_path_style)
                .bind(&new_s3_public_url)
                .bind(new_s3_ttl)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(update_sql)
                .bind(&new_backend)
                .bind(&new_local_path)
                .bind(new_max_bytes)
                .bind(&new_s3_endpoint)
                .bind(&new_s3_region)
                .bind(&new_s3_bucket)
                .bind(&new_s3_ak)
                .bind(&new_s3_sk)
                .bind(new_s3_path_style)
                .bind(&new_s3_public_url)
                .bind(new_s3_ttl)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    // 4. 热重载内存中的 StorageService
    if let Some(storage) = &state.storage {
        let storage_cfg = build_storage_config_from_db(&state.config, &updated_row);
        if let Err(e) = storage.reload(&storage_cfg).await {
            tracing::error!(error = %e, "热重载存储服务异常");
        } else {
            tracing::info!(backend = %new_backend, "在线存储配置已热重载生效");
        }
    }

    // 5. 审计日志
    AuditEntry::user_action(&user.id, "admin.storage_config_update")
        .with_target("config", "storage")
        .with_effective_role("administrator")
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let config = storage_config_json(&state.config, Some(&updated_row));
    Ok(Json(config).into_response())
}

#[derive(Deserialize, Default)]
#[allow(dead_code)]
struct StorageConfigUpdate {
    backend: Option<String>,
    #[serde(alias = "s3_path_style")]
    path_style: Option<bool>,
    s3_endpoint: Option<String>,
    s3_region: Option<String>,
    #[serde(alias = "s3_bucket")]
    bucket: Option<String>,
    signed_url_ttl_seconds: Option<u64>,
    #[serde(alias = "local_root")]
    local_path: Option<String>,
    s3_access_key_id: Option<String>,
    s3_secret_access_key: Option<String>,
    s3_public_base_url: Option<String>,
    upload_max_bytes: Option<i64>,
    expected_version: Option<i64>,
    reason: Option<String>,
}

/// 校验管理端提交的存储配置（不持久化；M06-QUOTA-11）。
///
/// `allow_http_endpoint`：非生产环境允许 `http://` endpoint（内网 MinIO 等，
/// 与 `BBLBB__S3_ENDPOINT` 环境变量行为一致）；生产仅 `https://`。
#[allow(clippy::result_large_err)] // AppError 为统一错误类型（与 auth/session 同约定）
fn validate_storage_config_update(
    update: &StorageConfigUpdate,
    allow_http_endpoint: bool,
    request_id: &str,
) -> Result<(), AppError> {
    if let Some(backend) = &update.backend {
        if !matches!(backend.as_str(), "local" | "s3") {
            return Err(AppError::bad_request(
                "backend must be 'local' or 's3'",
                request_id,
                None,
            ));
        }
        if backend == "s3" {
            let bucket = update.bucket.as_deref().unwrap_or("");
            if bucket.is_empty() {
                return Err(AppError::bad_request(
                    "bucket is required for s3 backend",
                    request_id,
                    None,
                ));
            }
        }
    }
    // path_style 仅对 s3 后端有意义（local 后端无 path-style 概念）
    if update.path_style.is_some() && update.backend.as_deref() == Some("local") {
        return Err(AppError::bad_request(
            "path_style is only valid for s3 backend",
            request_id,
            None,
        ));
    }
    if let Some(endpoint) = &update.s3_endpoint {
        validate_endpoint_scheme(endpoint, allow_http_endpoint, request_id)?;
    }
    if let Some(ttl) = update.signed_url_ttl_seconds {
        if !(60..=3600).contains(&ttl) {
            return Err(AppError::bad_request(
                "signed_url_ttl_seconds must be in 60..=3600",
                request_id,
                None,
            ));
        }
    }
    Ok(())
}

/// endpoint 协议门：生产仅 `https://`；非生产额外允许 `http://`（内网网关）。
#[allow(clippy::result_large_err)]
fn validate_endpoint_scheme(
    endpoint: &str,
    allow_http_endpoint: bool,
    request_id: &str,
) -> Result<(), AppError> {
    let endpoint = endpoint.trim();
    if endpoint.is_empty() {
        return Ok(());
    }
    let https = endpoint.starts_with("https://");
    let http_dev = allow_http_endpoint && endpoint.starts_with("http://");
    if !https && !http_dev {
        return Err(AppError::bad_request(
            "s3_endpoint must be https (http is allowed only outside production)",
            request_id,
            None,
        ));
    }
    Ok(())
}

/// 脱敏配置投影（优先数据库在线配置，回退环境变量；不返回 access/secret/session token）。
fn storage_config_json(config: &AppConfig, db_row: Option<&StorageSettingsRow>) -> Value {
    if let Some(row) = db_row {
        let is_s3 = row.storage_backend == "s3" && !row.storage_s3_bucket.is_empty();
        let secret_is_set = !row.storage_s3_secret_access_key.is_empty();
        let local_path_str = if !row.storage_local_path.is_empty() {
            row.storage_local_path.clone()
        } else {
            config.storage_dir.display().to_string()
        };
        json!({
            "backend": if is_s3 { "s3" } else { "local" },
            "source": "db",
            "version": 1,
            "configured": true,
            "local_root": local_path_str,
            "local_path": local_path_str,
            "path_style": row.storage_s3_path_style != 0,
            "s3_path_style": row.storage_s3_path_style != 0,
            "region": if is_s3 { json!(row.storage_s3_region) } else { Value::Null },
            "s3_region": if is_s3 { json!(row.storage_s3_region) } else { Value::Null },
            "endpoint": if is_s3 && !row.storage_s3_endpoint.is_empty() {
                json!(endpoint_host(&row.storage_s3_endpoint))
            } else {
                Value::Null
            },
            "s3_endpoint": if is_s3 && !row.storage_s3_endpoint.is_empty() {
                json!(row.storage_s3_endpoint)
            } else {
                Value::Null
            },
            "bucket": if is_s3 { json!(row.storage_s3_bucket) } else { Value::Null },
            "s3_bucket": if is_s3 { json!(row.storage_s3_bucket) } else { Value::Null },
            "s3_public_base_url": if !row.storage_s3_public_base_url.is_empty() { json!(row.storage_s3_public_base_url) } else { Value::Null },
            "upload_max_bytes": row.storage_upload_max_bytes,
            "signed_url_ttl_seconds": row.storage_s3_signed_url_ttl,
            "managed_by": "database",
            "secret_configured": secret_is_set,
            "credentials": json!({
                "access_key_id_configured": is_s3 && !row.storage_s3_access_key_id.is_empty(),
                "secret_configured": secret_is_set,
            }),
        })
    } else {
        let s3_configured = config.storage_backend == "s3" && !config.s3_bucket.is_empty();
        let secret_is_set = s3_configured && !config.s3_secret_access_key.is_empty();
        let local_path_str = config.storage_dir.display().to_string();
        json!({
            "backend": if s3_configured { "s3" } else { "local" },
            "source": "env",
            "version": 1,
            "configured": true,
            "local_root": local_path_str,
            "local_path": local_path_str,
            "path_style": config.s3_path_style,
            "s3_path_style": config.s3_path_style,
            "region": if s3_configured { json!(config.s3_region) } else { Value::Null },
            "s3_region": if s3_configured { json!(config.s3_region) } else { Value::Null },
            "endpoint": if s3_configured && !config.s3_endpoint.is_empty() {
                json!(endpoint_host(&config.s3_endpoint))
            } else {
                Value::Null
            },
            "s3_endpoint": if s3_configured && !config.s3_endpoint.is_empty() {
                json!(config.s3_endpoint)
            } else {
                Value::Null
            },
            "bucket": if s3_configured { json!(config.s3_bucket) } else { Value::Null },
            "s3_bucket": if s3_configured { json!(config.s3_bucket) } else { Value::Null },
            "signed_url_ttl_seconds": PRESIGN_TTL_SECS,
            "managed_by": "deployment",
            "secret_configured": secret_is_set,
            "credentials": json!({
                "access_key_id_configured": s3_configured && !config.s3_access_key_id.is_empty(),
                "secret_configured": secret_is_set,
            }),
        })
    }
}

/// 提取 endpoint 主机（脱敏：不显示完整 URL 路径/凭据）。
fn endpoint_host(endpoint: &str) -> String {
    endpoint
        .split("://")
        .nth(1)
        .unwrap_or(endpoint)
        .split('/')
        .next()
        .unwrap_or(endpoint)
        .to_string()
}

// ────────────────────────── 测试连接 ──────────────────────────────────────

/// POST /api/v1/admin/storage/test — 测试**候选或当前**配置（API.md §12.2）。
///
/// 请求体可携带候选配置（`backend`/`local_path`/`s3_*` 字段）；留空字段
/// 回退到当前部署配置，因此 `{}` 即测试当前配置。探测使用专用前缀
/// `.bblbb-probe-` 并立即清理（docs/API.md：只返回脱敏诊断，不回显凭证）。
async fn test_storage(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "postAdminStorageTest";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    require_admin(pool, &user.id, request_id).await?;
    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .unwrap_or("test storage connection")
        .to_string();
    require_step_up(pool, &headers, state.config.step_up_window_secs, request_id).await?;

    AuditEntry::user_action(&user.id, "admin.storage_test")
        .with_target("config", "storage")
        .with_effective_role("administrator")
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let candidate: StorageTestRequest = serde_json::from_value(body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    if let Some(endpoint) = candidate.s3_endpoint.as_deref() {
        // 与 PATCH 校验一致：非生产允许 http://（内网 MinIO）
        validate_endpoint_scheme(endpoint, !state.config.is_production(), request_id)?;
    }

    // 回退链：候选字段 → 数据库已保存配置 → 环境变量配置。
    // 必须含 DB 层：页面加载时 Secret 不回显（脱敏契约），表单提交的
    // secret 恒为空——若只回退环境变量，用户“已保存凭据再点测试”会变成
    // 无凭据探测（CredentialsNotLoaded 被误报为网络错误）。
    let saved = load_storage_settings(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let started = std::time::Instant::now();
    let result = run_storage_probe(&state.config, &candidate, saved.as_ref()).await;
    let elapsed_ms = started.elapsed().as_millis() as u64;
    Ok(Json(json!({
        "ok": result.ok,
        "backend": result.backend,
        "message": result.detail,
        "detail": result.detail,
        "error_class": result.error_class,
        "elapsed_ms": elapsed_ms,
    }))
    .into_response())
}

/// 测试连接请求（候选配置；空字段回退当前部署配置）。
#[derive(Deserialize, Default)]
#[serde(default)]
struct StorageTestRequest {
    backend: Option<String>,
    local_path: Option<String>,
    s3_endpoint: Option<String>,
    s3_region: Option<String>,
    #[serde(alias = "bucket")]
    s3_bucket: Option<String>,
    #[serde(alias = "path_style")]
    s3_path_style: Option<bool>,
    s3_access_key_id: Option<String>,
    s3_secret_access_key: Option<String>,
    #[allow(dead_code)] // 接受但探测不使用（与 StorageConfigUpdate 对齐）
    reason: Option<String>,
}

/// 探测结果。
struct ProbeResult {
    ok: bool,
    backend: &'static str,
    detail: String,
    error_class: &'static str,
}

/// 探测实现：local 写/读/删探针文件；s3 写/head/删探针对象（专用前缀，
/// 立即清理）。候选字段留空时回退：数据库已保存配置 → 环境变量配置。
async fn run_storage_probe(
    config: &AppConfig,
    candidate: &StorageTestRequest,
    saved: Option<&StorageSettingsRow>,
) -> ProbeResult {
    // 目标后端：候选指定优先；未指定 → 数据库已保存后端 → 环境变量。
    let saved_s3 = saved
        .map(|r| r.storage_backend == "s3" && !r.storage_s3_bucket.is_empty())
        .unwrap_or(false);
    let env_s3 = config.storage_backend == "s3" && !config.s3_bucket.is_empty();
    let probe_s3 = match candidate.backend.as_deref().map(str::trim) {
        Some("s3") => true,
        Some("local") => false,
        Some("") | None => saved_s3 || env_s3,
        Some(other) => {
            return ProbeResult {
                ok: false,
                backend: if saved_s3 || env_s3 { "s3" } else { "local" },
                detail: format!("backend must be 'local' or 's3' (got '{other}')"),
                error_class: "invalid",
            };
        }
    };

    if !probe_s3 {
        // local：候选 local_path → 数据库保存路径 → 部署 storage_dir。
        let root = candidate
            .local_path
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .or_else(|| saved.map(|r| r.storage_local_path.trim()))
            .filter(|s| !s.is_empty())
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| config.storage_dir.clone());
        return run_local_probe(root).await;
    }

    // s3：空字段回退（候选 → 数据库已保存 → 环境变量）。
    // Secret 留空 = 用已保存/已部署凭据（页面不回显 Secret，属正常路径）。
    let fallback = |candidate_value: Option<&String>, saved_value: &str, env_value: &str| {
        candidate_value
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                let v = saved_value.trim();
                (!v.is_empty()).then(|| v.to_string())
            })
            .unwrap_or_else(|| env_value.to_string())
    };
    let default_row = StorageSettingsRow::default();
    let saved_row = saved.unwrap_or(&default_row);
    let bucket = fallback(
        candidate.s3_bucket.as_ref(),
        &saved_row.storage_s3_bucket,
        &config.s3_bucket,
    );
    if bucket.is_empty() {
        return ProbeResult {
            ok: false,
            backend: "s3",
            detail: "bucket is required for s3 test".to_string(),
            error_class: "invalid",
        };
    }
    let region = {
        let r = fallback(
            candidate.s3_region.as_ref(),
            &saved_row.storage_s3_region,
            &config.s3_region,
        );
        if r.is_empty() {
            "auto".to_string()
        } else {
            r
        }
    };
    let endpoint = {
        let e = fallback(
            candidate.s3_endpoint.as_ref(),
            &saved_row.storage_s3_endpoint,
            &config.s3_endpoint,
        );
        if e.is_empty() {
            None
        } else {
            Some(e)
        }
    };
    let s3_config = S3Config {
        bucket,
        region,
        endpoint,
        path_style: candidate
            .s3_path_style
            .unwrap_or(if saved_row.storage_s3_path_style != 0 {
                true
            } else {
                config.s3_path_style
            }),
        access_key_id: {
            let v = fallback(
                candidate.s3_access_key_id.as_ref(),
                &saved_row.storage_s3_access_key_id,
                &config.s3_access_key_id,
            );
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        },
        secret_access_key: {
            let v = fallback(
                candidate.s3_secret_access_key.as_ref(),
                &saved_row.storage_s3_secret_access_key,
                &config.s3_secret_access_key,
            );
            if v.is_empty() {
                None
            } else {
                Some(v)
            }
        },
        session_token: None,
    };

    let probe_key = format!(".bblbb-probe-{}", uuid::Uuid::now_v7());
    let payload = b"bblbb-storage-probe";
    let result: Result<(), StorageError> = async {
        let adapter = S3Adapter::new(&s3_config).await?;
        adapter
            .write_object(&probe_key, payload, Some("text/plain"))
            .await?;
        let head = adapter.head_object(&probe_key).await?;
        let ok = head.exists && head.size_bytes == payload.len() as i64;
        let _ = adapter.delete_object(&probe_key).await;
        if ok {
            Ok(())
        } else {
            Err(StorageError::Verification(
                "probe object mismatch".to_string(),
            ))
        }
    }
    .await;

    let backend = "s3";
    match result {
        Ok(()) => ProbeResult {
            ok: true,
            backend,
            detail: format!("s3 bucket reachable (path_style={})", s3_config.path_style),
            error_class: "ok",
        },
        Err(e) => ProbeResult {
            ok: false,
            backend,
            detail: e.to_string(),
            error_class: classify_storage_error(&e),
        },
    }
}

/// local 探测：写/校验/删探针文件（候选根目录必须可写）。
async fn run_local_probe(root: std::path::PathBuf) -> ProbeResult {
    let probe_key = format!(".bblbb-probe-{}", uuid::Uuid::now_v7());
    let payload = b"bblbb-storage-probe";
    let result: Result<(), StorageError> = async {
        let storage = crate::storage::StorageService::local_only(root)?;
        let adapter = storage.adapter(crate::storage::StorageBackend::Local)?;
        adapter
            .write_object(&probe_key, payload, Some("text/plain"))
            .await?;
        let head = adapter.head_object(&probe_key).await?;
        let ok = head.exists && head.size_bytes == payload.len() as i64;
        let _ = adapter.delete_object(&probe_key).await;
        if ok {
            Ok(())
        } else {
            Err(StorageError::Verification(
                "probe object mismatch".to_string(),
            ))
        }
    }
    .await;

    let backend = "local";
    match result {
        Ok(()) => ProbeResult {
            ok: true,
            backend,
            detail: "local root is writable".to_string(),
            error_class: "ok",
        },
        Err(e) => ProbeResult {
            ok: false,
            backend,
            detail: e.to_string(),
            error_class: classify_storage_error(&e),
        },
    }
}

/// 错误分类（脱敏标签：network/auth/forbidden/upstream/invalid/internal）。
fn classify_storage_error(e: &StorageError) -> &'static str {
    match e {
        StorageError::Auth(_) => "auth",
        StorageError::Forbidden(_) => "forbidden",
        StorageError::Network(_) => "network",
        StorageError::RateLimited(_) => "rate_limited",
        StorageError::Upstream(_) => "upstream",
        StorageError::NotFound(_) => "not_found",
        StorageError::Invalid(_) => "invalid",
        StorageError::Verification(_) | StorageError::Mismatch(_) => "verification",
        _ => "internal",
    }
}

// ────────────────────────── 等级附件配额 ─────────────────────────────────

/// GET /api/v1/admin/levels/{id}/attachment-quota — 读取等级配额（最新修订 +
/// 全部修订历史；无策略时以站点默认 seed）。
async fn get_attachment_quota(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "getAdminLevelsIdAttachmentQuota";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let level = parse_level(&id, request_id)?;
    let policy = get_policy_for_level(pool, level, &user.id)
        .await
        .map_err(|e| storage_quota_error(e, request_id))?;
    let revisions = get_policy_revisions(pool, level)
        .await
        .map_err(|e| storage_quota_error(e, request_id))?;
    let items: Vec<Value> = revisions.iter().map(policy_json).collect();

    Ok(Json(json!({
        "level": level,
        "policy": policy_json(&policy),
        "revisions": items,
    }))
    .into_response())
}

/// PATCH /api/v1/admin/levels/{id}/attachment-quota — 更新等级配额。
///
/// 要求：admin.manage + reason + recent-auth（step-up）+ If-Match
/// （当前 policy_version）+ 审计；创建**新** policy_version，不修改旧行
/// （M06-QUOTA-02）。
async fn update_attachment_quota(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "patchAdminLevelsIdAttachmentQuota";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    require_admin(pool, &user.id, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    require_step_up(pool, &headers, state.config.step_up_window_secs, request_id).await?;

    let level = parse_level(&id, request_id)?;
    let expected_version: i64 = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current policy_version integer",
                request_id,
                None,
            )
        })?;

    let update: QuotaUpdate = serde_json::from_value(body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let policy = update_level_quota(
        pool,
        level,
        update.single_file_max_bytes,
        update.total_bytes,
        update.daily_upload_bytes,
        update.retention_days,
        expected_version,
        &user.id,
        crate::outbox::now_millis(),
    )
    .await
    .map_err(|e| storage_quota_error(e, request_id))?;

    AuditEntry::user_action(&user.id, "admin.attachment_quota_update")
        .with_target("level_quota", &level.to_string())
        .with_effective_role("administrator")
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(Json(json!({
        "level": level,
        "policy": policy_json(&policy),
    }))
    .into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct QuotaUpdate {
    single_file_max_bytes: i64,
    total_bytes: i64,
    daily_upload_bytes: i64,
    retention_days: i64,
}

fn policy_json(policy: &crate::storage::model::QuotaPolicy) -> Value {
    json!({
        "level": policy.level,
        "single_file_max_bytes": policy.single_file_max_bytes,
        "total_bytes": policy.total_bytes,
        "daily_upload_bytes": policy.daily_upload_bytes,
        "retention_days": policy.retention_days,
        "policy_version": policy.policy_version,
    })
}

#[allow(clippy::result_large_err)] // AppError 为统一错误类型（与 auth/session 同约定）
fn parse_level(id: &str, request_id: &str) -> Result<i64, AppError> {
    id.parse::<i64>()
        .map_err(|_| AppError::bad_request("level id must be an integer", request_id, None))
}

/// 配额管理错误 → Problem 响应（版本冲突 → `version_conflict`）。
fn storage_quota_error(e: StorageError, request_id: &str) -> AppError {
    match e {
        StorageError::Conflict(msg) => AppError::version_conflict(msg, request_id),
        StorageError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        StorageError::NotFound(msg) => AppError::not_found(msg, request_id),
        StorageError::Db(msg) => AppError::internal(msg, request_id),
        other => AppError::internal(other.to_string(), request_id),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn update(backend: Option<&str>, endpoint: Option<&str>) -> StorageConfigUpdate {
        StorageConfigUpdate {
            backend: backend.map(str::to_string),
            path_style: None,
            s3_endpoint: endpoint.map(str::to_string),
            s3_region: None,
            bucket: None,
            signed_url_ttl_seconds: None,
            local_path: None,
            s3_access_key_id: None,
            s3_secret_access_key: None,
            s3_public_base_url: None,
            upload_max_bytes: None,
            expected_version: None,
            reason: None,
        }
    }

    #[test]
    fn endpoint_scheme_dev_allows_http_and_prod_requires_https() {
        // 非生产：内网 MinIO 的 http endpoint 允许（与 BBLBB__S3_ENDPOINT 一致）
        assert!(validate_endpoint_scheme("http://10.10.10.10:9000", true, "t").is_ok());
        assert!(validate_endpoint_scheme("https://s3.example.com", true, "t").is_ok());
        // 生产：仅 https
        assert!(validate_endpoint_scheme("http://10.10.10.10:9000", false, "t").is_err());
        assert!(validate_endpoint_scheme("https://s3.example.com", false, "t").is_ok());
        // 无协议 / 其他协议一律拒绝；空串放行（= 使用默认 endpoint）
        assert!(validate_endpoint_scheme("ftp://x", true, "t").is_err());
        assert!(validate_endpoint_scheme("s3.example.com", true, "t").is_err());
        assert!(validate_endpoint_scheme("", true, "t").is_ok());
    }

    #[test]
    fn update_validation_dev_http_endpoint_passes() {
        let mut u = update(Some("s3"), Some("http://10.10.10.10:9000"));
        u.bucket = Some("bblbb".into());
        u.s3_region = Some("auto".into());
        assert!(
            validate_storage_config_update(&u, true, "t").is_ok(),
            "非生产环境必须允许 http:// 内网 endpoint"
        );
        assert!(
            validate_storage_config_update(&u, false, "t").is_err(),
            "生产环境必须拒绝 http:// endpoint"
        );
    }

    #[test]
    fn update_validation_s3_requires_bucket() {
        // backend=s3 必须给 bucket；region 允许缺省（69aaf1c：保存时
        // 回退 us-east-1/auto，探测时同样回退，不强制填写）。
        let err = validate_storage_config_update(&update(Some("s3"), None), true, "t");
        assert!(err.is_err());
        let mut u = update(Some("s3"), None);
        u.bucket = Some("bblbb".into());
        assert!(
            validate_storage_config_update(&u, true, "t").is_ok(),
            "region 缺省必须允许"
        );
        u.s3_region = Some("auto".into());
        assert!(validate_storage_config_update(&u, true, "t").is_ok());
    }

    #[test]
    fn test_request_parses_frontend_candidate_payload() {
        // 前端 test action 的载荷形状：未知字段容忍、null 容忍、空 backend 容忍。
        let payload = serde_json::json!({
            "backend": "s3",
            "local_path": null,
            "s3_endpoint": "http://10.10.10.10:9000",
            "s3_region": "auto",
            "s3_bucket": "bblbb",
            "s3_path_style": true,
            "s3_public_base_url": null,
            "signed_url_ttl_seconds": 300,
            "reason": "测试存储连接"
        });
        let req: StorageTestRequest = serde_json::from_value(payload).expect("parse candidate");
        assert_eq!(req.backend.as_deref(), Some("s3"));
        assert_eq!(req.s3_bucket.as_deref(), Some("bblbb"));
        assert_eq!(req.s3_path_style, Some(true));
        // `{}`（测试当前配置）与未知字段载荷也必须可解析
        let empty: StorageTestRequest = serde_json::from_value(serde_json::json!({})).unwrap();
        assert!(empty.backend.is_none());
        let unknown: Result<StorageTestRequest, _> =
            serde_json::from_value(serde_json::json!({ "probe": true }));
        assert!(unknown.is_ok());
    }

    #[tokio::test]
    async fn probe_backend_selection_falls_back_to_env() {
        // 未指定 backend → 环境/数据库生效后端；未知值 → invalid（不落 500）。
        let config = AppConfig::default(); // storage_backend=local
        let unspecified: StorageTestRequest =
            serde_json::from_value(serde_json::json!({ "backend": "" })).unwrap();
        let result = run_storage_probe(&config, &unspecified, None).await;
        assert_eq!(result.backend, "local");

        let bogus: StorageTestRequest =
            serde_json::from_value(serde_json::json!({ "backend": "ftp" })).unwrap();
        let result = run_storage_probe(&config, &bogus, None).await;
        assert!(!result.ok);
        assert_eq!(result.error_class, "invalid");

        // s3 候选但 bucket 缺失（环境/DB 均未配置）→ invalid 诊断，不 panic、不联网。
        let no_bucket: StorageTestRequest =
            serde_json::from_value(serde_json::json!({ "backend": "s3" })).unwrap();
        let result = run_storage_probe(&config, &no_bucket, None).await;
        assert!(!result.ok);
        assert_eq!(result.error_class, "invalid");
    }

    #[tokio::test]
    async fn probe_falls_back_to_saved_db_credentials() {
        // 页面不回显 Secret → 表单 secret 恒为空；空 secret 必须回退到
        // 数据库已保存凭据（而非仅环境变量），否则探测无凭据。
        let config = AppConfig::default(); // 环境未配置 s3
        let candidate: StorageTestRequest = serde_json::from_value(serde_json::json!({
            "backend": "s3",
            "endpoint": "http://127.0.0.1:1", // 端口 1 = 确定性连接拒绝，不依赖外网
            "bucket": "bblbb"
            // secret 留空：模拟页面提交已保存配置
        }))
        .unwrap();
        let mut saved = StorageSettingsRow::default();
        saved.storage_s3_access_key_id = "SAVEDKEY".to_string();
        saved.storage_s3_secret_access_key = "SAVEDSECRET".to_string();
        let result = run_storage_probe(&config, &candidate, Some(&saved)).await;
        // 无真实服务 → 网络失败，但凭据必须已注入（不再是 CredentialsNotLoaded）。
        assert!(!result.ok);
        assert_eq!(result.backend, "s3");
        assert!(
            !result.detail.contains("no credentials"),
            "空 secret 应回退已保存凭据：{detail}",
            detail = result.detail
        );
    }
}
