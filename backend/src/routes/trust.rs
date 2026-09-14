//! 信任等级路由（documented non-contract 端点，M20-TRUST）。
//!
//! - `GET  /api/v1/me/trust-level`（user.read_own）：当前等级 + 下一级逐项
//!   进度（惰性评估，展示即真实）；
//! - `POST /api/v1/me/trust-level/read-time`（user.edit_own）：阅读时长心跳，
//!   服务端钳制（单请求 ≤60s、每人每日 ≤7200s）；
//! - `GET  /api/v1/admin/trust-levels`（level.manage）：每级规则 + 用户数；
//! - `PATCH /api/v1/admin/trust-levels/{level}`（level.manage）：编辑单级规则
//!   （name/summary/is_enabled/requirements；If-Match=version 乐观锁 + reason
//!   审计；TL0 禁止阈值条件、TL4 强制 manual_only）；
//! - `POST /api/v1/admin/trust-levels/{level}/reset`（level.manage）：恢复该级
//!   为代码内置 LinuxDo 默认规则（reason 审计）；
//! - `POST /api/v1/admin/users/{user_id}/trust-level`（level.manage）：手动
//!   设置（TL4 唯一授予通道），写审计。
//!
//! 同 M12/M13 GAP-FIX 先例不进入冻结 223-op 契约，登记于
//! `scripts/check-route-coverage.rb` DOCUMENTED_NON_CONTRACT 与 docs/API.md。

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::authz::decision::{DenyReason, AUTHZ_POLICY_VERSION};
use crate::authz::enforce::{authorize_action, denied_reason, deny_to_error};
use crate::error::AppError;
use crate::outbox::now_millis;
use crate::trust::rules::Requirements;
use crate::trust::service as trust_service;
use crate::trust::store as trust_store;

/// 信任等级路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me/trust-level", get(get_my_trust_level))
        .route("/api/v1/me/trust-level/read-time", post(post_read_time))
        .route("/api/v1/admin/trust-levels", get(list_admin_trust_levels))
        .route(
            "/api/v1/admin/trust-levels/{level}",
            axum::routing::patch(update_admin_trust_level),
        )
        .route(
            "/api/v1/admin/trust-levels/{level}/reset",
            post(reset_admin_trust_level),
        )
        .route(
            "/api/v1/admin/users/{user_id}/trust-level",
            post(set_admin_user_trust_level),
        )
}

/// 私有数据响应统一 `Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 权限门（与 economy_ext 同模式：RBAC action 判定）。
async fn require_perm(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    permission: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, permission, None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(deny_to_error(
            denied_reason(&decision).unwrap_or(DenyReason::DefaultDeny),
            request_id,
        ));
    }
    Ok(())
}

fn map_trust_error(e: trust_service::TrustError, request_id: &str) -> AppError {
    match e {
        trust_service::TrustError::NotFound => AppError::not_found("user not found", request_id),
        trust_service::TrustError::Invalid(m) => AppError::bad_request(m, request_id, None),
        trust_service::TrustError::Db(e) => AppError::internal(e, request_id),
    }
}

/// GET /api/v1/me/trust-level — 当前信任等级 + 下一级逐项进度（惰性评估）。
async fn get_my_trust_level(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "get_my_trust_level";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let progress = trust_service::progress(pool, &user.id)
        .await
        .map_err(|e| map_trust_error(e, request_id))?;
    let body = Json(json!({
        "level": progress.level,
        "name": progress.name,
        "summary": progress.summary,
        "updated_at": progress.updated_at,
        "grace_until": progress.grace_until,
        "window": progress.window,
        "next_level": progress.next_level,
    }));
    Ok(private_no_store((StatusCode::OK, body).into_response()))
}

/// POST /api/v1/me/trust-level/read-time — 阅读时长心跳。
///
/// 请求体 `{"seconds": 1..=60}`；服务端钳制并按人/日累计（≤7200s/日）。
/// 响应 `{"credited_seconds", "day_total_seconds"}`。
async fn post_read_time(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "post_read_time";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // additionalProperties: false —— 多余字段拒绝（与 parse_cover_body 同风格）。
    if let Some(map) = body.as_object() {
        for key in map.keys() {
            if key != "seconds" {
                return Err(AppError::bad_request(
                    format!("unknown field: {key}"),
                    request_id,
                    None,
                ));
            }
        }
    } else {
        return Err(AppError::bad_request(
            "body must be a JSON object",
            request_id,
            None,
        ));
    }
    let seconds = body
        .get("seconds")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::bad_request("seconds must be an integer", request_id, None))?;
    if !(1..=trust_store::MAX_READ_HEARTBEAT_SECONDS).contains(&seconds) {
        return Err(AppError::bad_request(
            format!(
                "seconds must be 1..={}",
                trust_store::MAX_READ_HEARTBEAT_SECONDS
            ),
            request_id,
            None,
        ));
    }

    let (credited, day_total) = trust_service::add_read_time(pool, &user.id, seconds)
        .await
        .map_err(|e| map_trust_error(e, request_id))?;
    let body = Json(json!({
        "credited_seconds": credited,
        "day_total_seconds": day_total,
    }));
    Ok(private_no_store((StatusCode::OK, body).into_response()))
}

/// 规则行 → 管理 JSON（列表 / PATCH / reset 共用形状）。
fn rule_item_json(r: &trust_store::RuleRow, user_count: i64) -> Value {
    json!({
        "level": r.level,
        "name": r.name,
        "summary": r.summary,
        "requirements": serde_json::from_str::<Value>(
            r.requirements_json.as_deref().unwrap_or("{}"),
        )
        .unwrap_or(Value::Null),
        "is_enabled": r.is_enabled,
        "version": r.version,
        "user_count": user_count,
    })
}

/// GET /api/v1/admin/trust-levels — 每级规则 + 用户数（level.manage）。
async fn list_admin_trust_levels(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "list_admin_trust_levels";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "level.manage", request_id).await?;

    let rows = trust_store::load_rules(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let counts = trust_store::level_user_counts(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let mut items = Vec::with_capacity(rows.len());
    for r in &rows {
        let user_count = counts
            .iter()
            .find(|(level, _)| *level == r.level)
            .map(|(_, n)| *n)
            .unwrap_or(0);
        items.push(rule_item_json(r, user_count));
    }
    let body = Json(json!({ "items": items }));
    Ok(private_no_store((StatusCode::OK, body).into_response()))
}

/// If-Match 头解析（裸整数版本；缺失 → 400）。
fn parse_if_match(headers: &HeaderMap, request_id: &str) -> Result<i64, AppError> {
    headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current rule version integer",
                request_id,
                None,
            )
        })
}

/// reason 必填（1..=500 字符；写审计）。
fn required_reason(body: &Value, request_id: &str) -> Result<String, AppError> {
    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if reason.is_empty() || reason.chars().count() > 500 {
        return Err(AppError::bad_request(
            "reason is required (1..=500 chars)",
            request_id,
            None,
        ));
    }
    Ok(reason.to_string())
}

/// PATCH /api/v1/admin/trust-levels/{level} — 编辑单级规则（level.manage）。
///
/// 全量更新：`{"name": "1..=50 字", "summary": "≤200 字|null",
/// "is_enabled": bool, "requirements": {...}, "reason": "1..=500 字符"}`，
/// `If-Match` = 当前 rule version。requirements 未知键拒绝、逐级语义校验
/// （TL0 无条件、TL4 强制 manual_only）。停用级不参与自动晋升/TL3 自动降级，
/// 手动授予不受影响。写审计 `admin.trust_level_rules.update`。
#[allow(clippy::too_many_lines)]
async fn update_admin_trust_level(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(level): Path<i64>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "update_admin_trust_level";
    let admin = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &admin.id, "level.manage", request_id).await?;
    if !(0..=4).contains(&level) {
        return Err(AppError::bad_request(
            "level must be 0..=4",
            request_id,
            None,
        ));
    }
    if let Some(map) = body.as_object() {
        for key in map.keys() {
            if key != "name"
                && key != "summary"
                && key != "is_enabled"
                && key != "requirements"
                && key != "reason"
            {
                return Err(AppError::bad_request(
                    format!("unknown field: {key}"),
                    request_id,
                    None,
                ));
            }
        }
    } else {
        return Err(AppError::bad_request(
            "body must be a JSON object",
            request_id,
            None,
        ));
    }
    let if_match = parse_if_match(&headers, request_id)?;
    let reason = required_reason(&body, request_id)?;

    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .ok_or_else(|| AppError::bad_request("name must be a string", request_id, None))?;
    let name_len = name.chars().count();
    if !(1..=50).contains(&name_len) {
        return Err(AppError::bad_request(
            "name must be 1-50 characters",
            request_id,
            None,
        ));
    }
    let summary = match body.get("summary") {
        None | Some(Value::Null) => None,
        Some(Value::String(s)) => {
            let s = s.trim();
            if s.chars().count() > 200 {
                return Err(AppError::bad_request(
                    "summary must be at most 200 characters",
                    request_id,
                    None,
                ));
            }
            Some(s.to_string())
        }
        Some(_) => {
            return Err(AppError::bad_request(
                "summary must be a string or null",
                request_id,
                None,
            ))
        }
    };
    let is_enabled = body
        .get("is_enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| AppError::bad_request("is_enabled must be a boolean", request_id, None))?;
    let requirements_value = body
        .get("requirements")
        .cloned()
        .unwrap_or_else(|| json!({}));
    Requirements::validate_keys(&requirements_value)
        .map_err(|m| AppError::bad_request(m, request_id, None))?;
    let requirements: Requirements =
        serde_json::from_value(requirements_value.clone()).map_err(|e| {
            AppError::bad_request(format!("invalid requirements: {e}"), request_id, None)
        })?;
    requirements
        .validate_for_level(level)
        .map_err(|m| AppError::bad_request(m, request_id, None))?;
    let requirements_json = serde_json::to_string(&requirements)
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    // 变更字段清单（审计 metadata 用；以更新前的行为基准）。
    let rows = trust_store::load_rules(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let previous = rows.iter().find(|r| r.level == level).cloned();
    let mut changed: Vec<String> = Vec::new();
    if let Some(prev) = &previous {
        if prev.name != name {
            changed.push("name".into());
        }
        if prev.summary.clone().unwrap_or_default() != summary.clone().unwrap_or_default()
            || (prev.summary.is_none()) != summary.is_none()
        {
            changed.push("summary".into());
        }
        if prev.is_enabled != is_enabled {
            changed.push("is_enabled".into());
        }
        if prev.requirements_json.as_deref().unwrap_or("{}") != requirements_json {
            changed.push("requirements".into());
        }
    } else {
        changed.push("row".into());
    }

    let updated = trust_store::update_rule(
        pool,
        level,
        name,
        summary.as_deref(),
        &requirements_json,
        is_enabled,
        if_match,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    let Some(updated) = updated else {
        return Err(AppError::conflict(
            "trust level rule version mismatch (reload and retry)",
            request_id,
        ));
    };

    AuditEntry::user_action(&admin.id, "admin.trust_level_rules.update")
        .with_target("trust_level_rule", &level.to_string())
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "changed": changed, "is_enabled": is_enabled }))
        .with_request_id(request_id)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let counts = trust_store::level_user_counts(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let user_count = counts
        .iter()
        .find(|(l, _)| *l == level)
        .map(|(_, n)| *n)
        .unwrap_or(0);
    let body = Json(json!({ "item": rule_item_json(&updated, user_count) }));
    Ok(private_no_store((StatusCode::OK, body).into_response()))
}

/// POST /api/v1/admin/trust-levels/{level}/reset — 恢复该级为代码内置
/// LinuxDo 默认规则（level.manage，reason 审计；version + 1）。
async fn reset_admin_trust_level(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(level): Path<i64>,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "reset_admin_trust_level";
    let admin = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &admin.id, "level.manage", request_id).await?;
    if !(0..=4).contains(&level) {
        return Err(AppError::bad_request(
            "level must be 0..=4",
            request_id,
            None,
        ));
    }
    if let Some(map) = body.as_object() {
        for key in map.keys() {
            if key != "reason" {
                return Err(AppError::bad_request(
                    format!("unknown field: {key}"),
                    request_id,
                    None,
                ));
            }
        }
    }
    let reason = required_reason(&body, request_id)?;

    let updated = trust_store::reset_rule(pool, level)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;

    AuditEntry::user_action(&admin.id, "admin.trust_level_rules.reset")
        .with_target("trust_level_rule", &level.to_string())
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "at": now_millis() }))
        .with_request_id(request_id)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let counts = trust_store::level_user_counts(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let user_count = counts
        .iter()
        .find(|(l, _)| *l == level)
        .map(|(_, n)| *n)
        .unwrap_or(0);
    let body = Json(json!({ "item": rule_item_json(&updated, user_count) }));
    Ok(private_no_store((StatusCode::OK, body).into_response()))
}

/// POST /api/v1/admin/users/{user_id}/trust-level — 手动设置（level.manage）。
///
/// 请求体 `{"level": 0..=4, "reason": "1..=500 字符"}`；TL4 唯一授予通道。
/// 写审计 `admin.trust_level.set`；响应含变更前后等级。
async fn set_admin_user_trust_level(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(user_id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "set_admin_user_trust_level";
    let admin = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &admin.id, "level.manage", request_id).await?;

    if let Some(map) = body.as_object() {
        for key in map.keys() {
            if key != "level" && key != "reason" {
                return Err(AppError::bad_request(
                    format!("unknown field: {key}"),
                    request_id,
                    None,
                ));
            }
        }
    } else {
        return Err(AppError::bad_request(
            "body must be a JSON object",
            request_id,
            None,
        ));
    }
    let level = body
        .get("level")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| AppError::bad_request("level must be an integer", request_id, None))?;
    if !(0..=4).contains(&level) {
        return Err(AppError::bad_request(
            "level must be 0..=4",
            request_id,
            None,
        ));
    }
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if reason.is_empty() || reason.chars().count() > 500 {
        return Err(AppError::bad_request(
            "reason is required (1..=500 chars)",
            request_id,
            None,
        ));
    }

    let evaluation = trust_service::manual_set(pool, &admin.id, &user_id, level, reason)
        .await
        .map_err(|e| map_trust_error(e, request_id))?;

    // 审计：管理员手动改信任等级（M01-AUDIT-01 字段）。
    let entry = AuditEntry::user_action(&admin.id, "admin.trust_level.set")
        .with_target("user", &user_id)
        .with_reason(reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({
            "from_level": evaluation.from_level,
            "to_level": evaluation.to_level,
            "changed": evaluation.changed,
            "at": now_millis(),
        }))
        .with_request_id(request_id);
    if let Err(e) = entry.record(pool).await {
        return Err(AppError::internal(e.to_string(), request_id));
    }

    let body = Json(json!({
        "user_id": user_id,
        "from_level": evaluation.from_level,
        "to_level": evaluation.to_level,
        "changed": evaluation.changed,
    }));
    Ok(private_no_store((StatusCode::OK, body).into_response()))
}
