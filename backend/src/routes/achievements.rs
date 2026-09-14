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
//!   成就唯一来源；复用 [`crate::achievements::unlock`]，含 badge 通知）；
//! - `POST|DELETE /api/v1/admin/achievements/{code}/icon`：成就图标上传/
//!   移除（**不走 S3**：直写 `storage_dir/achievements/` 本地磁盘，
//!   [`crate::achievements::icon`]；魔数嗅探，≤2MB，reason 走查询参数）。
//!
//! 公开图标读取（匿名可读）：`GET /api/v1/achievements/{code}/icon`
//! （ETag + `Cache-Control: public`；未上传 → 404）。

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post, put},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::achievements::{
    achievement_public_json, icon, list_enabled_achievements, load_user_stats, unlock,
    AchievementRow, CONDITION_TYPES, MAX_EQUIPPED_SLOTS,
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
        .route(
            "/api/v1/achievements/{code}/icon",
            get(get_achievement_icon),
        )
        .route("/api/v1/me/achievements", get(list_my_achievements))
        .route("/api/v1/me/badges", put(update_my_badges))
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
        .route(
            "/api/v1/admin/achievements/{code}/icon",
            post(admin_upload_achievement_icon).delete(admin_delete_achievement_icon),
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

/// 成就查询参数（本人视图状态过滤）。
#[derive(Debug, Deserialize, Default)]
pub struct ListAchievementsQuery {
    pub status: Option<String>,
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

/// GET /api/v1/achievements/{code}/icon — 公开成就图标（匿名可读）。
///
/// 本地磁盘读取（不走 S3/附件域）；ETag = 文件名内容哈希段，
/// `If-None-Match` 命中 → 304；未上传/文件缺失 → 404。
async fn get_achievement_icon(
    State(state): State<AppState>,
    Path(code): Path<String>,
    headers: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    let request_id = "get_achievement_icon";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let achievement = achievement_by_code(pool, &code, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("achievement not found", request_id))?;
    let filename = achievement
        .icon_path
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::not_found("achievement has no icon", request_id))?;
    let ext = filename.rsplit('.').next().unwrap_or("");
    let content_type = icon::content_type_for_ext(ext)
        .ok_or_else(|| AppError::not_found("achievement icon is unreadable", request_id))?;

    let path = icon::icon_dir(&state.config.storage_dir).join(&filename);
    let bytes = tokio::fs::read(&path)
        .await
        .map_err(|_| AppError::not_found("achievement icon file missing", request_id))?;

    let etag = format!(
        "\"{}\"",
        icon::hash_of_filename(&filename).unwrap_or_default()
    );
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        == Some(etag.as_str())
    {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }

    let resp = (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("public, max-age=300"),
            ),
            (
                header::ETAG,
                HeaderValue::from_str(&etag).unwrap_or_else(|_| HeaderValue::from_static("\"\"")),
            ),
        ],
        bytes,
    )
        .into_response();
    Ok(resp)
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
    is_hidden: bool,
}

/// GET /api/v1/me/achievements — 本人成就视图（解锁/进度/装备 + stats）。
async fn list_my_achievements(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<ListAchievementsQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_my_achievements";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let sql = "SELECT a.code, a.name, a.condition_type, a.condition_threshold,
                      ua.unlocked_at, ua.progress, COALESCE(ua.equipped, 0) AS equipped,
                      a.is_hidden
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
    let mut items = Vec::new();

    for r in &rows {
        let is_unlocked = r.unlocked_at.is_some();
        if is_unlocked {
            unlocked += 1;
        }
        if r.equipped != 0 {
            equipped += 1;
        }
        let (unlocked_at, progress, progress_num) = match r.unlocked_at {
            Some(ts) => {
                let p = r.progress.unwrap_or(r.condition_threshold);
                (json!(ts), json!(p), p)
            }
            None => {
                let p = stats
                    .value_for(&r.condition_type)
                    .min(r.condition_threshold);
                (Value::Null, json!(p), p)
            }
        };

        let keep = match query.status.as_deref() {
            Some("unlocked") => is_unlocked,
            Some("in_progress") => !is_unlocked && (!r.is_hidden || progress_num > 0),
            Some("hidden") => r.is_hidden,
            _ => true,
        };

        if keep {
            items.push(json!({
                "code": r.code,
                "name": r.name,
                "unlocked_at": unlocked_at,
                "progress": progress,
                "target": r.condition_threshold,
                "equipped": r.equipped != 0,
            }));
        }
    }

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
                      reward_coin, is_hidden, is_enabled, sort_order, version, created_at, updated_at, icon_path
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

/// 批量装备徽章负载（最多 3 枚）。
#[derive(Debug, Deserialize)]
pub struct UpdateBadgesPayload {
    #[serde(default)]
    pub badges: Vec<String>,
}

/// PUT /api/v1/me/badges — 批量装备/卸下徽章（最多 3 枚）。
async fn update_my_badges(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(payload): Json<UpdateBadgesPayload>,
) -> Result<Response, AppError> {
    let request_id = "update_my_badges";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let mut badge_codes = Vec::new();
    for b in payload.badges {
        let code = b.trim().to_string();
        if code.is_empty() {
            continue;
        }
        if !badge_codes.contains(&code) {
            badge_codes.push(code);
        }
    }
    if badge_codes.len() > MAX_EQUIPPED_SLOTS as usize {
        return Err(AppError::conflict(
            format!("equipped badges cannot exceed {MAX_EQUIPPED_SLOTS}"),
            request_id,
        ));
    }

    for code in &badge_codes {
        let ach = achievement_by_code(pool, code, request_id)
            .await?
            .ok_or_else(|| {
                AppError::not_found(format!("achievement {code} not found"), request_id)
            })?;
        let unlocked: Option<i64> = match pool {
            Either::Left(p) => {
                sqlx::query_scalar(
                    "SELECT 1 FROM user_achievements WHERE user_id = ? AND achievement_id = ? AND unlocked_at IS NOT NULL",
                )
                .bind(&user.id)
                .bind(&ach.id)
                .fetch_optional(p)
                .await
            }
            Either::Right(p) => {
                sqlx::query_scalar(
                    "SELECT 1 FROM user_achievements WHERE user_id = ? AND achievement_id = ? AND unlocked_at IS NOT NULL",
                )
                .bind(&user.id)
                .bind(&ach.id)
                .fetch_optional(p)
                .await
            }
        }
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

        if unlocked.is_none() {
            return Err(AppError::conflict(
                format!("achievement {code} is not unlocked yet"),
                request_id,
            ));
        }
    }

    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE user_achievements SET equipped = 0 WHERE user_id = ?")
                .bind(&user.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for code in &badge_codes {
                sqlx::query(
                    "UPDATE user_achievements SET equipped = 1 WHERE user_id = ? AND achievement_id = (SELECT id FROM achievements WHERE code = ?)",
                )
                .bind(&user.id)
                .bind(code)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE user_achievements SET equipped = 0 WHERE user_id = ?")
                .bind(&user.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for code in &badge_codes {
                sqlx::query(
                    "UPDATE user_achievements SET equipped = 1 WHERE user_id = ? AND achievement_id = (SELECT id FROM achievements WHERE code = ?)",
                )
                .bind(&user.id)
                .bind(code)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let reason = format!("update equipped badges: {}", badge_codes.join(","));
    AuditEntry::user_action(&user.id, "me.badges.update")
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .ok();

    let resp = (StatusCode::OK, Json(json!({ "equipped": badge_codes }))).into_response();
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
    reward_coin: i64,
    is_hidden: i64,
    is_enabled: i64,
    sort_order: i64,
    version: i64,
    unlocked_count: i64,
    icon_path: Option<String>,
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
        "reward_coin": r.reward_coin,
        "is_hidden": r.is_hidden != 0,
        "is_enabled": r.is_enabled != 0,
        "sort_order": r.sort_order,
        "unlocked_count": r.unlocked_count,
        "version": r.version,
        "icon_url": crate::achievements::icon_url(&r.code, r.icon_path.as_deref()),
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
                      a.condition_threshold, a.reward_coin, a.is_hidden,
                      a.is_enabled, a.sort_order, a.version,
                      (SELECT COUNT(*) FROM user_achievements ua WHERE ua.achievement_id = a.id) AS unlocked_count,
                      a.icon_path
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
/// condition_threshold/reward_coin）+ 可选 code（仅创建校验）。
#[allow(clippy::too_many_arguments)] // 契约字段平铺，收束为结构体收益有限
fn validate_achievement_fields(
    code: Option<&str>,
    name: &str,
    description: &str,
    category: &str,
    condition_type: &str,
    condition_threshold: i64,
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
    if reward_coin < 0 {
        return Err("reward_coin must be >= 0".to_string());
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
               condition_threshold, reward_coin, is_hidden, is_enabled, sort_order, version, created_at, updated_at)
               VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)";
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
            "reward_coin": reward_coin,
            "is_hidden": is_hidden,
            "is_enabled": is_enabled,
            "sort_order": sort_order,
            "version": 1,
            "icon_url": Value::Null,
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
        reward_coin,
    )
    .map_err(|m| AppError::bad_request(m, request_id, None))?;

    let now = now_millis();
    let sql = "UPDATE achievements SET name = ?, description = ?, category = ?,
               condition_type = ?, condition_threshold = ?, reward_coin = ?,
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
            "reward_coin": reward_coin,
            "is_hidden": is_hidden,
            "is_enabled": is_enabled,
            "sort_order": sort_order,
            "version": current.version + 1,
            "icon_url": crate::achievements::icon_url(&code, current.icon_path.as_deref()),
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

    // 级联清理：图标文件（best-effort；列随行删除）。
    if let Some(filename) = current.icon_path.as_deref().filter(|p| !p.is_empty()) {
        let _ =
            tokio::fs::remove_file(icon::icon_dir(&state.config.storage_dir).join(filename)).await;
    }

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

// ───────────────────────── 成就图标（不走 S3） ─────────────────────────

/// 管理侧 icon 上传的查询参数：reason 必填（审计），二进制请求体无法内联。
#[derive(Debug, Deserialize)]
struct IconUploadQuery {
    reason: Option<String>,
}

/// 管理 icon 写操作共用的 reason 校验（查询参数形态）。
fn required_query_reason(raw: Option<&str>, request_id: &str) -> Result<String, AppError> {
    let reason = raw.map(str::trim).unwrap_or("").to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason query param is required for admin operation",
            request_id,
            None,
        ));
    }
    Ok(reason)
}

/// 校验 code 字符集（文件名路径安全双保险；创建时已校验，防历史脏数据）。
fn code_is_safe(code: &str) -> bool {
    !code.is_empty()
        && code.len() <= 64
        && code
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
}

/// POST /api/v1/admin/achievements/{code}/icon?reason=… — 上传成就图标。
///
/// 请求体 = 原始图片字节（png/jpeg/webp/gif，魔数嗅探，≤2MB）。**不走
/// S3**：直写 `{storage_dir}/achievements/`（[`crate::achievements::icon`]），
/// 内容寻址文件名，替换时清理旧文件；version 递增 +1（配置变更语义）。
async fn admin_upload_achievement_icon(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(code): Path<String>,
    Query(q): Query<IconUploadQuery>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "admin_upload_achievement_icon";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;

    let reason = required_query_reason(q.reason.as_deref(), request_id)?;
    if !code_is_safe(&code) {
        return Err(AppError::bad_request(
            "invalid achievement code",
            request_id,
            None,
        ));
    }

    let current = achievement_by_code(pool, &code, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("achievement not found", request_id))?;

    if body.is_empty() {
        return Err(AppError::bad_request(
            "icon body is empty",
            request_id,
            None,
        ));
    }
    // 兜底大小检查：正常情况下 axum `DefaultBodyLimit`（默认 2MB，与
    // MAX_ICON_BYTES 同值）已用 413 拦下超限请求。
    if body.len() > icon::MAX_ICON_BYTES {
        return Err(AppError::bad_request(
            "icon exceeds 2MB limit",
            request_id,
            None,
        ));
    }
    let Some((ext, content_type)) = icon::sniff_image(&body) else {
        return Err(AppError::bad_request(
            "unsupported image format (png/jpeg/webp/gif only)",
            request_id,
            None,
        ));
    };

    let filename = icon::icon_filename(&current.code, &body, ext);
    let dir = icon::icon_dir(&state.config.storage_dir);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| AppError::internal(format!("create icon dir: {e}"), request_id))?;
    tokio::fs::write(dir.join(&filename), &body)
        .await
        .map_err(|e| AppError::internal(format!("write icon: {e}"), request_id))?;

    // 内容寻址：同名即同内容；替换时清理旧文件（best-effort）。
    if let Some(old) = current
        .icon_path
        .as_deref()
        .filter(|p| !p.is_empty() && p != &filename)
    {
        let _ = tokio::fs::remove_file(dir.join(old)).await;
    }

    let now = now_millis();
    let sql = "UPDATE achievements SET icon_path = ?, version = version + 1, updated_at = ?
               WHERE id = ?";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&filename)
                .bind(now)
                .bind(&current.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&filename)
                .bind(now)
                .bind(&current.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    AuditEntry::user_action(&user.id, "admin.achievement.icon_upload")
        .with_target("achievement", &code)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({
            "bytes": body.len(),
            "content_type": content_type,
            "file": filename,
        }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (
        StatusCode::CREATED,
        Json(json!({
            "code": code,
            "icon_url": format!("/api/v1/achievements/{code}/icon"),
            "content_type": content_type,
            "bytes": body.len(),
            "version": current.version + 1,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// DELETE /api/v1/admin/achievements/{code}/icon — 移除成就图标（body {reason}）。
///
/// 删除磁盘文件（best-effort）+ 清空 `icon_path`；version 递增 +1。
async fn admin_delete_achievement_icon(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(code): Path<String>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "admin_delete_achievement_icon";
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
    let filename = current
        .icon_path
        .filter(|p| !p.is_empty())
        .ok_or_else(|| AppError::not_found("achievement has no icon", request_id))?;

    // 先删磁盘文件（best-effort；列清空后旧文件不再被引用）。
    let _ = tokio::fs::remove_file(icon::icon_dir(&state.config.storage_dir).join(&filename)).await;

    let now = now_millis();
    let sql = "UPDATE achievements SET icon_path = NULL, version = version + 1, updated_at = ?
               WHERE id = ?";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(now)
                .bind(&current.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(now)
                .bind(&current.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    AuditEntry::user_action(&user.id, "admin.achievement.icon_remove")
        .with_target("achievement", &code)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "file": filename }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (
        StatusCode::OK,
        Json(json!({
            "code": code,
            "icon_url": Value::Null,
            "version": current.version + 1,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}
