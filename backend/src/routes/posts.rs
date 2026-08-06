use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::Digest;
use sqlx::Either;

use crate::{
    app::AppState,
    audit::AuditEntry,
    auth::session::{is_step_up_required_for_session, AuthSession, SESSION_COOKIE_NAME},
    authz::decision::AUTHZ_POLICY_VERSION,
    authz::enforce::authorize_action,
    content::posts::command::{validate_post_create, CreatePostInput},
    content::posts::service::{edit_post, publish_new_post, EditPostInput, PublishError},
    db::DatabasePool,
    domain::{
        comments::CommentContent,
        posts::{PostContent, PostTitle},
    },
    error::AppError,
    outbox::now_millis,
};

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
    r#type: String,
    title: String,
    markdown: String,
    board_id: String,
    #[serde(default)]
    visibility_level: Option<u32>,
    access_policy: String,
    #[serde(default)]
    scheduled_at: Option<i64>,
    client_request_id: String,
}

#[derive(Deserialize)]
struct UpdatePostRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    markdown: Option<String>,
    /// 管理员代改时必填（PostPatch 无此字段，服务端宽松接收）。
    #[serde(default)]
    reason: Option<String>,
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
    /// 作者过滤（作者列表投影，M04-POSTS-07）。
    #[serde(default)]
    author_id: Option<String>,
    #[serde(default)]
    sort: Option<String>,
    /// keyset 游标：上一页最后一条 created_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

/// POST /api/v1/posts — 即时/定时发布新帖（M04-POSTS-06）。
///
/// 服务端权威流程：auth → 权限 → `validate_post_create` 字段校验 → 读取作者
/// 等级 → [`publish_new_post`]（再次预检 + 事务写 posts/post_contents/
/// post_revisions + 板块计数 + 搜索索引 Job）。
async fn create_post(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<CreatePostRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "create_post";
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

    let decision = authorize_action(pool, &user.id, "post.create", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "post.create permission required",
            request_id,
        ));
    }

    let level: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let author_level = level.unwrap_or(1).clamp(1, u32::MAX as i64) as u32;

    let cmd = validate_post_create(
        CreatePostInput {
            post_type: req.r#type,
            title: req.title,
            markdown: req.markdown,
            board_id: req.board_id,
            visibility_level: req.visibility_level,
            access_policy: req.access_policy,
            scheduled_at: req.scheduled_at,
            client_request_id: req.client_request_id,
        },
        author_level,
        now_millis(),
    )
    .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;

    let published = publish_new_post(pool, &cmd, &user.id, now_millis())
        .await
        .map_err(map_publish_error)?;

    let post = &published.post;
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "id": post.id,
            "board_id": post.board_id,
            "author": { "id": post.author_id },
            "post_type": post.post_type.as_str(),
            "title": post.title,
            "status": post.status.as_str(),
            "scheduled_at": post.scheduled_at,
            "published_at": post.published_at,
            "created_at": post.created_at,
            "updated_at": post.updated_at,
        })),
    ))
}

/// 发布错误 → Problem detail（预检阻断 → 409/403，其余 400/404/500）。
fn map_publish_error(err: PublishError) -> AppError {
    const RID: &str = "create_post";
    match err {
        PublishError::Blocked(b) => AppError::conflict(format!("publish blocked: {b}"), RID),
        PublishError::NotFound(msg) => AppError::not_found(msg, RID),
        PublishError::VersionMismatch { .. } => AppError::conflict(err.to_string(), RID),
        PublishError::Db(msg) => AppError::internal(msg, RID),
    }
}

/// GET /api/v1/posts — 列出帖子（cursor/ETag/Cache-Control，M04-POSTS-07）
///
/// keyset 分页：`after` = 上一页最后一条 `created_at`（毫秒，`created_at DESC,
/// id DESC` 排序）；返回 `PostPage{items, page{next_cursor, has_more}}`。
/// 可选项：`board_id`、`author_id`（作者列表）、`sort`（latest/popular）。
async fn list_posts(
    State(state): State<AppState>,
    Query(query): Query<ListPostsQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_posts";
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
    let author_id = query.author_id.as_deref().filter(|s| !s.is_empty());

    let (rows, has_more) = list_posts_page(
        pool,
        query.board_id.as_deref(),
        author_id,
        query.sort.as_deref(),
        after,
        limit,
        request_id,
    )
    .await?;

    let items: Vec<Value> = rows.iter().map(post_summary_json).collect();
    let next_cursor = if has_more {
        rows.last()
            .map(|r| r.created_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let body = json!({
        "items": items,
        "page": { "next_cursor": if next_cursor.is_empty() { Value::Null } else { Value::String(next_cursor) }, "has_more": has_more },
    });
    Ok(read_response(body, request_id))
}

/// 帖子列表行（不含正文；fetch limit+1 判断 has_more）。
#[derive(sqlx::FromRow)]
struct PostListRow {
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
}

fn post_summary_json(p: &PostListRow) -> Value {
    json!({
        "id": p.id,
        "board_id": p.board_id,
        "author": { "id": p.author_id, "username": p.author_name },
        "post_type": p.post_type,
        "title": p.title,
        "status": p.status,
        "reply_count": p.reply_count,
        "view_count": p.view_count,
        "pinned_at": p.pinned_at,
        "created_at": p.created_at,
        "updated_at": p.updated_at,
        "last_reply_at": p.last_reply_at,
    })
}

/// keyset 分页查询（published 帖子；cursor=created_at）。
async fn list_posts_page(
    pool: &DatabasePool,
    board_id: Option<&str>,
    author_id: Option<&str>,
    sort: Option<&str>,
    after: Option<i64>,
    limit: i64,
    request_id: &'static str,
) -> Result<(Vec<PostListRow>, bool), AppError> {
    let order = match sort {
        Some("popular") => "p.view_count DESC, p.reply_count DESC, p.id DESC",
        _ => "p.created_at DESC, p.id DESC",
    };
    let sql = format!(
        "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status,
                p.reply_count, p.view_count, p.created_at, p.updated_at, p.last_reply_at,
                p.pinned_at, u.username_normalized as author_name
         FROM posts p
         LEFT JOIN users u ON u.id = p.author_id
         WHERE p.status = 'published' AND p.deleted_at IS NULL
           AND (? IS NULL OR p.board_id = ?)
           AND (? IS NULL OR p.author_id = ?)
           AND (? IS NULL OR p.created_at < ?)
         ORDER BY {} LIMIT ?",
        order
    );
    let fetch_limit = limit + 1;
    let rows: Vec<PostListRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, PostListRow>(&sql)
            .bind(board_id)
            .bind(board_id)
            .bind(author_id)
            .bind(author_id)
            .bind(after)
            .bind(after)
            .bind(fetch_limit)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, PostListRow>(&sql)
            .bind(board_id)
            .bind(board_id)
            .bind(author_id)
            .bind(author_id)
            .bind(after)
            .bind(after)
            .bind(fetch_limit)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let has_more = rows.len() as i64 > limit;
    let rows = rows.into_iter().take(limit as usize).collect();
    Ok((rows, has_more))
}

/// 只读响应：Cache-Control + ETag（M04-POSTS-07）。
fn read_response(body: Value, _request_id: &'static str) -> Response {
    let etag = format!("\"read-{}\"", sha2_short(&body.to_string()));
    let mut resp = (StatusCode::OK, Json(body)).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    if let Ok(v) = HeaderValue::from_str(&etag) {
        resp.headers_mut().insert(header::ETAG, v);
    }
    resp
}

/// 轻量响应摘要（ETag 用；非安全相关）。
fn sha2_short(input: &str) -> String {
    let mut hasher = <sha2::Sha256 as Digest>::new();
    hasher.update(input.as_bytes());
    let out = hasher.finalize();
    out[..6].iter().map(|b| format!("{b:02x}")).collect()
}

/// GET /api/v1/posts/{id} — 详情投影（正文 + access_summary + ETag/Cache-Control）
async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "get_post";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let row: Option<PostDetailProjection> = match pool {
        Either::Left(p) => sqlx::query_as::<_, PostDetailProjection>(
            "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status,
                    p.reply_count, p.view_count, p.created_at, p.updated_at, p.last_reply_at,
                    p.pinned_at, p.scheduled_at, p.published_at, p.slug,
                    u.username_normalized as author_name,
                    c.body_html, c.excerpt, c.renderer_version
             FROM posts p
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN post_contents c ON c.post_id = p.id
             WHERE p.id = ? AND p.status IN ('published', 'hidden') AND p.deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, PostDetailProjection>(
            "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status,
                    p.reply_count, p.view_count, p.created_at, p.updated_at, p.last_reply_at,
                    p.pinned_at, p.scheduled_at, p.published_at, p.slug,
                    u.username_normalized as author_name,
                    c.body_html, c.excerpt, c.renderer_version
             FROM posts p
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN post_contents c ON c.post_id = p.id
             WHERE p.id = ? AND p.status IN ('published', 'hidden') AND p.deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let Some(r) = row else {
        return Err(AppError::not_found("post not found", request_id));
    };

    // 增加浏览量（非关键路径，失败忽略）
    match pool {
        Either::Left(p) => {
            let _ = sqlx::query("UPDATE posts SET view_count = view_count + 1 WHERE id = ?")
                .bind(&id)
                .execute(p)
                .await;
        }
        Either::Right(p) => {
            let _ = sqlx::query("UPDATE posts SET view_count = view_count + 1 WHERE id = ?")
                .bind(&id)
                .execute(p)
                .await;
        }
    }

    let body = json!({
        "id": r.id,
        "board_id": r.board_id,
        "author": { "id": r.author_id, "username": r.author_name },
        "post_type": r.post_type,
        "title": r.title,
        "status": r.status,
        "slug": r.slug,
        "body_html": r.body_html,
        "excerpt": r.excerpt,
        "access_summary": { "policy": "public", "unlocked": true },
        "capabilities": [],
        "reply_count": r.reply_count,
        "view_count": r.view_count + 1,
        "pinned_at": r.pinned_at,
        "scheduled_at": r.scheduled_at,
        "published_at": r.published_at,
        "created_at": r.created_at,
        "updated_at": r.updated_at,
        "last_reply_at": r.last_reply_at,
    });
    Ok(read_response(body, request_id))
}

#[derive(sqlx::FromRow)]
struct PostDetailProjection {
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
    scheduled_at: Option<i64>,
    published_at: Option<i64>,
    slug: Option<String>,
    author_name: Option<String>,
    body_html: Option<String>,
    excerpt: Option<String>,
}

/// PATCH /api/v1/posts/{id} — 编辑帖子（不可变 revision；管理员代改需 reason+recent-auth+审计，M04-POSTS-08）
async fn update_post(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    headers: axum::http::HeaderMap,
    Json(req): Json<UpdatePostRequest>,
) -> Result<Response, AppError> {
    let request_id = "update_post";
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

    // If-Match 版本校验
    let expected_version: i64 = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header required", request_id, None))?
        .parse()
        .map_err(|_| {
            AppError::bad_request("If-Match must be an integer version", request_id, None)
        })?;

    // 加载帖子（含作者）
    let post = get_post_row(pool, &id, request_id).await?;
    let post = post.ok_or_else(|| AppError::not_found("post not found", request_id))?;

    // 字段校验（PATCH 语义：仅当提供时校验）
    let title = req
        .title
        .as_deref()
        .map(PostTitle::parse)
        .transpose()
        .map_err(|detail| AppError::bad_request(detail, request_id, None))?;
    let markdown = req
        .markdown
        .as_deref()
        .map(PostContent::parse)
        .transpose()
        .map_err(|detail| AppError::bad_request(detail, request_id, None))?;

    // 权限判定：作者本人 → post.edit_own；他人 → 管理员代改（post.moderate）
    let (post_author_id, _post_status, _post_version, _post_updated_at) = post;
    let is_owner = post_author_id == user.id;
    if !is_owner {
        let decision =
            authorize_action(pool, &user.id, "post.moderate", None, AUTHZ_POLICY_VERSION)
                .await
                .map_err(|e| AppError::internal(e, request_id))?;
        if !decision.is_allowed() {
            return Err(AppError::forbidden(
                "post.moderate permission required for delegated edit",
                request_id,
            ));
        }
        // 代改必填 reason
        let reason = req.reason.as_deref().unwrap_or("").trim();
        if reason.is_empty() {
            return Err(AppError::bad_request(
                "reason is required for delegated post edit",
                request_id,
                None,
            ));
        }
        // recent-auth（step-up，5 分钟窗口）
        let session_token = session_token_from_headers(&headers)
            .ok_or_else(|| AppError::unauthorized("authentication required", request_id))?;
        let step_up =
            is_step_up_required_for_session(pool, &session_token, state.config.step_up_window_secs)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        if step_up {
            return Err(AppError::step_up_required(request_id));
        }
        // 审计：代改记录（reason/effective_role）
        AuditEntry::delegated_admin_action(
            &user.id,
            "moderator",
            "post.update",
            "post",
            &id,
            reason,
        )
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    }

    let refreshed = edit_post(
        pool,
        &id,
        &user.id,
        &EditPostInput {
            title: title.as_ref().map(|t| t.to_string()),
            markdown: markdown.as_ref().map(|c| c.to_string()),
            expected_version,
            change_reason: req.reason.clone(),
        },
        now_millis(),
    )
    .await
    .map_err(|e| match e {
        PublishError::VersionMismatch { .. } => {
            AppError::version_conflict(e.to_string(), request_id)
        }
        PublishError::NotFound(msg) => AppError::not_found(msg, request_id),
        PublishError::Blocked(b) => AppError::conflict(format!("edit blocked: {b}"), request_id),
        PublishError::Db(msg) => AppError::internal(msg, request_id),
    })?;

    let body = json!({
        "id": refreshed.id,
        "title": refreshed.title,
        "status": refreshed.status.as_str(),
        "version": refreshed.version,
        "updated_at": refreshed.updated_at,
    });
    let mut resp = (StatusCode::OK, Json(body)).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(resp)
}

/// 从 Cookie 头提取会话 token（step-up 判定用）。
fn session_token_from_headers(headers: &axum::http::HeaderMap) -> Option<String> {
    let cookie = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie.split(';').find_map(|part| {
        let (k, v) = part.trim().split_once('=')?;
        if k == SESSION_COOKIE_NAME {
            Some(v.to_string())
        } else {
            None
        }
    })
}

/// 读取帖子元数据行（含作者）。
async fn get_post_row(
    pool: &DatabasePool,
    id: &str,
    request_id: &'static str,
) -> Result<Option<(String, String, i64, i64)>, AppError> {
    // (author_id, status, version, updated_at)
    let row: Option<(String, String, i64, i64)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT author_id, status, version, updated_at FROM posts WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT author_id, status, version, updated_at FROM posts WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let Some((author_id, status, version, updated_at)) = row else {
        return Ok(None);
    };
    Ok(Some((author_id, status, version, updated_at)))
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

    // 领域校验：评论内容规则在 domain 层单一维护
    let content = CommentContent::parse(&req.content)
        .map_err(|detail| AppError::bad_request(detail, request_id, None))?;

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
            .bind(content.as_str())
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
            .bind(content.as_str())
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
