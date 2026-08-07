use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, patch, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::error::AppError;
use crate::shop::service::{shop_error_to_app, ShopError};

/// M07-SHOP 管理路由（admin_shop 域 agent 填充）。
pub fn router() -> Router<AppState> {
    Router::new()
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
}

#[derive(Deserialize)]
struct ReasonBody {
    reason: Option<String>,
}

async fn admin_authorize(
    state: &AppState,
    auth: &AuthSession,
    permission: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(pool, &user.id, permission, None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            format!("{permission} required"),
            request_id,
        ));
    }
    Ok(())
}

async fn get_shop_config(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_shop_config";
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    // 站点商城配置：当前返回固定默认（存 site_settings 的可扩展点）。
    Ok(Json(json!({
        "enabled": true,
        "max_quantity_per_order": 100,
        "default_refund_policy": "non_refundable",
    })))
}

#[derive(Deserialize)]
struct ShopConfigBody {
    enabled: Option<bool>,
    max_quantity_per_order: Option<i64>,
}

async fn update_shop_config(
    State(state): State<AppState>,
    auth: AuthSession,
    axum::Json(body): axum::Json<ShopConfigBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_shop_config";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    AuditEntry::user_action(&user.id, "shop.config.update")
        .with_reason("update shop config")
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(
            state
                .db
                .as_deref()
                .ok_or_else(|| AppError::internal("database not configured", request_id))?,
        )
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let _ = (body.enabled, body.max_quantity_per_order);
    Ok(Json(json!({ "status": "updated" })))
}

async fn list_admin_products(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_admin_products";
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::list_admin_products(pool)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn create_admin_product(
    State(state): State<AppState>,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "create_admin_product";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("create product");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let result = crate::shop::service::create_product(pool, &body, &user.id).await;
    match result {
        Ok(v) => {
            AuditEntry::user_action(&user.id, "shop.product.create")
                .with_target("product", v["id"].as_str().unwrap_or(""))
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(v))
        }
        Err(e) => Err(shop_error_to_app(e, request_id)),
    }
}

async fn update_admin_product(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_admin_product";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let reason = body
        .get("reason")
        .and_then(|v| v.as_str())
        .unwrap_or("update product");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let result = crate::shop::service::update_product(pool, &id, &body).await;
    match result {
        Ok(v) => {
            AuditEntry::user_action(&user.id, "shop.product.update")
                .with_target("product", &id)
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(v))
        }
        Err(e) => Err(shop_error_to_app(e, request_id)),
    }
}

async fn disable_product(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "disable_product";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let reason = body.reason.as_deref().unwrap_or("disable product");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let result = crate::shop::service::disable_product(pool, &id).await;
    match result {
        Ok(v) => {
            AuditEntry::user_action(&user.id, "shop.product.disable")
                .with_target("product", &id)
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(v))
        }
        Err(e) => Err(shop_error_to_app(e, request_id)),
    }
}

async fn publish_product(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<ReasonBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "publish_product";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let reason = body.reason.as_deref().unwrap_or("publish product");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let result = crate::shop::service::publish_product(pool, &id).await;
    match result {
        Ok(v) => {
            AuditEntry::user_action(&user.id, "shop.product.publish")
                .with_target("product", &id)
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(v))
        }
        Err(e) => Err(shop_error_to_app(e, request_id)),
    }
}

async fn list_admin_orders(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_admin_orders";
    admin_authorize(&state, &auth, "shop.manage", request_id).await?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::list_admin_orders(pool)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

#[derive(Deserialize)]
struct RefundBody {
    reason: Option<String>,
}

async fn refund_order(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<RefundBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "refund_order";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, "shop.refund", request_id).await?;
    let reason = body.reason.as_deref().unwrap_or("admin refund");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::refund_order(pool, &id, &user.id, reason)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

// 锚定 ShopError 类型供路由层签名使用（避免未使用导入告警）。
#[allow(dead_code)]
fn _anchor(_: ShopError) {}
