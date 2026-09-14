use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::Value;

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::error::AppError;
use crate::shop::service::shop_error_to_app;

/// 商城与权益路由（M07-SHOP）。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/shop/products", get(list_shop_products))
        .route("/api/v1/shop/products/{id}", get(get_shop_product))
        .route("/api/v1/shop/orders", post(create_shop_order))
        .route("/api/v1/shop/orders/{id}", get(get_shop_order))
        .route("/api/v1/me/entitlements", get(get_me_entitlements))
        .route(
            "/api/v1/me/entitlements/{id}/equip",
            post(equip_entitlement),
        )
        .route(
            "/api/v1/me/entitlements/{id}/unequip",
            post(unequip_entitlement),
        )
        .route("/api/v1/me/presentation", get(get_me_presentation))
}

#[derive(Deserialize)]
struct CreateOrderBody {
    product_id: String,
    #[serde(default = "default_quantity")]
    quantity: i64,
    idempotency_key: Option<String>,
    client_request_id: Option<String>,
    expected_product_version: Option<i64>,
}

fn default_quantity() -> i64 {
    1
}

#[derive(Deserialize, Default)]
struct PresentationVersionBody {
    expected_presentation_version: Option<i64>,
}

async fn list_shop_products(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_shop_products";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let user = auth.require_auth(request_id)?;
    let decision = authorize_action(pool, &user.id, "shop.read", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("shop read not allowed", request_id));
    }
    crate::shop::service::list_products(pool, false)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn get_shop_product(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_shop_product";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let _user = auth.require_auth(request_id)?;
    crate::shop::service::get_product(pool, &id)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn create_shop_order(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    axum::Json(body): axum::Json<CreateOrderBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "create_shop_order";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(pool, &user.id, "shop.purchase", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("shop purchase not allowed", request_id));
    }
    if let Some(expected_version) = body.expected_product_version {
        let product = crate::shop::service::get_product(pool, &body.product_id)
            .await
            .map_err(|e| shop_error_to_app(e, request_id))?;
        if product.get("version").and_then(Value::as_i64) != Some(expected_version) {
            return Err(AppError::conflict(
                "product version mismatch (reload and retry)",
                request_id,
            ));
        }
    }
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned)
        .or_else(|| body.idempotency_key.clone())
        .or_else(|| body.client_request_id.clone())
        .unwrap_or_else(|| uuid::Uuid::now_v7().simple().to_string());
    crate::shop::service::buy_product(pool, &user.id, &body.product_id, body.quantity, &key)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn get_shop_order(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_shop_order";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::get_order(pool, &user.id, &id, false)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn get_me_entitlements(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_me_entitlements";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::list_my_entitlements(pool, &user.id)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}

async fn equip_entitlement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Option<axum::Json<PresentationVersionBody>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "equip_entitlement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(
        pool,
        &user.id,
        "shop.entitlement.manage_own",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("not allowed", request_id));
    }
    crate::shop::service::equip_with_version(
        pool,
        &user.id,
        &id,
        body.and_then(|b| b.0.expected_presentation_version),
    )
    .await
    .map(Json)
    .map_err(|e| shop_error_to_app(e, request_id))
}

async fn unequip_entitlement(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Option<axum::Json<PresentationVersionBody>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "unequip_entitlement";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(
        pool,
        &user.id,
        "shop.entitlement.manage_own",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("not allowed", request_id));
    }
    crate::shop::service::unequip_with_version(
        pool,
        &user.id,
        &id,
        body.and_then(|b| b.0.expected_presentation_version),
    )
    .await
    .map(Json)
    .map_err(|e| shop_error_to_app(e, request_id))
}

async fn get_me_presentation(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_me_presentation";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::shop::service::get_presentation(pool, &user.id)
        .await
        .map(Json)
        .map_err(|e| shop_error_to_app(e, request_id))
}
