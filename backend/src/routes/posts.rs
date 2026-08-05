use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{app::AppState, auth::session::AuthSession, error::AppError};

/// 帖子路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/posts", get(list_posts).post(create_post))
        .route("/api/v1/posts/{id}", get(get_post).patch(update_post))
        .route(
            "/api/v1/posts/{id}/comments",
            get(list_comments).post(create_comment),
        )
        .route("/api/v1/posts/{id}/reactions", post(toggle_reaction))
}

#[derive(Deserialize)]
struct CreatePostRequest {
    board_slug: String,
    title: String,
    content: String,
    #[serde(default)]
    visibility: Option<String>,
}

#[derive(Deserialize)]
struct UpdatePostRequest {
    title: Option<String>,
    content: Option<String>,
}

#[derive(Deserialize)]
struct CreateCommentRequest {
    content: String,
    #[serde(default)]
    parent_id: Option<String>,
}

#[derive(Deserialize)]
struct ListQuery {
    /// 分页游标（接口契约保留字段，游标分页待实现）
    #[serde(default)]
    #[allow(dead_code)]
    cursor: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    20
}

#[derive(Deserialize)]
struct ListPostsQuery {
    #[serde(default)]
    board_id: Option<String>,
    #[serde(default)]
    sort: Option<String>,
    /// 分页游标（接口契约保留字段，游标分页待实现）
    #[serde(default)]
    #[allow(dead_code)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

/// POST /api/v1/posts — 创建帖子
async fn create_post(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<CreatePostRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "create_post";
    let user = auth.require_auth(request_id)?;

    // 检查邮箱验证
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

    // 验证输入
    if req.title.is_empty() || req.title.len() > 200 {
        return Err(AppError::bad_request(
            "title must be 1-200 characters",
            request_id,
            None,
        ));
    }
    if req.content.is_empty() || req.content.len() > 50000 {
        return Err(AppError::bad_request(
            "content must be 1-50000 characters",
            request_id,
            None,
        ));
    }

    // 查找板块
    let board_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT id FROM boards WHERE slug = ? AND is_active = 1")
                .bind(&req.board_slug)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT id FROM boards WHERE slug = ? AND is_active = 1")
                .bind(&req.board_slug)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let board_id = board_id.ok_or_else(|| AppError::not_found("board not found", request_id))?;

    let post_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp();
    let visibility = req.visibility.unwrap_or_else(|| "public".to_string());

    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(
                "INSERT INTO posts (id, board_id, author_id, title, content, content_format, status, visibility, reply_count, view_count, pinned, created_at, updated_at, last_reply_at, last_reply_by)
                 VALUES (?, ?, ?, ?, ?, 'markdown', 'published', ?, 0, 0, 0, ?, ?, ?, NULL)",
            )
            .bind(&post_id)
            .bind(&board_id)
            .bind(&user.id)
            .bind(&req.title)
            .bind(&req.content)
            .bind(&visibility)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            sqlx::query(
                "UPDATE boards SET post_count = post_count + 1, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(&board_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(
                "INSERT INTO posts (id, board_id, author_id, title, content, content_format, status, visibility, reply_count, view_count, pinned, created_at, updated_at, last_reply_at, last_reply_by)
                 VALUES (?, ?, ?, ?, ?, 'markdown', 'published', ?, 0, 0, 0, ?, ?, ?, NULL)",
            )
            .bind(&post_id)
            .bind(&board_id)
            .bind(&user.id)
            .bind(&req.title)
            .bind(&req.content)
            .bind(&visibility)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            sqlx::query(
                "UPDATE boards SET post_count = post_count + 1, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(&board_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": post_id,
            "board_id": board_id,
            "author_id": user.id,
            "title": req.title,
            "status": "published",
            "visibility": visibility,
            "created_at": now,
        })),
    ))
}

/// GET /api/v1/posts — 列出帖子（公开，可按板块过滤/排序）
async fn list_posts(
    State(state): State<AppState>,
    Query(query): Query<ListPostsQuery>,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_posts";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);
    let sort_order = match query.sort.as_deref() {
        Some("popular") => "view_count DESC, reply_count DESC",
        _ => "pinned DESC, last_reply_at DESC, created_at DESC",
    };

    let sql = format!(
        "SELECT p.id, p.board_id, p.author_id, p.title, p.status, p.visibility,
                p.reply_count, p.view_count, p.pinned, p.created_at, p.last_reply_at,
                u.username_normalized as author_name
         FROM posts p
         LEFT JOIN users u ON u.id = p.author_id
         WHERE p.status = 'published' AND (? IS NULL OR p.board_id = ?)
         ORDER BY {} LIMIT ?",
        sort_order
    );

    let posts = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, PostListRowFull>(&sql)
                .bind(&query.board_id)
                .bind(&query.board_id)
                .bind(limit)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, PostListRowFull>(&sql)
                .bind(&query.board_id)
                .bind(&query.board_id)
                .bind(limit)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let items: Vec<Value> = posts
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "board_id": p.board_id,
                "author": {
                    "id": p.author_id,
                    "username": p.author_name,
                },
                "title": p.title,
                "status": p.status,
                "visibility": p.visibility,
                "reply_count": p.reply_count,
                "view_count": p.view_count,
                "pinned": p.pinned != 0,
                "created_at": p.created_at,
                "last_reply_at": p.last_reply_at,
            })
        })
        .collect();

    Ok(Json(json!({
        "items": items,
        "page": { "next_cursor": null, "has_more": false },
    })))
}

/// GET /api/v1/posts/{id} — 获取帖子详情
async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_post";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let row = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, PostDetailRow>(
                "SELECT p.id, p.board_id, p.author_id, p.title, p.content, p.content_format, p.status, p.visibility,
                        p.reply_count, p.view_count, p.pinned, p.created_at, p.updated_at, p.last_reply_at,
                        u.username_normalized as author_name
                 FROM posts p
                 LEFT JOIN users u ON u.id = p.author_id
                 WHERE p.id = ? AND p.status != 'deleted'",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, PostDetailRow>(
                "SELECT p.id, p.board_id, p.author_id, p.title, p.content, p.content_format, p.status, p.visibility,
                        p.reply_count, p.view_count, p.pinned, p.created_at, p.updated_at, p.last_reply_at,
                        u.username_normalized as author_name
                 FROM posts p
                 LEFT JOIN users u ON u.id = p.author_id
                 WHERE p.id = ? AND p.status != 'deleted'",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match row {
        Some(r) => {
            // 增加浏览量
            match pool {
                Either::Left(p) => {
                    let _ =
                        sqlx::query("UPDATE posts SET view_count = view_count + 1 WHERE id = ?")
                            .bind(&id)
                            .execute(p)
                            .await;
                }
                Either::Right(p) => {
                    let _ =
                        sqlx::query("UPDATE posts SET view_count = view_count + 1 WHERE id = ?")
                            .bind(&id)
                            .execute(p)
                            .await;
                }
            }

            Ok(Json(json!({
                "id": r.id,
                "board_id": r.board_id,
                "author_id": r.author_id,
                "author_name": r.author_name,
                "title": r.title,
                "content": r.content,
                "content_format": r.content_format,
                "status": r.status,
                "visibility": r.visibility,
                "reply_count": r.reply_count,
                "view_count": r.view_count + 1,
                "pinned": r.pinned != 0,
                "created_at": r.created_at,
                "updated_at": r.updated_at,
                "last_reply_at": r.last_reply_at,
            })))
        }
        None => Err(AppError::not_found("post not found", request_id)),
    }
}

/// PATCH /api/v1/posts/{id} — 更新帖子
async fn update_post(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    Json(req): Json<UpdatePostRequest>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_post";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let now = chrono::Utc::now().timestamp();

    // 验证所有权
    let author_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT author_id FROM posts WHERE id = ? AND status != 'deleted'")
                .bind(&id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT author_id FROM posts WHERE id = ? AND status != 'deleted'")
                .bind(&id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let author_id = author_id.ok_or_else(|| AppError::not_found("post not found", request_id))?;
    if author_id != user.id {
        return Err(AppError::forbidden("not the author", request_id));
    }

    if let Some(title) = &req.title {
        if title.is_empty() || title.len() > 200 {
            return Err(AppError::bad_request(
                "title must be 1-200 characters",
                request_id,
                None,
            ));
        }
    }
    if let Some(content) = &req.content {
        if content.is_empty() || content.len() > 50000 {
            return Err(AppError::bad_request(
                "content must be 1-50000 characters",
                request_id,
                None,
            ));
        }
    }

    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET title = COALESCE(?, title), content = COALESCE(?, content), updated_at = ? WHERE id = ?")
                .bind(&req.title)
                .bind(&req.content)
                .bind(now)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("UPDATE posts SET title = COALESCE(?, title), content = COALESCE(?, content), updated_at = ? WHERE id = ?")
                .bind(&req.title)
                .bind(&req.content)
                .bind(now)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok(Json(json!({ "id": id, "updated_at": now })))
}

/// GET /api/v1/posts/{id}/comments — 列出评论
async fn list_comments(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_comments";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);

    let comments = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, CommentRow>(
                "SELECT c.id, c.post_id, c.author_id, c.parent_id, c.content, c.content_format, c.status, c.floor, c.created_at,
                        u.username_normalized as author_name
                 FROM comments c
                 LEFT JOIN users u ON u.id = c.author_id
                 WHERE c.post_id = ? AND c.status = 'published'
                 ORDER BY c.floor ASC LIMIT ?",
            )
            .bind(&id)
            .bind(limit)
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, CommentRow>(
                "SELECT c.id, c.post_id, c.author_id, c.parent_id, c.content, c.content_format, c.status, c.floor, c.created_at,
                        u.username_normalized as author_name
                 FROM comments c
                 LEFT JOIN users u ON u.id = c.author_id
                 WHERE c.post_id = ? AND c.status = 'published'
                 ORDER BY c.floor ASC LIMIT ?",
            )
            .bind(&id)
            .bind(limit)
            .fetch_all(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let items: Vec<Value> = comments
        .iter()
        .map(|c| {
            json!({
                "id": c.id,
                "post_id": c.post_id,
                "author_id": c.author_id,
                "author_name": c.author_name,
                "parent_id": c.parent_id,
                "content": c.content,
                "content_format": c.content_format,
                "floor": c.floor,
                "created_at": c.created_at,
            })
        })
        .collect();

    Ok(Json(
        json!({ "items": items, "next_cursor": null, "has_more": false }),
    ))
}

/// POST /api/v1/posts/{id}/comments — 创建评论
async fn create_comment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    Json(req): Json<CreateCommentRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "create_comment";
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

    if req.content.is_empty() || req.content.len() > 10000 {
        return Err(AppError::bad_request(
            "content must be 1-10000 characters",
            request_id,
            None,
        ));
    }

    let comment_id = uuid::Uuid::now_v7().to_string();
    let now = chrono::Utc::now().timestamp();

    // 获取当前 floor 数
    let floor: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM comments WHERE post_id = ?")
                .bind(&id)
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM comments WHERE post_id = ?")
                .bind(&id)
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let floor = floor + 1;

    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(
                "INSERT INTO comments (id, post_id, author_id, parent_id, content, content_format, status, floor, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 'markdown', 'published', ?, ?, ?)",
            )
            .bind(&comment_id)
            .bind(&id)
            .bind(&user.id)
            .bind(&req.parent_id)
            .bind(&req.content)
            .bind(floor)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            sqlx::query("UPDATE posts SET reply_count = reply_count + 1, last_reply_at = ?, last_reply_by = ?, updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(&user.id)
                .bind(now)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(
                "INSERT INTO comments (id, post_id, author_id, parent_id, content, content_format, status, floor, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, 'markdown', 'published', ?, ?, ?)",
            )
            .bind(&comment_id)
            .bind(&id)
            .bind(&user.id)
            .bind(&req.parent_id)
            .bind(&req.content)
            .bind(floor)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            sqlx::query("UPDATE posts SET reply_count = reply_count + 1, last_reply_at = ?, last_reply_by = ?, updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(&user.id)
                .bind(now)
                .bind(&id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": comment_id,
            "post_id": id,
            "author_id": user.id,
            "author_name": user.username,
            "parent_id": req.parent_id,
            "content": req.content,
            "floor": floor,
            "created_at": now,
        })),
    ))
}

/// POST /api/v1/posts/{id}/reactions — 切换反应（点赞）
async fn toggle_reaction(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "toggle_reaction";
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
            "DELETE FROM post_reactions WHERE post_id = ? AND user_id = ? AND reaction = ?",
        )
        .bind(&id)
        .bind(&user.id)
        .bind(reaction)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "DELETE FROM post_reactions WHERE post_id = ? AND user_id = ? AND reaction = ?",
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
        // 没有删除任何行，说明之前没有反应 → 添加反应（toggle on）
        match pool {
            Either::Left(p) => {
                sqlx::query("INSERT INTO post_reactions (post_id, user_id, reaction, created_at) VALUES (?, ?, ?, ?)")
                    .bind(&id)
                    .bind(&user.id)
                    .bind(reaction)
                    .bind(now)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            Either::Right(p) => {
                sqlx::query("INSERT INTO post_reactions (post_id, user_id, reaction, created_at) VALUES (?, ?, ?, ?)")
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
                "SELECT COUNT(*) FROM post_reactions WHERE post_id = ? AND reaction = ?",
            )
            .bind(&id)
            .bind(reaction)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM post_reactions WHERE post_id = ? AND reaction = ?",
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

#[derive(sqlx::FromRow)]
struct PostDetailRow {
    id: String,
    board_id: String,
    author_id: String,
    title: String,
    content: String,
    content_format: String,
    status: String,
    visibility: String,
    reply_count: i64,
    view_count: i64,
    pinned: i64,
    created_at: i64,
    updated_at: i64,
    last_reply_at: Option<i64>,
    author_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct PostListRowFull {
    id: String,
    board_id: String,
    author_id: String,
    title: String,
    status: String,
    visibility: String,
    reply_count: i64,
    view_count: i64,
    pinned: i64,
    created_at: i64,
    last_reply_at: Option<i64>,
    author_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct CommentRow {
    id: String,
    post_id: String,
    author_id: String,
    parent_id: Option<String>,
    content: String,
    content_format: String,
    #[allow(dead_code)]
    status: String,
    floor: i64,
    created_at: i64,
    author_name: Option<String>,
}
