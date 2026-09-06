//! 社交域成就路由（GAP-FIX 社交域）。
//!
//! 用户侧（本人资源，`require_auth`）：
//! - `GET /api/v1/achievements`：公共成就目录（隐藏成就 description 脱敏为
//!   「隐藏成就」；匿名可读）；
//! - `GET /api/v1/me/achievements`：本人成就视图（全部成就 + 解锁状态/
//!   进度/装备位 + stats 汇总）；
//! - `PUT /api/v1/me/achievements/{code}/equip`：装备（仅已解锁可装备；
//!   超过 [`MAX_EQUIPPED_SLOTS`] → 409）；
//! - `DELETE /api/v1/me/achievements/{code}/equip`：卸下。
//!
//! 管理侧（`admin.manage` + reason + 审计）：
//! - `GET|POST /api/v1/admin/achievements`：目录（含隐藏条件）/ 新建；
//! - `PATCH|DELETE /api/v1/admin/achievements/{code}`：更新（If-Match 版本
//!   递增）/ 删除；
//! - `POST /api/v1/admin/achievements/{code}/grant`：手工授予（manual 类
//!   成就唯一来源；复用 [`crate::achievements::unlock`]，含 badge 通知）。

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde_json::{json, Value};
use sqlx::Either;

use crate::achievements::{
    achievement_public_json, list_enabled_achievements, load_user_stats, unlock, AchievementRow,
    CONDITION_TYPES, MAX_EQUIPPED_SLOTS,
};
use crate::{
    app::AppState, audit::AuditEntry, auth::session::AuthSession,
    authz::decision::AUTHZ_POLICY_VERSION, authz::enforce::authorize_action, error::AppError,
    outbox::now_millis,
};

/// 成就路由（用户侧 + 管理侧）。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/achievements", get(list_achievements))
        .route("/api/v1/me/achievements", get(list_my_achievements))
        .route(
            "/api/v1/me/achievements/{code}/equip",
            put(equip_achievement).delete(unequip_achievement),
        )
        .route(
            "/api/v1/admin/achievements",
            get(admin_list_achievements).post(admin_create_achievement),
        )
        .route(
            "/api/v1/admin/achievements/{code}",
            axum::routing::patch(admin_update_achievement).delete(admin_delete_achievement),
        )
        .route(
            "/api/v1/admin/achievements/{code}/grant",
            post(admin_grant_achievement),
        )
}

/// 私有响应统一 `Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 管理权限门（与 admin.rs require_admin 同模式）。
async fn require_admin(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, "admin.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "admin.manage permission required",
            request_id,
        ));
    }
    Ok(())
}

/// 管理写操作必填 reason。
#[allow(clippy::result_large_err)] // AppError 为路由层统一错误载体（体积固定可接受）
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

/// GET /api/v1/achievements — 公共成就目录（隐藏成就描述脱敏）。
async fn list_achievements(State(state): State<AppState>) -> Result<Response, AppError> {
    let request_id = "list_achievements";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let achievements = list_enabled_achievements(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let items: Vec<Value> = achievements.iter().map(achievement_public_json).collect();
    let resp = (StatusCode::OK, Json(json!({ "items": items }))).into_response();
    Ok(private_no_store(resp))
}

/// 本人成就视图行（成就 + 解锁状态 JOIN 投影）。
#[derive(sqlx::FromRow)]
struct MyAchievementRow {
    code: String,
    name: String,
    condition_type: String,
    condition_threshold: i64,
    unlocked_at: Option<i64>,
    progress: Option<i64>,
    equipped: i64,
}

/// GET /api/v1/me/achievements — 本人成就视图（解锁/进度/装备 + stats）。
async fn list_my_achievements(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "list_my_achievements";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let sql = "SELECT a.code, a.name, a.condition_type, a.condition_threshold,
                      ua.unlocked_at, ua.progress, COALESCE(ua.equipped, 0) AS equipped
               FROM achievements a
               LEFT JOIN user_achievements ua ON ua.achievement_id = a.id AND ua.user_id = ?
               WHERE a.is_enabled = 1
               ORDER BY a.sort_order ASC, a.code ASC";
    let rows: Vec<MyAchievementRow> = match pool {
        Either::Left(p) => sqlx::query_as(sql).bind(&user.id).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as(sql).bind(&user.id).fetch_all(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    // 未解锁成就的 progress 取实时统计（live 计数，钳到阈值上限）。
    let stats = load_user_stats(pool, &user.id)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;

    let mut unlocked = 0i64;
    let mut equipped = 0i64;
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            let is_unlocked = r.unlocked_at.is_some();
            if is_unlocked {
                unlocked += 1;
            }
            if r.equipped != 0 {
                equipped += 1;
            }
            let (unlocked_at, progress) = match r.unlocked_at {
                Some(ts) => (
                    json!(ts),
                    json!(r.progress.unwrap_or(r.condition_threshold)),
                ),
                None => (
                    Value::Null,
                    json!(stats
                        .value_for(&r.condition_type)
                        .min(r.condition_threshold)),
                ),
            };
            json!({
                "code": r.code,
                "name": r.name,
                "unlocked_at": unlocked_at,
                "progress": progress,
                "target": r.condition_threshold,
                "equipped": r.equipped != 0,
            })
        })
        .collect();

    let resp = (
        StatusCode::OK,
        Json(json!({
            "items": items,
            "stats": {
                "unlocked": unlocked,
                "total": rows.len(),
                "equipped": equipped,
                "max_slots": MAX_EQUIPPED_SLOTS,
            },
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// 按成就 code 查启用成就行；不存在 → None。
async fn achievement_by_code(
    pool: &crate::db::DatabasePool,
    code: &str,
    request_id: &str,
) -> Result<Option<AchievementRow>, AppError> {
    let sql = "SELECT id, code, name, description, category, condition_type, condition_threshold,
                      reward_exp, reward_coin, is_hidden, is_enabled, sort_order, version, created_at, updated_at
               FROM achievements WHERE code = ?";
    let row: Option<AchievementRow> = match pool {
        Either::Left(p) => sqlx::query_as(sql).bind(code).fetch_optional(p).await,
        Either::Right(p) => sqlx::query_as(sql).bind(code).fetch_optional(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row)
}

/// 本人已装备成就数。
async fn equipped_count(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<i64, AppError> {
    let count: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM user_achievements WHERE user_id = ? AND equipped = 1",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM user_achievements WHERE user_id = ? AND equipped = 1",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(count)
}

/// PUT /api/v1/me/achievements/{code}/equip — 装备成就。
async fn equip_achievement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(code): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "equip_achievement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let achievement = achievement_by_code(pool, &code, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("achievement not found", request_id))?;

    // 仅已解锁可装备。
    let unlocked_row: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT 1 FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
            )
            .bind(&user.id)
            .bind(&achievement.id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT 1 FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
            )
            .bind(&user.id)
            .bind(&achievement.id)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if unlocked_row != Some(1) {
        return Err(AppError::conflict(
            "achievement not unlocked yet",
            request_id,
        ));
    }

    // 装备槽上限（已装备则幂等直接成功）。
    let already: i64 =
        match pool {
            Either::Left(p) => sqlx::query_scalar(
                "SELECT equipped FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
            )
            .bind(&user.id)
            .bind(&achievement.id)
            .fetch_one(p)
            .await,
            Either::Right(p) => sqlx::query_scalar(
                "SELECT equipped FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
            )
            .bind(&user.id)
            .bind(&achievement.id)
            .fetch_one(p)
            .await,
        }
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if already == 0 && equipped_count(pool, &user.id, request_id).await? >= MAX_EQUIPPED_SLOTS {
        return Err(AppError::conflict(
            "equipped achievement slots are full",
            request_id,
        ));
    }

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE user_achievements SET equipped = 1 WHERE user_id = ? AND achievement_id = ?",
            )
            .bind(&user.id)
            .bind(&achievement.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE user_achievements SET equipped = 1 WHERE user_id = ? AND achievement_id = ?",
            )
            .bind(&user.id)
            .bind(&achievement.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let resp = (StatusCode::OK, Json(json!({ "equipped": true }))).into_response();
    Ok(private_no_store(resp))
}

/// DELETE /api/v1/me/achievements/{code}/equip — 卸下成就（幂等）。
async fn unequip_achievement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(code): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "unequip_achievement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let achievement = achievement_by_code(pool, &code, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("achievement not found", request_id))?;

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE user_achievements SET equipped = 0 WHERE user_id = ? AND achievement_id = ?",
            )
            .bind(&user.id)
            .bind(&achievement.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE user_achievements SET equipped = 0 WHERE user_id = ? AND achievement_id = ?",
            )
            .bind(&user.id)
            .bind(&achievement.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let resp = (StatusCode::OK, Json(json!({ "equipped": false }))).into_response();
    Ok(private_no_store(resp))
}

// ───────────────────────────── 管理侧 ─────────────────────────────

/// 管理视图行（含解锁计数）。
#[derive(sqlx::FromRow)]
struct AdminAchievementRow {
    code: String,
    name: String,
    description: String,
    category: String,
    condition_type: String,
    condition_threshold: i64,
    reward_exp: i64,
    reward_coin: i64,
    is_hidden: i64,
    is_enabled: i64,
    sort_order: i64,
    version: i64,
    unlocked_count: i64,
}

/// 管理视图行 → JSON（隐藏成就条件对管理员可见）。
fn admin_achievement_json(r: &AdminAchievementRow) -> Value {
    json!({
        "code": r.code,
        "name": r.name,
        "description": r.description,
        "category": r.category,
        "condition_type": r.condition_type,
        "condition_threshold": r.condition_threshold,
        "reward_exp": r.reward_exp,
        "reward_coin": r.reward_coin,
        "is_hidden": r.is_hidden != 0,
        "is_enabled": r.is_enabled != 0,
        "sort_order": r.sort_order,
        "unlocked_count": r.unlocked_count,
        "version": r.version,
    })
}

/// GET /api/v1/admin/achievements — 全部成就（含隐藏条件与解锁计数）。
async fn admin_list_achievements(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "admin_list_achievements";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let sql = "SELECT a.code, a.name, a.description, a.category, a.condition_type,
                      a.condition_threshold, a.reward_exp, a.reward_coin, a.is_hidden,
                      a.is_enabled, a.sort_order, a.version,
                      (SELECT COUNT(*) FROM user_achievements ua WHERE ua.achievement_id = a.id) AS unlocked_count
               FROM achievements a ORDER BY a.sort_order ASC, a.code ASC";
    let rows: Vec<AdminAchievementRow> = match pool {
        Either::Left(p) => sqlx::query_as(sql).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as(sql).fetch_all(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let items: Vec<Value> = rows.iter().map(admin_achievement_json).collect();
    let resp = (StatusCode::OK, Json(json!({ "items": items }))).into_response();
    Ok(private_no_store(resp))
}

/// 校验成就字段（创建/更新共用；返回错误消息）。
///
/// 参数为契约字段的平铺（name/description/category/condition_type/
/// condition_threshold/reward_exp/reward_coin）+ 可选 code（仅创建校验）。
#[allow(clippy::too_many_arguments)] // 契约字段平铺，收束为结构体收益有限
fn validate_achievement_fields(
    code: Option<&str>,
    name: &str,
    description: &str,
    category: &str,
    condition_type: &str,
    condition_threshold: i64,
    reward_exp: i64,
    reward_coin: i64,
) -> Result<(), String> {
    if let Some(code) = code {
        let valid = !code.is_empty()
            && code.len() <= 64
            && code
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-');
        if !valid {
            return Err("code must be 1-64 chars of lowercase/digits/_/-".to_string());
        }
    }
    if name.is_empty() || name.chars().count() > 120 {
        return Err("name must be 1-120 characters".to_string());
    }
    if description.is_empty() || description.chars().count() > 500 {
        return Err("description must be 1-500 characters".to_string());
    }
    if category.is_empty() || category.chars().count() > 32 {
        return Err("category must be 1-32 characters".to_string());
    }
    if !CONDITION_TYPES.contains(&condition_type) {
        return Err("unsupported condition_type".to_string());
    }
    if condition_threshold < 0 {
        return Err("condition_threshold must be >= 0".to_string());
    }
    if reward_exp < 0 || reward_coin < 0 {
        return Err("rewards must be >= 0".to_string());
    }
    Ok(())
}

/// POST /api/v1/admin/achievements — 新建成就（admin.manage + reason）。
async fn admin_create_achievement(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "admin_create_achievement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&req, request_id)?;

    let code = req
        .get("code")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("code is required", request_id, None))?
        .to_string();
    let name = req
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let description = req
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let category = req
        .get("category")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let condition_type = req
        .get("condition_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let condition_threshold = req
        .get("condition_threshold")
        .and_then(Value::as_i64)
        .unwrap_or(-1);
    let reward_exp = req.get("reward_exp").and_then(Value::as_i64).unwrap_or(0);
    let reward_coin = req.get("reward_coin").and_then(Value::as_i64).unwrap_or(0);
    let is_hidden = req
        .get("is_hidden")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let is_enabled = req
        .get("is_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let sort_order = req.get("sort_order").and_then(Value::as_i64).unwrap_or(0);

    validate_achievement_fields(
        Some(&code),
        &name,
        &description,
        &category,
        &condition_type,
        condition_threshold,
        reward_exp,
        reward_coin,
    )
    .map_err(|m| AppError::bad_request(m, request_id, None))?;

    // code 唯一：已存在 → 409。
    if achievement_by_code(pool, &code, request_id)
        .await?
        .is_some()
    {
        return Err(AppError::conflict(
            "achievement code already exists",
            request_id,
        ));
    }

    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let sql = "INSERT INTO achievements (id, code, name, description, category, condition_type,
               condition_threshold, reward_exp, reward_coin, is_hidden, is_enabled, sort_order, version, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&id)
                .bind(&code)
                .bind(&name)
                .bind(&description)
                .bind(&category)
                .bind(&condition_type)
                .bind(condition_threshold)
                .bind(reward_exp)
                .bind(reward_coin)
                .bind(if is_hidden { 1 } else { 0 })
                .bind(if is_enabled { 1 } else { 0 })
                .bind(sort_order)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&id)
                .bind(&code)
                .bind(&name)
                .bind(&description)
                .bind(&category)
                .bind(&condition_type)
                .bind(condition_threshold)
                .bind(reward_exp)
                .bind(reward_coin)
                .bind(if is_hidden { 1 } else { 0 })
                .bind(if is_enabled { 1 } else { 0 })
                .bind(sort_order)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    AuditEntry::user_action(&user.id, "admin.achievement.create")
        .with_target("achievement", &code)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (
        StatusCode::CREATED,
        Json(json!({
            "code": code,
            "name": name,
            "description": description,
            "category": category,
            "condition_type": condition_type,
            "condition_threshold": condition_threshold,
            "reward_exp": reward_exp,
            "reward_coin": reward_coin,
            "is_hidden": is_hidden,
            "is_enabled": is_enabled,
            "sort_order": sort_order,
            "version": 1,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// PATCH /api/v1/admin/achievements/{code} — 更新成就（If-Match，version 递增）。
async fn admin_update_achievement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(code): Path<String>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "admin_update_achievement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&req, request_id)?;

    let current = achievement_by_code(pool, &code, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("achievement not found", request_id))?;

    // If-Match 版本校验。
    let expected_version: i64 = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header required", request_id, None))?
        .trim()
        .parse()
        .map_err(|_| {
            AppError::bad_request("If-Match must be an integer version", request_id, None)
        })?;
    if expected_version != current.version {
        return Err(AppError::version_conflict(
            "achievement version mismatch",
            request_id,
        ));
    }

    // PATCH 语义：仅更新出现字段。
    let name = req
        .get("name")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| current.name.clone());
    let description = req
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| current.description.clone());
    let category = req
        .get("category")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| current.category.clone());
    let condition_type = req
        .get("condition_type")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| current.condition_type.clone());
    let condition_threshold = req
        .get("condition_threshold")
        .and_then(Value::as_i64)
        .unwrap_or(current.condition_threshold);
    let reward_exp = req
        .get("reward_exp")
        .and_then(Value::as_i64)
        .unwrap_or(current.reward_exp);
    let reward_coin = req
        .get("reward_coin")
        .and_then(Value::as_i64)
        .unwrap_or(current.reward_coin);
    let is_hidden = match req.get("is_hidden").and_then(Value::as_bool) {
        Some(v) => v,
        None => current.is_hidden != 0,
    };
    let is_enabled = match req.get("is_enabled").and_then(Value::as_bool) {
        Some(v) => v,
        None => current.is_enabled != 0,
    };
    let sort_order = req
        .get("sort_order")
        .and_then(Value::as_i64)
        .unwrap_or(current.sort_order);

    validate_achievement_fields(
        None,
        &name,
        &description,
        &category,
        &condition_type,
        condition_threshold,
        reward_exp,
        reward_coin,
    )
    .map_err(|m| AppError::bad_request(m, request_id, None))?;

    let now = now_millis();
    let sql = "UPDATE achievements SET name = ?, description = ?, category = ?,
               condition_type = ?, condition_threshold = ?, reward_exp = ?, reward_coin = ?,
               is_hidden = ?, is_enabled = ?, sort_order = ?, version = version + 1, updated_at = ?
               WHERE code = ?";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&name)
                .bind(&description)
                .bind(&category)
                .bind(&condition_type)
                .bind(condition_threshold)
                .bind(reward_exp)
                .bind(reward_coin)
                .bind(if is_hidden { 1 } else { 0 })
                .bind(if is_enabled { 1 } else { 0 })
                .bind(sort_order)
                .bind(now)
                .bind(&code)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&name)
                .bind(&description)
                .bind(&category)
                .bind(&condition_type)
                .bind(condition_threshold)
                .bind(reward_exp)
                .bind(reward_coin)
                .bind(if is_hidden { 1 } else { 0 })
                .bind(if is_enabled { 1 } else { 0 })
                .bind(sort_order)
                .bind(now)
                .bind(&code)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    AuditEntry::user_action(&user.id, "admin.achievement.update")
        .with_target("achievement", &code)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (
        StatusCode::OK,
        Json(json!({
            "code": code,
            "name": name,
            "description": description,
            "category": category,
            "condition_type": condition_type,
            "condition_threshold": condition_threshold,
            "reward_exp": reward_exp,
            "reward_coin": reward_coin,
            "is_hidden": is_hidden,
            "is_enabled": is_enabled,
            "sort_order": sort_order,
            "version": current.version + 1,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// DELETE /api/v1/admin/achievements/{code} — 删除成就（级联解锁记录）。
async fn admin_delete_achievement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(code): Path<String>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "admin_delete_achievement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let req: Value = if body.is_empty() {
        json!({})
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?
    };
    let reason = required_reason(&req, request_id)?;

    let current = achievement_by_code(pool, &code, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("achievement not found", request_id))?;

    match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM achievements WHERE id = ?")
                .bind(&current.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM achievements WHERE id = ?")
                .bind(&current.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    AuditEntry::user_action(&user.id, "admin.achievement.delete")
        .with_target("achievement", &code)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (StatusCode::NO_CONTENT, Json(json!({}))).into_response();
    Ok(private_no_store(resp))
}

/// POST /api/v1/admin/achievements/{code}/grant — 手工授予成就（+通知）。
async fn admin_grant_achievement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(code): Path<String>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "admin_grant_achievement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&req, request_id)?;
    let username = req
        .get("username")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| AppError::bad_request("username is required", request_id, None))?;

    let achievement = achievement_by_code(pool, &code, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("achievement not found", request_id))?;

    let target_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT id FROM users WHERE username_normalized = ? AND status <> 'deleted'",
            )
            .bind(username.to_lowercase())
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT id FROM users WHERE username_normalized = ? AND status <> 'deleted'",
            )
            .bind(username.to_lowercase())
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let target_id = target_id.ok_or_else(|| AppError::not_found("user not found", request_id))?;

    // 幂等授予：已解锁返回既有 unlocked_at。
    unlock(pool, &target_id, &achievement)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;

    let unlocked_at: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT unlocked_at FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
        )
        .bind(&target_id)
        .bind(&achievement.id)
        .fetch_one(p)
        .await,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT unlocked_at FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
        )
        .bind(&target_id)
        .bind(&achievement.id)
        .fetch_one(p)
        .await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    AuditEntry::user_action(&user.id, "admin.achievement.grant")
        .with_target("achievement", &code)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "username": username.to_lowercase() }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (
        StatusCode::CREATED,
        Json(json!({ "code": code, "unlocked_at": unlocked_at })),
    )
        .into_response();
    Ok(private_no_store(resp))
}
