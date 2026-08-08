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
    moderation::appeals::service as appeals,
    moderation::appeals::service::AppealsError,
    moderation::cases::service as cases,
    moderation::cases::service::CasesError,
    moderation::model::{
        AppealDecisionValue, CasePriority, CaseStatus, ReportReasonCode, ReportTargetType,
    },
    notifications::service as notifications,
};

/// 审核路由：举报、案件、申诉、处罚、通知
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/reports", get(list_own_reports).post(create_report))
        .route("/api/v1/reports/{id}/withdraw", post(withdraw_report))
        .route("/api/v1/appeals", get(list_own_appeals).post(create_appeal))
        .route("/api/v1/appeals/{id}", get(get_own_appeal))
        .route("/api/v1/appeals/{id}/withdraw", post(withdraw_appeal))
        .route("/api/v1/notifications", get(list_notifications))
        .route(
            "/api/v1/notifications/{id}/read",
            post(mark_notification_read),
        )
        .route(
            "/api/v1/notifications/read-all",
            post(mark_all_notifications_read),
        )
        .route(
            "/api/v1/notifications/preferences",
            get(get_notification_preferences).put(put_notification_preferences),
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

/// GET /api/v1/notifications — 列出当前用户的通知（游标分页 + 权限复查）
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

    let unread_filter = query.unread_only.unwrap_or(false);
    let (items, has_more) = notifications::list_notifications(
        pool,
        &user.id,
        query.limit,
        unread_filter,
        query.cursor.as_deref(),
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let unread_count = notifications::unread_count(pool, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    // M05-NOTIFY-06：读取时权限复查，隐藏/删除资源只显示安全失效状态。
    let items = notifications::project_list(pool, items)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(Json(json!({
        "items": items,
        "unread_count": unread_count,
        "next_cursor": items.last().and_then(|n| n.get("id")).and_then(|v| v.as_str()),
        "has_more": has_more,
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

    let hit = notifications::mark_read(pool, &user.id, &id, notifications::now())
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if !hit {
        return Err(AppError::not_found(
            "notification not found or already read",
            request_id,
        ));
    }
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/notifications/read-all — 批量已读（M05-NOTIFY-03）
async fn mark_all_notifications_read(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "mark_all_notifications_read";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let updated = notifications::mark_all_read(pool, &user.id, notifications::now())
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(Json(json!({ "updated": updated })))
}

/// GET /api/v1/notifications/preferences — 类别偏好（M05-NOTIFY-04）
async fn get_notification_preferences(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_notification_preferences";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let prefs = notifications::get_preferences(pool, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let items: Vec<Value> = prefs
        .into_iter()
        .map(|p| {
            json!({
                "category": p.category.as_str(),
                "email_enabled": p.email_enabled,
                "in_app_enabled": p.in_app_enabled,
                "push_enabled": p.push_enabled,
                "updated_at": p.updated_at,
            })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}

#[derive(serde::Deserialize)]
struct NotificationPreferenceRequest {
    category: String,
    email_enabled: bool,
    in_app_enabled: bool,
    push_enabled: bool,
}

/// PUT /api/v1/notifications/preferences — 更新类别偏好（security 不可全关）
async fn put_notification_preferences(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<NotificationPreferenceRequest>,
) -> Result<Json<Value>, AppError> {
    let request_id = "put_notification_preferences";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let category = crate::notifications::model::NotificationCategory::parse(&req.category)
        .ok_or_else(|| {
            AppError::bad_request(
                "category must be activity, moderation, system, security or digest",
                request_id,
                None,
            )
        })?;
    notifications::set_preference(
        pool,
        &user.id,
        category,
        req.email_enabled,
        req.in_app_enabled,
        req.push_enabled,
        notifications::now(),
    )
    .await
    .map_err(|e| match e {
        notifications::NotifyError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        notifications::NotifyError::Db(msg) => AppError::internal(msg, request_id),
    })?;
    Ok(Json(
        json!({ "category": category.as_str(), "updated": true }),
    ))
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

// ─── 申诉端点（M05-APPEALS） ───────────────────────────────────────────

fn map_appeals_error(err: AppealsError, request_id: &'static str) -> AppError {
    match err {
        AppealsError::NotFound(msg) => AppError::not_found(msg, request_id),
        AppealsError::Forbidden(msg) => AppError::forbidden(msg, request_id),
        AppealsError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        AppealsError::Conflict(msg) => AppError::conflict(msg, request_id),
        AppealsError::ReviewerConflict(msg) => AppError::conflict(msg, request_id),
        AppealsError::StaleVersion => {
            AppError::conflict("appeal version mismatch: concurrent decision", request_id)
        }
        AppealsError::Db(msg) => AppError::internal(msg, request_id),
    }
}

#[derive(serde::Deserialize)]
struct CreateAppealRequest {
    sanction_id: String,
    content: String,
}

/// GET /api/v1/appeals — 我的申诉列表（申诉人侧投影）
async fn list_own_appeals(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_own_appeals";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let items = appeals::list_own_appeals(pool, &user.id, 50)
        .await
        .map_err(|e| map_appeals_error(e, request_id))?;
    let items: Vec<Value> = items.iter().map(appeals::own_appeal_projection).collect();
    Ok(Json(
        json!({ "items": items, "next_cursor": null, "has_more": false }),
    ))
}

/// POST /api/v1/appeals — 创建申诉
async fn create_appeal(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<CreateAppealRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "create_appeal";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let appeal = appeals::create_appeal(
        pool,
        &user.id,
        appeals::CreateAppealInput {
            sanction_id: req.sanction_id,
            message: req.content,
        },
        appeals::now(),
    )
    .await
    .map_err(|e| map_appeals_error(e, request_id))?;
    Ok((
        StatusCode::CREATED,
        Json(appeals::own_appeal_projection(&appeal)),
    ))
}

/// GET /api/v1/appeals/{id} — 我的申诉详情
async fn get_own_appeal(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_own_appeal";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let appeal = appeals::get_own_appeal(pool, &user.id, &id)
        .await
        .map_err(|e| map_appeals_error(e, request_id))?;
    Ok(Json(appeals::own_appeal_projection(&appeal)))
}

/// POST /api/v1/appeals/{id}/withdraw — 未审理前撤回
async fn withdraw_appeal(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let request_id = "withdraw_appeal";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    appeals::withdraw_appeal(pool, &user.id, &id, appeals::now())
        .await
        .map_err(|e| map_appeals_error(e, request_id))?;
    Ok(StatusCode::NO_CONTENT)
}

// ─── 管理端审核端点（M05-CASES-03/04/05） ───────────────────────────────

async fn require_moderation_perm(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    permission: &str,
    request_id: &'static str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, permission, None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            format!("{permission} permission required"),
            request_id,
        ));
    }
    Ok(())
}

async fn require_moderation(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    _board_id: Option<&str>,
    request_id: &'static str,
) -> Result<(), AppError> {
    require_moderation_perm(pool, user_id, "moderation.review", request_id).await
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

/// GET /api/v1/admin/moderation/appeals — 管理端申诉列表（审核员侧投影）
async fn list_moderation_appeals(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_moderation_appeals";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_moderation(pool, &user.id, None, request_id).await?;

    let items = appeals::list_all_appeals(pool, 100)
        .await
        .map_err(|e| map_appeals_error(e, request_id))?;
    let mut projected = Vec::with_capacity(items.len());
    for appeal in items {
        let decisions = appeals::list_decisions(pool, &appeal.id)
            .await
            .map_err(|e| map_appeals_error(e, request_id))?;
        projected.push(appeals::admin_appeal_projection(&appeal, &decisions));
    }
    Ok(Json(json!({ "items": projected })))
}

/// GET /api/v1/admin/moderation/appeals/{id} — 管理端申诉详情
async fn get_moderation_appeal(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_moderation_appeal";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_moderation(pool, &user.id, None, request_id).await?;

    let row: Option<AppealAdminRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, AppealAdminRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals WHERE id = ?",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, AppealAdminRow>(
            "SELECT id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at
             FROM appeals WHERE id = ?",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let Some(row) = row else {
        return Err(AppError::not_found("appeal not found", request_id));
    };
    let appeal = row.into_model();
    let decisions = appeals::list_decisions(pool, &id)
        .await
        .map_err(|e| map_appeals_error(e, request_id))?;
    Ok(Json(appeals::admin_appeal_projection(&appeal, &decisions)))
}

#[derive(serde::Deserialize)]
struct ModerationDecisionRequest {
    decision: String,
    reason: String,
    expected_version: i64,
}

/// PATCH /api/v1/admin/moderation/appeals/{id} — 决定申诉（uphold/partial/reject）
async fn decide_moderation_appeal(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    Json(req): Json<ModerationDecisionRequest>,
) -> Result<Json<Value>, AppError> {
    let request_id = "decide_moderation_appeal";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_moderation_perm(pool, &user.id, "moderation.sanction", request_id).await?;

    let decision = AppealDecisionValue::parse(&req.decision).ok_or_else(|| {
        AppError::bad_request(
            "decision must be upheld, partially_upheld or rejected",
            request_id,
            None,
        )
    })?;
    let result = appeals::decide_appeal(
        pool,
        &user.id,
        &id,
        decision,
        &req.reason,
        req.expected_version,
        appeals::now(),
    )
    .await
    .map_err(|e| map_appeals_error(e, request_id))?;
    Ok(Json(result))
}

/// POST /api/v1/admin/moderation/sanctions — 创建处罚（M13-ADMIN-03：
/// 服务端重新校验版主范围；reason 必填；走线上 sanctions 服务）。
#[derive(Deserialize)]
struct CreateSanctionRequest {
    target_user_id: String,
    board_id: Option<String>,
    kind: String,
    reason: String,
    starts_at: Option<i64>,
    ends_at: Option<i64>,
}

async fn create_sanction(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<CreateSanctionRequest>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_moderation_sanctions";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    // 版主范围在 API 再校验（moderation.sanction；板块范围按 board_id）。
    require_moderation_perm(pool, &user.id, "moderation.sanction", request_id).await?;

    let kind = crate::moderation::model::SanctionKind::parse(&req.kind).ok_or_else(|| {
        AppError::bad_request(
            "kind must be one of warning|rate_limit|mute|board_mute|ban",
            request_id,
            None,
        )
    })?;
    let now = crate::moderation::sanctions::service::now();
    let input = crate::moderation::sanctions::service::CreateSanctionInput {
        target_user_id: req.target_user_id,
        board_id: req.board_id,
        kind,
        reason: req.reason,
        starts_at: req.starts_at.unwrap_or(now),
        ends_at: req.ends_at,
    };
    let sanction =
        crate::moderation::sanctions::service::create_sanction(pool, &user.id, input, now)
            .await
            .map_err(|e| map_sanctions_error(e, request_id))?;
    Ok(Json(sanction_to_json(&sanction)))
}

fn sanction_to_json(s: &crate::moderation::model::Sanction) -> Value {
    json!({
        "id": s.id,
        "user_id": s.user_id,
        "board_id": s.board_id,
        "kind": s.kind.as_str(),
        "status": s.status.as_str(),
        "reason": s.reason,
        "starts_at": s.starts_at,
        "ends_at": s.ends_at,
        "created_by": s.created_by,
        "created_at": s.created_at,
        "revoked_at": s.revoked_at,
        "revoked_by": s.revoked_by,
    })
}

fn map_sanctions_error(
    e: crate::moderation::sanctions::service::SanctionsError,
    request_id: &str,
) -> AppError {
    match e {
        crate::moderation::sanctions::service::SanctionsError::NotFound(m) => {
            AppError::not_found(m, request_id)
        }
        crate::moderation::sanctions::service::SanctionsError::Forbidden(m) => {
            AppError::forbidden(m, request_id)
        }
        crate::moderation::sanctions::service::SanctionsError::Escalation(m) => {
            AppError::forbidden(m, request_id)
        }
        crate::moderation::sanctions::service::SanctionsError::Invalid(m) => {
            AppError::bad_request(m, request_id, None)
        }
        crate::moderation::sanctions::service::SanctionsError::Db(m) => {
            AppError::internal(m, request_id)
        }
    }
}

// ─── 数据库行结构 ─────────────────────────────────────────────────────────

/// 管理端申诉详情行（管理端读取任意申诉）。
#[derive(sqlx::FromRow)]
struct AppealAdminRow {
    id: String,
    sanction_id: String,
    user_id: String,
    message: String,
    status: String,
    reviewed_by: Option<String>,
    decided_at: Option<i64>,
    submitted_at: i64,
    updated_at: i64,
}

impl AppealAdminRow {
    fn into_model(self) -> crate::moderation::model::Appeal {
        crate::moderation::model::Appeal {
            id: self.id,
            sanction_id: self.sanction_id,
            user_id: self.user_id,
            message: self.message,
            status: crate::moderation::model::AppealStatus::parse(&self.status)
                .unwrap_or(crate::moderation::model::AppealStatus::Submitted),
            reviewed_by: self.reviewed_by,
            decided_at: self.decided_at,
            submitted_at: self.submitted_at,
            updated_at: self.updated_at,
        }
    }
}
