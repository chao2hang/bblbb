use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, patch, post},
    Router,
};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Either;

use crate::{
    app::AppState,
    audit::AuditEntry,
    auth::session::AuthSession,
    authz::decision::AUTHZ_POLICY_VERSION,
    authz::enforce::authorize_action,
    content::comments::service::{
        comment_json, load_comment_projection, soft_delete_comment,
        update_comment as service_update_comment, EditCommentError, EditCommentInput,
    },
    db::DatabasePool,
    domain::comments::CommentContent,
    error::AppError,
    outbox::now_millis,
};

/// 评论路由（单个评论操作 — 列表和创建在 posts 路由中）
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/comments/{id}",
            patch(update_comment).delete(delete_comment),
        )
        .route(
            "/api/v1/comments/{id}/reactions",
            post(create_comment_reaction),
        )
        .route(
            "/api/v1/comments/{id}/reactions/{reaction}",
            delete(delete_comment_reaction),
        )
}

/// PATCH 请求体：`{markdown, version?}`（version 与 `If-Match` 头二选一）。
#[derive(Deserialize)]
struct UpdateCommentRequest {
    markdown: String,
    #[serde(default)]
    version: Option<i64>,
    /// 管理员代改/修订说明（可选；写入 comment_revisions.change_reason）。
    #[serde(default)]
    reason: Option<String>,
}

/// DELETE 请求体（可选：管理员删除时 reason；普通作者删除可省略）。
#[derive(Deserialize, Default)]
struct DeleteCommentRequest {
    #[serde(default)]
    reason: Option<String>,
}

/// PATCH /api/v1/comments/{id} — 更新评论（M04-COMMENTS-05，作者限时编辑）。
///
/// 作者本人（`comment.author_id == session user`）+ status published +
/// 未软删 → 否则 404/403；`created_at` 起 [`COMMENT_EDIT_WINDOW_MS`] 内可编辑，
/// 超窗 409 `conflict`；版本守卫（`If-Match` 头优先，否则 body `version`）冲突
/// 409 `version_conflict`；写入 = 版本守卫 UPDATE + 不可变 `comment_revisions`
/// 快照（同事务）。响应 = 更新后 Comment 投影 + `Cache-Control: private, no-store`。
async fn update_comment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "update_comment";
    let user = auth.require_auth(request_id)?;
    if !user.email_verified {
        return Err(AppError::forbidden(
            "email verification required",
            request_id,
        ));
    }
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let req: UpdateCommentRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let content = CommentContent::parse(&req.markdown)
        .map_err(|detail| AppError::bad_request(detail, request_id, None))?;

    // 当前评论行（含软删/状态；已删或非 published 统一 404，不泄漏）
    let current = load_comment_row(pool, &id, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("comment not found", request_id))?;
    let (author_id, status, version, created_at, deleted_at) = current;
    if status != "published" || deleted_at.is_some() {
        return Err(AppError::not_found("comment not found", request_id));
    }
    if author_id != user.id {
        return Err(AppError::forbidden(
            "only the author can edit this comment",
            request_id,
        ));
    }

    // 版本守卫：If-Match 头优先，否则 body version
    let expected_version: i64 =
        if let Some(raw) = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()) {
            raw.parse().map_err(|_| {
                AppError::bad_request("If-Match must be an integer version", request_id, None)
            })?
        } else if let Some(v) = req.version {
            v
        } else {
            return Err(AppError::bad_request(
                "If-Match or version is required",
                request_id,
                None,
            ));
        };
    if expected_version != version {
        return Err(AppError::version_conflict(
            "comment version mismatch",
            request_id,
        ));
    }

    // 限时编辑窗口（created_at 起 30 分钟）
    let now = now_millis();
    if now - created_at > crate::content::comments::service::COMMENT_EDIT_WINDOW_MS {
        return Err(AppError::conflict(
            "comment edit window expired",
            request_id,
        ));
    }

    service_update_comment(
        pool,
        &EditCommentInput {
            comment_id: &id,
            editor_id: &user.id,
            new_markdown: content.as_str(),
            expected_version,
            change_reason: req.reason.as_deref(),
            now,
        },
    )
    .await
    .map_err(|e| match e {
        EditCommentError::VersionMismatch { .. } => {
            AppError::version_conflict("comment version mismatch", request_id)
        }
        EditCommentError::NotFound => AppError::not_found("comment not found", request_id),
        EditCommentError::Db(msg) => AppError::internal(msg, request_id),
    })?;

    let projection = load_comment_projection(pool, &id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("comment not found", request_id))?;
    let resp = (StatusCode::OK, Json(comment_json(&projection))).into_response();
    Ok(private_no_store(resp))
}

/// DELETE /api/v1/comments/{id} — 删除评论（M04-COMMENTS-06）。
///
/// - 作者删除：软删除（`status='deleted'` + `deleted_at`），行保留（占位
///   投影/审计），可选 `If-Match`，响应 204；
/// - 管理员/审核员（`post.moderate`，权限注册表无 `comment.moderate`）删除
///   他人评论：软删除 + 写审计（`delegated_admin_action`，复用 M04-POSTS-08
///   模式）；
/// - 软删**不递减** `posts.reply_count`（占位保留楼层，见 agent-A report）。
async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "delete_comment";
    let user = auth.require_auth(request_id)?;
    if !user.email_verified {
        return Err(AppError::forbidden(
            "email verification required",
            request_id,
        ));
    }
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let req: DeleteCommentRequest = if body.is_empty() {
        DeleteCommentRequest::default()
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?
    };

    // 当前评论行；已删或非 published 统一 404，不泄漏
    let current = load_comment_row(pool, &id, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("comment not found", request_id))?;
    let (author_id, status, version, _created_at, deleted_at) = current;
    if status != "published" || deleted_at.is_some() {
        return Err(AppError::not_found("comment not found", request_id));
    }

    // 可选 If-Match（防并发覆盖）
    if let Some(raw) = headers.get(header::IF_MATCH).and_then(|v| v.to_str().ok()) {
        let expected: i64 = raw.parse().map_err(|_| {
            AppError::bad_request("If-Match must be an integer version", request_id, None)
        })?;
        if expected != version {
            return Err(AppError::version_conflict(
                "comment version mismatch",
                request_id,
            ));
        }
    }

    let is_author = author_id == user.id;
    if !is_author {
        let decision =
            authorize_action(pool, &user.id, "post.moderate", None, AUTHZ_POLICY_VERSION)
                .await
                .map_err(|e| AppError::internal(e, request_id))?;
        if !decision.is_allowed() {
            return Err(AppError::forbidden(
                "post.moderate permission required",
                request_id,
            ));
        }
        // 管理员删除写审计（M01-AUDIT-01 强制路径；复用 M04-POSTS-08 模式）
        let reason = req.reason.as_deref().unwrap_or("moderator delete").trim();
        let reason = if reason.is_empty() {
            "moderator delete"
        } else {
            reason
        };
        AuditEntry::delegated_admin_action(
            &user.id,
            "moderator",
            "comment.delete",
            "comment",
            &id,
            reason,
        )
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    }

    let now = now_millis();
    soft_delete_comment(pool, &id, now)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = StatusCode::NO_CONTENT.into_response();
    Ok(private_no_store(resp))
}

/// 读取评论行 `(author_id, status, version, created_at, deleted_at)`。
async fn load_comment_row(
    pool: &DatabasePool,
    id: &str,
    request_id: &'static str,
) -> Result<Option<(String, String, i64, i64, Option<i64>)>, AppError> {
    let row: Option<(String, String, i64, i64, Option<i64>)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT author_id, status, version, created_at, deleted_at FROM comments WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT author_id, status, version, created_at, deleted_at FROM comments WHERE id = ?",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    Ok(row)
}

/// 写响应：`Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// POST /api/v1/comments/{id}/reactions — 创建评论反应（M07-SHOP-08）
async fn create_comment_reaction(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "create_comment_reaction";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let reaction = "like";
    crate::reactions::service::add_reaction(pool, &user.id, "comment", &id, reaction, false)
        .await
        .map(Json)
        .map_err(|e| map_reaction_error(e, request_id))
}

/// DELETE /api/v1/comments/{id}/reactions/{reaction} — 移除评论反应
async fn delete_comment_reaction(
    State(state): State<AppState>,
    auth: AuthSession,
    Path((id, reaction)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let request_id = "delete_comment_reaction";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::reactions::service::remove_reaction(pool, &user.id, "comment", &id, &reaction)
        .await
        .map(Json)
        .map_err(|e| map_reaction_error(e, request_id))
}

/// 反应错误 → AppError（不泄漏目标细节）。
fn map_reaction_error(e: crate::reactions::ReactionError, request_id: &str) -> AppError {
    use crate::reactions::ReactionError;
    match e {
        ReactionError::Db(m) => AppError::internal(m, request_id),
        ReactionError::NotFound(m) => AppError::not_found(m, request_id),
        ReactionError::Invalid(m) => AppError::bad_request(m, request_id, None),
        ReactionError::Forbidden(m) => AppError::forbidden(m, request_id),
        ReactionError::SelfReaction => {
            AppError::bad_request("cannot react to own content", request_id, None)
        }
        ReactionError::RateLimited { retry_after_ms } => AppError::rate_limited(
            "too many reactions",
            request_id,
            (retry_after_ms / 1000).max(1) as u64,
            20,
            0,
            crate::outbox::now_millis() / 1000 + (retry_after_ms / 1000),
        ),
        ReactionError::PackExhausted => {
            AppError::bad_request("reaction pack exhausted", request_id, None)
        }
        ReactionError::AlreadyExists => AppError::conflict("reaction already exists", request_id),
        ReactionError::NotFoundReaction => AppError::not_found("reaction not found", request_id),
    }
}
