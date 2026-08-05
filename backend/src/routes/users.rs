use axum::{
    extract::{Path, State},
    response::Json,
    routing::get,
    Router,
};
use serde_json::{json, Value};
use sqlx::Either;

use crate::users::dto::{Me, PublicProfile};
use crate::{app::AppState, auth::session::AuthSession, error::AppError};

/// 公开资料查询行：
/// (id, username_normalized, display_name, bio, level, avatar_attachment_id,
/// signature, created_at)。
type PublicUserRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
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

/// GET /api/v1/me — 获取当前用户（本人投影 DTO，M03-PROFILE-01）
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
    // 当前从会话投影构建；bio/timezone 持久化读取在 M03-PROFILE-03 落地。
    Ok(Json(Me::from_session(user, mfa_enabled, None, "UTC")))
}

/// PATCH /api/v1/me — 更新当前用户资料（昵称/简介）
async fn update_me(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<Value>,
) -> Result<Json<Me>, AppError> {
    let request_id = "update_me";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let now = chrono::Utc::now().timestamp();
    let display_name = body.get("display_name").and_then(|v| v.as_str());
    let bio = body.get("bio").and_then(|v| v.as_str());

    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET display_name = ?, bio = ?, updated_at = ? WHERE id = ?")
                .bind(display_name)
                .bind(bio)
                .bind(now)
                .bind(&user.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE users SET display_name = ?, bio = ?, updated_at = ? WHERE id = ?")
                .bind(display_name)
                .bind(bio)
                .bind(now)
                .bind(&user.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok(Json(Me::from_session(
        user,
        crate::auth::has_confirmed_totp(pool, &user.id)
            .await
            .unwrap_or(false),
        bio.map(|s| s.to_string()),
        "UTC",
    )))
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
                "SELECT id, username_normalized, display_name, bio, level, avatar_attachment_id, signature, created_at
                 FROM users WHERE username_normalized = ? AND status != 'deleted'",
            )
            .bind(&username_normalized)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, username_normalized, display_name, bio, level, avatar_attachment_id, signature, created_at
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
            signature,
            created_at,
        )) => Ok(Json(PublicProfile {
            id,
            username,
            display_name,
            bio,
            level,
            avatar_attachment_id,
            signature,
            created_at,
        })),
        None => Err(AppError::not_found("user not found", request_id)),
    }
}

/// GET /api/v1/me/preferences/theme — 获取主题偏好
async fn get_theme_pref(
    State(_state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let _user = auth.require_auth("get_theme_pref")?;
    Ok(Json(json!({ "theme": "default" })))
}

/// PUT /api/v1/me/preferences/theme — 更新主题偏好
///
/// 主题持久化属于 M2/UX 波次（user_preferences 表迁移）；当前实现与
/// GET 桩保持一致：校验合法值并回显，路由契约完整。
async fn update_theme_pref(
    _state: State<AppState>,
    auth: AuthSession,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_theme_pref";
    let _user = auth.require_auth(request_id)?;

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

    Ok(Json(json!({ "theme": theme })))
}
