use axum::{extract::State, response::IntoResponse, routing::get, Json, Router};

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::error::AppError;
use crate::theme::{resolve_active_theme, ThemeError};

/// 主题路由（用户偏好部分在 users.rs，管理部分在 admin.rs）
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/themes/active", get(get_active_theme))
}

/// GET /api/v1/themes/active — 当前生效主题（含 revision）。
///
/// - 登录用户：用户偏好主题（仅当主题 active+兼容，否则回退站点默认/内置）；
/// - 匿名：站点默认主题 → 内置 default 兜底。
/// - `revision` 与 SSR/浏览器/缓存/用户偏好共享（M13-THEME-05）：SSR 页面
///   以该 revision 生成 ETag；主题变更立即提升 revision。
/// - 个人化响应（随用户偏好变化）不进入共享缓存：`Cache-Control: private,
///   no-store`；token 只含封闭 schema 校验通过的已知值。
async fn get_active_theme(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<impl IntoResponse, AppError> {
    let request_id = "get_themes_active";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let user_id = auth.user.as_ref().map(|u| u.id.as_str());
    let active = resolve_active_theme(pool, user_id)
        .await
        .map_err(|e| theme_error_to_app(e, request_id))?;
    let mut response = Json(active.json()).into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

pub fn theme_error_to_app(e: ThemeError, request_id: &str) -> AppError {
    match e {
        ThemeError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        ThemeError::NotFound(msg) => AppError::not_found(msg, request_id),
        ThemeError::Conflict(msg) => AppError::version_conflict(msg, request_id),
        ThemeError::Incompatible(msg) => AppError::bad_request(msg, request_id, None),
        ThemeError::Corrupt(msg) => AppError::internal(msg, request_id),
    }
}
