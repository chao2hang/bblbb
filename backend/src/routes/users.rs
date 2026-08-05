use axum::{
    extract::{Path, State},
    http::HeaderMap,
    response::Json,
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
/// cover_attachment_id, signature, created_at)。
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
/// M03-PROFILE-01/02：不含邮箱、状态、Session、IP、处罚与审计信息）
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
                "SELECT id, username_normalized, display_name, bio, level, avatar_attachment_id, cover_attachment_id, signature, created_at
                 FROM users WHERE username_normalized = ? AND status != 'deleted'",
            )
            .bind(&username_normalized)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, username_normalized, display_name, bio, level, avatar_attachment_id, cover_attachment_id, signature, created_at
                 FROM users WHERE username_normalized = ? AND status != 'deleted'",
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
        )) => Ok(Json(PublicProfile {
            id,
            username,
            display_name,
            bio,
            level,
            avatar_attachment_id,
            cover_attachment_id,
            signature,
            created_at,
        })),
        None => Err(AppError::not_found("user not found", request_id)),
    }
}

/// GET /api/v1/me/preferences/theme — 获取主题偏好（user_preferences.theme_name，
/// 缺失返回 default）
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
    let theme = match pool {
        Either::Left(p) => sqlx::query_scalar::<_, Option<String>>(
            "SELECT theme_name FROM user_preferences WHERE user_id = ?",
        )
        .bind(&user.id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .flatten(),
        Either::Right(p) => sqlx::query_scalar::<_, Option<String>>(
            "SELECT theme_name FROM user_preferences WHERE user_id = ?",
        )
        .bind(&user.id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .flatten(),
    };
    Ok(Json(
        json!({ "theme": theme.unwrap_or_else(|| "default".to_string()) }),
    ))
}

/// PUT /api/v1/me/preferences/theme — 更新主题偏好（持久化
/// user_preferences.theme_name，行首访惰性创建）
async fn update_theme_pref(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
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

    if !matches!(theme, "default" | "dark" | "light") {
        return Err(AppError::bad_request(
            "theme must be one of: default, dark, light",
            request_id,
            None,
        ));
    }

    let now = chrono::Utc::now().timestamp();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_preferences (user_id, timezone, locale, updated_at)
                 VALUES (?, 'UTC', 'zh-CN', ?)",
            )
            .bind(&user.id)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(
                "UPDATE user_preferences SET theme_name = ?, updated_at = ? WHERE user_id = ?",
            )
            .bind(theme)
            .bind(now)
            .bind(&user.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO user_preferences (user_id, timezone, locale, updated_at)
                 VALUES (?, 'UTC', 'zh-CN', ?)",
            )
            .bind(&user.id)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(
                "UPDATE user_preferences SET theme_name = ?, updated_at = ? WHERE user_id = ?",
            )
            .bind(theme)
            .bind(now)
            .bind(&user.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok(Json(json!({ "theme": theme })))
}
