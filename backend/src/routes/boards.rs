use axum::{
    extract::{Path, Query, State},
    response::Json,
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Either;

use crate::auth::session::AuthSession;
use crate::authz::decision::BoardVisibility;
use crate::boards::{board_read_gate, filter_visible_board_ids, VisibilityDeny};
use crate::{app::AppState, error::AppError};

/// 板块路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/boards", get(list_boards))
        .route("/api/v1/boards/{slug}", get(get_board))
        .route("/api/v1/boards/{slug}/posts", get(list_board_posts))
        .route("/api/v1/tags", get(list_tags))
}

#[derive(Serialize)]
struct BoardResponse {
    id: String,
    slug: String,
    name: String,
    description: Option<String>,
    post_count: i64,
    is_active: bool,
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

/// GET /api/v1/boards — 列出可见板块（按请求方可见性过滤，M03-BOARDS-03）
async fn list_boards(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_boards";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let actor = auth.user.as_ref().map(|u| u.id.as_str());

    let boards =
        match pool {
            Either::Left(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, post_count, is_active, sort_order, visibility
                 FROM boards WHERE is_active = 1 AND deleted_at IS NULL
                 ORDER BY sort_order ASC, created_at ASC",
            )
            .fetch_all(p)
            .await,
            Either::Right(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, post_count, is_active, sort_order, visibility
                 FROM boards WHERE is_active = 1 AND deleted_at IS NULL
                 ORDER BY sort_order ASC, created_at ASC",
            )
            .fetch_all(p)
            .await,
        }
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let with_visibility: Vec<(String, BoardVisibility)> = boards
        .iter()
        .map(|b| {
            let visibility =
                BoardVisibility::parse(&b.visibility).unwrap_or(BoardVisibility::Public);
            (b.id.clone(), visibility)
        })
        .collect();
    let visible = filter_visible_board_ids(pool, &with_visibility, actor)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;

    let items: Vec<Value> = boards
        .iter()
        .filter(|b| visible.contains(&b.id))
        .map(|b| {
            json!({
                "id": b.id,
                "slug": b.slug,
                "name": b.name,
                "description": b.description,
                "post_count": b.post_count,
                "is_active": b.is_active != 0,
            })
        })
        .collect();

    Ok(Json(
        json!({ "items": items, "next_cursor": null, "has_more": false }),
    ))
}

/// GET /api/v1/boards/{slug} — 获取板块详情（可见性门，M03-BOARDS-03）
async fn get_board(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    auth: AuthSession,
) -> Result<Json<BoardResponse>, AppError> {
    let request_id = "get_board";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let row =
        match pool {
            Either::Left(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, post_count, is_active, sort_order, visibility
                 FROM boards WHERE slug = ? AND is_active = 1 AND deleted_at IS NULL",
            )
            .bind(&slug)
            .fetch_optional(p)
            .await,
            Either::Right(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, post_count, is_active, sort_order, visibility
                 FROM boards WHERE slug = ? AND is_active = 1 AND deleted_at IS NULL",
            )
            .bind(&slug)
            .fetch_optional(p)
            .await,
        }
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let Some(b) = row else {
        return Err(AppError::not_found("board not found", request_id));
    };
    let visibility = BoardVisibility::parse(&b.visibility)
        .ok_or_else(|| AppError::internal("invalid board visibility", request_id))?;
    let access = board_read_gate(
        pool,
        &b.id,
        visibility,
        auth.user.as_ref().map(|u| u.id.as_str()),
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !access.visible {
        let deny = access.deny.unwrap_or(VisibilityDeny::MissingPermission);
        return Err(deny.to_error(request_id));
    }

    Ok(Json(BoardResponse {
        id: b.id,
        slug: b.slug,
        name: b.name,
        description: b.description,
        post_count: b.post_count,
        is_active: b.is_active != 0,
    }))
}

/// GET /api/v1/boards/{slug}/posts — 列出板块下的帖子
async fn list_board_posts(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_board_posts";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);

    // 先查找板块
    let board_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT id FROM boards WHERE slug = ? AND is_active = 1")
                .bind(&slug)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT id FROM boards WHERE slug = ? AND is_active = 1")
                .bind(&slug)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let board_id = board_id.ok_or_else(|| AppError::not_found("board not found", request_id))?;

    let posts = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, PostListRow>(
                "SELECT id, title, author_id, reply_count, view_count, pinned, created_at, last_reply_at
                 FROM posts WHERE board_id = ? AND status = 'published'
                 ORDER BY pinned DESC, last_reply_at DESC, created_at DESC LIMIT ?",
            )
            .bind(&board_id)
            .bind(limit)
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, PostListRow>(
                "SELECT id, title, author_id, reply_count, view_count, pinned, created_at, last_reply_at
                 FROM posts WHERE board_id = ? AND status = 'published'
                 ORDER BY pinned DESC, last_reply_at DESC, created_at DESC LIMIT ?",
            )
            .bind(&board_id)
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
                "title": p.title,
                "author_id": p.author_id,
                "reply_count": p.reply_count,
                "view_count": p.view_count,
                "pinned": p.pinned != 0,
                "created_at": p.created_at,
                "last_reply_at": p.last_reply_at,
            })
        })
        .collect();

    Ok(Json(
        json!({ "items": items, "next_cursor": null, "has_more": false }),
    ))
}

/// GET /api/v1/tags — 列出标签
async fn list_tags(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let request_id = "list_tags";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let tags = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, TagRow>(
                "SELECT id, name, usage_count FROM tags ORDER BY usage_count DESC LIMIT 100",
            )
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, TagRow>(
                "SELECT id, name, usage_count FROM tags ORDER BY usage_count DESC LIMIT 100",
            )
            .fetch_all(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let items: Vec<Value> = tags
        .iter()
        .map(|t| json!({ "id": t.id, "name": t.name, "usage_count": t.usage_count }))
        .collect();

    Ok(Json(json!({ "items": items })))
}

#[derive(sqlx::FromRow)]
struct BoardRow {
    id: String,
    slug: String,
    name: String,
    description: Option<String>,
    post_count: i64,
    is_active: i64,
    #[allow(dead_code)]
    sort_order: i64,
    visibility: String,
}

#[derive(sqlx::FromRow)]
struct PostListRow {
    id: String,
    title: String,
    author_id: String,
    reply_count: i64,
    view_count: i64,
    pinned: i64,
    created_at: i64,
    last_reply_at: Option<i64>,
}

#[derive(sqlx::FromRow)]
struct TagRow {
    id: String,
    name: String,
    usage_count: i64,
}
