//! M13-PLUGIN-06 管理路由：插件安装/更新/启停/调用摘要/策略修订与错误指标。
//!
//! 权限门：`admin.manage` + reason + recent-auth（step-up）+ 审计（与
//! M11/M12 管理 handler 同一风格）。这些端点不在冻结的 193 OpenAPI 契约中
//! （v1 契约未包含插件域）；作为 v1 新增受控管理能力随 docs/PLUGIN.md 发布。

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, patch, post},
    Router,
};
use axum_extra::extract::CookieJar;
use serde_json::{json, Value};

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::error::AppError;
use crate::plugins::{
    self, install_plugin, list_plugin_metrics, list_plugins, set_plugin_status, uninstall_plugin,
    update_plugin_settings, PluginError, MAX_PACKAGE_BYTES,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/plugins",
            get(list_plugins_admin).post(install_plugin_admin),
        )
        .route(
            "/api/v1/admin/plugins/capabilities",
            get(get_plugin_capabilities),
        )
        .route(
            "/api/v1/admin/plugins/{id}",
            get(get_plugin_admin).delete(uninstall_plugin_admin),
        )
        .route(
            "/api/v1/admin/plugins/{id}/settings",
            patch(update_plugin_settings_admin),
        )
        .route(
            "/api/v1/admin/plugins/{id}/enable",
            post(enable_plugin_admin),
        )
        .route(
            "/api/v1/admin/plugins/{id}/disable",
            post(disable_plugin_admin),
        )
        .route(
            "/api/v1/admin/plugins/{id}/metrics",
            get(get_plugin_metrics_admin),
        )
}

/// 管理权限门（与 admin.rs require_admin 同一语义）。
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
#[allow(clippy::result_large_err)] // AppError 为统一错误类型
fn required_reason(body: &Value, request_id: &str) -> Result<String, AppError> {
    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin plugin operation",
            request_id,
            None,
        ));
    }
    Ok(reason)
}

fn plugin_error_to_app(e: PluginError, request_id: &str) -> AppError {
    match e {
        PluginError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        PluginError::NotFound(msg) => AppError::not_found(msg, request_id),
        PluginError::Conflict(msg) => AppError::version_conflict(msg, request_id),
        PluginError::Incompatible(msg) => AppError::bad_request(msg, request_id, None),
    }
}

/// GET /api/v1/admin/plugins — 插件列表（含禁用/错误态；脱敏 settings）。
async fn list_plugins_admin(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_plugins";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let plugins = list_plugins(pool)
        .await
        .map_err(|e| plugin_error_to_app(e, request_id))?;
    Ok(Json(json!({
        "plugins": plugins.iter().map(|p| p.json()).collect::<Vec<_>>(),
        "capability_allowlist": plugins::KNOWN_CAPABILITIES,
        "known_events": plugins::KNOWN_EVENTS,
    })))
}

/// GET /api/v1/admin/plugins/capabilities — 能力白名单 + 最小服务接口 +
/// 受控 Provider Adapter（UI 菜单数据源；不是安全边界，服务端仍逐操作校验）。
async fn get_plugin_capabilities(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_plugins_capabilities";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    Ok(Json(json!({
        "capabilities": plugins::KNOWN_CAPABILITIES,
        "events": plugins::KNOWN_EVENTS,
        "service_interface": plugins::service_interface(),
        "provider_adapters": plugins::provider_adapters(),
        "v1_execution": "config_only",
        "note": "code/WASM plugin execution is a v2 research item; no online code path in v1 (docs/PLUGIN.md §10)",
    })))
}

/// POST /api/v1/admin/plugins — 安装（admin.manage + reason + recent-auth +
/// 审计；默认 disabled 隔离态）。
async fn install_plugin_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "post_admin_plugins";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = crate::routes::admin::require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;

    // 数据包大小限制（M13-PLUGIN-04：解压限制）。
    let raw = serde_json::to_vec(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    if raw.len() > MAX_PACKAGE_BYTES {
        return Err(AppError::bad_request(
            "plugin package exceeds size limit",
            request_id,
            None,
        ));
    }

    let installed = install_plugin(pool, &body, &user.id)
        .await
        .map_err(|e| plugin_error_to_app(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = crate::audit::AuditEntry::user_action(&user.id, "plugin.install")
        .with_target("plugin", &installed.plugin_id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "plugin": installed.json() })),
    ))
}

/// GET /api/v1/admin/plugins/{id} — 单插件。
async fn get_plugin_admin(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_plugins_id";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let plugin = match crate::plugins::load_plugin(pool, &id).await {
        Ok(Some(p)) => p,
        Ok(None) => return Err(AppError::not_found("plugin not found", request_id)),
        Err(e) => return Err(plugin_error_to_app(e, request_id)),
    };
    Ok(Json(json!({ "plugin": plugin.json() })))
}

/// PATCH /api/v1/admin/plugins/{id}/settings — 更新 settings（If-Match
/// policy_revision + reason + recent-auth + 审计）。
async fn update_plugin_settings_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "patch_admin_plugins_id_settings";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = crate::routes::admin::require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let expected = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current policy_revision integer",
                request_id,
                None,
            )
        })?;
    let settings = body
        .get("settings")
        .ok_or_else(|| AppError::bad_request("settings required", request_id, None))?;
    let updated = update_plugin_settings(pool, &id, settings, &user.id, &reason, expected)
        .await
        .map_err(|e| plugin_error_to_app(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = crate::audit::AuditEntry::user_action(&user.id, "plugin.settings.update")
        .with_target("plugin", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(Json(json!({ "plugin": updated.json() })))
}

/// POST /api/v1/admin/plugins/{id}/enable — 启用（If-Match + reason + 审计）。
async fn enable_plugin_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    set_status_plugin_admin(state, jar, auth, headers, id, body, "enable").await
}

/// POST /api/v1/admin/plugins/{id}/disable — 停用（停用后不再消费新事件）。
async fn disable_plugin_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    set_status_plugin_admin(state, jar, auth, headers, id, body, "disable").await
}

async fn set_status_plugin_admin(
    state: AppState,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    id: String,
    body: Value,
    action: &str,
) -> Result<Json<Value>, AppError> {
    let request_id = format!("post_admin_plugins_id_{action}");
    let user = auth.require_auth(&request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", &request_id))?;
    require_admin(pool, &user.id, &request_id).await?;
    let token = crate::routes::admin::require_recent_auth(&state, &jar, &request_id).await?;
    let reason = required_reason(&body, &request_id)?;
    let expected = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", &request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current policy_revision integer",
                &request_id,
                None,
            )
        })?;
    let status = if action == "enable" {
        "enabled"
    } else {
        "disabled"
    };
    let updated = set_plugin_status(pool, &id, status, &user.id, &reason, expected)
        .await
        .map_err(|e| plugin_error_to_app(e, &request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = crate::audit::AuditEntry::user_action(&user.id, format!("plugin.{status}"))
        .with_target("plugin", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(Json(json!({ "plugin": updated.json() })))
}

/// DELETE /api/v1/admin/plugins/{id} — 卸载（仅 disabled；reason + 审计）。
async fn uninstall_plugin_admin(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "delete_admin_plugins_id";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = crate::routes::admin::require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    uninstall_plugin(pool, &id, &user.id, &reason)
        .await
        .map_err(|e| plugin_error_to_app(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = crate::audit::AuditEntry::user_action(&user.id, "plugin.uninstall")
        .with_target("plugin", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(Json(json!({ "uninstalled": id })))
}

/// GET /api/v1/admin/plugins/{id}/metrics — 调用摘要与错误指标。
async fn get_plugin_metrics_admin(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_plugins_id_metrics";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(100)
        .clamp(1, 500);
    let items = list_plugin_metrics(pool, &id, limit)
        .await
        .map_err(|e| plugin_error_to_app(e, request_id))?;
    Ok(Json(json!({ "plugin_id": id, "metrics": items })))
}
