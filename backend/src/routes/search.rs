use axum::{
    extract::{Query, State},
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{app::AppState, error::AppError};

/// 搜索路由
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/search", get(search_public_content))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    20
}

/// GET /api/v1/search — 搜索公开内容
async fn search_public_content(
    State(state): State<AppState>,
    Query(query): Query<SearchQuery>,
) -> Result<Json<Value>, AppError> {
    let request_id = "search";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);
    let pattern = format!("%{}%", query.q.replace('%', "\\%").replace('_', "\\_"));

    let posts = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, SearchResultRow>(
                "SELECT p.id, p.title, p.board_id, p.author_id, p.reply_count, p.view_count, p.created_at,
                        u.username_normalized as author_name, b.slug as board_slug, b.name as board_name
                 FROM posts p
                 LEFT JOIN users u ON u.id = p.author_id
                 LEFT JOIN boards b ON b.id = p.board_id
                 WHERE p.status = 'published' AND p.visibility = 'public'
                   AND (p.title LIKE ? ESCAPE '\\' OR p.content LIKE ? ESCAPE '\\')
                 ORDER BY p.created_at DESC LIMIT ?",
            )
            .bind(&pattern)
            .bind(&pattern)
            .bind(limit)
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, SearchResultRow>(
                "SELECT p.id, p.title, p.board_id, p.author_id, p.reply_count, p.view_count, p.created_at,
                        u.username_normalized as author_name, b.slug as board_slug, b.name as board_name
                 FROM posts p
                 LEFT JOIN users u ON u.id = p.author_id
                 LEFT JOIN boards b ON b.id = p.board_id
                 WHERE p.status = 'published' AND p.visibility = 'public'
                   AND (p.title LIKE ? ESCAPE '\\' OR p.content LIKE ? ESCAPE '\\')
                 ORDER BY p.created_at DESC LIMIT ?",
            )
            .bind(&pattern)
            .bind(&pattern)
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
                "board_id": p.board_id,
                "board_slug": p.board_slug,
                "board_name": p.board_name,
                "author_id": p.author_id,
                "author_name": p.author_name,
                "reply_count": p.reply_count,
                "view_count": p.view_count,
                "created_at": p.created_at,
            })
        })
        .collect();

    Ok(Json(
        json!({ "items": items, "query": query.q, "next_cursor": null, "has_more": false }),
    ))
}

#[derive(sqlx::FromRow)]
struct SearchResultRow {
    id: String,
    title: String,
    board_id: String,
    author_id: String,
    reply_count: i64,
    view_count: i64,
    created_at: i64,
    author_name: Option<String>,
    board_slug: Option<String>,
    board_name: Option<String>,
}
