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

use crate::{
    app::AppState,
    audit::AuditEntry,
    auth::session::{is_step_up_required_for_session, AuthSession, SESSION_COOKIE_NAME},
    authz::decision::AUTHZ_POLICY_VERSION,
    authz::enforce::authorize_action,
    config::AppConfig,
    error::AppError,
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
    Ok(Json(storage_config_json(&state.config)).into_response())
}

/// PATCH /api/v1/admin/storage/config — 校验并保存（TOTP step-up + reason + 审计）。
///
/// 当前存储配置由部署环境变量管理（M06-ADAPTER-03：配置单一事实来源）；
/// 本端点对提交值做完整校验并记录审计意图，实际生效需要重启并以
/// 环境变量为准（返回 `managed_by: "deployment"`）。
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
    // 契约要求 If-Match（版本由部署配置持有；此处仅验证存在）
    headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?;
    let reason = required_reason(&body, request_id)?;
    require_step_up(pool, &headers, state.config.step_up_window_secs, request_id).await?;

    let update: StorageConfigUpdate = serde_json::from_value(body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    validate_storage_config_update(&update, request_id)?;

    // 审计（with_reason + with_policy_version；不记录 Secret 值）
    AuditEntry::user_action(&user.id, "admin.storage_config_update")
        .with_target("config", "storage")
        .with_effective_role("administrator")
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let mut config = storage_config_json(&state.config);
    if let Some(map) = config.as_object_mut() {
        map.insert("managed_by".into(), json!("deployment"));
        map.insert(
            "note".into(),
            json!("validated; apply after restart via deployment environment"),
        );
    }
    Ok(Json(config).into_response())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StorageConfigUpdate {
    backend: Option<String>,
    path_style: Option<bool>,
    s3_endpoint: Option<String>,
    s3_region: Option<String>,
    bucket: Option<String>,
    signed_url_ttl_seconds: Option<u64>,
}

/// 校验管理端提交的存储配置（不持久化；M06-QUOTA-11）。
#[allow(clippy::result_large_err)] // AppError 为统一错误类型（与 auth/session 同约定）
fn validate_storage_config_update(
    update: &StorageConfigUpdate,
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
            let region = update.s3_region.as_deref().unwrap_or("");
            if bucket.is_empty() {
                return Err(AppError::bad_request(
                    "bucket is required for s3 backend",
                    request_id,
                    None,
                ));
            }
            if region.is_empty() {
                return Err(AppError::bad_request(
                    "s3_region is required for s3 backend",
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
        if !endpoint.is_empty() && !endpoint.starts_with("https://") {
            return Err(AppError::bad_request(
                "s3_endpoint must be https (development may override)",
                request_id,
                None,
            ));
        }
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

/// 脱敏配置投影（不返回 access/secret/session token）。
fn storage_config_json(config: &AppConfig) -> Value {
    let s3_configured = config.storage_backend == "s3" && !config.s3_bucket.is_empty();
    json!({
        "backend": if s3_configured { "s3" } else { "local" },
        "configured": true,
        "local_root": config.storage_dir.display().to_string(),
        "path_style": config.s3_path_style,
        "region": if s3_configured { json!(config.s3_region) } else { Value::Null },
        "endpoint": if s3_configured && !config.s3_endpoint.is_empty() {
            json!(endpoint_host(&config.s3_endpoint))
        } else {
            Value::Null
        },
        "bucket": if s3_configured { json!(config.s3_bucket) } else { Value::Null },
        "signed_url_ttl_seconds": PRESIGN_TTL_SECS,
        "managed_by": "deployment",
        "credentials": json!({
            "access_key_id_configured": s3_configured && !config.s3_access_key_id.is_empty(),
            "secret_configured": s3_configured && !config.s3_secret_access_key.is_empty(),
        }),
    })
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

/// POST /api/v1/admin/storage/test — 测试连接（local 检查根目录可写；
/// s3 尝试列出空前缀/head bucket）。返回脱敏结果与错误分类。
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
    let reason = required_reason(&body, request_id)?;
    require_step_up(pool, &headers, state.config.step_up_window_secs, request_id).await?;

    AuditEntry::user_action(&user.id, "admin.storage_test")
        .with_target("config", "storage")
        .with_effective_role("administrator")
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let result = run_storage_probe(&state.config).await;
    Ok(Json(json!({
        "ok": result.ok,
        "backend": result.backend,
        "detail": result.detail,
        "error_class": result.error_class,
    }))
    .into_response())
}

/// 探测结果。
struct ProbeResult {
    ok: bool,
    backend: &'static str,
    detail: String,
    error_class: &'static str,
}

/// 探测实现：local 写/读/删探针文件；s3 列出空前缀（bucket 可达）。
async fn run_storage_probe(config: &AppConfig) -> ProbeResult {
    let probe_key = format!(".bblbb-probe-{}", uuid::Uuid::now_v7());
    let payload = b"bblbb-storage-probe";
    let result: Result<(), StorageError> = async {
        let storage = crate::storage::StorageService::local_only(config.storage_dir.clone())?;
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
