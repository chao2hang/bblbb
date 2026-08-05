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

    /// 功能或 Provider 当前关闭（M01-CONFIG-06；OpenAPI/ERROR-CODES：409 feature_disabled）
    pub fn feature_disabled(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "feature_disabled",
            title: "Feature Disabled",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
        }
    }

    /// 清理错误详情中的敏感信息
    ///
    /// 集中清除：SQL 语句、栈/回溯、密码、Token、Secret、API Key、
    /// 签名 URL（AWS/Google）与私钥块。
    fn sanitize_detail(&self) -> String {
        let detail = &self.detail;
        let patterns = [
            // 认证/授权凭据
            ("password=", "[redacted]"),
            ("password :", "[redacted]"),
            ("\"password\":", "[redacted]"),
            ("token=", "[redacted]"),
            ("\"token\":", "[redacted]"),
            ("access_token=", "[redacted]"),
            ("refresh_token=", "[redacted]"),
            ("secret=", "[redacted]"),
            ("\"secret\":", "[redacted]"),
            ("client_secret", "[redacted]"),
            ("api_key", "[redacted]"),
            ("apikey", "[redacted]"),
            ("Authorization: Bearer ", "[redacted] "),
            // 签名 URL 参数（AWS SigV4 / Google 签名 URL）
            ("X-Amz-Signature", "[signed-url]"),
            ("X-Amz-Credential", "[signed-url]"),
            ("X-Amz-Security-Token", "[signed-url]"),
            ("X-Goog-Signature", "[signed-url]"),
            ("X-Goog-Credential", "[signed-url]"),
            ("signature=", "[redacted]"),
            // 私钥块
            ("BEGIN RSA PRIVATE KEY", "[private-key]"),
            ("BEGIN EC PRIVATE KEY", "[private-key]"),
            ("BEGIN OPENSSH PRIVATE KEY", "[private-key]"),
            ("BEGIN PRIVATE KEY", "[private-key]"),
            // SQL 语句片段
            ("SELECT ", "[sql] "),
            ("INSERT ", "[sql] "),
            ("UPDATE ", "[sql] "),
            ("DELETE ", "[sql] "),
            ("WHERE ", "[sql] "),
            ("FROM ", "[sql] "),
            ("JOIN ", "[sql] "),
            ("GROUP BY ", "[sql] "),
            ("ORDER BY ", "[sql] "),
            // 栈/回溯特征
            ("\n    at ", " [stack]"),
            ("stack backtrace:", " [stack]"),
            ("backtrace:", " [stack]"),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn problem_serializes_with_required_fields() {
        let problem = Problem {
            type_uri: "about:blank",
            title: "Bad Request",
            status: 400,
            code: "bad_request",
            detail: "invalid input".to_string(),
            instance: Some("/api/v1/auth/register".to_string()),
            request_id: "req-1".to_string(),
            errors: Some(serde_json::json!({ "field": "username" })),
        };
        let value = serde_json::to_value(&problem).unwrap();
        for key in [
            "type",
            "title",
            "status",
            "code",
            "detail",
            "instance",
            "request_id",
            "errors",
        ] {
            assert!(value.get(key).is_some(), "missing field {key}");
        }
        assert_eq!(value["type"], "about:blank");
        assert_eq!(value["status"], 400);
        assert_eq!(value["instance"], "/api/v1/auth/register");
    }

    #[test]
    fn sanitize_detail_redacts_sensitive_patterns() {
        let error = AppError::internal(
            "sqlx error: SELECT password FROM users WHERE token=abc123; \
             stack backtrace:\n    at src/main.rs:42 \
             client_secret=supersecret X-Amz-Signature=deadbeef \
             BEGIN RSA PRIVATE KEY-----abcdef-----END RSA PRIVATE KEY",
            "req-1",
        );
        let detail = error.sanitize_detail();
        assert!(!detail.contains("password="), "password leaked: {detail}");
        assert!(!detail.contains("token="), "token leaked: {detail}");
        assert!(!detail.contains("SELECT "), "sql leaked: {detail}");
        assert!(!detail.contains("FROM "), "sql leaked: {detail}");
        assert!(!detail.contains("client_secret"), "secret leaked: {detail}");
        assert!(
            !detail.contains("X-Amz-Signature"),
            "signed url leaked: {detail}"
        );
        assert!(
            !detail.contains("BEGIN RSA PRIVATE KEY"),
            "private key leaked: {detail}"
        );
        assert!(
            !detail.contains("stack backtrace:"),
            "stack leaked: {detail}"
        );
    }

    #[test]
    fn sanitize_detail_keeps_benign_detail() {
        let error = AppError::internal("user not found for identifier", "req-1");
        let detail = error.sanitize_detail();
        assert_eq!(detail, "user not found for identifier");
    }

    #[test]
    fn app_error_into_response_has_problem_shape() {
        use axum::response::IntoResponse;
        let response = AppError::bad_request("bad username", "req-9", None).into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/problem+json"
        );
    }
}
