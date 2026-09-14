//! 社交域关注路由（GAP-FIX 社交域）。
//!
//! - `POST /api/v1/users/{username}/follow`：关注用户 → 200
//!   `{following:true, followers:n}`（关注自己 → 422 `cannot_follow_self`）；
//! - `DELETE /api/v1/users/{username}/follow`：取关 → 200
//!   `{following:false, followers:n}`；
//! - `GET /api/v1/users/{username}/followers|following?after=&limit=`：
//!   粉丝/关注列表（keyset on `user_follows.created_at DESC`）；
//! - `POST|DELETE /api/v1/boards/{slug}/follow`：板块关注开关 →
//!   `{following:true|false}`；
//! - `GET /api/v1/me/following`：本人关注汇总 `{users:[...], boards:[...]}`
//!   （各 limit 200）。
//!
//! 幂等取舍：与 favorites 同——`user_follows`/`board_follows` 复合主键是
//! 天然幂等键（重复 POST 返回当前态），不引入 idempotency_records。
//! `client_request_id` 按契约接收但不参与判定。
//!
//! 成就钩子：新关注成功后对**被关注者** best-effort 调用
//! `achievements::evaluate`（follower_count 类成就解锁；失败只 warn）。

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{app::AppState, auth::session::AuthSession, error::AppError, outbox::now_millis};

/// 粉丝/关注列表分页上限。
const MAX_LIST_LIMIT: i64 = 100;
const DEFAULT_LIST_LIMIT: i64 = 30;
/// /me/following 汇总上限（spec：limit 200）。
const MY_FOLLOWING_LIMIT: i64 = 200;

/// 关注路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/users/{username}/follow",
            axum::routing::post(follow_user).delete(unfollow_user),
        )
        .route("/api/v1/users/{username}/followers", get(list_followers))
        .route("/api/v1/users/{username}/following", get(list_following))
        .route(
            "/api/v1/boards/{slug}/follow",
            axum::routing::post(follow_board).delete(unfollow_board),
        )
        .route("/api/v1/me/following", get(list_my_following))
}

#[derive(Deserialize)]
struct ListFollowQuery {
    /// keyset 游标：上一页最后一条 user_follows.created_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    DEFAULT_LIST_LIMIT
}

/// 写/私有响应统一 `Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 按用户名（normalized）查目标用户 id；不存在/已注销 → None。
async fn user_id_by_username(
    pool: &crate::db::DatabasePool,
    username: &str,
    request_id: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT id FROM users WHERE username_normalized = ? AND status <> 'deleted'",
            )
            .bind(username.to_lowercase())
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT id FROM users WHERE username_normalized = ? AND status <> 'deleted'",
            )
            .bind(username.to_lowercase())
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row)
}

/// 目标用户粉丝数。
async fn followers_count(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<i64, AppError> {
    let count: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_follows WHERE followee_id = ?")
                .bind(user_id)
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_follows WHERE followee_id = ?")
                .bind(user_id)
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(count)
}

/// POST /api/v1/users/{username}/follow — 关注用户（复合主键幂等）。
///
/// 契约 body 为 `{client_request_id}`：幂等由复合主键保证（见模块注释），
/// 该字段不参与判定，body 原样接收不解析。
async fn follow_user(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(username): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "follow_user";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let target = user_id_by_username(pool, &username, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("user not found", request_id))?;

    // 禁止关注自己（422 稳定错误码）。
    if target == user.id {
        return Err(AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "cannot_follow_self",
            "Unprocessable Entity",
            "cannot follow yourself",
            request_id,
        ));
    }

    let newly_followed = insert_follow(pool, &user.id, &target, request_id).await?;
    let followers = followers_count(pool, &target, request_id).await?;

    // 成就钩子（best-effort）：被关注者的 follower_count 类成就。
    if newly_followed {
        if let Err(e) = crate::achievements::evaluate(pool, &target).await {
            tracing::warn!(user_id = %target, error = %e, "achievement evaluate failed (follow)");
        }
    }

    let resp = (
        StatusCode::OK,
        Json(json!({ "following": true, "followers": followers })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// 事务内「查重 + 插入」关注行；返回是否本次新插入。
async fn insert_follow(
    pool: &crate::db::DatabasePool,
    follower_id: &str,
    followee_id: &str,
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
                    "SELECT COUNT(*) FROM user_follows WHERE follower_id = ? AND followee_id = ?",
                )
                .bind(follower_id)
                .bind(followee_id)
                .fetch_one(&mut *conn)
                .await?;
                if dup > 0 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO user_follows (follower_id, followee_id, created_at) VALUES (?, ?, ?)",
                )
                .bind(follower_id)
                .bind(followee_id)
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
                    "SELECT COUNT(*) FROM user_follows WHERE follower_id = ? AND followee_id = ?",
                )
                .bind(follower_id)
                .bind(followee_id)
                .fetch_one(&mut *tx)
                .await?;
                if dup > 0 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO user_follows (follower_id, followee_id, created_at) VALUES (?, ?, ?)",
                )
                .bind(follower_id)
                .bind(followee_id)
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

/// DELETE /api/v1/users/{username}/follow — 取消关注（幂等）。
async fn unfollow_user(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(username): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "unfollow_user";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let target = user_id_by_username(pool, &username, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("user not found", request_id))?;

    match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM user_follows WHERE follower_id = ? AND followee_id = ?")
                .bind(&user.id)
                .bind(&target)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM user_follows WHERE follower_id = ? AND followee_id = ?")
                .bind(&user.id)
                .bind(&target)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }
    let followers = followers_count(pool, &target, request_id).await?;

    let resp = (
        StatusCode::OK,
        Json(json!({ "following": false, "followers": followers })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// 粉丝/关注列表行投影。
#[derive(sqlx::FromRow)]
struct FollowUserRow {
    username: String,
    display_name: Option<String>,
    level: i64,
    follow_created_at: i64,
}

/// followers / following 共用分页查询。
///
/// `direction = "followers"`：关注目标的人（f.followee_id = target）；
/// `direction = "following"`：目标关注的人（f.follower_id = target）。
async fn list_follow_page(
    pool: &crate::db::DatabasePool,
    target_user_id: &str,
    direction: &str,
    before: Option<i64>,
    limit: i64,
    request_id: &str,
) -> Result<Vec<FollowUserRow>, AppError> {
    let (where_clause, order_col) = if direction == "followers" {
        ("f.followee_id = ?", "f.created_at")
    } else {
        ("f.follower_id = ?", "f.created_at")
    };
    let sql = format!(
        "SELECT u.username_normalized AS username, u.display_name AS display_name,
                u.trust_level AS level, f.created_at AS follow_created_at
         FROM user_follows f
         JOIN users u ON u.id = {}
         WHERE {where_clause} AND u.status <> 'deleted'
           AND (? IS NULL OR f.created_at < ?)
         ORDER BY {order_col} DESC, u.username_normalized ASC
         LIMIT ?",
        if direction == "followers" {
            "f.follower_id"
        } else {
            "f.followee_id"
        }
    );
    let fetch_limit = limit + 1;
    let rows: Vec<FollowUserRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as(&sql)
                .bind(target_user_id)
                .bind(before)
                .bind(before)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as(&sql)
                .bind(target_user_id)
                .bind(before)
                .bind(before)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(rows)
}

/// 粉丝/关注列表 → 响应体（共用投影）。
async fn follow_list_response(
    state: &AppState,
    auth: &AuthSession,
    username: &str,
    direction: &str,
    query: ListFollowQuery,
    request_id: &'static str,
) -> Result<Response, AppError> {
    let _ = auth;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let target = user_id_by_username(pool, username, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("user not found", request_id))?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let before = match query.after.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(raw.parse::<i64>().map_err(|_| {
            AppError::bad_request("after must be an integer cursor", request_id, None)
        })?),
    };

    let rows = list_follow_page(pool, &target, direction, before, limit, request_id).await?;
    let has_more = rows.len() as i64 > limit;
    let page: Vec<FollowUserRow> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| r.follow_created_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let items: Vec<Value> = page
        .iter()
        .map(|r| {
            json!({
                "username": r.username,
                "display_name": r.display_name,
                "level": r.level,
                "created_at": r.follow_created_at,
            })
        })
        .collect();

    let resp = (
        StatusCode::OK,
        Json(json!({ "items": items, "next_cursor": next_cursor })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// GET /api/v1/users/{username}/followers — 粉丝列表（keyset DESC）。
async fn list_followers(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(username): Path<String>,
    Query(query): Query<ListFollowQuery>,
) -> Result<Response, AppError> {
    follow_list_response(
        &state,
        &auth,
        &username,
        "followers",
        query,
        "list_followers",
    )
    .await
}

/// GET /api/v1/users/{username}/following — 关注列表（keyset DESC）。
async fn list_following(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(username): Path<String>,
    Query(query): Query<ListFollowQuery>,
) -> Result<Response, AppError> {
    follow_list_response(
        &state,
        &auth,
        &username,
        "following",
        query,
        "list_following",
    )
    .await
}

/// 按 slug 查启用板块 id；不存在 → None。
async fn board_id_by_slug(
    pool: &crate::db::DatabasePool,
    slug: &str,
    request_id: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT id FROM boards WHERE slug = ? AND is_active = 1 AND deleted_at IS NULL",
            )
            .bind(slug)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT id FROM boards WHERE slug = ? AND is_active = 1 AND deleted_at IS NULL",
            )
            .bind(slug)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row)
}

/// POST /api/v1/boards/{slug}/follow — 关注板块（复合主键幂等）。
async fn follow_board(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(slug): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "follow_board";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let board_id = board_id_by_slug(pool, &slug, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("board not found", request_id))?;

    // 幂等：复合主键 (user_id, board_id)——事务内查重后插入（跨方言，
    // 不用 INSERT OR IGNORE / INSERT IGNORE 方言语法）。
    insert_board_follow(pool, &user.id, &board_id, request_id).await?;

    let resp = (StatusCode::OK, Json(json!({ "following": true }))).into_response();
    Ok(private_no_store(resp))
}

/// 事务内「查重 + 插入」板块关注行；返回是否本次新插入。
async fn insert_board_follow(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    board_id: &str,
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
                    "SELECT COUNT(*) FROM board_follows WHERE user_id = ? AND board_id = ?",
                )
                .bind(user_id)
                .bind(board_id)
                .fetch_one(&mut *conn)
                .await?;
                if dup > 0 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO board_follows (user_id, board_id, created_at) VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(board_id)
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
                    "SELECT COUNT(*) FROM board_follows WHERE user_id = ? AND board_id = ?",
                )
                .bind(user_id)
                .bind(board_id)
                .fetch_one(&mut *tx)
                .await?;
                if dup > 0 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO board_follows (user_id, board_id, created_at) VALUES (?, ?, ?)",
                )
                .bind(user_id)
                .bind(board_id)
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

/// DELETE /api/v1/boards/{slug}/follow — 取消关注板块（幂等）。
async fn unfollow_board(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(slug): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "unfollow_board";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let board_id = board_id_by_slug(pool, &slug, request_id)
        .await?
        .ok_or_else(|| AppError::not_found("board not found", request_id))?;

    match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM board_follows WHERE user_id = ? AND board_id = ?")
                .bind(&user.id)
                .bind(&board_id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM board_follows WHERE user_id = ? AND board_id = ?")
                .bind(&user.id)
                .bind(&board_id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let resp = (StatusCode::OK, Json(json!({ "following": false }))).into_response();
    Ok(private_no_store(resp))
}

/// GET /api/v1/me/following — 本人关注汇总（用户名 + 板块 slug，各 200）。
async fn list_my_following(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "list_my_following";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let users: Vec<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT u.username_normalized FROM user_follows f
                 JOIN users u ON u.id = f.followee_id
                 WHERE f.follower_id = ? AND u.status <> 'deleted'
                 ORDER BY f.created_at DESC LIMIT ?",
            )
            .bind(&user.id)
            .bind(MY_FOLLOWING_LIMIT)
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT u.username_normalized FROM user_follows f
                 JOIN users u ON u.id = f.followee_id
                 WHERE f.follower_id = ? AND u.status <> 'deleted'
                 ORDER BY f.created_at DESC LIMIT ?",
            )
            .bind(&user.id)
            .bind(MY_FOLLOWING_LIMIT)
            .fetch_all(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let boards: Vec<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT b.slug FROM board_follows bf
                 JOIN boards b ON b.id = bf.board_id
                 WHERE bf.user_id = ?
                 ORDER BY bf.created_at DESC LIMIT ?",
            )
            .bind(&user.id)
            .bind(MY_FOLLOWING_LIMIT)
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT b.slug FROM board_follows bf
                 JOIN boards b ON b.id = bf.board_id
                 WHERE bf.user_id = ?
                 ORDER BY bf.created_at DESC LIMIT ?",
            )
            .bind(&user.id)
            .bind(MY_FOLLOWING_LIMIT)
            .fetch_all(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (
        StatusCode::OK,
        Json(json!({ "users": users, "boards": boards })),
    )
        .into_response();
    Ok(private_no_store(resp))
}
