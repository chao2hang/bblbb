use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Problem {
    #[serde(rename = "type")]
    pub type_uri: &'static str,
    pub title: &'static str,
    pub status: u16,
    pub code: &'static str,
    pub detail: String,
    pub request_id: String,
}

#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    detail: String,
    request_id: String,
}

impl AppError {
    pub fn internal(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            title: "Internal Server Error",
            detail: detail.into(),
            request_id: request_id.into(),
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let problem = Problem {
            type_uri: "about:blank",
            title: self.title,
            status: self.status.as_u16(),
            code: self.code,
            detail: self.detail,
            request_id: self.request_id,
        };

        (
            self.status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem),
        )
            .into_response()
    }
}
