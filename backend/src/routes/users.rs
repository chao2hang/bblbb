use axum::{
    extract::{Path, State},
    response::Json,
    routing::get,
    Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{app::AppState, auth::session::AuthSession, error::AppError};

/// 用户路由：个人资料、公开用户
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me", get(get_me).patch(update_me))
        .route("/api/v1/me/preferences/theme", get(get_theme_pref))
        .route("/api/v1/users/{username}", get(get_public_user))
}

#[derive(Serialize)]
struct MeResponse {
    id: String,
    username: String,
    email: String,
    email_verified: bool,
    status: String,
    display_name: Option<String>,
    bio: Option<String>,
    timezone: String,
    level: i64,
    roles: Vec<String>,
}

/// GET /api/v1/me — 获取当前用户
async fn get_me(
    State(_state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<MeResponse>, AppError> {
    let user = auth.require_auth("get_me")?;
    Ok(Json(MeResponse {
        id: user.id.clone(),
        username: user.username.clone(),
        email: user.email.clone(),
        email_verified: user.email_verified,
        status: user.status.clone(),
        display_name: user.display_name.clone(),
        bio: None,
        timezone: "UTC".to_string(),
        level: user.level,
        roles: user.roles.clone(),
    }))
}

/// PATCH /api/v1/me — 更新当前用户资料
async fn update_me(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<Value>,
) -> Result<Json<MeResponse>, AppError> {
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

    Ok(Json(MeResponse {
        id: user.id.clone(),
        username: user.username.clone(),
        email: user.email.clone(),
        email_verified: user.email_verified,
        status: user.status.clone(),
        display_name: display_name.map(|s| s.to_string()),
        bio: bio.map(|s| s.to_string()),
        timezone: "UTC".to_string(),
        level: user.level,
        roles: user.roles.clone(),
    }))
}

/// GET /api/v1/users/{username} — 获取公开用户信息
async fn get_public_user(
    State(state): State<AppState>,
    Path(username): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_public_user";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let username_normalized = username.to_lowercase();

    let row: Option<(String, String, Option<String>)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT username_normalized, status, display_name FROM users WHERE username_normalized = ? AND status != 'deleted'")
                .bind(&username_normalized)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT username_normalized, status, display_name FROM users WHERE username_normalized = ? AND status != 'deleted'")
                .bind(&username_normalized)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match row {
        Some((username, status, display_name)) => Ok(Json(json!({
            "username": username,
            "display_name": display_name,
            "status": if status == "active" { "active" } else { "restricted" },
        }))),
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
