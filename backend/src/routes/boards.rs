use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
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
use crate::db::DatabasePool;
use crate::{app::AppState, error::AppError};

/// Board 公开投影字段（匿名请求恒不含；M03-BOARDS-08 防计数/面包屑推断）：
/// 板块自身的可见性/发帖模式/计数只对已认证请求方暴露。
/// icon 为装饰性视觉标识（非敏感），匿名同样投影；authed 计数追加
/// today_post_count（UTC 日界线内新帖）。
fn board_json(b: &BoardRow, authed: bool, visible_parents: &[String]) -> Value {
    let mut v = json!({
        "id": b.id,
        "version": b.updated_at,
        "created_at": b.created_at,
        "updated_at": b.updated_at,
        "slug": b.slug,
        "name": b.name,
        "description": b.description,
        "icon": b.icon,
    });
    if authed {
        v["visibility"] = json!(b.visibility);
        v["posting_mode"] = json!(b.posting_mode);
        v["post_count"] = json!(b.post_count);
        v["is_active"] = json!(1);
        // 面包屑：仅当父板块对请求方可见时才暴露 parent_id——
        // 隐藏父板块绝不通过可见子板块泄漏存在性。
        if let Some(pid) = &b.parent_id {
            if visible_parents.contains(pid) {
                v["parent_id"] = json!(pid);
            }
        }
    }
    v
}

/// 板块路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/boards", get(list_boards))
        .route("/api/v1/boards/{slug}", get(get_board))
        .route("/api/v1/boards/{slug}/posts", get(list_board_posts))
        .route("/api/v1/tags", get(list_tags))
        .route("/api/v1/tags/{slug}/posts", get(list_tag_posts))
}

#[derive(Deserialize)]
struct ListQuery {
    /// 游标分页（OpenAPI `After`；不透明，最后一条返回项的排序键编码）
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
    /// 帖子列表排序（GAP-FIX 筛选补齐 + M18-BOARD-01 原型对齐）：
    /// latest（默认，创建时间倒序）| hot（view_count 倒序）|
    /// featured（精华优先，featured_at 非空在前）|
    /// unanswered（未回复，reply_count=0）——仅 /boards/{slug}/posts 消费。
    #[serde(default)]
    sort: Option<String>,
    /// 作者/标题过滤（M18-BOARD-01，原型「按作者或标题筛选…」）：
    /// 对 title 与作者 display_name/username 做 LIKE 匹配，仅 /boards/{slug}/posts 消费。
    #[serde(default)]
    q: Option<String>,
}

/// LIKE 通配符转义（M18-BOARD-01；同 admin_ext::like_escape 约定，
/// ESCAPE '!'，SQLite/MySQL/MariaDB 行为一致）。
fn like_escape(input: &str) -> String {
    input
        .replace('!', "!!")
        .replace('%', "!%")
        .replace('_', "!_")
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
                "SELECT id, slug, name, description, icon, parent_id, sort_order, visibility, posting_mode, post_count, created_at, updated_at
                 FROM boards WHERE is_active = 1 AND deleted_at IS NULL
                 ORDER BY sort_order ASC, created_at ASC, id ASC",
            )
            .fetch_all(p)
            .await,
            Either::Right(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, icon, parent_id, sort_order, visibility, posting_mode, post_count, created_at, updated_at
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
    // GAP-FIX 社交域：today_post_count（UTC 日界线内新帖，authed 计数）。
    let today_counts = if actor.is_some() {
        load_today_post_counts(pool, now_millis_utc_day_start(), request_id).await?
    } else {
        HashMap::new()
    };
    let mut items: Vec<Value> = Vec::new();
    let mut last_key: Option<BoardCursor> = None;
    let mut more_after_full = false;
    let authed = actor.is_some();
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
        items.push(board_json(b, authed, &visible));
        last_key = Some(key);
    }
    let has_more = more_after_full;
    let next_cursor = if has_more { last_key } else { None };

    // today_post_count 注入（authed 时；与 post_count 同一暴露策略）。
    if authed {
        for item in items.iter_mut() {
            let board_id = item["id"].as_str().unwrap_or_default().to_string();
            let count = today_counts.get(&board_id).copied().unwrap_or(0);
            if let Some(map) = item.as_object_mut() {
                map.insert("today_post_count".into(), json!(count));
            }
        }
    }

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
                "SELECT id, slug, name, description, icon, parent_id, sort_order, visibility, posting_mode, post_count, created_at, updated_at
                 FROM boards WHERE slug = ? AND is_active = 1 AND deleted_at IS NULL",
            )
            .bind(&slug)
            .fetch_optional(p)
            .await,
            Either::Right(p) => sqlx::query_as::<_, BoardRow>(
                "SELECT id, slug, name, description, icon, parent_id, sort_order, visibility, posting_mode, post_count, created_at, updated_at
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

    // 面包屑：parent_id 仅在父板块对请求方可见时暴露（防隐藏父级泄漏）。
    let mut visible_parents: Vec<String> = Vec::new();
    if let Some(pid) = &b.parent_id {
        let parent_visible =
            parent_board_visible(pool, pid, auth.user.as_ref().map(|u| u.id.as_str()))
                .await
                .map_err(|e| AppError::internal(e, request_id))?;
        if parent_visible {
            visible_parents.push(pid.clone());
        }
    }

    let cache = if visibility == BoardVisibility::Public {
        CACHE_PUBLIC
    } else {
        CACHE_PRIVATE
    };
    let mut body = board_json(&b, auth.user.is_some(), &visible_parents);
    // GAP-FIX 社交域：today_post_count（authed 计数，与 post_count 同策略）。
    if auth.user.is_some() {
        let today_count = board_today_post_count(pool, &b.id, request_id).await?;
        if let Some(map) = body.as_object_mut() {
            map.insert("today_post_count".into(), json!(today_count));
        }
    }
    Ok(([(header::CACHE_CONTROL, cache)], Json(body)).into_response())
}

/// UTC 今日零点（毫秒）：today_post_count 聚合窗口下界。
fn now_millis_utc_day_start() -> i64 {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    now - now.rem_euclid(86_400_000)
}

/// 批量读取各板块今日新帖数（published 且未删除；UTC 日界线）。
///
/// 一次 GROUP BY 聚合全表（与 list_boards 全量加载同一量级），避免逐板块
/// N+1；返回 {board_id: count}。
async fn load_today_post_counts(
    pool: &DatabasePool,
    since: i64,
    request_id: &str,
) -> Result<HashMap<String, i64>, AppError> {
    let sql = "SELECT board_id, COUNT(*) FROM posts
               WHERE created_at >= ? AND status = 'published' AND deleted_at IS NULL
               GROUP BY board_id";
    let rows: Vec<(String, i64)> = match pool {
        Either::Left(p) => sqlx::query_as(sql).bind(since).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as(sql).bind(since).fetch_all(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(rows.into_iter().collect())
}

/// 单板块今日新帖数（get_board 详情用）。
async fn board_today_post_count(
    pool: &DatabasePool,
    board_id: &str,
    request_id: &str,
) -> Result<i64, AppError> {
    let since = now_millis_utc_day_start();
    let count: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM posts
                 WHERE board_id = ? AND created_at >= ? AND status = 'published' AND deleted_at IS NULL",
            )
            .bind(board_id)
            .bind(since)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM posts
                 WHERE board_id = ? AND created_at >= ? AND status = 'published' AND deleted_at IS NULL",
            )
            .bind(board_id)
            .bind(since)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(count)
}

/// 父板块是否对请求方可见（用于面包屑字段；隐藏/不可见父级不暴露）。
async fn parent_board_visible(
    pool: &DatabasePool,
    parent_id: &str,
    actor: Option<&str>,
) -> Result<bool, String> {
    let row: Option<(String,)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT visibility FROM boards WHERE id = ? AND is_active = 1 AND deleted_at IS NULL",
        )
        .bind(parent_id)
        .fetch_optional(p)
        .await,
        Either::Right(p) => sqlx::query_as(
            "SELECT visibility FROM boards WHERE id = ? AND is_active = 1 AND deleted_at IS NULL",
        )
        .bind(parent_id)
        .fetch_optional(p)
        .await,
    }
    .map_err(|e| e.to_string())?;

    let Some((vis,)) = row else {
        return Ok(false);
    };
    let Some(visibility) = BoardVisibility::parse(&vis) else {
        return Ok(false);
    };
    let access = board_read_gate(pool, parent_id, visibility, actor).await?;
    Ok(access.visible)
}

/// GET /api/v1/boards/{slug}/posts — 板块帖子列表（cursor/ETag/Cache-Control，M04-POSTS-07）
async fn list_board_posts(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_board_posts";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 100);
    // keyset 游标：上一页最后一条 created_at（毫秒）
    let after = match query.after.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(raw.parse::<i64>().map_err(|_| {
            AppError::bad_request("after must be an integer cursor", request_id, None)
        })?),
    };

    // 板块存在 + 未删除
    let board_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT id FROM boards WHERE slug = ? AND deleted_at IS NULL")
                .bind(&slug)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT id FROM boards WHERE slug = ? AND deleted_at IS NULL")
                .bind(&slug)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let board_id = board_id.ok_or_else(|| AppError::not_found("board not found", request_id))?;

    let fetch_limit = limit + 1;
    // 排序（M18-BOARD-01 原型对齐；游标仍为 created_at 键，与既有 hot 排序
    // 的处理一致——筛选页翻页深度有限）：
    // - hot：view_count 倒序（GAP-FIX）；
    // - featured：精华优先（featured_at 非空在前），再 created_at 倒序；
    // - unanswered：仅未回复（reply_count=0），created_at 倒序。
    let sort = query.sort.as_deref().unwrap_or("latest");
    let order = match sort {
        "hot" => "p.view_count DESC, p.id DESC",
        "featured" => "p.featured_at IS NOT NULL DESC, p.created_at DESC, p.id DESC",
        _ => "p.created_at DESC, p.id DESC",
    };
    // 作者/标题过滤（M18-BOARD-01，原型「按作者或标题筛选…」）：
    // LIKE + ESCAPE '!'（SQLite/MySQL/MariaDB 行为一致，同 admin_ext 约定）。
    let q = query.q.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let q_like = q.map(|s| format!("%{}%", like_escape(s)));
    let unanswered_clause = if sort == "unanswered" {
        " AND p.reply_count = 0"
    } else {
        ""
    };
    let q_clause = if q_like.is_some() {
        " AND (p.title LIKE ? ESCAPE '!' OR u.display_name LIKE ? ESCAPE '!' OR u.username_normalized LIKE ? ESCAPE '!')"
    } else {
        ""
    };
    let mut sql = String::from(
        "SELECT p.id, p.title, p.author_id, u.username_normalized AS author_name,
                u.display_name AS author_display_name,
                u.avatar_attachment_id AS avatar_attachment_id,
                p.reply_count, p.view_count, p.pinned, p.created_at, p.last_reply_at
         FROM posts p
         LEFT JOIN users u ON u.id = p.author_id
         WHERE p.board_id = ? AND p.status = 'published' AND p.deleted_at IS NULL
           AND (? IS NULL OR p.created_at < ?)",
    );
    sql.push_str(unanswered_clause);
    sql.push_str(q_clause);
    sql.push_str(&format!(" ORDER BY {order} LIMIT ?"));
    let posts = match pool {
        Either::Left(p) => {
            let mut stmt = sqlx::query_as::<_, PostListRow>(&sql)
                .bind(&board_id)
                .bind(after)
                .bind(after);
            if let Some(pattern) = &q_like {
                stmt = stmt.bind(pattern).bind(pattern).bind(pattern);
            }
            stmt.bind(fetch_limit).fetch_all(p).await
        }
        Either::Right(p) => {
            let mut stmt = sqlx::query_as::<_, PostListRow>(&sql)
                .bind(&board_id)
                .bind(after)
                .bind(after);
            if let Some(pattern) = &q_like {
                stmt = stmt.bind(pattern).bind(pattern).bind(pattern);
            }
            stmt.bind(fetch_limit).fetch_all(p).await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = posts.len() as i64 > limit;
    let page: Vec<_> = posts.into_iter().take(limit as usize).collect();
    // 参与者预览（一页一查询）：楼主之外已发布回复的不同作者，每帖 ≤2 个。
    let post_ids: Vec<String> = page.iter().map(|p| p.id.clone()).collect();
    let participants =
        crate::routes::posts::fetch_post_participants(pool, &post_ids, request_id).await?;
    // 作者装扮投影（作者去重后逐个查询）：与首页列表行同构（已装备头像框）。
    let author_ids: Vec<String> = page.iter().map(|p| p.author_id.clone()).collect();
    let author_tokens =
        crate::routes::posts::fetch_author_presentation_tokens(pool, &author_ids).await;
    let empty_participants: Vec<crate::routes::posts::PostParticipantRow> = Vec::new();
    let items: Vec<Value> = page
        .iter()
        .map(|p| {
            let participants_json: Vec<Value> = participants
                .get(&p.id)
                .map(|v| v.as_slice())
                .unwrap_or(&empty_participants)
                .iter()
                .map(|u| {
                    let mut value = json!({
                        "id": u.user_id,
                        "username": u.username,
                        "display_name": u.display_name,
                    });
                    if let Some(attachment_id) = &u.avatar_attachment_id {
                        value["avatar_attachment_id"] = json!(attachment_id);
                    }
                    if let Some(tokens) = &u.presentation_tokens {
                        value["presentation_tokens"] = json!(tokens);
                    }
                    value
                })
                .collect();
            let mut author = json!({
                "id": p.author_id,
                "username": p.author_name,
                "display_name": p.author_display_name,
            });
            if let Some(attachment_id) = &p.avatar_attachment_id {
                author["avatar_attachment_id"] = json!(attachment_id);
            }
            if let Some(tokens) = author_tokens.get(&p.author_id) {
                author["presentation_tokens"] = json!(tokens);
            }
            json!({
                "id": p.id,
                "title": p.title,
                "author": author,
                "participants": participants_json,
                "reply_count": p.reply_count,
                "view_count": p.view_count,
                "pinned": p.pinned != 0,
                "created_at": p.created_at,
                "last_reply_at": p.last_reply_at,
            })
        })
        .collect();
    let next_cursor = if has_more {
        items
            .last()
            .and_then(|v| v["created_at"].as_i64())
            .map(|ts| ts.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let body = json!({
        "items": items,
        "page": {
            "next_cursor": if next_cursor.is_empty() { Value::Null } else { Value::String(next_cursor) },
            "has_more": has_more,
        },
    });

    // ETag + Cache-Control（M04-POSTS-07）
    let mut resp = (StatusCode::OK, Json(body)).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    Ok(resp)
}

/// GET /api/v1/tags — 列出启用标签与标签组（M03-BOARDS-06）
async fn list_tags(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let request_id = "list_tags";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let tags = crate::tags::load_active_tags(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let groups = crate::tags::load_tag_groups(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;

    let items: Vec<Value> = tags
        .iter()
        .map(|t| {
            json!({
                "id": t.id,
                "slug": t.slug,
                "name": t.name,
                "description": t.description,
                "color": t.color,
                "group_id": t.group_id,
                "usage_count": t.usage_count,
            })
        })
        .collect();
    let group_items: Vec<Value> = groups
        .iter()
        .map(|g| {
            json!({
                "id": g.id,
                "name": g.name,
                "slug": g.slug,
                "sort_order": g.sort_order,
            })
        })
        .collect();

    Ok(Json(json!({ "items": items, "groups": group_items })))
}

/// GET /api/v1/tags/{slug}/posts — 标签聚合帖子列表（M18-TAG-01，原型标签详情页）。
///
/// 权限=公开（与 /boards/{slug}/posts 同级）；published + 未删除；
/// created_at 键序游标分页；投影含板块名/slug 与作者手写摘要（原型卡片结构）。
async fn list_tag_posts(
    State(state): State<AppState>,
    Path(slug): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_tag_posts";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 100);
    let after = match query.after.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(raw.parse::<i64>().map_err(|_| {
            AppError::bad_request("after must be an integer cursor", request_id, None)
        })?),
    };

    let fetch_limit = limit + 1;
    let sql = String::from(
        "SELECT p.id, p.title, p.author_id, u.username_normalized AS author_name,
                u.display_name AS author_display_name,
                u.avatar_attachment_id AS avatar_attachment_id,
                p.reply_count, p.view_count, p.summary, p.created_at, p.last_reply_at,
                b.slug AS board_slug, b.name AS board_name
         FROM post_tags pt
         JOIN tags t ON t.id = pt.tag_id
         JOIN posts p ON p.id = pt.post_id
         JOIN boards b ON b.id = p.board_id
         LEFT JOIN users u ON u.id = p.author_id
         WHERE t.slug = ? AND t.is_active = 1
           AND p.status = 'published' AND p.deleted_at IS NULL
           AND b.deleted_at IS NULL
           AND (? IS NULL OR p.created_at < ?)
         ORDER BY p.created_at DESC, p.id DESC
         LIMIT ?",
    );
    let rows = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, TagPostRow>(&sql)
                .bind(&slug)
                .bind(after)
                .bind(after)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, TagPostRow>(&sql)
                .bind(&slug)
                .bind(after)
                .bind(after)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = rows.len() as i64 > limit;
    let author_ids: Vec<String> = rows.iter().map(|r| r.author_id.clone()).collect();
    let author_tokens =
        crate::routes::posts::fetch_author_presentation_tokens(pool, &author_ids).await;
    let items: Vec<Value> = rows
        .into_iter()
        .take(limit as usize)
        .map(|r| {
            let mut author = json!({
                "id": r.author_id,
                "username": r.author_name,
                "display_name": r.author_display_name,
                "avatar_attachment_id": r.avatar_attachment_id,
            });
            if let Some(tokens) = author_tokens.get(author["id"].as_str().unwrap_or_default()) {
                author["presentation_tokens"] = json!(tokens);
            }
            json!({
                "id": r.id,
                "title": r.title,
                "author": author,
                "reply_count": r.reply_count,
                "view_count": r.view_count,
                "summary": r.summary,
                "board": { "slug": r.board_slug, "name": r.board_name },
                "created_at": r.created_at,
                "last_reply_at": r.last_reply_at,
            })
        })
        .collect();
    let next_cursor = if has_more {
        items
            .last()
            .and_then(|v| v["created_at"].as_i64())
            .map(|ts| ts.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let body = json!({
        "items": items,
        "page": {
            "next_cursor": if next_cursor.is_empty() { Value::Null } else { Value::String(next_cursor) },
            "has_more": has_more,
        },
    });
    let mut resp = (StatusCode::OK, Json(body)).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    Ok(resp)
}

/// 标签聚合帖子行（M18-TAG-01）：含板块投影与作者摘要。
#[derive(sqlx::FromRow)]
struct TagPostRow {
    id: String,
    title: String,
    author_id: String,
    author_name: Option<String>,
    /// 作者昵称（users.display_name；前台列表优先显示昵称，缺省回退用户名）。
    author_display_name: Option<String>,
    avatar_attachment_id: Option<String>,
    reply_count: i64,
    view_count: i64,
    summary: Option<String>,
    created_at: i64,
    last_reply_at: Option<i64>,
    board_slug: String,
    board_name: String,
}

/// Board 投影（OpenAPI `Board` = ResourceMeta + slug/name/description）。
///
/// boards 无独立 version 列：以 `updated_at`（Unix 毫秒）为乐观并发版本
/// （≥1 且每次更新递增，BOARDS-05 If-Match 同源）。
#[derive(sqlx::FromRow)]
struct BoardRow {
    id: String,
    slug: String,
    name: String,
    description: Option<String>,
    /// 板块图标（lucide 图标名；NULL = 未设置，前台回退默认视觉）。
    icon: Option<String>,
    parent_id: Option<String>,
    sort_order: i64,
    visibility: String,
    posting_mode: String,
    post_count: i64,
    created_at: i64,
    updated_at: i64,
}

#[derive(sqlx::FromRow)]
struct PostListRow {
    id: String,
    title: String,
    author_id: String,
    author_name: Option<String>,
    /// 作者昵称（users.display_name；前台列表优先显示昵称，缺省回退用户名）。
    author_display_name: Option<String>,
    /// 作者上传头像附件 id（公开引用；参与者列楼主头像渲染用，可空）。
    avatar_attachment_id: Option<String>,
    reply_count: i64,
    view_count: i64,
    pinned: i64,
    created_at: i64,
    last_reply_at: Option<i64>,
}
