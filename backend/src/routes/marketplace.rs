use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;

/// Marketplace 路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/marketplace/offers", post(create_offer))
        .route(
            "/api/v1/marketplace/offers/{id}",
            get(get_offer).patch(update_offer),
        )
        .route(
            "/api/v1/marketplace/checkout-intents",
            post(create_checkout_intent),
        )
        .route(
            "/api/v1/marketplace/checkout-intents/{id}/confirm",
            post(confirm_checkout_intent),
        )
        .route("/api/v1/marketplace/purchases", get(list_purchases))
        .route("/api/v1/marketplace/purchases/{id}", get(get_purchase))
        .route(
            "/api/v1/marketplace/purchases/{id}/refund",
            post(refund_purchase),
        )
}

async fn create_offer(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_marketplace_offers")
}

async fn get_offer(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_marketplace_offers_id_")
}

async fn update_offer(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("patch_marketplace_offers_id_")
}

async fn create_checkout_intent(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_marketplace_checkout_intents")
}

async fn confirm_checkout_intent(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_marketplace_checkout_intents_id_confirm")
}

async fn list_purchases(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_marketplace_purchases")
}

async fn get_purchase(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_marketplace_purchases_id_")
}

async fn refund_purchase(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_marketplace_purchases_id_refund")
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
