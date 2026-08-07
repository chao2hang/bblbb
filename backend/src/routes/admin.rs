use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{delete, get, patch, post, put},
    Router,
};
use serde_json::{json, Value};
use sqlx::Row;

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::boards::admin::{create_board, update_board, BoardCreateInput, BoardUpdateInput};
use crate::error::AppError;
use crate::tags::admin::{create_tag, update_tag};

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
            "reason is required for admin operation",
            request_id,
            None,
        ));
    }
    Ok(reason)
}

/// 管理后台路由
///
/// M6/M7 域路由委托给按域分区的子模块（admin_storage / admin_download /
/// admin_activity / admin_shop），由对应域 agent 填充，避免单文件并发冲突。
use super::{admin_activity, admin_download, admin_shop, admin_storage};

pub fn router() -> Router<AppState> {
    Router::new()
        // 用户管理
        .route(
            "/api/v1/admin/users",
            get(list_admin_users).post(create_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}",
            get(get_admin_user).patch(update_admin_user),
        )
        // 角色管理
        .route(
            "/api/v1/admin/roles",
            get(list_admin_roles).post(create_admin_role),
        )
        .route(
            "/api/v1/admin/roles/{id}",
            get(get_admin_role).patch(update_admin_role),
        )
        // 板块管理
        .route(
            "/api/v1/admin/boards",
            get(list_admin_boards).post(create_admin_board),
        )
        .route(
            "/api/v1/admin/boards/{id}",
            get(get_admin_board).patch(update_admin_board),
        )
        // 标签管理
        .route(
            "/api/v1/admin/tags",
            get(list_admin_tags).post(create_admin_tag),
        )
        .route(
            "/api/v1/admin/tags/{id}",
            get(get_admin_tag).patch(update_admin_tag),
        )
        // M6 存储配额（admin_storage 域 agent 填充）
        .merge(admin_storage::router())
        // M6 下载计费（admin_download 域 agent 填充）
        .merge(admin_download::router())
        // M7 活跃任务（admin_activity 域 agent 填充）
        .merge(admin_activity::router())
        // M7 商城（admin_shop 域 agent 填充）
        .merge(admin_shop::router())
        // AI 管理
        .route(
            "/api/v1/admin/ai/config",
            get(get_admin_ai_config).patch(update_ai_config),
        )
        .route("/api/v1/admin/ai/providers/test", post(test_ai_provider))
        .route("/api/v1/admin/ai/tasks", get(list_ai_tasks))
        .route(
            "/api/v1/admin/ai/tasks/{id}/cancel",
            post(cancel_ai_task_admin),
        )
        .route("/api/v1/admin/ai/tasks/{id}/retry", post(retry_ai_task))
        // 视频管理
        .route("/api/v1/admin/video/policies", get(list_video_policies))
        .route("/api/v1/admin/video/policies/test", post(test_video_policy))
        .route(
            "/api/v1/admin/video/policies/{provider}",
            get(get_video_policy).patch(update_video_policy),
        )
        // OAuth 客户端
        .route(
            "/api/v1/admin/oauth-clients",
            get(list_oauth_clients).post(create_oauth_client),
        )
        .route(
            "/api/v1/admin/oauth-clients/{id}",
            get(get_oauth_client).patch(update_oauth_client),
        )
        // Marketplace 管理
        .route(
            "/api/v1/admin/marketplace/clients",
            get(list_marketplace_clients),
        )
        .route(
            "/api/v1/admin/marketplace/clients/{id}",
            patch(update_marketplace_client),
        )
        .route(
            "/api/v1/admin/marketplace/clients/{id}/rotate-webhook-secret",
            post(rotate_webhook_secret),
        )
        .route(
            "/api/v1/admin/marketplace/transactions",
            get(list_marketplace_transactions),
        )
        // 主题（管理端，公开端在 themes.rs）
        .route("/api/v1/admin/themes", get(list_themes))
        .route(
            "/api/v1/admin/themes/data-packages",
            post(upload_theme_package),
        )
        .route("/api/v1/admin/themes/default", put(set_default_theme))
        .route("/api/v1/admin/themes/{name}", delete(delete_theme))
        .route(
            "/api/v1/admin/themes/{name}/settings",
            patch(update_theme_settings),
        )
}

async fn list_admin_users(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listAdminUsers")
}
async fn create_admin_user(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("createAdminUser")
}
async fn get_admin_user(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getAdminUser")
}
async fn update_admin_user(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("updateAdminUser")
}
async fn list_admin_roles(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listAdminRoles")
}
async fn create_admin_role(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("createAdminRole")
}
async fn get_admin_role(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getAdminRole")
}
async fn update_admin_role(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("updateAdminRole")
}
async fn list_admin_boards(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listAdminBoards")
}
/// POST /api/v1/admin/boards — 创建板块（权限门 + 校验 + 审计，M03-BOARDS-05）
async fn create_admin_board(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "createAdminBoard";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "board.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "board.manage permission required",
            request_id,
        ));
    }

    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin board create",
            request_id,
            None,
        ));
    }

    let input = BoardCreateInput {
        slug: body
            .get("slug")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name: body
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        sort_order: body.get("sort_order").and_then(Value::as_i64).unwrap_or(0),
        parent_id: body
            .get("parent_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        visibility: body
            .get("visibility")
            .and_then(Value::as_str)
            .unwrap_or("public")
            .to_string(),
        posting_mode: body
            .get("posting_mode")
            .and_then(Value::as_str)
            .unwrap_or("normal")
            .to_string(),
    };

    let created = create_board(pool, &user.id, input, &reason, request_id).await?;
    Ok(Json(created))
}

/// PATCH /api/v1/admin/boards/{id} — 更新板块（If-Match 版本 + reason + 审计，
/// M03-BOARDS-05）
async fn update_admin_board(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    auth: AuthSession,
    body: Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "updateAdminBoard";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "board.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "board.manage permission required",
            request_id,
        ));
    }

    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current version integer",
                request_id,
                None,
            )
        })?;

    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin board update",
            request_id,
            None,
        ));
    }

    let input = BoardUpdateInput {
        slug: body.get("slug").and_then(Value::as_str).map(str::to_string),
        name: body.get("name").and_then(Value::as_str).map(str::to_string),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        sort_order: body.get("sort_order").and_then(Value::as_i64),
        parent_id: body
            .get("parent_id")
            .map(|v| v.as_str().map(str::to_string)),
        is_active: body.get("is_active").and_then(Value::as_bool),
        visibility: body
            .get("visibility")
            .and_then(Value::as_str)
            .map(str::to_string),
        posting_mode: body
            .get("posting_mode")
            .and_then(Value::as_str)
            .map(str::to_string),
    };

    let updated = update_board(pool, &user.id, &id, input, if_match, &reason, request_id).await?;
    Ok(Json(updated))
}
async fn get_admin_board(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getAdminBoard")
}
/// GET /api/v1/admin/tags — 全部标签（含禁用状态，M03-BOARDS-06）
async fn list_admin_tags(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "listAdminTags";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "tag.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "tag.manage permission required",
            request_id,
        ));
    }

    let tags = crate::tags::load_all_tags(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let items: Vec<Value> = tags
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "slug": t.slug,
                "name": t.name,
                "description": t.description,
                "color": t.color,
                "group_id": t.group_id,
                "usage_count": t.usage_count,
                "is_active": t.is_active != 0,
            })
        })
        .collect();
    Ok(Json(json!({ "items": items })))
}
/// POST /api/v1/admin/tags — 创建标签（唯一性 + 审计，M03-BOARDS-07）
async fn create_admin_tag(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "createAdminTag";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "tag.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "tag.manage permission required",
            request_id,
        ));
    }

    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin tag create",
            request_id,
            None,
        ));
    }

    let input = crate::tags::TagCreateInput {
        name: body
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        slug: body.get("slug").and_then(Value::as_str).map(str::to_string),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        color: body
            .get("color")
            .and_then(Value::as_str)
            .map(str::to_string),
        group_id: body
            .get("group_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    let created = create_tag(pool, &user.id, input, &reason, request_id).await?;
    Ok(Json(created))
}

/// PATCH /api/v1/admin/tags/{id} — 更新标签（If-Match 版本 + 审计，M03-BOARDS-07）
async fn update_admin_tag(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    auth: AuthSession,
    body: Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "updateAdminTag";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "tag.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "tag.manage permission required",
            request_id,
        ));
    }

    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current version integer",
                request_id,
                None,
            )
        })?;

    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin tag update",
            request_id,
            None,
        ));
    }

    let input = crate::tags::TagUpdateInput {
        name: body.get("name").and_then(Value::as_str).map(str::to_string),
        slug: body.get("slug").map(|v| v.as_str().map(str::to_string)),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        color: body.get("color").map(|v| v.as_str().map(str::to_string)),
        group_id: body.get("group_id").map(|v| v.as_str().map(str::to_string)),
        is_active: body.get("is_active").and_then(Value::as_bool),
    };
    let updated = update_tag(pool, &user.id, &id, input, if_match, &reason, request_id).await?;
    Ok(Json(updated))
}
async fn get_admin_tag(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getAdminTag")
}
/// GET /api/v1/admin/ai/config — Provider 列表（脱敏）+ 默认策略（M09-UI-06）。
async fn get_admin_ai_config(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_ai_config";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let items: Vec<Value> = match pool {
        sqlx::Either::Left(p) => sqlx::query(
            "SELECT id, name, adapter_type, base_url, default_model, status, secret_configured, data_mode, timeout_ms, max_input_tokens, max_output_tokens, max_concurrency, version, created_at, updated_at
             FROM ai_providers ORDER BY name",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(ai_provider_config_row)
        .collect(),
        sqlx::Either::Right(p) => sqlx::query(
            "SELECT id, name, adapter_type, base_url, default_model, status, secret_configured, data_mode, timeout_ms, max_input_tokens, max_output_tokens, max_concurrency, version, created_at, updated_at
             FROM ai_providers ORDER BY name",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(ai_provider_config_row_mysql)
        .collect(),
    };
    Ok(Json(json!({ "providers": items })))
}

fn ai_provider_config_row(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "name": r.get::<String,_>("name"),
        "adapter_type": r.get::<String,_>("adapter_type"),
        "base_url": r.get::<String,_>("base_url"),
        "default_model": r.get::<String,_>("default_model"),
        "status": r.get::<String,_>("status"),
        "secret_configured": r.get::<i64,_>("secret_configured") != 0,
        "data_mode": r.get::<String,_>("data_mode"),
        "timeout_ms": r.get::<i64,_>("timeout_ms"),
        "max_input_tokens": r.get::<i64,_>("max_input_tokens"),
        "max_output_tokens": r.get::<i64,_>("max_output_tokens"),
        "max_concurrency": r.get::<i64,_>("max_concurrency"),
        "version": r.get::<i64,_>("version"),
        "created_at": r.get::<i64,_>("created_at"),
        "updated_at": r.get::<i64,_>("updated_at"),
    })
}

fn ai_provider_config_row_mysql(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "name": r.get::<String,_>("name"),
        "adapter_type": r.get::<String,_>("adapter_type"),
        "base_url": r.get::<String,_>("base_url"),
        "default_model": r.get::<String,_>("default_model"),
        "status": r.get::<String,_>("status"),
        "secret_configured": r.get::<i64,_>("secret_configured") != 0,
        "data_mode": r.get::<String,_>("data_mode"),
        "timeout_ms": r.get::<i64,_>("timeout_ms"),
        "max_input_tokens": r.get::<i64,_>("max_input_tokens"),
        "max_output_tokens": r.get::<i64,_>("max_output_tokens"),
        "max_concurrency": r.get::<i64,_>("max_concurrency"),
        "version": r.get::<i64,_>("version"),
        "created_at": r.get::<i64,_>("created_at"),
        "updated_at": r.get::<i64,_>("updated_at"),
    })
}

/// PATCH /api/v1/admin/ai/config — 新建/更新 Provider（admin.manage + reason + 审计）。
async fn update_ai_config(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "patch_admin_ai_config";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("name required", request_id, None))?
        .trim()
        .to_string();
    if name.is_empty() || name.len() > 120 {
        return Err(AppError::bad_request("invalid name", request_id, None));
    }
    let base_url = body
        .get("base_url")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("base_url required", request_id, None))?
        .to_string();
    let default_model = body
        .get("default_model")
        .and_then(Value::as_str)
        .unwrap_or("gpt-4o-mini")
        .to_string();
    let adapter_type = body
        .get("adapter_type")
        .and_then(Value::as_str)
        .unwrap_or("openai_compatible")
        .to_string();
    let data_mode = body
        .get("data_mode")
        .and_then(Value::as_str)
        .unwrap_or("redacted")
        .to_string();
    let status = body
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("disabled")
        .to_string();
    let now = crate::ai::consent::now();
    let provider_id = uuid::Uuid::now_v7().to_string();
    let affected = match pool {
        sqlx::Either::Left(p) => {
            sqlx::query(
                "INSERT INTO ai_providers
                     (id, name, adapter_type, base_url, api_type, default_model, status, data_mode, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'chat', ?, ?, ?, ?, ?)
                 ON CONFLICT(name) DO UPDATE SET base_url = excluded.base_url, default_model = excluded.default_model,
                     status = excluded.status, data_mode = excluded.data_mode, updated_at = excluded.updated_at",
            )
            .bind(&provider_id)
            .bind(&name)
            .bind(&adapter_type)
            .bind(&base_url)
            .bind(&default_model)
            .bind(&status)
            .bind(&data_mode)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected()
        }
        sqlx::Either::Right(p) => {
            sqlx::query(
                "INSERT INTO ai_providers
                     (id, name, adapter_type, base_url, api_type, default_model, status, data_mode, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'chat', ?, ?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE base_url = VALUES(base_url), default_model = VALUES(default_model),
                     status = VALUES(status), data_mode = VALUES(data_mode), updated_at = VALUES(updated_at)",
            )
            .bind(&provider_id)
            .bind(&name)
            .bind(&adapter_type)
            .bind(&base_url)
            .bind(&default_model)
            .bind(&status)
            .bind(&data_mode)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected()
        }
    };
    AuditEntry::user_action(&user.id, "ai.config.update")
        .with_target("ai_provider", &name)
        .with_reason(&reason)
        .with_policy_version(crate::authz::decision::AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(Json(
        json!({ "provider": name, "upserted": affected, "status": status }),
    ))
}

/// POST /api/v1/admin/ai/providers/test — 固定脱敏探针（不接收用户正文）。
async fn test_ai_provider(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_ai_providers_test";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let url = body
        .get("base_url")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("base_url required", request_id, None))?;
    let policy = crate::ai::gateway::EgressPolicy::default();
    match policy.validate_endpoint(url) {
        Ok(_) => Ok(Json(
            json!({ "ok": true, "backend": "ai_provider", "detail": "egress policy ok" }),
        )),
        Err(e) => Ok(Json(
            json!({ "ok": false, "backend": "ai_provider", "error_class": e.code() }),
        )),
    }
}

/// GET /api/v1/admin/ai/tasks — 管理端任务列表（安全投影，不扩大可见性）。
async fn list_ai_tasks(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_ai_tasks";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let items: Vec<Value> = match pool {
        sqlx::Either::Left(p) => sqlx::query(
            "SELECT id, task_type, target_type, target_id, user_id, status, attempt, error_class, created_at
             FROM ai_tasks ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(ai_task_row)
        .collect(),
        sqlx::Either::Right(p) => sqlx::query(
            "SELECT id, task_type, target_type, target_id, user_id, status, attempt, error_class, created_at
             FROM ai_tasks ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(ai_task_row_mysql)
        .collect(),
    };
    Ok(Json(json!({ "items": items })))
}

fn ai_task_row(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "task_type": r.get::<String,_>("task_type"),
        "target_type": r.get::<String,_>("target_type"),
        "target_id": r.get::<String,_>("target_id"),
        "user_id": r.get::<String,_>("user_id"),
        "status": r.get::<String,_>("status"),
        "attempt": r.get::<i64,_>("attempt"),
        "error_class": r.get::<Option<String>,_>("error_class"),
        "created_at": r.get::<i64,_>("created_at"),
    })
}

fn ai_task_row_mysql(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "task_type": r.get::<String,_>("task_type"),
        "target_type": r.get::<String,_>("target_type"),
        "target_id": r.get::<String,_>("target_id"),
        "user_id": r.get::<String,_>("user_id"),
        "status": r.get::<String,_>("status"),
        "attempt": r.get::<i64,_>("attempt"),
        "error_class": r.get::<Option<String>,_>("error_class"),
        "created_at": r.get::<i64,_>("created_at"),
    })
}

/// POST /api/v1/admin/ai/tasks/{id}/cancel — 管理端取消任意任务。
async fn cancel_ai_task_admin(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_ai_tasks_id_cancel";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let now = crate::ai::tasks::now();
    let affected = match pool {
        sqlx::Either::Left(p) => sqlx::query(
            "UPDATE ai_tasks SET status = 'cancelled', finished_at = ?, updated_at = ?
                 WHERE id = ? AND status IN ('queued','running','retry_wait')",
        )
        .bind(now)
        .bind(now)
        .bind(&id)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
        sqlx::Either::Right(p) => sqlx::query(
            "UPDATE ai_tasks SET status = 'cancelled', finished_at = ?, updated_at = ?
                 WHERE id = ? AND status IN ('queued','running','retry_wait')",
        )
        .bind(now)
        .bind(now)
        .bind(&id)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
    };
    Ok(Json(json!({ "id": id, "cancelled": affected })))
}

/// POST /api/v1/admin/ai/tasks/{id}/retry — 重置 retry_wait/dead 到 queued。
async fn retry_ai_task(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_ai_tasks_id_retry";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let now = crate::ai::tasks::now();
    let affected = match pool {
        sqlx::Either::Left(p) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'queued', attempt = 0, error_class = NULL, error_message_safe = NULL, finished_at = NULL, updated_at = ?
                 WHERE id = ? AND status IN ('retry_wait','dead')",
            )
            .bind(now)
            .bind(&id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected()
        }
        sqlx::Either::Right(p) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'queued', attempt = 0, error_class = NULL, error_message_safe = NULL, finished_at = NULL, updated_at = ?
                 WHERE id = ? AND status IN ('retry_wait','dead')",
            )
            .bind(now)
            .bind(&id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected()
        }
    };
    Ok(Json(json!({ "id": id, "retried": affected })))
}
async fn list_video_policies(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_video_policies")
}
async fn test_video_policy(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_admin_video_policies_test")
}
async fn get_video_policy(
    State(_state): State<AppState>,
    Path(_provider): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_video_policies_provider_")
}
async fn update_video_policy(
    State(_state): State<AppState>,
    Path(_provider): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("patch_admin_video_policies_provider_")
}
async fn list_oauth_clients(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listAdminOAuthClients")
}
async fn create_oauth_client(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("createAdminOAuthClient")
}
async fn get_oauth_client(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getAdminOAuthClient")
}
async fn update_oauth_client(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("updateAdminOAuthClient")
}
async fn list_marketplace_clients(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_marketplace_clients")
}
async fn update_marketplace_client(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("patch_admin_marketplace_clients_id_")
}
async fn rotate_webhook_secret(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_admin_marketplace_clients_id_rotate_webhook_secret")
}
async fn list_marketplace_transactions(
    State(_state): State<AppState>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_marketplace_transactions")
}
async fn list_themes(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_themes")
}
async fn upload_theme_package(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_admin_themes_data_packages")
}
async fn set_default_theme(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("put_admin_themes_default")
}
async fn delete_theme(
    State(_state): State<AppState>,
    Path(_name): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("delete_admin_themes_name_")
}
async fn update_theme_settings(
    State(_state): State<AppState>,
    Path(_name): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("patch_admin_themes_name_settings")
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
