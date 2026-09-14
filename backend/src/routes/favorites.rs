//! 社交域收藏路由（GAP-FIX 社交域）。
//!
//! - `POST /api/v1/posts/{id}/favorite`：收藏帖子 → 200
//!   `{favorited:true, favorite_count:n}`；
//! - `DELETE /api/v1/posts/{id}/favorite`：取消收藏 → 200
//!   `{favorited:false, favorite_count:n}`；
//! - `GET /api/v1/me/favorites?after=&limit=30`：本人收藏列表（keyset on
//!   `favorites.created_at DESC`，PostSummary 复用 posts.rs 列表投影）。
//!
//! 幂等取舍：favorite/follow 这类 toggle 写操作没有独立的业务产物（无
//! id 生成、无副作用链），`favorites(user_id, post_id)` 复合主键本身就是
//! 天然幂等键——重复 POST 返回当前态、重复 DELETE 同样幂等。因此不引入
//! `idempotency_records`（begin_or_replay/complete）——那要求每个 toggle
//! 都持久化一条记录并在重放时回放响应，收益为零（响应只是当前态计数），
//! 只增加写放大。body 中的 `client_request_id` 按契约接收但不参与判定。
//!
//! 帖子可见性：只允许收藏 `published`/`hidden` 且未删除的帖子（与评论
//! 创建路径同一判定），否则 404（不泄露存在性）。

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{app::AppState, auth::session::AuthSession, error::AppError, outbox::now_millis};

/// 收藏列表 keyset 游标上限。
const MAX_LIST_LIMIT: i64 = 100;
const DEFAULT_LIST_LIMIT: i64 = 30;

/// 收藏路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/posts/{id}/favorite",
            post(favorite_post).delete(unfavorite_post),
        )
        .route("/api/v1/me/favorites", get(list_my_favorites))
}

#[derive(Deserialize)]
struct ListFavoritesQuery {
    /// keyset 游标：上一页最后一条 favorites.created_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    DEFAULT_LIST_LIMIT
}

/// 写响应统一 `Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 帖子可见性（可收藏）：published/hidden 且未删除。
async fn post_favoritable(
    pool: &crate::db::DatabasePool,
    post_id: &str,
    request_id: &str,
) -> Result<bool, AppError> {
    let row: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT 1 FROM posts WHERE id = ? AND status IN ('published', 'hidden') AND deleted_at IS NULL",
            )
            .bind(post_id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT 1 FROM posts WHERE id = ? AND status IN ('published', 'hidden') AND deleted_at IS NULL",
            )
            .bind(post_id)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row == Some(1))
}

/// 当前收藏计数（不含已删帖子的收藏行也无意义，直接全量计数）。
async fn favorite_count(
    pool: &crate::db::DatabasePool,
    post_id: &str,
    request_id: &str,
) -> Result<i64, AppError> {
    let count: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM favorites WHERE post_id = ?")
                .bind(post_id)
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM favorites WHERE post_id = ?")
                .bind(post_id)
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(count)
}

/// POST /api/v1/posts/{id}/favorite — 收藏帖子（复合主键幂等）。
///
/// 契约 body 为 `{client_request_id}`：幂等由复合主键保证（见模块注释），
/// 该字段不参与判定，body 原样接收不解析。
async fn favorite_post(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "favorite_post";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    if !post_favoritable(pool, &id, request_id).await? {
        return Err(AppError::not_found("post not found", request_id));
    }

    // 幂等：复合主键 (user_id, post_id)——先查后插（事务内），
    // 已存在则直接返回当前态（重复调用幂等，不产生重复行）。
    insert_favorite(pool, &user.id, &id, request_id).await?;
    let count = favorite_count(pool, &id, request_id).await?;
    let resp = (
        StatusCode::OK,
        Json(json!({ "favorited": true, "favorite_count": count })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// 事务内「查重 + 插入」收藏行；返回是否本次新插入。
async fn insert_favorite(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    post_id: &str,
    request_id: &str,
) -> Result<bool, AppError> {
    let now = now_millis();
    let outcome = match pool {
        Either::Left(p) => {
            let mut conn = p
                .acquire()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut *conn)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let r: Result<bool, sqlx::Error> = async {
                let dup: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM favorites WHERE user_id = ? AND post_id = ?",
                )
                .bind(user_id)
                .bind(post_id)
                .fetch_one(&mut *conn)
                .await?;
                if dup > 0 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO favorites (user_id, post_id, created_at) VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(post_id)
                .bind(now)
                .execute(&mut *conn)
                .await?;
                Ok(true)
            }
            .await;
            match r {
                Ok(v) => {
                    sqlx::query("COMMIT")
                        .execute(&mut *conn)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let r: Result<bool, sqlx::Error> = async {
                let dup: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM favorites WHERE user_id = ? AND post_id = ?",
                )
                .bind(user_id)
                .bind(post_id)
                .fetch_one(&mut *tx)
                .await?;
                if dup > 0 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO favorites (user_id, post_id, created_at) VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(post_id)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                Ok(true)
            }
            .await;
            match r {
                Ok(v) => {
                    tx.commit()
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            }
        }
    };
    outcome.map_err(|e| AppError::internal(e.to_string(), request_id))
}

/// DELETE /api/v1/posts/{id}/favorite — 取消收藏（幂等）。
async fn unfavorite_post(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "unfavorite_post";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    remove_favorite(pool, &user.id, &id, request_id).await?;
    let count = favorite_count(pool, &id, request_id).await?;
    let resp = (
        StatusCode::OK,
        Json(json!({ "favorited": false, "favorite_count": count })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// 删除收藏行（不存在也幂等成功）。
async fn remove_favorite(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    post_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM favorites WHERE user_id = ? AND post_id = ?")
                .bind(user_id)
                .bind(post_id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM favorites WHERE user_id = ? AND post_id = ?")
                .bind(user_id)
                .bind(post_id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }
    Ok(())
}

/// 本人收藏列表行（PostSummary 投影 + favorited_at 游标键）。
#[derive(sqlx::FromRow)]
struct FavoritePostRow {
    id: String,
    board_id: String,
    author_id: String,
    post_type: String,
    title: String,
    status: String,
    reply_count: i64,
    view_count: i64,
    created_at: i64,
    updated_at: i64,
    last_reply_at: Option<i64>,
    pinned_at: Option<i64>,
    author_name: Option<String>,
    /// 作者昵称（users.display_name；前台列表优先显示昵称，缺省回退用户名）。
    author_display_name: Option<String>,
    favorited_at: i64,
}

/// GET /api/v1/me/favorites — 本人收藏的帖子（keyset on created_at DESC）。
async fn list_my_favorites(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<ListFavoritesQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_my_favorites";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let before = match query.after.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(raw.parse::<i64>().map_err(|_| {
            AppError::bad_request("after must be an integer cursor", request_id, None)
        })?),
    };

    // 只返回收藏时仍可见（published/hidden 未删除）的帖子；软删帖自动消失。
    let sql = "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status,
                      p.reply_count, p.view_count, p.created_at, p.updated_at, p.last_reply_at,
                      p.pinned_at, u.username_normalized AS author_name,
                      u.display_name AS author_display_name, f.created_at AS favorited_at
               FROM favorites f
               JOIN posts p ON p.id = f.post_id
               LEFT JOIN users u ON u.id = p.author_id
               WHERE f.user_id = ? AND p.deleted_at IS NULL
                 AND p.status IN ('published', 'hidden')
                 AND (? IS NULL OR f.created_at < ?)
               ORDER BY f.created_at DESC, p.id DESC
               LIMIT ?";
    let fetch_limit = limit + 1;
    let rows: Vec<FavoritePostRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as(sql)
                .bind(&user.id)
                .bind(before)
                .bind(before)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as(sql)
                .bind(&user.id)
                .bind(before)
                .bind(before)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = rows.len() as i64 > limit;
    let page: Vec<FavoritePostRow> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| r.favorited_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    // PostSummary 复用 posts.rs 列表投影字段集（同一键集）。
    let items: Vec<Value> = page
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "board_id": p.board_id,
                "author": {
                    "id": p.author_id,
                    "username": p.author_name,
                    "display_name": p.author_display_name,
                },
                "post_type": p.post_type,
                "title": p.title,
                "status": p.status,
                "reply_count": p.reply_count,
                "view_count": p.view_count,
                "pinned_at": p.pinned_at,
                "created_at": p.created_at,
                "updated_at": p.updated_at,
                "last_reply_at": p.last_reply_at,
                "favorited_at": p.favorited_at,
            })
        })
        .collect();

    let resp = (
        StatusCode::OK,
        Json(json!({
            "items": items,
            "next_cursor": next_cursor,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}
