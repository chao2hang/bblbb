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

/// 管理后台路由
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
        // 存储配额
        .route(
            "/api/v1/admin/storage/config",
            get(get_storage_config).patch(update_storage_config),
        )
        .route("/api/v1/admin/storage/test", post(test_storage))
        .route(
            "/api/v1/admin/levels/{id}/attachment-quota",
            get(get_attachment_quota).patch(update_attachment_quota),
        )
        // 下载计费
        .route(
            "/api/v1/admin/attachments/{id}/download-policy",
            get(get_admin_download_policy).patch(update_admin_download_policy),
        )
        .route(
            "/api/v1/admin/download-billing/config",
            get(get_billing_config).patch(update_billing_config),
        )
        // 活跃任务
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
        // 商城
        .route(
            "/api/v1/admin/shop/config",
            get(get_shop_config).patch(update_shop_config),
        )
        .route(
            "/api/v1/admin/shop/products",
            get(list_admin_products).post(create_admin_product),
        )
        .route(
            "/api/v1/admin/shop/products/{id}",
            patch(update_admin_product),
        )
        .route(
            "/api/v1/admin/shop/products/{id}/disable",
            post(disable_product),
        )
        .route(
            "/api/v1/admin/shop/products/{id}/publish",
            post(publish_product),
        )
        .route("/api/v1/admin/shop/orders", get(list_admin_orders))
        .route("/api/v1/admin/shop/orders/{id}/refund", post(refund_order))
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
async fn list_admin_tags(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listAdminTags")
}
async fn create_admin_tag(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("createAdminTag")
}
async fn get_admin_tag(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getAdminTag")
}
async fn update_admin_tag(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("updateAdminTag")
}
async fn get_storage_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_storage_config")
}
async fn update_storage_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("patch_admin_storage_config")
}
async fn test_storage(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_admin_storage_test")
}
async fn get_attachment_quota(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_admin_levels_id_attachment_quota")
}
async fn update_attachment_quota(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("patch_admin_levels_id_attachment_quota")
}
async fn get_admin_download_policy(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("getAttachmentDownloadPolicyAdmin")
}
async fn update_admin_download_policy(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("updateAttachmentDownloadPolicyAdmin")
}
async fn get_billing_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("getDownloadBillingConfig")
}
async fn update_billing_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("updateDownloadBillingConfig")
}
async fn get_activity_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("getAdminActivityConfig")
}
async fn update_activity_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("updateAdminActivityConfig")
}
async fn list_activity_tasks(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listAdminActivityTasks")
}
async fn create_activity_task(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("createAdminActivityTask")
}
async fn update_activity_task(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("updateAdminActivityTask")
}
async fn get_shop_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("getAdminShopConfig")
}
async fn update_shop_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("updateAdminShopConfig")
}
async fn list_admin_products(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listAdminShopProducts")
}
async fn create_admin_product(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("createAdminShopProduct")
}
async fn update_admin_product(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("updateAdminShopProduct")
}
async fn disable_product(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("disableAdminShopProduct")
}
async fn publish_product(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("publishAdminShopProduct")
}
async fn list_admin_orders(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("listAdminShopOrders")
}
async fn refund_order(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("refundAdminShopOrder")
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
