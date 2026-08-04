use axum::{
    extract::Request,
    http::{HeaderName, HeaderValue},
    middleware::Next,
    response::Response,
};
use uuid::Uuid;

pub const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

#[derive(Clone, Debug)]
pub struct RequestId(pub String);

pub async fn request_id(mut request: Request, next: Next) -> Response {
    let request_id = request
        .headers()
        .get(&X_REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .filter(|value| is_valid_request_id(value))
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::now_v7().to_string());

    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));

    let mut response = next.run(request).await;
    if let Ok(value) = HeaderValue::from_str(&request_id) {
        response.headers_mut().insert(X_REQUEST_ID, value);
    }
    response
}

fn is_valid_request_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.is_ascii()
        && !value.bytes().any(|byte| byte.is_ascii_control())
}
