use axum::{
    extract::State,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde_json::json;

use crate::app::AppState;
use crate::error::AppError;

use super::admin_ext::load_site_settings;

/// 站点公开信息路由（全站文案统一，0065）。
///
/// `GET /api/v1/site` 是登录页/注册页等前台文案的公开只读投影：匿名可读、
/// 无需认证，仅暴露站点名称/描述与登录注册文案 + 维护模式标记；SMTP、
/// 开关等运营字段一律不进该投影（留在 GET /api/v1/admin/settings）。
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/site", get(get_public_site))
}

/// GET /api/v1/site — 站点公开信息（匿名可读）。
///
/// - 空文案字段表示「使用前端内置通用文案兜底」（前端渲染层兜底，
///   接口原样透传空串，不在此拼接默认值）；
/// - `maintenance_mode` 供前台渲染维护提示（公开布尔，无敏感信息）；
/// - 响应随站点设置变化且被 SSR 内部 fetch 传播进页面响应，固定
///   `private, no-store`（与 themes/active 同策略，禁止进入共享缓存）。
async fn get_public_site(State(state): State<AppState>) -> Result<Response, AppError> {
    let request_id = "get_public_site";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let row = load_site_settings(pool, request_id).await?;
    let google_login_enabled = row.google_auth_enabled != 0 && !row.google_client_id.trim().is_empty();
    let github_login_enabled = row.github_auth_enabled != 0 && !row.github_client_id.trim().is_empty();
    let mut response = Json(json!({
        "site_name": row.site_name,
        "site_description": row.site_description,
        "login_eyebrow": row.login_eyebrow,
        "login_title": row.login_title,
        "login_subtitle": row.login_subtitle,
        "register_eyebrow": row.register_eyebrow,
        "register_title": row.register_title,
        "register_subtitle": row.register_subtitle,
        "maintenance_mode": row.maintenance_mode != 0,
        "google_login_enabled": google_login_enabled,
        "github_login_enabled": github_login_enabled,
        "version": row.version,
    }))
    .into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}
