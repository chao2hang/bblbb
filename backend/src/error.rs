use axum::{
    body::Body,
    http::{header, HeaderName, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
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
    /// 限流场景：`Retry-After` 秒数（其余场景为 None）。
    retry_after_secs: Option<u64>,
    /// 限流场景：(limit, remaining, reset_unix_secs) → `RateLimit-*` 头。
    rate_limit: Option<(u32, u32, i64)>,
}

/// 集中清理错误详情中的敏感信息（供领域层复用，如 theme/plugins 的告警与
/// Problem detail）。独立于 `AppError::sanitize_detail`，纯字符串变换。
pub fn sanitize(detail: &str) -> String {
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

impl AppError {
    pub fn internal(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            code: "internal_error",
            title: "Internal Server Error",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
            retry_after_secs: None,
            rate_limit: None,
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
            retry_after_secs: None,
            rate_limit: None,
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
            retry_after_secs: None,
            rate_limit: None,
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
            retry_after_secs: None,
            rate_limit: None,
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
            retry_after_secs: None,
            rate_limit: None,
        }
    }

    /// 高风险操作要求近期重认证（M02-MFA-07 step-up）：403 `step_up_required`。
    /// 前端据 `code` 判定需展示 re-auth 表单（M02-UX-06）。
    pub fn step_up_required(request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            code: "step_up_required",
            title: "Forbidden",
            detail: "recent authentication required".to_string(),
            request_id: request_id.into(),
            errors: None,
            retry_after_secs: None,
            rate_limit: None,
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
            retry_after_secs: None,
            rate_limit: None,
        }
    }

    /// 乐观并发冲突（`If-Match`/version 过期），错误码 `version_conflict`
    /// （ERROR-CODES.md；OpenAPI `Conflict` 409 响应）。
    pub fn version_conflict(detail: impl Into<String>, request_id: impl Into<String>) -> Self {
        Self {
            status: StatusCode::CONFLICT,
            code: "version_conflict",
            title: "Conflict",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
            retry_after_secs: None,
            rate_limit: None,
        }
    }

    /// 可见等级超过作者当前等级（M04-VISIBILITY-03/04；ERROR-CODES.md：
    /// 422 `visibility_level_exceeds_author`）。创建/编辑/草稿发布/定时发布/
    /// 管理员代发均必须服务端重读作者等级并稳定返回此错误。
    pub fn visibility_level_exceeds_author(
        detail: impl Into<String>,
        request_id: impl Into<String>,
    ) -> Self {
        Self {
            status: StatusCode::UNPROCESSABLE_ENTITY,
            code: "visibility_level_exceeds_author",
            title: "Unprocessable Entity",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
            retry_after_secs: None,
            rate_limit: None,
        }
    }

    /// 限流拒绝（OpenAPI `RateLimited` 响应，错误码 `rate_limited`）。
    ///
    /// 响应携带 `Retry-After` 与 `RateLimit-Limit/Remaining/Reset` 头
    /// （docs/API.md §17）。`reset_at_unix_secs` 为窗口重置的 Unix 秒。
    pub fn rate_limited(
        detail: impl Into<String>,
        request_id: impl Into<String>,
        retry_after_secs: u64,
        limit: u32,
        remaining: u32,
        reset_at_unix_secs: i64,
    ) -> Self {
        Self {
            status: StatusCode::TOO_MANY_REQUESTS,
            code: "rate_limited",
            title: "Too Many Requests",
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
            retry_after_secs: Some(retry_after_secs.max(1)),
            rate_limit: Some((limit, remaining, reset_at_unix_secs)),
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
            retry_after_secs: None,
            rate_limit: None,
        }
    }

    /// 反爬等场景的专有稳定错误码（M08-CRAWL）。
    ///
    /// 不新增 OpenAPI 操作，只扩展 Problem `code`/`title`：
    /// - `crawler_denied`（403）：AI 训练爬虫默认拒绝；
    /// - `challenge_required`（403）：需要一次性挑战 token；
    /// - `temporarily_banned`（403）：临时封禁（不泄漏检测规则）。
    pub fn with_code(
        status: StatusCode,
        code: &'static str,
        title: &'static str,
        detail: impl Into<String>,
        request_id: impl Into<String>,
    ) -> Self {
        Self {
            status,
            code,
            title,
            detail: detail.into(),
            request_id: request_id.into(),
            errors: None,
            retry_after_secs: None,
            rate_limit: None,
        }
    }

    /// 清理错误详情中的敏感信息
    ///
    /// 集中清除：SQL 语句、栈/回溯、密码、Token、Secret、API Key、
    /// 签名 URL（AWS/Google）与私钥块。
    fn sanitize_detail(&self) -> String {
        sanitize(&self.detail)
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

        let body = Body::from(serde_json::to_vec(&problem).unwrap_or_default());
        let mut response = Response::new(body);
        *response.status_mut() = self.status;
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("application/problem+json"),
        );
        if let Some(secs) = self.retry_after_secs {
            response
                .headers_mut()
                .insert(header::RETRY_AFTER, secs.to_string().parse().unwrap());
        }
        if let Some((limit, remaining, reset)) = self.rate_limit {
            // 头名使用小写（HTTP/2 强制；HTTP/1 读取大小写不敏感）
            response.headers_mut().insert(
                HeaderName::from_static("ratelimit-limit"),
                limit.to_string().parse().unwrap(),
            );
            response.headers_mut().insert(
                HeaderName::from_static("ratelimit-remaining"),
                remaining.to_string().parse().unwrap(),
            );
            response.headers_mut().insert(
                HeaderName::from_static("ratelimit-reset"),
                reset.to_string().parse().unwrap(),
            );
        }
        response
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
