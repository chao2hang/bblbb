use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{delete, get, patch, post, put},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::boards::admin::{create_board, update_board, BoardCreateInput, BoardUpdateInput};
use crate::error::AppError;
use crate::tags::admin::{create_tag, update_tag};

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
            get(get_ai_config).patch(update_ai_config),
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
async fn get_ai_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_ai_config")
}
async fn update_ai_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("patch_admin_ai_config")
}
async fn test_ai_provider(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_admin_ai_providers_test")
}
async fn list_ai_tasks(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_ai_tasks")
}
async fn cancel_ai_task_admin(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_admin_ai_tasks_id_cancel")
}
async fn retry_ai_task(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_admin_ai_tasks_id_retry")
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
