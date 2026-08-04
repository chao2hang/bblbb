use axum::{extract::State, http::StatusCode, response::Json, routing::get, Router};
use serde_json::{json, Value};

use crate::app::AppState;

/// 主题路由（用户偏好部分在 users.rs，管理部分在 admin.rs）
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/themes/active", get(get_active_theme))
}

async fn get_active_theme(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    (
        StatusCode::OK,
        Json(json!({
            "name": "default",
            "revision": 1,
        })),
    )
}
