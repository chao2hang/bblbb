use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;

/// 视频嵌入路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/video-embeds", post(create_video_embed))
        .route("/api/v1/video-embeds/resolve", post(resolve_video_embed))
        .route(
            "/api/v1/video-embeds/{id}",
            get(get_video_embed)
                .patch(update_video_embed)
                .delete(delete_video_embed),
        )
        .route(
            "/api/v1/video-embeds/{id}/refresh",
            post(refresh_video_embed),
        )
}

async fn create_video_embed(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_video_embeds")
}

async fn resolve_video_embed(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_video_embeds_resolve")
}

async fn get_video_embed(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_video_embeds_id_")
}

async fn update_video_embed(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("patch_video_embeds_id_")
}

async fn delete_video_embed(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("delete_video_embeds_id_")
}

async fn refresh_video_embed(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_video_embeds_id_refresh")
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
