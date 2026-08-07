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

use crate::{
    app::AppState,
    auth::session::AuthSession,
    authz::decision::AUTHZ_POLICY_VERSION,
    authz::enforce::authorize_action,
    error::AppError,
    moderation::cases::service as cases,
    moderation::cases::service::CasesError,
    moderation::model::{CasePriority, CaseStatus, ReportReasonCode, ReportTargetType},
};

/// 审核路由：举报、案件、申诉、处罚、通知
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/reports", get(list_own_reports).post(create_report))
        .route("/api/v1/reports/{id}/withdraw", post(withdraw_report))
        .route("/api/v1/appeals", get(list_own_appeals).post(create_appeal))
        .route("/api/v1/appeals/{id}", get(get_own_appeal))
        .route("/api/v1/notifications", get(list_notifications))
        .route(
            "/api/v1/notifications/{id}/read",
            post(mark_notification_read),
        )
        .route("/api/v1/admin/moderation/cases", get(list_moderation_cases))
        .route(
            "/api/v1/admin/moderation/cases",
            post(create_moderation_case),
        )
        .route(
            "/api/v1/admin/moderation/cases/{id}",
            get(get_moderation_case).patch(update_moderation_case),
        )
        .route(
            "/api/v1/admin/moderation/cases/{id}/assign",
            post(assign_moderation_case),
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

fn map_cases_error(err: CasesError, request_id: &'static str) -> AppError {
    match err {
        CasesError::NotFound(msg) => AppError::not_found(msg, request_id),
        CasesError::Forbidden(msg) => AppError::forbidden(msg, request_id),
        CasesError::InvalidReason(msg) => AppError::bad_request(msg, request_id, None),
        CasesError::DetailTooLong => AppError::bad_request(
            "report detail must not exceed 2000 characters",
            request_id,
            None,
        ),
        CasesError::DuplicateReport { existing_id } => AppError::conflict(
            format!("report already exists within the dedup window ({existing_id})"),
            request_id,
        ),
        CasesError::InvalidTransition { from, to } => AppError::conflict(
            format!("invalid case transition: {from} -> {to}"),
            request_id,
        ),
        CasesError::TargetNotFound => AppError::not_found("report target not found", request_id),
        CasesError::Db(msg) => AppError::internal(msg, request_id),
    }
}

/// POST /api/v1/reports — 创建举报（M05-CASES-01/02）
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

    let target_type = ReportTargetType::parse(&req.target_type).ok_or_else(|| {
        AppError::bad_request(
            "target_type must be post, comment, user or board",
            request_id,
            None,
        )
    })?;
    let reason_code = ReportReasonCode::parse(&req.reason).ok_or_else(|| {
        AppError::bad_request(
            "reason must be one of: spam, harassment, illegal, nsfw, misinformation, impersonation, other",
            request_id,
            None,
        )
    })?;

    let now = cases::now();
    let report = cases::create_report(
        pool,
        &user.id,
        cases::CreateReportInput {
            target_type,
            target_id: req.target_id,
            reason_code,
            details: req.detail,
        },
        now,
    )
    .await
    .map_err(|e| map_cases_error(e, request_id))?;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": report.id,
            "target_type": report.target_type.as_str(),
            "target_id": report.target_id,
            "reason_code": report.reason_code.as_str(),
            "status": report.status.as_str(),
            "created_at": report.created_at,
        })),
    ))
}

/// GET /api/v1/reports — 我的举报列表（安全投影，M05-CASES-01/02）
async fn list_own_reports(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_own_reports";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let items = cases::list_own_reports(pool, &user.id, 50)
        .await
        .map_err(|e| map_cases_error(e, request_id))?;
    let items: Vec<Value> = items
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "target_type": r.target_type,
                "target_id": r.target_id,
                "reason_code": r.reason_code,
                "status": r.status,
                "created_at": r.created_at,
                "updated_at": r.updated_at,
            })
        })
        .collect();
    Ok(Json(
        json!({ "items": items, "next_cursor": null, "has_more": false }),
    ))
}

/// POST /api/v1/reports/{id}/withdraw — 撤回举报（M05-CASES-02）
async fn withdraw_report(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let request_id = "withdraw_report";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    cases::withdraw_report(pool, &user.id, &id, cases::now())
        .await
        .map_err(|e| map_cases_error(e, request_id))?;
    Ok(StatusCode::NO_CONTENT)
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

// ─── 管理端审核端点（M05-CASES-03/04/05） ───────────────────────────────

async fn require_moderation(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    board_id: Option<&str>,
    request_id: &'static str,
) -> Result<(), AppError> {
    let decision = authorize_action(
        pool,
        user_id,
        "moderation.review",
        board_id,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "moderation.review permission required",
            request_id,
        ));
    }
    Ok(())
}

#[derive(serde::Deserialize)]
struct CreateCaseRequest {
    report_id: String,
    #[serde(default = "default_priority")]
    priority: String,
}

fn default_priority() -> String {
    "normal".to_string()
}

async fn create_moderation_case(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<CreateCaseRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "create_moderation_case";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_moderation(pool, &user.id, None, request_id).await?;

    let priority = CasePriority::parse(&req.priority).ok_or_else(|| {
        AppError::bad_request(
            "priority must be low, normal, high or urgent",
            request_id,
            None,
        )
    })?;
    let case_id =
        cases::open_case_from_report(pool, &user.id, &req.report_id, priority, cases::now())
            .await
            .map_err(|e| map_cases_error(e, request_id))?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": case_id, "status": "open" })),
    ))
}

async fn list_moderation_cases(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_moderation_cases";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_moderation(pool, &user.id, None, request_id).await?;

    type CaseListRow = (String, String, String, String, Option<String>, i64, i64);
    let rows: Vec<CaseListRow> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT id, title, status, priority, assigned_to, created_at, updated_at
             FROM moderation_cases ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT id, title, status, priority, assigned_to, created_at, updated_at
             FROM moderation_cases ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let items: Vec<Value> = rows
        .into_iter()
        .map(
            |(id, title, status, priority, assigned_to, created_at, updated_at)| {
                json!({
                    "id": id,
                    "title": title,
                    "status": status,
                    "priority": priority,
                    "assigned_to": assigned_to,
                    "created_at": created_at,
                    "updated_at": updated_at,
                })
            },
        )
        .collect();
    Ok(Json(json!({ "items": items })))
}

async fn get_moderation_case(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_moderation_case";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_moderation(pool, &user.id, None, request_id).await?;

    type CaseDetailRow = (
        String,
        String,
        String,
        String,
        Option<String>,
        i64,
        i64,
        Option<i64>,
        Option<String>,
    );
    let row: Option<CaseDetailRow> = match pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT id, title, status, priority, assigned_to, created_at, updated_at, resolved_at, resolution
                 FROM moderation_cases WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            Either::Right(p) => sqlx::query_as(
                "SELECT id, title, status, priority, assigned_to, created_at, updated_at, resolved_at, resolution
                 FROM moderation_cases WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        };
    let Some((
        id,
        title,
        status,
        priority,
        assigned_to,
        created_at,
        updated_at,
        resolved_at,
        resolution,
    )) = row
    else {
        return Err(AppError::not_found("case not found", request_id));
    };
    Ok(Json(json!({
        "id": id,
        "title": title,
        "status": status,
        "priority": priority,
        "assigned_to": assigned_to,
        "created_at": created_at,
        "updated_at": updated_at,
        "resolved_at": resolved_at,
        "resolution": resolution,
    })))
}

#[derive(serde::Deserialize)]
struct UpdateCaseRequest {
    status: String,
    #[serde(default)]
    resolution: Option<String>,
}

async fn update_moderation_case(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    Json(req): Json<UpdateCaseRequest>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_moderation_case";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_moderation(pool, &user.id, None, request_id).await?;

    let target = CaseStatus::parse(&req.status).ok_or_else(|| {
        AppError::bad_request(
            "status must be open, triaged, investigating, resolved, rejected or reopened",
            request_id,
            None,
        )
    })?;
    cases::transition_case(
        pool,
        &user.id,
        &id,
        target,
        req.resolution.as_deref(),
        cases::now(),
    )
    .await
    .map_err(|e| map_cases_error(e, request_id))?;
    Ok(Json(json!({ "id": id, "status": target.as_str() })))
}

#[derive(serde::Deserialize)]
struct AssignCaseRequest {
    assignee_id: String,
    #[serde(default)]
    note: Option<String>,
}

async fn assign_moderation_case(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    Json(req): Json<AssignCaseRequest>,
) -> Result<Json<Value>, AppError> {
    let request_id = "assign_moderation_case";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_moderation(pool, &user.id, None, request_id).await?;

    cases::assign_case(
        pool,
        &user.id,
        &id,
        &req.assignee_id,
        req.note.as_deref(),
        cases::now(),
    )
    .await
    .map_err(|e| map_cases_error(e, request_id))?;
    Ok(Json(json!({ "id": id, "assigned_to": req.assignee_id })))
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
