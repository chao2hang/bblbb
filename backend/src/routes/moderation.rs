use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{app::AppState, auth::session::AuthSession, error::AppError};

/// 审核路由：举报、案件、申诉、处罚、通知
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/reports", post(create_report))
        .route("/api/v1/appeals", get(list_own_appeals).post(create_appeal))
        .route("/api/v1/appeals/{id}", get(get_own_appeal))
        .route("/api/v1/notifications", get(list_notifications))
        .route(
            "/api/v1/notifications/{id}/read",
            post(mark_notification_read),
        )
        .route("/api/v1/admin/moderation/cases", get(list_moderation_cases))
        .route(
            "/api/v1/admin/moderation/cases/{id}",
            get(get_moderation_case).patch(update_moderation_case),
        )
        .route(
            "/api/v1/admin/moderation/appeals",
            get(list_moderation_appeals),
        )
        .route(
            "/api/v1/admin/moderation/appeals/{id}",
            get(get_moderation_appeal).patch(decide_moderation_appeal),
        )
        .route("/api/v1/admin/moderation/sanctions", post(create_sanction))
}

// ─── 通知端点 ─────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NotificationListQuery {
    #[serde(default)]
    unread_only: Option<bool>,
    #[serde(default = "default_limit")]
    limit: i64,
    /// 分页游标（接口契约保留字段，游标分页待实现）
    #[serde(default)]
    #[allow(dead_code)]
    cursor: Option<String>,
}

fn default_limit() -> i64 {
    20
}

/// GET /api/v1/notifications — 列出当前用户的通知
async fn list_notifications(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<NotificationListQuery>,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_notifications";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);
    let unread_filter = query.unread_only.unwrap_or(false);

    let rows = match pool {
        Either::Left(p) => {
            if unread_filter {
                sqlx::query_as::<_, NotificationRow>(
                    "SELECT id, type, title, body, link, is_read, created_at, read_at
                     FROM notifications WHERE user_id = ? AND is_read = 0
                     ORDER BY created_at DESC LIMIT ?",
                )
                .bind(&user.id)
                .bind(limit)
                .fetch_all(p)
                .await
            } else {
                sqlx::query_as::<_, NotificationRow>(
                    "SELECT id, type, title, body, link, is_read, created_at, read_at
                     FROM notifications WHERE user_id = ?
                     ORDER BY created_at DESC LIMIT ?",
                )
                .bind(&user.id)
                .bind(limit)
                .fetch_all(p)
                .await
            }
        }
        Either::Right(p) => {
            if unread_filter {
                sqlx::query_as::<_, NotificationRow>(
                    "SELECT id, type, title, body, link, is_read, created_at, read_at
                     FROM notifications WHERE user_id = ? AND is_read = 0
                     ORDER BY created_at DESC LIMIT ?",
                )
                .bind(&user.id)
                .bind(limit)
                .fetch_all(p)
                .await
            } else {
                sqlx::query_as::<_, NotificationRow>(
                    "SELECT id, type, title, body, link, is_read, created_at, read_at
                     FROM notifications WHERE user_id = ?
                     ORDER BY created_at DESC LIMIT ?",
                )
                .bind(&user.id)
                .bind(limit)
                .fetch_all(p)
                .await
            }
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    // 获取未读计数
    let unread_count: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND is_read = 0",
            )
            .bind(&user.id)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND is_read = 0",
            )
            .bind(&user.id)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let items: Vec<Value> = rows
        .iter()
        .map(|n| {
            json!({
                "id": n.id,
                "type": n.type_field,
                "title": n.title,
                "body": n.body,
                "link": n.link,
                "is_read": n.is_read != 0,
                "created_at": n.created_at,
                "read_at": n.read_at,
            })
        })
        .collect();

    Ok(Json(json!({
        "items": items,
        "unread_count": unread_count,
        "next_cursor": null,
        "has_more": false,
    })))
}

/// POST /api/v1/notifications/{id}/read — 标记通知为已读
async fn mark_notification_read(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let request_id = "mark_notification_read";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let now = chrono::Utc::now().timestamp();

    let affected = match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE notifications SET is_read = 1, read_at = ? WHERE id = ? AND user_id = ? AND is_read = 0")
                .bind(now)
                .bind(&id)
                .bind(&user.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query("UPDATE notifications SET is_read = 1, read_at = ? WHERE id = ? AND user_id = ? AND is_read = 0")
                .bind(now)
                .bind(&id)
                .bind(&user.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
    };

    if affected == 0 {
        return Err(AppError::not_found(
            "notification not found or already read",
            request_id,
        ));
    }

    Ok(StatusCode::NO_CONTENT)
}

// ─── 举报端点 ─────────────────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct CreateReportRequest {
    target_type: String,
    target_id: String,
    reason: String,
    #[serde(default)]
    detail: Option<String>,
}

/// POST /api/v1/reports — 创建举报
async fn create_report(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<CreateReportRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "create_report";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    if req.reason.is_empty() || req.reason.len() > 200 {
        return Err(AppError::bad_request(
            "reason must be 1-200 characters",
            request_id,
            None,
        ));
    }

    // 存储举报为系统通知（简化实现：将举报记录为通知）
    let report_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp();

    // 创建一条通知给管理员（简化：记录到审计日志表）
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO audit_logs (id, actor_id, action, target_type, target_id, metadata, created_at)
                 VALUES (?, ?, 'report', ?, ?, ?, ?)",
            )
            .bind(&report_id)
            .bind(&user.id)
            .bind(&req.target_type)
            .bind(&req.target_id)
            .bind(serde_json::to_string(&json!({
                "reason": req.reason,
                "detail": req.detail,
            }))
            .unwrap_or_default())
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO audit_logs (id, actor_id, action, target_type, target_id, metadata, created_at)
                 VALUES (?, ?, 'report', ?, ?, ?, ?)",
            )
            .bind(&report_id)
            .bind(&user.id)
            .bind(&req.target_type)
            .bind(&req.target_id)
            .bind(serde_json::to_string(&json!({
                "reason": req.reason,
                "detail": req.detail,
            }))
            .unwrap_or_default())
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": report_id,
            "status": "submitted",
            "created_at": now,
        })),
    ))
}

// ─── 申诉端点（桩实现） ──────────────────────────────────────────────────

async fn list_own_appeals(
    State(_state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let _user = auth.require_auth("list_own_appeals")?;
    Ok(Json(
        json!({ "items": [], "next_cursor": null, "has_more": false }),
    ))
}

async fn create_appeal(
    State(_state): State<AppState>,
    auth: AuthSession,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let _user = auth.require_auth("create_appeal")?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": uuid::Uuid::now_v7().to_string(), "status": "pending" })),
    ))
}

async fn get_own_appeal(
    State(_state): State<AppState>,
    auth: AuthSession,
    Path(_id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let _user = auth.require_auth("get_own_appeal")?;
    Err(AppError::not_found("appeal not found", "get_own_appeal"))
}

// ─── 管理端审核端点（桩实现） ────────────────────────────────────────────

async fn list_moderation_cases(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listModerationCases")
}

async fn get_moderation_case(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getModerationCase")
}

async fn update_moderation_case(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("updateModerationCase")
}

async fn list_moderation_appeals(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listModerationAppeals")
}

async fn get_moderation_appeal(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getModerationAppeal")
}

async fn decide_moderation_appeal(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("decideModerationAppeal")
}

async fn create_sanction(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_admin_moderation_sanctions")
}

// ─── 数据库行结构 ─────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: String,
    #[sqlx(rename = "type")]
    type_field: String,
    title: String,
    body: Option<String>,
    link: Option<String>,
    is_read: i64,
    created_at: i64,
    read_at: Option<i64>,
}

fn not_implemented(operation: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "type": "about:blank",
            "title": "Not Implemented",
            "status": 501,
            "code": "not_implemented",
            "detail": format!("Operation '{}' is not yet implemented", operation),
        })),
    )
}
