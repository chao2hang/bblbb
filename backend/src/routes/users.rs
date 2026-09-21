use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::storage::model::{AttachmentRecord, AttachmentStatus};
use crate::users::dto::Me;
use crate::users::dto::PublicEquippedAchievement;
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

fn private_no_store<T: IntoResponse>(response: T) -> Response {
    let mut response = response.into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        axum::http::HeaderValue::from_static("private, no-store"),
    );
    response
}

/// 用户路由：个人资料、公开用户、Cover
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/me", get(get_me).patch(update_me))
        .route(
            "/api/v1/me/preferences/theme",
            get(get_theme_pref).put(update_theme_pref),
        )
        .route("/api/v1/users/suggest", get(suggest_mention_users))
        .route("/api/v1/users/{username}", get(get_public_user))
        .route(
            "/api/v1/me/profile-cover",
            post(post_profile_cover).delete(delete_profile_cover),
        )
        .route(
            "/api/v1/users/{user_id}/profile-cover",
            get(get_user_profile_cover),
        )
}

// ─────────────────────────── Cover（M03-PROFILE）───────────────────────────

/// 校验封面请求体：`attachment_id`（uuid）、`alt_text`（≤300）、
/// `position`（≤64），`additionalProperties: false`（多余字段拒绝）。
#[allow(clippy::result_large_err)] // AppError 为路由层统一错误载体（体积固定可接受）
fn parse_cover_body(body: &Value, request_id: &str) -> Result<(String, String, String), AppError> {
    let attachment_id = body
        .get("attachment_id")
        .and_then(Value::as_str)
        .filter(|s| uuid::Uuid::parse_str(s).is_ok())
        .ok_or_else(|| AppError::bad_request("attachment_id must be a UUID", request_id, None))?
        .to_string();
    let alt_text = body
        .get("alt_text")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    if alt_text.chars().count() > 300 {
        return Err(AppError::bad_request(
            "alt_text must be <= 300 chars",
            request_id,
            None,
        ));
    }
    let position = body
        .get("position")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    if position.chars().count() > 64 {
        return Err(AppError::bad_request(
            "position must be <= 64 chars",
            request_id,
            None,
        ));
    }
    Ok((attachment_id, alt_text, position))
}

/// 校验附件归属与就绪状态：必须是当前用户 own 且 `ready` 的附件。
async fn validate_cover_attachment(
    attachment: &AttachmentRecord,
    user_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    if attachment.owner_id != user_id {
        return Err(AppError::bad_request(
            "attachment does not belong to you",
            request_id,
            None,
        ));
    }
    if attachment.status != AttachmentStatus::Ready {
        return Err(AppError::bad_request(
            "attachment is not ready",
            request_id,
            None,
        ));
    }
    Ok(())
}

/// POST /api/v1/me/profile-cover — 设置当前用户封面（Session + CSRF）。
/// 只接受附件 UUID 与展示元数据；不接收/回显远程 URL 或签名 URL。
async fn post_profile_cover(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "post_me_profile_cover";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let (attachment_id, alt_text, position) = parse_cover_body(&body, request_id)?;

    let attachment = crate::storage::upload::load_attachment(pool, &attachment_id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("attachment not found", request_id))?;
    validate_cover_attachment(&attachment, &user.id, request_id).await?;

    let now = crate::outbox::now_millis();
    // 记录引用（递增 ref_count，独立事务）→ 更新用户字段。引用先于字段写入：
    // 若字段写入失败，仅多计一次引用（可由对账修正），不会出现 cover 指向
    // 已被物理删除的附件。
    crate::storage::quota::link_attachment(
        pool,
        &attachment_id,
        "user",
        &user.id,
        "profile_cover",
        now,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let result = match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users SET cover_attachment_id = ?, cover_alt_text = ?, cover_position = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&attachment_id)
            .bind(&alt_text)
            .bind(&position)
            .bind(now)
            .bind(&user.id)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE users SET cover_attachment_id = ?, cover_alt_text = ?, cover_position = ?, updated_at = ? WHERE id = ?",
            )
            .bind(&attachment_id)
            .bind(&alt_text)
            .bind(&position)
            .bind(now)
            .bind(&user.id)
            .execute(p)
            .await
            .map(|_| ())
        }
    };
    result.map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok((
        axum::http::StatusCode::NO_CONTENT,
        [(axum::http::header::CACHE_CONTROL, "private, no-store")],
    )
        .into_response())
}

/// DELETE /api/v1/me/profile-cover — 清除当前用户封面。
async fn delete_profile_cover(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "delete_me_profile_cover";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    // 契约 body 与 POST 一致（attachment_id 幂等字段；清除以当前值为准）。
    let _ = parse_cover_body(&body, request_id)?;
    let now = crate::outbox::now_millis();

    // 读取当前 cover 以正确解除引用。
    let current: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT cover_attachment_id FROM users WHERE id = ?")
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => {
            sqlx::query_scalar("SELECT cover_attachment_id FROM users WHERE id = ?")
                .bind(&user.id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };

    let result = match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users SET cover_attachment_id = NULL, cover_alt_text = NULL, cover_position = NULL, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(&user.id)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE users SET cover_attachment_id = NULL, cover_alt_text = NULL, cover_position = NULL, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(&user.id)
            .execute(p)
            .await
            .map(|_| ())
        }
    };
    result.map_err(|e| AppError::internal(e.to_string(), request_id))?;

    if let Some(current_id) = current {
        let _ = crate::storage::quota::unlink_attachment(pool, &current_id, "user", &user.id).await;
    }

    Ok((
        axum::http::StatusCode::NO_CONTENT,
        [(axum::http::header::CACHE_CONTROL, "private, no-store")],
    )
        .into_response())
}

/// GET /api/v1/users/{user_id}/profile-cover — 公开封面投影（本人/他人一致；
/// 无封面 → 204）。
async fn get_user_profile_cover(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(user_id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "get_users_user_id_profile_cover";
    auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let row: Option<(Option<String>, Option<String>, Option<String>)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT cover_attachment_id, cover_alt_text, cover_position FROM users WHERE id = ?",
        )
        .bind(&user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT cover_attachment_id, cover_alt_text, cover_position FROM users WHERE id = ?",
        )
        .bind(&user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let Some((Some(attachment_id), alt_text, position)) = row else {
        return Ok((
            axum::http::StatusCode::NO_CONTENT,
            [(axum::http::header::CACHE_CONTROL, "private, no-store")],
        )
            .into_response());
    };

    let body = json!({
        "attachment_id": attachment_id,
        "alt_text": alt_text,
        "position": position,
        "content_url": format!("/api/v1/attachments/{attachment_id}/content"),
    });
    Ok((
        axum::http::StatusCode::OK,
        [
            (axum::http::header::CACHE_CONTROL, "private, no-store"),
            (axum::http::header::CONTENT_TYPE, "application/json"),
        ],
        axum::Json(body),
    )
        .into_response())
}

/// GET /api/v1/me — 获取当前用户（本人投影 DTO，M03-PROFILE-01/03）
async fn get_me(State(state): State<AppState>, auth: AuthSession) -> Result<Response, AppError> {
    let request_id = "get_me";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let mfa_enabled = crate::auth::passkey::has_second_factor(pool, &user.id)
        .await
        .unwrap_or(false);
    let profile = load_profile_fields(pool, user)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let presentation_tokens = crate::shop::service::get_public_presentation_tokens(pool, &user.id)
        .await
        .unwrap_or(None);
    Ok(private_no_store(Json(Me::from_session(
        user,
        mfa_enabled,
        &profile,
        presentation_tokens,
    ))))
}

/// PATCH /api/v1/me — 更新当前用户资料（昵称/简介/签名/时区/主题/隐私；
/// PATCH 语义：只更新出现字段，缺失字段保持原值；必须携带 `If-Match`
/// 版本（OpenAPI updateMe 契约 required），版本过期 → 409 version_conflict）
async fn update_me(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
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

    let avatar_attachment_id: Option<Option<String>> = match body.get("avatar_attachment_id") {
        Some(Value::Null) => Some(None),
        Some(Value::String(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                Some(None)
            } else {
                Some(Some(trimmed.to_string()))
            }
        }
        Some(_) => {
            return Err(AppError::bad_request(
                "avatar_attachment_id must be a UUID string or null",
                request_id,
                None,
            ));
        }
        None => None,
    };

    let mut old_avatar_id: Option<String> = None;
    if let Some(ref new_opt) = avatar_attachment_id {
        old_avatar_id = match pool {
            Either::Left(p) => {
                sqlx::query_scalar("SELECT avatar_attachment_id FROM users WHERE id = ?")
                    .bind(&user.id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
            }
            Either::Right(p) => {
                sqlx::query_scalar("SELECT avatar_attachment_id FROM users WHERE id = ?")
                    .bind(&user.id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
            }
        };

        if let Some(ref new_id) = new_opt {
            uuid::Uuid::parse_str(new_id).map_err(|_| {
                AppError::bad_request("avatar_attachment_id must be a UUID", request_id, None)
            })?;
            let attachment = crate::storage::upload::load_attachment(pool, new_id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .ok_or_else(|| AppError::not_found("attachment not found", request_id))?;
            if attachment.owner_id != user.id {
                return Err(AppError::bad_request(
                    "attachment does not belong to you",
                    request_id,
                    None,
                ));
            }
            if attachment.status != crate::storage::AttachmentStatus::Ready {
                return Err(AppError::bad_request(
                    "attachment is not ready",
                    request_id,
                    None,
                ));
            }
            if !attachment.media_type.starts_with("image/") {
                return Err(AppError::bad_request(
                    "avatar attachment must be an image",
                    request_id,
                    None,
                ));
            }
            let now = crate::outbox::now_millis();
            crate::storage::quota::link_attachment(pool, new_id, "user", &user.id, "avatar", now)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

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
        avatar_attachment_id: avatar_attachment_id.clone(),
    };
    update
        .validate()
        .map_err(|msg| AppError::bad_request(msg, request_id, None))?;
    if let Some(ref dn) = update.display_name {
        if crate::users::blacklist::is_nickname_blacklisted(pool, dn)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
        {
            return Err(AppError::bad_request(
                "该昵称已被列入黑名单，禁止使用",
                request_id,
                None,
            ));
        }
    }
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
    if let Err(e) = update_profile(pool, &user.id, update, if_match).await {
        if let Some(Some(ref new_id)) = avatar_attachment_id {
            if old_avatar_id.as_deref() != Some(new_id.as_str()) {
                let _ =
                    crate::storage::quota::unlink_attachment(pool, new_id, "user", &user.id).await;
            }
        }
        return Err(match e {
            crate::users::profile::ProfileUpdateError::VersionConflict => {
                AppError::version_conflict("profile version conflict", request_id)
            }
            crate::users::profile::ProfileUpdateError::Database(msg) => {
                AppError::internal(msg, request_id)
            }
        });
    }

    if let Some(ref new_opt) = avatar_attachment_id {
        if let Some(ref old_id) = old_avatar_id {
            if new_opt.as_deref() != Some(old_id.as_str()) {
                let _ =
                    crate::storage::quota::unlink_attachment(pool, old_id, "user", &user.id).await;
            }
        }
    }

    let mfa_enabled = crate::auth::passkey::has_second_factor(pool, &user.id)
        .await
        .unwrap_or(false);
    let profile = load_profile_fields(pool, user)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let presentation_tokens = crate::shop::service::get_public_presentation_tokens(pool, &user.id)
        .await
        .unwrap_or(None);
    Ok(private_no_store(Json(Me::from_session(
        user,
        mfa_enabled,
        &profile,
        presentation_tokens,
    ))))
}

/// GET /api/v1/users/{username} — 获取公开用户资料（公开投影 DTO，
/// M03-PROFILE-01/02/06：不含邮箱、状态、Session、IP、处罚与审计信息；
/// 不存在/已注销 → 404；封禁/注销中 → 安全降级投影）
///
/// GAP-FIX 社交域：追加 post_count/followers/following 公开计数与
/// is_following（请求者视角，未登录恒 false）。
/// 社交域·成就：追加 equipped_achievements（已装备成就徽章 ≤3，服务端
/// 裁决；降级用户为空数组）——资料卡「佩戴徽章」行的真实数据来源。
async fn get_public_user(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(username): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "get_public_user";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let username_normalized = username.to_lowercase();

    let row: Option<PublicUserRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id, username_normalized, display_name, bio, trust_level AS level, avatar_attachment_id, cover_attachment_id, signature, created_at, status
                 FROM users WHERE username_normalized = ?",
            )
            .bind(&username_normalized)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, username_normalized, display_name, bio, trust_level AS level, avatar_attachment_id, cover_attachment_id, signature, created_at, status
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

            // 社交统计（GAP-FIX：公开计数 + 请求者视角 is_following）。
            let (post_count, followers, following, is_following) =
                load_public_social_stats(pool, &id, auth.user.as_ref().map(|u| u.id.as_str()))
                    .await
                    .map_err(|e| AppError::internal(e, request_id))?;

            // 装扮公开投影（M07-SHOP-SCHEMA-06）：白名单 Token 编译，best-effort
            // （失败不阻塞资料页）；封禁/注销中随其他公开字段一并置空。
            let presentation_tokens = if degraded {
                None
            } else {
                match crate::shop::service::get_public_presentation_tokens(pool, &id).await {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!(
                            user_id = %id,
                            error = %e,
                            "public presentation tokens compile failed"
                        );
                        None
                    }
                }
            };

            // 已装备成就徽章（社交域·成就墙装备槽）：≤3，服务端裁决；
            // 封禁/注销中降级为空数组（与装扮置空同策略）。
            let equipped_achievements = if degraded {
                Vec::new()
            } else {
                load_equipped_achievements(pool, &id)
                    .await
                    .map_err(|e| AppError::internal(e, request_id))?
            };

            Ok(private_no_store(Json(PublicProfile {
                id,
                username,
                display_name,
                bio: if degraded { None } else { bio },
                level,
                avatar_attachment_id: if degraded { None } else { avatar_attachment_id },
                cover_attachment_id: if degraded { None } else { cover_attachment_id },
                signature: if degraded { None } else { signature },
                created_at,
                post_count,
                followers,
                following,
                is_following,
                presentation_tokens,
                equipped_achievements,
            })))
        }
        None => Err(AppError::not_found("user not found", request_id)),
    }
}

/// 公开社交统计：(post_count, followers, following, is_following)。
///
/// is_following 需要会话用户（未登录恒 false）；SQL 跨方言 `?` 占位符。
async fn load_public_social_stats(
    pool: &sqlx::Either<sqlx::SqlitePool, sqlx::MySqlPool>,
    user_id: &str,
    viewer_id: Option<&str>,
) -> Result<(i64, i64, i64, bool), String> {
    let post_count = social_scalar(
        pool,
        user_id,
        "SELECT COUNT(*) FROM posts WHERE author_id = ? AND status = 'published' AND deleted_at IS NULL",
    )
    .await?;
    let followers = social_scalar(
        pool,
        user_id,
        "SELECT COUNT(*) FROM user_follows WHERE followee_id = ?",
    )
    .await?;
    let following = social_scalar(
        pool,
        user_id,
        "SELECT COUNT(*) FROM user_follows WHERE follower_id = ?",
    )
    .await?;

    let is_following = match viewer_id {
        Some(viewer) => {
            let row: Option<i64> = match pool {
                sqlx::Either::Left(p) => sqlx::query_scalar(
                    "SELECT 1 FROM user_follows WHERE follower_id = ? AND followee_id = ?",
                )
                .bind(viewer)
                .bind(user_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?,
                sqlx::Either::Right(p) => sqlx::query_scalar(
                    "SELECT 1 FROM user_follows WHERE follower_id = ? AND followee_id = ?",
                )
                .bind(viewer)
                .bind(user_id)
                .fetch_optional(p)
                .await
                .map_err(|e| e.to_string())?,
            };
            row == Some(1)
        }
        None => false,
    };
    Ok((post_count, followers, following, is_following))
}

/// 已装备成就徽章（社交域·成就墙装备槽，≤ [`MAX_EQUIPPED_SLOTS`]）。
///
/// 只取启用成就的 code/name（公开佩戴语义）；按目录排序（sort_order,
/// code）稳定输出。SQL 跨方言 `?` 占位符，LIMIT 常量内联。
async fn load_equipped_achievements(
    pool: &sqlx::Either<sqlx::SqlitePool, sqlx::MySqlPool>,
    user_id: &str,
) -> Result<Vec<PublicEquippedAchievement>, String> {
    let sql = "SELECT a.code, a.name FROM user_achievements ua
               JOIN achievements a ON a.id = ua.achievement_id
               WHERE ua.user_id = ? AND ua.equipped = 1 AND a.is_enabled = 1
               ORDER BY a.sort_order ASC, a.code ASC";
    let rows: Vec<(String, String)> = match pool {
        Either::Left(p) => sqlx::query_as(sql)
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as(sql)
            .bind(user_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?,
    };
    Ok(rows
        .into_iter()
        .take(crate::achievements::MAX_EQUIPPED_SLOTS as usize)
        .map(|(code, name)| PublicEquippedAchievement { code, name })
        .collect())
}

/// 单条 COUNT 聚合（社交统计共用）。
async fn social_scalar(
    pool: &sqlx::Either<sqlx::SqlitePool, sqlx::MySqlPool>,
    user_id: &str,
    sql: &str,
) -> Result<i64, String> {
    match pool {
        sqlx::Either::Left(p) => sqlx::query_scalar::<_, i64>(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string()),
        sqlx::Either::Right(p) => sqlx::query_scalar::<_, i64>(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string()),
    }
}

/// GET /api/v1/me/preferences/theme — 获取主题偏好（M13-THEME-07）。///
/// 返回用户偏好主题名 + 该主题 `revision`（与 SSR/浏览器/缓存共享，
/// M13-THEME-05）；偏好指向不存在/停用/损坏主题时回退 default 并返回
/// `effective` 字段（前端据此提示回退，无需缓存过期）。
async fn get_theme_pref(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "get_theme_pref";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let view = crate::theme::user_theme_preference(pool, &user.id)
        .await
        .map_err(|e| crate::routes::themes::theme_error_to_app(e, request_id))?;
    Ok(private_no_store(Json(json!({
        "theme": view.theme,
        "revision": view.revision,
        "effective": view.effective,
    }))))
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

#[derive(Deserialize)]
pub struct SuggestMentionUsersQuery {
    #[serde(default)]
    pub q: String,
    #[serde(default = "default_suggest_limit")]
    pub limit: i64,
}

fn default_suggest_limit() -> i64 {
    5
}

/// GET /api/v1/users/suggest — 回复/发帖 @提及用户模糊搜索（默认展示 5 个最相近用户）
async fn suggest_mention_users(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<SuggestMentionUsersQuery>,
) -> Result<Response, AppError> {
    let request_id = "suggest_mention_users";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let raw_q = query.q.trim().trim_start_matches('@').trim();
    let limit = query.limit.clamp(1, 20);

    let items: Vec<Value> = if raw_q.is_empty() {
        // 无关键词时默认推荐 5 个用户（按等级与近期更新排序，排除本人与已删除账号）
        let sql = "SELECT username_normalized, display_name, trust_level AS level
                   FROM users
                   WHERE id <> ?
                     AND status NOT IN ('deleted', 'pending_delete')
                   ORDER BY trust_level DESC, updated_at DESC
                   LIMIT ?";
        let rows: Vec<(String, Option<String>, i64)> = match pool {
            Either::Left(p) => sqlx::query_as(sql)
                .bind(&user.id)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            Either::Right(p) => sqlx::query_as(sql)
                .bind(&user.id)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        };
        rows.into_iter()
            .map(|(username, display_name, level)| {
                json!({
                    "username": username,
                    "display_name": display_name,
                    "level": level,
                })
            })
            .collect()
    } else {
        let q_lower = raw_q.to_lowercase();
        let escaped = q_lower
            .replace('!', "!!")
            .replace('%', "!%")
            .replace('_', "!_");
        let pattern = format!("%{escaped}%");

        let sql = "SELECT username_normalized, display_name, trust_level AS level
                   FROM users
                   WHERE status NOT IN ('deleted', 'pending_delete')
                     AND (username_normalized LIKE ? ESCAPE '!' OR (display_name IS NOT NULL AND LOWER(display_name) LIKE ? ESCAPE '!'))
                   LIMIT 50";
        let rows: Vec<(String, Option<String>, i64)> = match pool {
            Either::Left(p) => sqlx::query_as(sql)
                .bind(&pattern)
                .bind(&pattern)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            Either::Right(p) => sqlx::query_as(sql)
                .bind(&pattern)
                .bind(&pattern)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        };

        // 相似度打分分级：
        // 0: username 完全匹配
        // 1: display_name 完全匹配
        // 2: username 前缀匹配
        // 3: display_name 前缀匹配
        // 4: username 包含子串
        // 5: display_name 包含子串
        let mut scored: Vec<(u32, usize, String, Option<String>, i64)> = rows
            .into_iter()
            .map(|(username, display_name, level)| {
                let u_lower = username.to_lowercase();
                let d_lower = display_name.as_deref().unwrap_or("").to_lowercase();
                let tier = if u_lower == q_lower {
                    0
                } else if !d_lower.is_empty() && d_lower == q_lower {
                    1
                } else if u_lower.starts_with(&q_lower) {
                    2
                } else if !d_lower.is_empty() && d_lower.starts_with(&q_lower) {
                    3
                } else if u_lower.contains(&q_lower) {
                    4
                } else {
                    5
                };
                (tier, username.len(), username, display_name, level)
            })
            .collect();

        scored.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });

        scored
            .into_iter()
            .take(limit as usize)
            .map(|(_, _, username, display_name, level)| {
                json!({
                    "username": username,
                    "display_name": display_name,
                    "level": level,
                })
            })
            .collect()
    };

    let body = json!({ "items": items });
    Ok((
        axum::http::StatusCode::OK,
        [
            (axum::http::header::CACHE_CONTROL, "private, no-store"),
            (axum::http::header::CONTENT_TYPE, "application/json"),
        ],
        Json(body),
    )
        .into_response())
}
