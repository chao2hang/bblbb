//! Feature Flag 管理路由（P0 整改）。
//!
//! - `GET  /api/v1/admin/feature-flags`：当前 Flag 快照（DB 行 + kill switch 状态）；
//! - `PATCH /api/v1/admin/feature-flags/{name}`：启停单个 Flag（If-Match
//!   expected_version 乐观锁 + reason 审计），成功后同进程原地重载快照；
//! - `POST /api/v1/admin/feature-flags/kill-switch`：紧急关闭（所有可选能力
//!   置禁用 + 审计 + 快照 kill_switch 置真）。
//!
//! Flag 只负责启停，不绕过权限/CSRF/审计/账本（docs/CONFIGURATION.md §1.5）。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};
use sqlx::Either;

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::{authorize_action, denied_reason, deny_to_error};
use crate::config::flags::{FeatureFlags, FeatureName};
use crate::db::DatabasePool;
use crate::error::AppError;
use crate::outbox::now_millis;

/// 私有数据响应统一 `Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 权限门（admin.manage）。
async fn require_admin(
    pool: &DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, "admin.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(deny_to_error(
            denied_reason(&decision).unwrap_or(crate::authz::decision::DenyReason::DefaultDeny),
            request_id,
        ));
    }
    Ok(())
}

/// 必填 reason（管理写操作审计）。
#[allow(clippy::result_large_err)]
fn required_reason(body: &Value, request_id: &str) -> Result<String, AppError> {
    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin operation",
            request_id,
            None,
        ));
    }
    Ok(reason)
}

/// 从数据库读取 Flag 行（缺失表 → 空列表 + 默认快照兜底）。
async fn load_rows(pool: &DatabasePool) -> Vec<(String, i64, i64, i64, Option<String>, i64)> {
    let sql = "SELECT name, enabled, effective_at, version, updated_by, updated_at
               FROM feature_flags ORDER BY name";
    match pool {
        Either::Left(p) => sqlx::query_as(sql).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as(sql).fetch_all(p).await,
    }
    .unwrap_or_default()
}

/// GET /api/v1/admin/feature-flags — Flag 列表 + kill switch 状态。
async fn list_flags(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "get_admin_feature_flags";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let rows = load_rows(pool).await;
    let kill_switch = state.flags.read().map(|f| f.kill_switch()).unwrap_or(false);
    let items: Vec<Value> = rows
        .into_iter()
        .map(
            |(name, enabled, effective_at, version, updated_by, updated_at)| {
                json!({
                    "name": name,
                    "enabled": enabled != 0,
                    "effective_at": effective_at,
                    "version": version,
                    "updated_by": updated_by,
                    "updated_at": updated_at,
                })
            },
        )
        .collect();
    let resp = (
        StatusCode::OK,
        Json(json!({ "flags": items, "kill_switch": kill_switch })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// PATCH /api/v1/admin/feature-flags/{name} — 启停单个 Flag。
///
/// body {enabled, expected_version, effective_at?, reason}；If-Match 头优先，
/// 回落 body.expected_version。成功后 DB version+1、审计、快照原地重载。
async fn update_flag(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Path(name): Path<String>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "patch_admin_feature_flag";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&req, request_id)?;

    let Some(feature) = FeatureName::ALL.iter().find(|f| f.as_str() == name) else {
        return Err(AppError::not_found("unknown feature flag", request_id));
    };

    let enabled = req
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| AppError::bad_request("enabled must be a boolean", request_id, None))?;
    // 版本门：If-Match 头优先，回落 body.expected_version。
    let expected_version: i64 = match headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().trim_matches('"').parse::<i64>())
    {
        Some(Ok(v)) => v,
        Some(Err(_)) => {
            return Err(AppError::bad_request(
                "If-Match must be the current version integer",
                request_id,
                None,
            ))
        }
        None => req
            .get("expected_version")
            .and_then(Value::as_i64)
            .ok_or_else(|| {
                AppError::bad_request(
                    "If-Match header or expected_version is required",
                    request_id,
                    None,
                )
            })?,
    };
    let effective_at = req.get("effective_at").and_then(Value::as_i64);

    let now = now_millis();
    let sql = "UPDATE feature_flags
               SET enabled = ?, effective_at = COALESCE(?, effective_at),
                   version = version + 1, updated_by = ?, updated_at = ?
               WHERE name = ? AND version = ?";
    let affected = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(enabled as i64)
            .bind(effective_at)
            .bind(&user.id)
            .bind(now)
            .bind(feature.as_str())
            .bind(expected_version)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
        Either::Right(p) => sqlx::query(sql)
            .bind(enabled as i64)
            .bind(effective_at)
            .bind(&user.id)
            .bind(now)
            .bind(feature.as_str())
            .bind(expected_version)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
    };
    if affected == 0 {
        return Err(AppError::conflict(
            "feature flag version mismatch (reload and retry)",
            request_id,
        ));
    }

    AuditEntry::user_action(&user.id, "admin.feature_flag.update")
        .with_target("feature_flag", feature.as_str())
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "enabled": enabled, "version": expected_version + 1 }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    reload_snapshot(&state, pool).await;
    let rows = load_rows(pool).await;
    let item = rows.iter().find(|(n, ..)| n == feature.as_str()).map(
        |(name, enabled, effective_at, version, updated_by, updated_at)| {
            json!({
                "name": name,
                "enabled": *enabled != 0,
                "effective_at": effective_at,
                "version": version,
                "updated_by": updated_by,
                "updated_at": updated_at,
            })
        },
    );
    let resp = (
        StatusCode::OK,
        Json(json!({ "flag": item, "status": "updated" })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// POST /api/v1/admin/feature-flags/kill-switch — 紧急关闭全部可选能力。
///
/// DB 侧全部置禁用（持久），快照侧 kill_switch 置真（立即、优先于一切）。
async fn kill_switch(
    State(state): State<AppState>,
    auth: AuthSession,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "post_admin_feature_kill_switch";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&req, request_id)?;

    let now = now_millis();
    let sql = "UPDATE feature_flags
               SET enabled = 0, version = version + 1, updated_by = ?, updated_at = ?";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&user.id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&user.id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    };

    AuditEntry::user_action(&user.id, "admin.feature_flag.kill_switch")
        .with_target("feature_flag", "*")
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    {
        let mut guard = state
            .flags
            .write()
            .map_err(|_| AppError::internal("flags lock poisoned", request_id))?;
        guard.emergency_off(&user.id, &reason, now);
    }

    let resp = (StatusCode::OK, Json(json!({ "status": "killed" }))).into_response();
    Ok(private_no_store(resp))
}

/// 管理员变更后原地重载进程内快照（保留既有 kill switch 状态）。
async fn reload_snapshot(state: &AppState, pool: &DatabasePool) {
    let mut loaded = FeatureFlags::load(pool).await;
    let had_kill_switch = state.flags.read().map(|f| f.kill_switch()).unwrap_or(false);
    if had_kill_switch {
        loaded.emergency_off("system", "kill switch active (reload)", now_millis());
    }
    if let Ok(mut guard) = state.flags.write() {
        *guard = loaded;
    }
}

/// Feature Flag 管理路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/feature-flags",
            get(list_flags).post(kill_switch_hint),
        )
        .route("/api/v1/admin/feature-flags/kill-switch", post(kill_switch))
        .route(
            "/api/v1/admin/feature-flags/{name}",
            axum::routing::patch(update_flag),
        )
}

/// POST /api/v1/admin/feature-flags — 提示使用 kill-switch 专用路径。
async fn kill_switch_hint() -> Response {
    AppError::bad_request(
        "use POST /api/v1/admin/feature-flags/kill-switch",
        "post_admin_feature_flags",
        None,
    )
    .into_response()
}
