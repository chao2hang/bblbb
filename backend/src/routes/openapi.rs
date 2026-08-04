use axum::{
    http::{header, StatusCode},
    response::IntoResponse,
};
use tokio::fs;

use crate::{app::AppState, error::AppError, middleware::request_id::RequestId};

pub async fn openapi(
    axum::extract::State(state): axum::extract::State<AppState>,
    axum::extract::Extension(request_id): axum::extract::Extension<RequestId>,
) -> Result<impl IntoResponse, AppError> {
    let document = fs::read_to_string(&state.config.openapi_path)
        .await
        .map_err(|error| {
            tracing::error!(path = %state.config.openapi_path.display(), %error, "failed to read OpenAPI document");
            AppError::internal("OpenAPI document is unavailable", request_id.0.clone())
        })?;
    let document: serde_json::Value = serde_yaml::from_str(&document).map_err(|error| {
        tracing::error!(path = %state.config.openapi_path.display(), %error, "failed to parse OpenAPI document");
        AppError::internal("OpenAPI document is invalid", request_id.0.clone())
    })?;

    Ok((
        StatusCode::OK,
        [(header::CONTENT_TYPE, "application/json; charset=utf-8")],
        axum::Json(document),
    ))
}
