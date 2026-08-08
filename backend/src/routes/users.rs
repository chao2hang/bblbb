use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::{IntoResponse, Json},
    routing::get,
    Router,
};
use serde_json::{json, Value};
use sqlx::Either;

use crate::users::dto::Me;
use crate::users::dto::PublicProfile;
use crate::users::profile::{load_profile_fields, update_profile, ProfileUpdate};
use crate::{app::AppState, auth::session::AuthSession, error::AppError};

/// 公开资料查询行：
/// (id, username_normalized, display_name, bio, level, avatar_attachment_id,
/// cover_attachment_id, signature, created_at, status)。
type PublicUserRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
    String,
);

/// 用户路由：个人资料、公开用户
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me", get(get_me).patch(update_me))
        .route(
            "/api/v1/me/preferences/theme",
            get(get_theme_pref).put(update_theme_pref),
        )
        .route("/api/v1/users/{username}", get(get_public_user))
}

/// GET /api/v1/me — 获取当前用户（本人投影 DTO，M03-PROFILE-01/03）
async fn get_me(State(state): State<AppState>, auth: AuthSession) -> Result<Json<Me>, AppError> {
    let request_id = "get_me";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let mfa_enabled = crate::auth::has_confirmed_totp(pool, &user.id)
        .await
        .unwrap_or(false);
    let profile = load_profile_fields(pool, user)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    Ok(Json(Me::from_session(user, mfa_enabled, &profile)))
}

/// PATCH /api/v1/me — 更新当前用户资料（昵称/简介/签名/时区/主题/隐私；
/// PATCH 语义：只更新出现字段，缺失字段保持原值；必须携带 `If-Match`
/// 版本（OpenAPI updateMe 契约 required），版本过期 → 409 version_conflict）
async fn update_me(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Json<Me>, AppError> {
    let request_id = "update_me";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // If-Match 版本（乐观并发，M03-PROFILE-04）
    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?;
    let if_match = if_match.trim().parse::<i64>().map_err(|_| {
        AppError::bad_request(
            "If-Match must be the current version integer",
            request_id,
            None,
        )
    })?;

    let update = ProfileUpdate {
        display_name: body
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        bio: body
            .get("bio")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        signature: body
            .get("signature")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        timezone: body
            .get("timezone")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        theme_name: body
            .get("theme")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        email_visible_to: body
            .get("email_visible_to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
        profile_visible_to: body
            .get("profile_visible_to")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };
    update
        .validate()
        .map_err(|msg| AppError::bad_request(msg, request_id, None))?;
    // M13-THEME-07：PATCH /me 的 theme 也必须是 default 或已安装且激活的
    // 数据主题（服务端再次校验；未知/停用/损坏 → 400）。
    if let Some(theme_name) = update.theme_name.as_deref() {
        if theme_name != crate::theme::DEFAULT_THEME_NAME {
            crate::theme::load_theme_checked(pool, theme_name)
                .await
                .map_err(|_| {
                    AppError::bad_request("theme not installed or not active", request_id, None)
                })?;
        }
    }
    update_profile(pool, &user.id, update, if_match)
        .await
        .map_err(|e| match e {
            crate::users::profile::ProfileUpdateError::VersionConflict => {
                AppError::version_conflict("profile version conflict", request_id)
            }
            crate::users::profile::ProfileUpdateError::Database(msg) => {
                AppError::internal(msg, request_id)
            }
        })?;

    let mfa_enabled = crate::auth::has_confirmed_totp(pool, &user.id)
        .await
        .unwrap_or(false);
    let profile = load_profile_fields(pool, user)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    Ok(Json(Me::from_session(user, mfa_enabled, &profile)))
}

/// GET /api/v1/users/{username} — 获取公开用户资料（公开投影 DTO，
/// M03-PROFILE-01/02/06：不含邮箱、状态、Session、IP、处罚与审计信息；
/// 不存在/已注销 → 404；封禁/注销中 → 安全降级投影）
async fn get_public_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<PublicProfile>, AppError> {
    let request_id = "get_public_user";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let username_normalized = username.to_lowercase();

    let row: Option<PublicUserRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id, username_normalized, display_name, bio, level, avatar_attachment_id, cover_attachment_id, signature, created_at, status
                 FROM users WHERE username_normalized = ?",
            )
            .bind(&username_normalized)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, username_normalized, display_name, bio, level, avatar_attachment_id, cover_attachment_id, signature, created_at, status
                 FROM users WHERE username_normalized = ?",
            )
            .bind(&username_normalized)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match row {
        Some((
            id,
            username,
            display_name,
            bio,
            level,
            avatar_attachment_id,
            cover_attachment_id,
            signature,
            created_at,
            status,
        )) => {
            // 已注销/删除：不泄漏存在性的 404
            if matches!(status.as_str(), "deleted") {
                return Err(AppError::not_found("user not found", request_id));
            }
            // 封禁/注销中：安全降级投影（bio/签名/头像/Cover 置空，
            // 保留 id/username/display_name/level 与全键集；不泄漏状态）
            let degraded = matches!(status.as_str(), "banned" | "pending_delete");
            Ok(Json(PublicProfile {
                id,
                username,
                display_name,
                bio: if degraded { None } else { bio },
                level,
                avatar_attachment_id: if degraded { None } else { avatar_attachment_id },
                cover_attachment_id: if degraded { None } else { cover_attachment_id },
                signature: if degraded { None } else { signature },
                created_at,
            }))
        }
        None => Err(AppError::not_found("user not found", request_id)),
    }
}

/// GET /api/v1/me/preferences/theme — 获取主题偏好（M13-THEME-07）。///
/// 返回用户偏好主题名 + 该主题 `revision`（与 SSR/浏览器/缓存共享，
/// M13-THEME-05）；偏好指向不存在/停用/损坏主题时回退 default 并返回
/// `effective` 字段（前端据此提示回退，无需缓存过期）。
async fn get_theme_pref(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_theme_pref";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let view = crate::theme::user_theme_preference(pool, &user.id)
        .await
        .map_err(|e| crate::routes::themes::theme_error_to_app(e, request_id))?;
    Ok(Json(json!({
        "theme": view.theme,
        "revision": view.revision,
        "effective": view.effective,
    })))
}

/// PUT /api/v1/me/preferences/theme — 更新主题偏好（M13-THEME-07）。
///
/// 安全约束：
/// - `If-Match` 必须等于当前生效主题 revision（乐观锁；冲突 → 409
///   `version_conflict`，前端刷新后再保存）；
/// - 只允许 default 或已安装且 active 的数据主题名（服务端再次校验；
///   不在列表内的名称 400）；
/// - 响应带 `Cache-Control: private, no-store`（个人化，不进共享缓存）。
async fn update_theme_pref(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let request_id = "update_theme_pref";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let theme = body
        .get("theme")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::bad_request("theme is required", request_id, None))?;
    if !crate::theme::validate_theme_name(theme) {
        return Err(AppError::bad_request(
            "invalid theme name (lowercase ascii/digits/hyphens, <=64)",
            request_id,
            None,
        ));
    }
    let expected_revision: i64 = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse()
        .map_err(|_| AppError::bad_request("If-Match must be an integer", request_id, None))?;

    let view = crate::theme::update_user_theme_preference(pool, &user.id, theme, expected_revision)
        .await
        .map_err(|e| crate::routes::themes::theme_error_to_app(e, request_id))?;

    let mut response = axum::Json(json!({
        "theme": view.theme,
        "revision": view.revision,
        "effective": view.effective,
    }))
    .into_response();
    response.headers_mut().insert(
        axum::http::header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}
