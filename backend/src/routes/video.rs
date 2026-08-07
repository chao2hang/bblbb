//! 视频嵌入路由（M10-VIDEO 用户侧）。
//!
//! OpenAPI 契约（冻结）：`post_video_embeds_resolve`、`post_video_embeds`、
//! `get_video_embeds_id_`、`patch_video_embeds_id_`、
//! `delete_video_embeds_id_`、`post_video_embeds_id_refresh`。
//!
//! 路由前缀 `/api/v1/video-embeds/` 由 Feature Gate（FeatureName::Video，
//! 默认关闭）门控。resolve 不信任客户端：签名 URL、Key、iframe HTML 在分类
//! 阶段拒绝且永不回显。刷新/解析结果写入 `Cache-Control: no-store`（解析
//! 响应含一次性 resolution_id）。

use axum::{
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::error::AppError;
use crate::outbox::now_millis;
use crate::video::{
    create_embed, delete_embed, get_embed, refresh_embed, resolve_source, update_embed,
    valid_target_type, EmbedView, VideoError, VideoTarget,
};

/// 视频嵌入路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/video-embeds", post(create_video_embed))
        .route("/api/v1/video-embeds/resolve", post(resolve_video_embed))
        .route(
            "/api/v1/video-embeds/{id}",
            get(get_video_embed)
                .patch(update_video_embed)
                .delete(delete_video_embed),
        )
        .route(
            "/api/v1/video-embeds/{id}/refresh",
            post(refresh_video_embed),
        )
}

/// POST /api/v1/video-embeds/resolve — 解析 URL，返回类型与安全元数据。
async fn resolve_video_embed(
    State(state): State<AppState>,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_video_embeds_resolve";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let source_url = body
        .get("source_url")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("source_url required", request_id, None))?;
    if source_url.trim().is_empty() || source_url.len() > 2048 {
        return Err(AppError::bad_request(
            "invalid source_url",
            request_id,
            None,
        ));
    }
    let target_type = body
        .get("target_type")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("target_type required", request_id, None))?;
    if !valid_target_type(target_type) {
        return Err(AppError::bad_request(
            "target_type must be post or comment",
            request_id,
            None,
        ));
    }
    let target_id = body
        .get("target_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    let target_id = match target_id {
        Some(id) if uuid::Uuid::parse_str(&id).is_ok() => id,
        Some(_) => {
            return Err(AppError::bad_request("invalid target_id", request_id, None));
        }
        None => String::new(),
    };
    let target = VideoTarget {
        target_type: target_type.to_string(),
        target_id,
    };

    let view = resolve_source(pool, &user.id, source_url, &target, now_millis())
        .await
        .map_err(|e| video_error_to_app(e, request_id))?;
    Ok(Json(json!({
        "resolution_id": view.resolution_id,
        "provider": view.provider,
        "media_type": view.media_type,
        "official_url": view.official_url,
        "source_host": view.source_host,
        "title": view.title,
        "policy_version": view.policy_version,
        "policy_enabled": view.policy_enabled,
        "embeddable": view.embeddable,
        "expires_at": view.expires_at,
        "target": { "type": view.target.target_type, "id": view.target.target_id },
    })))
}

/// POST /api/v1/video-embeds — 创建结构化视频引用（消费 resolution_id）。
async fn create_video_embed(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "post_video_embeds";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let resolution_id = body
        .get("resolution_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("resolution_id required", request_id, None))?;
    if uuid::Uuid::parse_str(resolution_id).is_err() {
        return Err(AppError::bad_request(
            "invalid resolution_id",
            request_id,
            None,
        ));
    }
    let target_type = body
        .get("target_type")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("target_type required", request_id, None))?;
    if !valid_target_type(target_type) {
        return Err(AppError::bad_request(
            "target_type must be post or comment",
            request_id,
            None,
        ));
    }
    let target_id = body
        .get("target_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("target_id required", request_id, None))?;
    if uuid::Uuid::parse_str(target_id).is_err() {
        return Err(AppError::bad_request("invalid target_id", request_id, None));
    }
    let expected_policy_version = body
        .get("expected_policy_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            AppError::bad_request("expected_policy_version required", request_id, None)
        })?;
    if expected_policy_version < 1 {
        return Err(AppError::bad_request(
            "expected_policy_version must be >= 1",
            request_id,
            None,
        ));
    }
    let target = VideoTarget {
        target_type: target_type.to_string(),
        target_id: target_id.to_string(),
    };
    let now = now_millis();

    // 可选幂等（M01-AUDIT-04）：相同 key+摘要返回原引用；不同摘要 → 409。
    if let Some(key) = headers.get("idempotency-key").and_then(|v| v.to_str().ok()) {
        if !(16..=200).contains(&key.len()) {
            return Err(AppError::bad_request(
                "Idempotency-Key must be 16..=200 chars",
                request_id,
                None,
            ));
        }
        let idem = crate::idempotency::IdempotencyKey::new("video.create", key)
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
        let hash = crate::idempotency::request_hash(&serde_json::to_vec(&body).unwrap_or_default());
        let outcome = crate::idempotency::begin_or_replay(
            pool,
            &idem,
            &hash,
            24 * 60 * 60 * 1000,
            crate::idempotency::FailureCachePolicy::Cache,
        )
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        return match outcome {
            crate::idempotency::IdempotencyOutcome::Created { record_id } => {
                let view = create_embed(
                    pool,
                    &user.id,
                    resolution_id,
                    &target,
                    expected_policy_version,
                    now,
                )
                .await
                .map_err(|e| video_error_to_app(e, request_id))?;
                let _ = crate::idempotency::complete(pool, &record_id, &view.id).await;
                Ok((StatusCode::CREATED, Json(embed_json(&view))))
            }
            crate::idempotency::IdempotencyOutcome::Replay { response_reference } => {
                if let Some(embed_id) = response_reference {
                    if let Ok(view) = get_embed(pool, &user.id, &embed_id, now).await {
                        return Ok((StatusCode::CREATED, Json(embed_json(&view))));
                    }
                }
                Err(AppError::conflict(
                    "idempotent replay but original embed not found",
                    request_id,
                ))
            }
            crate::idempotency::IdempotencyOutcome::InProgress => Err(AppError::conflict(
                "request already in progress",
                request_id,
            )),
            crate::idempotency::IdempotencyOutcome::Conflict => Err(AppError::conflict(
                "idempotency key reused with different request",
                request_id,
            )),
            crate::idempotency::IdempotencyOutcome::Failed { .. } => Err(AppError::conflict(
                "previous attempt failed; retry with a new idempotency key",
                request_id,
            )),
        };
    }

    let view = create_embed(
        pool,
        &user.id,
        resolution_id,
        &target,
        expected_policy_version,
        now,
    )
    .await
    .map_err(|e| video_error_to_app(e, request_id))?;
    Ok((StatusCode::CREATED, Json(embed_json(&view))))
}

/// GET /api/v1/video-embeds/{id} — 当前请求方可见投影。
async fn get_video_embed(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_video_embeds_id_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let view = get_embed(pool, &user.id, &id, now_millis())
        .await
        .map_err(|e| video_error_to_app(e, request_id))?;
    Ok(Json(embed_json(&view)))
}

/// PATCH /api/v1/video-embeds/{id} — 修改用户可编辑的标题/展示字段。
async fn update_video_embed(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "patch_video_embeds_id_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| AppError::bad_request("If-Match must be an integer", request_id, None))?;

    let title_override = body
        .get("title_override")
        .map(|v| v.as_str().map(str::to_string));
    if let Some(Some(title)) = &title_override {
        if title.chars().count() > 240 {
            return Err(AppError::bad_request(
                "title_override must be <= 240 chars",
                request_id,
                None,
            ));
        }
    }
    let poster_override = body
        .get("poster_override_attachment_id")
        .map(|v| v.as_str().map(str::to_string));
    if let Some(Some(poster)) = &poster_override {
        if uuid::Uuid::parse_str(poster).is_err() {
            return Err(AppError::bad_request(
                "invalid poster_override_attachment_id",
                request_id,
                None,
            ));
        }
    }

    let view = update_embed(
        pool,
        &user.id,
        &id,
        title_override,
        poster_override,
        if_match,
        now_millis(),
    )
    .await
    .map_err(|e| video_error_to_app(e, request_id))?;
    Ok(Json(embed_json(&view)))
}

/// DELETE /api/v1/video-embeds/{id} — 删除未引用视频引用（软删为 removed）。
async fn delete_video_embed(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: AuthSession,
) -> Result<StatusCode, AppError> {
    let request_id = "delete_video_embeds_id_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    delete_embed(pool, &user.id, &id, now_millis())
        .await
        .map_err(|e| video_error_to_app(e, request_id))?;
    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/video-embeds/{id}/refresh — 202 + 异步任务；任务完成后更新
/// Embed，失败时保留安全外链。
async fn refresh_video_embed(
    State(state): State<AppState>,
    Path(id): Path<String>,
    auth: AuthSession,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "post_video_embeds_id_refresh";
    let user = auth.require_auth(request_id)?;
    // 先做可见性预检（避免对不存在/无权限的资源返回 202）。
    let pool_ref = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let now = now_millis();
    get_embed(pool_ref, &user.id, &id, now)
        .await
        .map_err(|e| video_error_to_app(e, request_id))?;

    let Some(db) = state.db.clone() else {
        return Err(AppError::internal("database not configured", request_id));
    };
    let user_id = user.id.clone();
    let embed_id = id.clone();
    // 异步执行（默认 egress 客户端；真实部署由平台注入 egress 实现）。
    tokio::spawn(async move {
        let client = crate::video::UnavailableClient;
        let _ = refresh_embed(db.as_ref(), &user_id, &embed_id, &client, now).await;
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "task_id": id,
            "status": "queued",
            "poll_url": format!("/api/v1/video-embeds/{id}"),
        })),
    ))
}

/// EmbedView → 安全投影（不含 source 原文；render mode=none 时已省略
/// official_url）。resolved 元数据非权限依据，阅读时必须重新校验。
fn embed_json(view: &EmbedView) -> Value {
    json!({
        "id": view.id,
        "user_id": view.user_id,
        "provider": view.provider,
        "status": view.status,
        "target": { "type": view.target.target_type, "id": view.target.target_id },
        "title": view.title,
        "poster_attachment_id": view.poster_attachment_id,
        "official_url": view.official_url,
        "source_host": view.source_host,
        "media_type": view.media_type,
        "external_id": view.external_id,
        "error_class": view.error_class,
        "policy_version": view.policy_version,
        "version": view.version,
        "created_at": view.created_at,
        "updated_at": view.updated_at,
        "render": {
            "mode": view.render.mode,
            "csp": {
                "frame_src": view.render.csp.frame_src,
                "media_src": view.render.csp.media_src,
                "connect_src": view.render.csp.connect_src,
                "img_src": view.render.csp.img_src,
                "script_src": view.render.csp.script_src,
            },
            "sandbox": view.render.csp.sandbox,
            "allow": view.render.csp.allow,
            "referrer_policy": view.render.csp.referrer_policy,
            "iframe_url": view.render.iframe_url,
        },
    })
}

/// VideoError → AppError（稳定 Problem code；detail 不回显源 URL）。
fn video_error_to_app(e: VideoError, request_id: &str) -> AppError {
    match e {
        VideoError::Classify(_) => AppError::with_code(
            StatusCode::BAD_REQUEST,
            e.code(),
            "Bad Request",
            "source_url rejected by video policy",
            request_id,
        ),
        VideoError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        VideoError::ProviderDisabled => AppError::with_code(
            StatusCode::CONFLICT,
            e.code(),
            "Conflict",
            "video provider is currently disabled",
            request_id,
        ),
        VideoError::HostNotAllowed(_) => AppError::with_code(
            StatusCode::BAD_REQUEST,
            e.code(),
            "Bad Request",
            "host not allowed by provider policy",
            request_id,
        ),
        VideoError::PolicyVersionConflict { .. } => AppError::with_code(
            StatusCode::CONFLICT,
            e.code(),
            "Conflict",
            "policy version changed; resolve again",
            request_id,
        ),
        VideoError::TargetNotFound => AppError::with_code(
            StatusCode::NOT_FOUND,
            e.code(),
            "Not Found",
            "target not found",
            request_id,
        ),
        VideoError::TargetForbidden => AppError::with_code(
            StatusCode::FORBIDDEN,
            e.code(),
            "Forbidden",
            "you do not own this target",
            request_id,
        ),
        VideoError::TargetConflict => AppError::with_code(
            StatusCode::CONFLICT,
            e.code(),
            "Conflict",
            "a video reference already exists for this target",
            request_id,
        ),
        VideoError::PosterAttachmentInvalid => AppError::with_code(
            StatusCode::BAD_REQUEST,
            e.code(),
            "Bad Request",
            "poster attachment is invalid",
            request_id,
        ),
        VideoError::ResolutionExpired => AppError::with_code(
            StatusCode::BAD_REQUEST,
            e.code(),
            "Bad Request",
            "resolution expired; resolve again",
            request_id,
        ),
        VideoError::EmbedNotFound => AppError::with_code(
            StatusCode::NOT_FOUND,
            e.code(),
            "Not Found",
            "video embed not found",
            request_id,
        ),
        VideoError::EmbedReferenced => AppError::with_code(
            StatusCode::CONFLICT,
            e.code(),
            "Conflict",
            "embed is referenced by published content; remove it from the post first",
            request_id,
        ),
        VideoError::VersionConflict { .. } => {
            AppError::version_conflict("video embed version conflict", request_id)
        }
        // refresh-only（写入 error_class，保留外链；此处 502 为兜底）。
        VideoError::EgressTimeout
        | VideoError::EgressTooLarge(_)
        | VideoError::EgressTooManyRedirects
        | VideoError::EgressPrivateIp(_)
        | VideoError::EgressUnavailable
        | VideoError::EgressHttp { .. }
        | VideoError::MimeMismatch(_)
        | VideoError::Takedown
        | VideoError::ProviderRatelimited
        | VideoError::NoEmbedPermission
        | VideoError::ProviderUnavailable(_)
        | VideoError::PolicyChanged
        | VideoError::Hls(_) => AppError::with_code(
            StatusCode::BAD_GATEWAY,
            e.code(),
            "Bad Gateway",
            "video provider refresh failed",
            request_id,
        ),
        VideoError::Db(m) => AppError::internal(m, request_id),
        VideoError::Internal(m) => AppError::internal(m, request_id),
    }
}
