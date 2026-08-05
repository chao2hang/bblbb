use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;

/// AI 路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/ai/capabilities", get(get_ai_capabilities))
        .route(
            "/api/v1/ai/consent",
            post(post_ai_consent).delete(delete_ai_consent),
        )
        .route("/api/v1/ai/drafts/{draft_id}/format", post(format_draft))
        .route(
            "/api/v1/ai/posts/{post_id}/moderation-suggestion",
            post(moderation_suggestion),
        )
        .route(
            "/api/v1/ai/posts/{post_id}/seo-suggestion",
            post(seo_suggestion),
        )
        .route("/api/v1/ai/suggestions/{id}", get(get_suggestion))
        .route(
            "/api/v1/ai/suggestions/{id}/accept",
            post(accept_suggestion),
        )
        .route("/api/v1/ai/tasks/{id}", get(get_ai_task))
        .route("/api/v1/ai/tasks/{id}/cancel", post(cancel_ai_task))
}

async fn get_ai_capabilities(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_ai_capabilities")
}

async fn post_ai_consent(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_ai_consent")
}

async fn delete_ai_consent(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("delete_ai_consent")
}

async fn format_draft(
    State(_state): State<AppState>,
    Path(_draft_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_ai_drafts_draft_id_format")
}

async fn moderation_suggestion(
    State(_state): State<AppState>,
    Path(_post_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_ai_posts_post_id_moderation_suggestion")
}

async fn seo_suggestion(
    State(_state): State<AppState>,
    Path(_post_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_ai_posts_post_id_seo_suggestion")
}

async fn get_suggestion(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_ai_suggestions_id_")
}

async fn accept_suggestion(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_ai_suggestions_id_accept")
}

async fn get_ai_task(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_ai_tasks_id_")
}

async fn cancel_ai_task(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_ai_tasks_id_cancel")
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
