//! Problem 响应补齐中间件（M00-BACKEND-05）
//!
//! 处理器在构造 `AppError` 时无法访问请求上下文，因此这里在响应路径上
//! 统一补齐 RFC 9457 Problem Details 中依赖请求上下文的字段：
//!
//! - `instance`：缺失或为空时填入请求路径
//! - `request_id`：读取响应头 `x-request-id`（由内层 `request_id` 中间件
//!   写入的权威值）并回填 Problem 体，保证响应头与响应体一致
//!
//! 该中间件是**最外层**中间件：即使内层中间件（如 Host/Origin 校验）提前
//! 返回 Problem 响应，也能被补齐。仅处理
//! `Content-Type: application/problem+json` 的响应，其他响应原样放行。

use axum::{
    body::{to_bytes, Body},
    extract::Request,
    http::{header, HeaderValue},
    middleware::Next,
    response::Response,
};

const PROBLEM_CONTENT_TYPE: &str = "application/problem+json";
const MAX_PROBLEM_BODY: usize = 1024 * 1024;

/// 补齐 Problem 的 instance / request_id 字段
pub async fn problem_instance(request: Request, next: Next) -> Response {
    let instance = request.uri().path().to_string();

    let response = next.run(request).await;

    let is_problem = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .map(|value| value.starts_with(PROBLEM_CONTENT_TYPE))
        .unwrap_or(false);
    if !is_problem {
        return response;
    }

    // 响应头 x-request-id 由 request_id 中间件在响应路径写入，是权威值
    let request_id = response
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

    let (mut parts, body) = response.into_parts();
    let bytes = match to_bytes(body, MAX_PROBLEM_BODY).await {
        Ok(bytes) => bytes,
        Err(_) => {
            let fallback = serde_json::json!({
                "type": "about:blank",
                "title": "Error",
                "status": parts.status.as_u16(),
                "code": "error",
                "detail": "error response unavailable",
                "instance": instance,
                "request_id": request_id,
            });
            let bytes = serde_json::to_vec(&fallback).unwrap_or_default();
            parts.headers.insert(
                header::CONTENT_LENGTH,
                HeaderValue::from(bytes.len() as u64),
            );
            return Response::from_parts(parts, Body::from(bytes));
        }
    };

    let mut value: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => {
            // 非 JSON 的 problem 响应（理论不出现），原样放行
            return Response::from_parts(parts, Body::from(bytes));
        }
    };

    let missing_instance = value
        .get("instance")
        .and_then(|value| value.as_str())
        .map(|instance| instance.trim().is_empty())
        .unwrap_or(true);
    if missing_instance {
        value["instance"] = serde_json::Value::String(instance);
    }
    value["request_id"] = serde_json::Value::String(request_id);

    let bytes = serde_json::to_vec(&value).unwrap_or_default();
    parts.headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(bytes.len() as u64),
    );
    Response::from_parts(parts, Body::from(bytes))
}
