use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;

/// 附件与下载路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/attachments", post(create_attachment))
        .route(
            "/api/v1/attachments/{id}",
            get(get_attachment).delete(delete_attachment),
        )
        .route(
            "/api/v1/attachments/{id}/complete",
            post(complete_attachment),
        )
        .route(
            "/api/v1/attachments/{id}/content",
            get(get_attachment_content),
        )
        .route(
            "/api/v1/attachments/{id}/download-policy",
            get(get_download_policy),
        )
        .route(
            "/api/v1/attachments/{id}/download",
            post(download_attachment),
        )
        .route(
            "/api/v1/download-authorizations/{id}",
            get(get_download_authorization),
        )
        .route(
            "/api/v1/download-authorizations/{id}/sign-url",
            post(sign_download_url),
        )
}

async fn create_attachment(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("createAttachment")
}

async fn get_attachment(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_attachments_id_")
}

async fn delete_attachment(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("delete_attachments_id_")
}

async fn complete_attachment(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_attachments_id_complete")
}

async fn get_attachment_content(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_attachments_id_content")
}

async fn get_download_policy(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_attachments_id_download_policy")
}

async fn download_attachment(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_attachments_id_download")
}

async fn get_download_authorization(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_download_authorizations_id_")
}

async fn sign_download_url(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_download_authorizations_id_sign_url")
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
