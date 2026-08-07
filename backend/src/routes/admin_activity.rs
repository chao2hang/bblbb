//! M07-LEVELS 管理路由：活动配置与自定义任务奖励。
//!
//! - `GET/PATCH /api/v1/admin/activity/config`（getAdminActivityConfig/
//!   updateAdminActivityConfig）：站点时区、默认签到规则、奖励开关；更新要求
//!   `activity.manage` 权限 + reason + 近期重认证（step-up）+ 审计，创建新
//!   `version`。
//! - `GET/POST /api/v1/admin/activity/tasks`（listAdminActivityTasks/
//!   createAdminActivityTask）+ `PATCH .../{id}`（updateAdminActivityTask）：
//!   管理自定义任务奖励（活动规则），写操作同样要求 reason + step-up + 审计。

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, patch},
    Json, Router,
};
use serde_json::Value;

use crate::{
    app::AppState,
    auth::session::{is_step_up_required_for_session, AuthSession, SESSION_COOKIE_NAME},
    authz::decision::AUTHZ_POLICY_VERSION,
    authz::enforce::authorize_action,
    economy::activity::service::{self as activity_service, ActivityConfigUpdate, TaskInput},
    error::AppError,
    outbox::now_millis,
};

/// M07-LEVELS 管理路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/activity/config",
            get(get_activity_config).patch(update_activity_config),
        )
        .route(
            "/api/v1/admin/activity/tasks",
            get(list_activity_tasks).post(create_activity_task),
        )
        .route(
            "/api/v1/admin/activity/tasks/{id}",
            patch(update_activity_task),
        )
}

/// 管理权限门（activity.manage）。
async fn require_manage(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    request_id: &'static str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, "activity.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "activity.manage permission required",
            request_id,
        ));
    }
    Ok(())
}

/// reason 必填（管理写操作；M07-LEVELS-09）。
#[allow(clippy::result_large_err)] // AppError 为统一错误类型（与 auth/session 同约定）
fn require_reason(body: &Value, request_id: &'static str) -> Result<String, AppError> {
    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for activity admin writes",
            request_id,
            None,
        ));
    }
    Ok(reason)
}

/// 近期重认证门（M02-MFA-07 step-up，5 分钟窗口）。
async fn require_recent_auth(
    pool: &crate::db::DatabasePool,
    headers: &HeaderMap,
    step_up_window_secs: u64,
    request_id: &'static str,
) -> Result<(), AppError> {
    let session_token = session_token_from_headers(headers)
        .ok_or_else(|| AppError::unauthorized("authentication required", request_id))?;
    let step_up = is_step_up_required_for_session(pool, &session_token, step_up_window_secs)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if step_up {
        return Err(AppError::step_up_required(request_id));
    }
    Ok(())
}

/// 从 Cookie 头提取会话 token（step-up 判定用）。
fn session_token_from_headers(headers: &HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        if k == SESSION_COOKIE_NAME {
            Some(v.to_string())
        } else {
            None
        }
    })
}

/// GET /api/v1/admin/activity/config
async fn get_activity_config(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "getAdminActivityConfig";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_manage(pool, &user.id, request_id).await?;

    let config = activity_service::ensure_default_activity_config(pool, now_millis())
        .await
        .map_err(|e| map_service_error(e, request_id))?;
    Ok(Json(config.to_value()))
}

/// PATCH /api/v1/admin/activity/config — 更新站点时区/默认签到规则/奖励开关。
async fn update_activity_config(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "updateAdminActivityConfig";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_manage(pool, &user.id, request_id).await?;
    let reason = require_reason(&body, request_id)?;
    require_recent_auth(pool, &headers, state.config.step_up_window_secs, request_id).await?;

    let check_in = body.get("check_in").unwrap_or(&Value::Null);
    let input = ActivityConfigUpdate {
        site_timezone: body
            .get("site_timezone")
            .and_then(Value::as_str)
            .map(str::to_string),
        check_in_enabled: body.get("check_in_enabled").and_then(Value::as_bool),
        check_in_amount: body
            .get("check_in_amount")
            .and_then(Value::as_i64)
            .or_else(|| check_in.get("amount").and_then(Value::as_i64)),
        check_in_daily_limit: body
            .get("check_in_daily_limit")
            .and_then(Value::as_i64)
            .or_else(|| check_in.get("daily_limit").and_then(Value::as_i64)),
        rewards_enabled: body.get("rewards_enabled").and_then(Value::as_bool),
        reason,
    };
    let config = activity_service::update_activity_config(pool, &user.id, &input, now_millis())
        .await
        .map_err(|e| map_service_error(e, request_id))?;

    let mut resp = (StatusCode::OK, Json(config.to_value())).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(resp)
}

/// GET /api/v1/admin/activity/tasks — 全部活动规则。
async fn list_activity_tasks(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "listAdminActivityTasks";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_manage(pool, &user.id, request_id).await?;

    let body = activity_service::list_activity_tasks(pool)
        .await
        .map_err(|e| map_service_error(e, request_id))?;
    Ok(Json(body))
}

/// POST /api/v1/admin/activity/tasks — 创建自定义任务奖励。
async fn create_activity_task(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "createAdminActivityTask";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_manage(pool, &user.id, request_id).await?;
    let reason = require_reason(&body, request_id)?;
    require_recent_auth(pool, &headers, state.config.step_up_window_secs, request_id).await?;

    let input = task_input_from_body(&body);
    let rule =
        activity_service::create_activity_task(pool, &user.id, &reason, &input, now_millis())
            .await
            .map_err(|e| map_service_error(e, request_id))?;

    let mut resp = (
        StatusCode::CREATED,
        Json(activity_service::rule_to_value(&rule)),
    )
        .into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(resp)
}

/// PATCH /api/v1/admin/activity/tasks/{id} — 更新任务奖励（version+1）。
async fn update_activity_task(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "updateAdminActivityTask";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_manage(pool, &user.id, request_id).await?;
    let reason = require_reason(&body, request_id)?;
    require_recent_auth(pool, &headers, state.config.step_up_window_secs, request_id).await?;

    let input = task_input_from_body(&body);
    let rule =
        activity_service::update_activity_task(pool, &user.id, &reason, &id, &input, now_millis())
            .await
            .map_err(|e| map_service_error(e, request_id))?;

    let mut resp = (StatusCode::OK, Json(activity_service::rule_to_value(&rule))).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(resp)
}

fn task_input_from_body(body: &Value) -> TaskInput {
    TaskInput {
        kind: body.get("kind").and_then(Value::as_str).map(str::to_string),
        amount: body.get("amount").and_then(Value::as_i64),
        currency_id: body
            .get("currency_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        daily_limit: body.get("daily_limit").and_then(Value::as_i64),
        cooldown_seconds: body.get("cooldown_seconds").and_then(Value::as_i64),
        conditions_json: body
            .get("conditions")
            .and_then(Value::as_str)
            .map(str::to_string),
        is_enabled: body.get("is_enabled").and_then(Value::as_bool),
    }
}

/// 服务错误 → Problem detail。
fn map_service_error(e: activity_service::ActivityError, request_id: &'static str) -> AppError {
    use activity_service::ActivityError;
    match e {
        ActivityError::Db(msg) | ActivityError::Ledger(msg) => AppError::internal(msg, request_id),
        ActivityError::NotFound(msg) => AppError::not_found(msg, request_id),
        ActivityError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        ActivityError::AlreadyClaimed => AppError::conflict("already claimed", request_id),
        ActivityError::NotEligible(msg) => {
            AppError::conflict(format!("activity not eligible: {msg}"), request_id)
        }
    }
}
