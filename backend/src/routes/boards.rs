use axum::{
    extract::{Path, Query, State},
    http::header,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::auth::session::AuthSession;
use crate::authz::decision::BoardVisibility;
use crate::boards::{
    board_read_gate, decode_cursor, encode_cursor, filter_visible_board_ids, BoardCursor,
    VisibilityDeny,
};
use crate::{app::AppState, error::AppError};

/// 板块路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/boards", get(list_boards))
        .route("/api/v1/boards/{slug}", get(get_board))
        .route("/api/v1/boards/{slug}/posts", get(list_board_posts))
        .route("/api/v1/tags", get(list_tags))
}

#[derive(Deserialize)]
struct ListQuery {
    /// 游标分页（OpenAPI `After`；不透明，最后一条返回项的排序键编码）
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    30
}

/// 公开数据缓存策略：匿名公开投影可缓存 60s；按请求方裁剪的列表必须私有。
const CACHE_PUBLIC: &str = "public, max-age=60";
const CACHE_PRIVATE: &str = "private, no-store";

/// GET /api/v1/boards — 列出可见板块（cursor 分页 + 稳定排序 + Cache-Control，
/// M03-BOARDS-03/04）
async fn list_boards(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "list_boards";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let actor = auth.user.as_ref().map(|u| u.id.as_str());

    let limit = query.limit.clamp(1, 100);
    let after = match &query.after {
        None => None,
        Some(raw) => Some(
            decode_cursor(raw)
                .map_err(|_| AppError::bad_request("invalid after cursor", request_id, None))?,
        ),
    };

    // 稳定排序：sort_order ASC, created_at ASC, id ASC（id 兜底确定性）
    let boards =
        match pool {
            Either::Left(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, sort_order, visibility, created_at, updated_at
                 FROM boards WHERE is_active = 1 AND deleted_at IS NULL
                 ORDER BY sort_order ASC, created_at ASC, id ASC",
            )
            .fetch_all(p)
            .await,
            Either::Right(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, sort_order, visibility, created_at, updated_at
                 FROM boards WHERE is_active = 1 AND deleted_at IS NULL
                 ORDER BY sort_order ASC, created_at ASC, id ASC",
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

    // 按可见性过滤 + 游标跳过 + 取 limit 条；next_cursor = 最后一条已返回键
    let mut items: Vec<Value> = Vec::new();
    let mut last_key: Option<BoardCursor> = None;
    let mut more_after_full = false;
    let mut iter = boards.iter().filter(|b| visible.contains(&b.id));
    for b in iter.by_ref() {
        let key = BoardCursor {
            sort_order: b.sort_order,
            created_at: b.created_at,
            id: b.id.clone(),
        };
        if let Some(after) = &after {
            if !key.gt(after) {
                continue;
            }
        }
        if items.len() == limit as usize {
            // 已取满且后面还有可见板块：下页从最后一条已返回键之后继续
            more_after_full = true;
            break;
        }
        items.push(board_json(b));
        last_key = Some(key);
    }
    let has_more = more_after_full;
    let next_cursor = if has_more { last_key } else { None };

    let page = json!({
        "items": items,
        "page": {
            "next_cursor": next_cursor.as_ref().map(encode_cursor),
            "has_more": has_more,
        },
    });

    let cache = if actor.is_some() {
        CACHE_PRIVATE
    } else {
        CACHE_PUBLIC
    };
    Ok(([(header::CACHE_CONTROL, cache)], Json(page)).into_response())
}

/// GET /api/v1/boards/{slug} — 获取板块详情（可见性门 + Cache-Control，
/// M03-BOARDS-03/04）
async fn get_board(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "get_board";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let row =
        match pool {
            Either::Left(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, sort_order, visibility, created_at, updated_at
                 FROM boards WHERE slug = ? AND is_active = 1 AND deleted_at IS NULL",
            )
            .bind(&slug)
            .fetch_optional(p)
            .await,
            Either::Right(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, sort_order, visibility, created_at, updated_at
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

    let cache = if visibility == BoardVisibility::Public {
        CACHE_PUBLIC
    } else {
        CACHE_PRIVATE
    };
    Ok(([(header::CACHE_CONTROL, cache)], Json(board_json(&b))).into_response())
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

/// Board 投影（OpenAPI `Board` = ResourceMeta + slug/name/description）。
///
/// boards 无独立 version 列：以 `updated_at`（Unix 毫秒）为乐观并发版本
/// （≥1 且每次更新递增，BOARDS-05 If-Match 同源）。
fn board_json(b: &BoardRow) -> Value {
    json!({
        "id": b.id,
        "version": b.updated_at,
        "created_at": b.created_at,
        "updated_at": b.updated_at,
        "slug": b.slug,
        "name": b.name,
        "description": b.description,
    })
}

#[derive(sqlx::FromRow)]
struct BoardRow {
    id: String,
    slug: String,
    name: String,
    description: Option<String>,
    sort_order: i64,
    visibility: String,
    created_at: i64,
    updated_at: i64,
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
