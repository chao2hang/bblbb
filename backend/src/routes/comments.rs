use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{delete, get, post},
    Router,
};
use serde_json::{json, Value};
use sqlx::Either;

use crate::{app::AppState, auth::session::AuthSession, error::AppError};

/// 评论路由（单个评论操作 — 列表和创建在 posts 路由中）
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/comments/{id}",
            get(get_comment)
                .patch(update_comment)
                .delete(delete_comment),
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

/// GET /api/v1/comments/{id} — 获取评论详情
async fn get_comment(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_comment";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let row = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, CommentDetailRow>(
                "SELECT c.id, c.post_id, c.author_id, c.parent_id, c.content, c.content_format, c.floor, c.created_at,
                        u.username_normalized as author_name
                 FROM comments c LEFT JOIN users u ON u.id = c.author_id
                 WHERE c.id = ? AND c.status = 'published'",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, CommentDetailRow>(
                "SELECT c.id, c.post_id, c.author_id, c.parent_id, c.content, c.content_format, c.floor, c.created_at,
                        u.username_normalized as author_name
                 FROM comments c LEFT JOIN users u ON u.id = c.author_id
                 WHERE c.id = ? AND c.status = 'published'",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match row {
        Some(r) => Ok(Json(json!({
            "id": r.id,
            "post_id": r.post_id,
            "author_id": r.author_id,
            "author_name": r.author_name,
            "parent_id": r.parent_id,
            "content": r.content,
            "content_format": r.content_format,
            "floor": r.floor,
            "created_at": r.created_at,
        }))),
        None => Err(AppError::not_found("comment not found", request_id)),
    }
}

/// PATCH /api/v1/comments/{id} — 更新评论
async fn update_comment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_comment";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let content = body
        .get("content")
        .and_then(|v| v.as_str())
        .ok_or_else(|| AppError::bad_request("content is required", request_id, None))?;

    if content.is_empty() || content.len() > 10000 {
        return Err(AppError::bad_request(
            "content must be 1-10000 characters",
            request_id,
            None,
        ));
    }

    // 验证所有权
    let author_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT author_id FROM comments WHERE id = ? AND status != 'deleted'",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT author_id FROM comments WHERE id = ? AND status != 'deleted'",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let author_id =
        author_id.ok_or_else(|| AppError::not_found("comment not found", request_id))?;
    if author_id != user.id {
        return Err(AppError::forbidden("not the author", request_id));
    }

    let now = chrono::Utc::now().timestamp();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE comments SET content = ?, updated_at = ? WHERE id = ?")
                .bind(content)
                .bind(now)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE comments SET content = ?, updated_at = ? WHERE id = ?")
                .bind(content)
                .bind(now)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok(Json(json!({ "id": id, "updated_at": now })))
}

/// DELETE /api/v1/comments/{id} — 删除评论（软删除）
async fn delete_comment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let request_id = "delete_comment";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let author_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT author_id FROM comments WHERE id = ? AND status != 'deleted'",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT author_id FROM comments WHERE id = ? AND status != 'deleted'",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let author_id =
        author_id.ok_or_else(|| AppError::not_found("comment not found", request_id))?;
    if author_id != user.id {
        return Err(AppError::forbidden("not the author", request_id));
    }

    let now = chrono::Utc::now().timestamp();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE comments SET status = 'deleted', updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE comments SET status = 'deleted', updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

/// POST /api/v1/comments/{id}/reactions — 创建评论反应（toggle）
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

    let now = chrono::Utc::now().timestamp();
    let reaction = "like";

    // 尝试删除已有反应，如果删除了行说明之前有反应（toggle off）
    let deleted = match pool {
        Either::Left(p) => sqlx::query(
            "DELETE FROM comment_reactions WHERE comment_id = ? AND user_id = ? AND reaction = ?",
        )
        .bind(&id)
        .bind(&user.id)
        .bind(reaction)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "DELETE FROM comment_reactions WHERE comment_id = ? AND user_id = ? AND reaction = ?",
        )
        .bind(&id)
        .bind(&user.id)
        .bind(reaction)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
    };

    let has_reaction = if deleted == 0 {
        match pool {
            Either::Left(p) => {
                sqlx::query("INSERT INTO comment_reactions (comment_id, user_id, reaction, created_at) VALUES (?, ?, ?, ?)")
                    .bind(&id)
                    .bind(&user.id)
                    .bind(reaction)
                    .bind(now)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            Either::Right(p) => {
                sqlx::query("INSERT INTO comment_reactions (comment_id, user_id, reaction, created_at) VALUES (?, ?, ?, ?)")
                    .bind(&id)
                    .bind(&user.id)
                    .bind(reaction)
                    .bind(now)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
        }
        true
    } else {
        false
    };

    // 获取总反应数
    let count: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM comment_reactions WHERE comment_id = ? AND reaction = ?",
            )
            .bind(&id)
            .bind(reaction)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM comment_reactions WHERE comment_id = ? AND reaction = ?",
            )
            .bind(&id)
            .bind(reaction)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(Json(json!({
        "reaction": reaction,
        "active": has_reaction,
        "count": count,
    })))
}

/// DELETE /api/v1/comments/{id}/reactions/{reaction} — 删除评论反应
async fn delete_comment_reaction(
    State(state): State<AppState>,
    auth: AuthSession,
    Path((id, reaction)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    let request_id = "delete_comment_reaction";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM comment_reactions WHERE comment_id = ? AND user_id = ? AND reaction = ?")
                .bind(&id)
                .bind(&user.id)
                .bind(&reaction)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM comment_reactions WHERE comment_id = ? AND user_id = ? AND reaction = ?")
                .bind(&id)
                .bind(&user.id)
                .bind(&reaction)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok(StatusCode::NO_CONTENT)
}

#[derive(sqlx::FromRow)]
struct CommentDetailRow {
    id: String,
    post_id: String,
    author_id: String,
    parent_id: Option<String>,
    content: String,
    content_format: String,
    floor: i64,
    created_at: i64,
    author_name: Option<String>,
}
