use axum::{
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

/// RFC 9457 Problem Details 响应体
#[derive(Debug, Serialize)]
pub struct Problem {
    #[serde(rename = "type")]
    pub type_uri: &'static str,
    pub title: &'static str,
    pub status: u16,
    pub code: &'static str,
    pub detail: String,
    pub instance: Option<String>,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub errors: Option<serde_json::Value>,
}

/// 应用错误类型
#[derive(Debug)]
pub struct AppError {
    status: StatusCode,
    code: &'static str,
    title: &'static str,
    detail: String,
    request_id: String,
    errors: Option<serde_json::Value>,
}

impl AppError {
    pub fn internal(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            title: "Internal Server Error",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
        }
    }

    pub fn not_found(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            title: "Not Found",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
        }
    }

    pub fn bad_request(
        detail: impl Into<String>,
        request_id: impl Into<String>,
        errors: Option<serde_json::Value>,
    ) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            code: "bad_request",
            title: "Bad Request",
            detail: detail.into(),
            request_id: request_id.into(),
            errors,
        }
    }

    pub fn unauthorized(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            code: "unauthorized",
            title: "Unauthorized",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
        }
    }

    pub fn forbidden(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "forbidden",
            title: "Forbidden",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
        }
    }

    pub fn conflict(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "conflict",
            title: "Conflict",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
        }
    }

    pub fn too_many_requests(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "too_many_requests",
            title: "Too Many Requests",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
        }
    }

    /// 清理错误详情中的敏感信息
    fn sanitize_detail(&self) -> String {
        let detail = &self.detail;
        // 清除 SQL 语句、栈跟踪、Secret、Token、签名 URL
        let patterns = [
            ("password=", "[redacted]"),
            ("token=", "[redacted]"),
            ("secret=", "[redacted]"),
            ("SELECT ", "[sql] "),
            ("INSERT ", "[sql] "),
            ("UPDATE ", "[sql] "),
            ("DELETE ", "[sql] "),
        ];
        let mut result = detail.to_string();
        for (pattern, replacement) in &patterns {
            result = result.replace(pattern, replacement);
        }
        result
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let problem = Problem {
            type_uri: "about:blank",
            title: self.title,
            status: self.status.as_u16(),
            code: self.code,
            detail: self.sanitize_detail(),
            instance: None,
            request_id: self.request_id,
            errors: self.errors,
        };

        (
            self.status,
            [(header::CONTENT_TYPE, "application/problem+json")],
            Json(problem),
        )
            .into_response()
    }
}
