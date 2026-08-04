use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;

/// 商城与活跃路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/shop/products", get(list_shop_products))
        .route("/api/v1/shop/products/{id}", get(get_shop_product))
        .route("/api/v1/shop/orders", post(create_shop_order))
        .route("/api/v1/shop/orders/{id}", get(get_shop_order))
        .route("/api/v1/activity/summary", get(get_activity_summary))
        .route("/api/v1/activity/visit", post(record_visit))
}

async fn list_shop_products(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_shop_products")
}

async fn get_shop_product(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_shop_products_id_")
}

async fn create_shop_order(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_shop_orders")
}

async fn get_shop_order(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_shop_orders_id_")
}

async fn get_activity_summary(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_activity_summary")
}

async fn record_visit(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("recordAuthenticatedVisit")
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
