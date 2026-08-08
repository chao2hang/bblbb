use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
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
    content::comments::service::{
        comment_json, create_comment as service_create_comment, list_comments_page,
        load_comment_projection, validate_parent_scope, CommentCursor, CreateCommentError,
        CreateCommentInput,
    },
    content::posts::command::{validate_post_create, CreatePostInput},
    content::posts::publish::PublishBlocked,
    content::posts::service::{edit_post, publish_new_post, EditPostInput, PublishError},
    content::visibility::cache::cache_headers_for,
    content::visibility::evaluate::{
        evaluate, post_grant_key, AccessContent, Actor, DbGrantLookup, EvaluateContext,
    },
    content::visibility::projection::{project_post, PostFields},
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
        .route("/api/v1/posts/{id}/revisions", get(list_post_revisions))
        .route(
            "/api/v1/posts/{id}/revisions/{revision_id}",
            get(get_post_revision),
        )
        .route("/api/v1/posts/{id}/reactions", post(toggle_reaction))
        .route(
            "/api/v1/posts/{id}/reactions/{reaction}",
            delete(delete_post_reaction),
        )
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
    markdown: String,
    #[serde(default)]
    parent_id: Option<String>,
    client_request_id: String,
}

#[derive(Deserialize)]
struct ListQuery {
    /// keyset 游标（`base64url("floor:id")`，M04-COMMENTS-04）。
    #[serde(default)]
    after: Option<String>,
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

/// POST /api/v1/posts — 即时/定时发布新帖（M04-POSTS-06/10，幂等）。
///
/// 服务端权威流程：auth → 权限 → `validate_post_create` 字段校验 → 读取作者
/// 等级 → 幂等门（scope `post.create`，key=client_request_id，同 key+摘要重放
/// 返回原帖、不同摘要 409）→ [`publish_new_post`]（再次预检 + 事务写
/// posts/post_contents/post_revisions + 板块计数 + 搜索索引 Job）。
async fn create_post(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Bytes,
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

    let req: CreatePostRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let hash = crate::idempotency::request_hash(&body);

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
            client_request_id: req.client_request_id.clone(),
        },
        author_level,
        now_millis(),
    )
    .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;

    // 幂等门（M04-POSTS-10 重复请求）
    let idem_key = crate::idempotency::IdempotencyKey::new("post.create", &req.client_request_id)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let outcome = crate::idempotency::begin_or_replay(
        pool,
        &idem_key,
        &hash,
        24 * 60 * 60 * 1000,
        crate::idempotency::FailureCachePolicy::Cache,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match outcome {
        crate::idempotency::IdempotencyOutcome::Created { record_id } => {
            let published = publish_new_post(pool, &cmd, &user.id, now_millis())
                .await
                .map_err(map_publish_error)?;
            let _ = crate::idempotency::complete(pool, &record_id, &published.post.id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok((
                StatusCode::CREATED,
                Json(post_created_json(&published.post)),
            ))
        }
        crate::idempotency::IdempotencyOutcome::Replay { response_reference } => {
            // 同 key+摘要重放：返回原帖（按引用读取）
            if let Some(post_id) = response_reference {
                if let Ok(Some(post)) = get_post_by_id(pool, &post_id, request_id).await {
                    return Ok((StatusCode::CREATED, Json(post_created_json(&post))));
                }
            }
            Err(AppError::conflict(
                "idempotent replay but original post not found",
                request_id,
            ))
        }
        crate::idempotency::IdempotencyOutcome::InProgress => Err(AppError::conflict(
            "request already in progress",
            request_id,
        )),
        crate::idempotency::IdempotencyOutcome::Conflict => Err(AppError::conflict(
            "idempotency key reused with different request",
            request_id,
        )),
        crate::idempotency::IdempotencyOutcome::Failed { .. } => Err(AppError::conflict(
            "previous attempt failed; retry with a new idempotency key",
            request_id,
        )),
    }
}

fn post_created_json(post: &crate::content::model::Post) -> Value {
    json!({
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
    })
}

async fn get_post_by_id(
    pool: &DatabasePool,
    post_id: &str,
    request_id: &'static str,
) -> Result<Option<crate::content::model::Post>, AppError> {
    crate::content::repository::get_post(pool, post_id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))
}

/// 发布错误 → Problem detail（预检阻断 → 409/403，其余 400/404/500）。
///
/// M04-VISIBILITY-03/04：`VisibilityExceedsLevel` 稳定映射为 422
/// `visibility_level_exceeds_author`（作者等级不足），其余阻断保持 409。
fn map_publish_error(err: PublishError) -> AppError {
    const RID: &str = "create_post";
    match err {
        PublishError::Blocked(PublishBlocked::VisibilityExceedsLevel {
            requested,
            author_level,
        }) => AppError::visibility_level_exceeds_author(
            format!("visibility_level {requested} exceeds author level {author_level}"),
            RID,
        ),
        PublishError::Blocked(b) => AppError::conflict(format!("publish blocked: {b}"), RID),
        PublishError::NotFound(msg) => AppError::not_found(msg, RID),
        PublishError::VersionMismatch { .. } => AppError::conflict(err.to_string(), RID),
        PublishError::Risk(e) => AppError::internal(format!("risk evaluation failed: {e}"), RID),
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

/// GET /api/v1/posts/{id} — 详情投影（M04-VISIBILITY-07/08/09 集成）。
///
/// 统一评估链路：读取帖子 + 访问策略行 → `evaluate(actor, content, ctx)`
/// （after_reply/paid 走 `content_access_grants`，fail-closed）→ 经
/// `project_post` 投影（未解锁时 `body_html`/`excerpt`/附件/高亮等敏感键
/// **完全缺失**，`access_summary`/`capabilities` 恒存在）→ persona 感知
/// 缓存头（public → `public, max-age=60` + `Vary: Cookie` + 稳定 ETag；
/// 其余策略 → `private, no-store`，无 ETag，禁止跨 persona 304 泄漏）。
async fn get_post(
    State(state): State<AppState>,
    auth: AuthSession,
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
                    p.review_status, p.reply_count, p.view_count, p.created_at, p.updated_at, p.last_reply_at,
                    p.pinned_at, p.scheduled_at, p.published_at, p.slug, p.closed_at,
                    u.username_normalized as author_name, u.display_name as author_display_name,
                    u.level as author_level,
                    pol.kind as policy_kind, pol.min_level as policy_min_level,
                    c.body_html, c.excerpt, c.renderer_version
             FROM posts p
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN post_contents c ON c.post_id = p.id
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             WHERE p.id = ? AND p.deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, PostDetailProjection>(
            "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status,
                    p.review_status, p.reply_count, p.view_count, p.created_at, p.updated_at, p.last_reply_at,
                    p.pinned_at, p.scheduled_at, p.published_at, p.slug, p.closed_at,
                    u.username_normalized as author_name, u.display_name as author_display_name,
                    u.level as author_level,
                    pol.kind as policy_kind, pol.min_level as policy_min_level,
                    c.body_html, c.excerpt, c.renderer_version
             FROM posts p
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN post_contents c ON c.post_id = p.id
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             WHERE p.id = ? AND p.deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let Some(r) = row else {
        return Err(AppError::not_found("post not found", request_id));
    };

    // M05-RISK-03/06：pending_review（status='draft' + review_status）只对
    // 作者本人可见，且投影为安全的审核状态（不含举报人/内部 note/规则细节）。
    let pending_review =
        r.status == "draft" && r.review_status.as_deref() == Some("pending_review");
    let requester_is_author = auth.user.as_ref().is_some_and(|u| u.id == r.author_id);
    if !matches!(r.status.as_str(), "published" | "hidden")
        && !(pending_review && requester_is_author)
    {
        return Err(AppError::not_found("post not found", request_id));
    }
    let is_pending_author_view = pending_review && requester_is_author;

    // pending_review 不计数浏览量（内容尚未公开）。
    if !is_pending_author_view {
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
    }

    // ── M04-VISIBILITY：统一评估 + 投影 + persona 缓存头 ──
    let policy =
        crate::domain::posts::AccessPolicy::parse(r.policy_kind.as_deref().unwrap_or("public"))
            .unwrap_or(crate::domain::posts::AccessPolicy::Public);
    let min_level = r
        .policy_min_level
        .map(|lv| lv.clamp(1, i64::from(u32::MAX)) as u32);
    let key = post_grant_key(&id);
    let actor = auth.user.as_ref().map(|u| Actor {
        id: &u.id,
        level: u.level.clamp(1, i64::from(u32::MAX)) as u32,
        username: &u.username,
    });
    let author_level = r.author_level.unwrap_or(1).clamp(1, i64::from(u32::MAX)) as u32;
    let content = AccessContent {
        grant_target_key: Some(&key),
        author_id: Some(&r.author_id),
        policy,
        min_level,
        visibility_level: 1,
        author_level,
    };
    let lookup = DbGrantLookup { pool };
    let ctx = EvaluateContext {
        grants: &lookup,
        now: now_millis(),
        moderator_override: false,
    };
    let grant = evaluate(actor.as_ref(), &content, &ctx).await;

    let fields = PostFields {
        id: r.id,
        title: r.title,
        author_id: r.author_id,
        author_username: r.author_name,
        author_display_name: r.author_display_name,
        author_level: r.author_level.unwrap_or(1),
        post_type: r.post_type,
        status: r.status,
        board_id: r.board_id,
        slug: r.slug,
        reply_count: r.reply_count,
        view_count: r.view_count + 1,
        created_at: r.created_at,
        updated_at: r.updated_at,
        pinned_at: r.pinned_at,
        scheduled_at: r.scheduled_at,
        published_at: r.published_at,
        last_reply_at: r.last_reply_at,
        closed_at: r.closed_at,
        body_html: r.body_html,
        excerpt: r.excerpt,
        attachments: Vec::new(),
        search_highlight: None,
        restricted_html: None,
    };
    let mut body = project_post(fields, grant, author_level);

    // M05-RISK-06：作者查看自己待审帖子 → 投影安全审核状态（只含类别）。
    if is_pending_author_view {
        let reason_category = load_pending_reason_category(pool, &id, request_id).await;
        if let Some(map) = body.as_object_mut() {
            map.insert("status".into(), json!("pending_review"));
            map.insert(
                "review".into(),
                json!({
                    "status": "pending_review",
                    "reason_category": reason_category,
                }),
            );
        }
    }

    if is_pending_author_view {
        // 未公开内容：禁止任何缓存（private, no-store）。
        let mut resp = (StatusCode::OK, Json(body)).into_response();
        resp.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        return Ok(resp);
    }

    let ch = cache_headers_for(&grant, &body.to_string());

    let mut resp = (StatusCode::OK, Json(body)).into_response();
    if let Ok(v) = HeaderValue::from_str(ch.cache_control) {
        resp.headers_mut().insert(header::CACHE_CONTROL, v);
    }
    if let Some(vary) = ch.vary {
        if let Ok(v) = HeaderValue::from_str(vary) {
            resp.headers_mut().insert(header::VARY, v);
        }
    }
    if let Some(etag) = ch.etag {
        if let Ok(v) = HeaderValue::from_str(&etag) {
            resp.headers_mut().insert(header::ETAG, v);
        }
    }
    Ok(resp)
}

#[derive(sqlx::FromRow)]
struct PostDetailProjection {
    id: String,
    board_id: String,
    author_id: String,
    post_type: String,
    title: String,
    status: String,
    review_status: Option<String>,
    reply_count: i64,
    view_count: i64,
    created_at: i64,
    updated_at: i64,
    last_reply_at: Option<i64>,
    pinned_at: Option<i64>,
    scheduled_at: Option<i64>,
    published_at: Option<i64>,
    slug: Option<String>,
    closed_at: Option<i64>,
    author_name: Option<String>,
    author_display_name: Option<String>,
    author_level: Option<i64>,
    policy_kind: Option<String>,
    policy_min_level: Option<i64>,
    body_html: Option<String>,
    excerpt: Option<String>,
}

/// 读取待审帖子的评估 reason category（作者安全投影；缺失 → null）。
async fn load_pending_reason_category(
    pool: &DatabasePool,
    post_id: &str,
    request_id: &'static str,
) -> Option<String> {
    let row: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT reason_category FROM risk_evaluations
             WHERE post_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(post_id)
        .fetch_optional(p)
        .await
        .ok()
        .flatten(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT reason_category FROM risk_evaluations
             WHERE post_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(post_id)
        .fetch_optional(p)
        .await
        .ok()
        .flatten(),
    };
    let _ = request_id;
    row
}

/// 读取帖子作者与状态（revisions 可见性判定用）。
async fn load_post_visibility(
    pool: &DatabasePool,
    id: &str,
    request_id: &'static str,
) -> Result<Option<(String, String)>, AppError> {
    // (author_id, status)
    let row: Option<(String, String)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT author_id, status FROM posts WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT author_id, status FROM posts WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    Ok(row)
}

/// 判定请求者是否可查看修订正文（作者本人 或 post.moderate）。
async fn can_view_revision_body(
    pool: &DatabasePool,
    user_id: &str,
    post_author_id: &str,
    request_id: &'static str,
) -> Result<bool, AppError> {
    if user_id == post_author_id {
        return Ok(true);
    }
    let decision = authorize_action(pool, user_id, "post.moderate", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    Ok(decision.is_allowed())
}

/// GET /api/v1/posts/{id}/revisions — 修订列表（元数据；正文仅作者/管理可见）。
async fn list_post_revisions(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "list_post_revisions";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(
        pool,
        &user.id,
        "post.read_revision",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "post.read_revision permission required",
            request_id,
        ));
    }
    let Some((post_author_id, status)) = load_post_visibility(pool, &id, request_id).await? else {
        return Err(AppError::not_found("post not found", request_id));
    };
    if !matches!(status.as_str(), "published" | "hidden") {
        return Err(AppError::not_found("post not found", request_id));
    }
    let can_body = can_view_revision_body(pool, &user.id, &post_author_id, request_id).await?;

    let revisions = crate::content::repository::list_post_revisions(pool, &id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let items: Vec<Value> = revisions
        .iter()
        .map(|r| {
            let mut v = json!({
                "id": r.id,
                "resource_id": r.post_id,
                "version": r.version,
                "editor": { "id": r.editor_id },
                "reason": r.change_reason,
                "created_at": r.created_at,
            });
            if can_body {
                v["body_html"] = Value::String(r.body_html.clone());
            }
            v
        })
        .collect();
    let body = json!({ "items": items });
    Ok(read_response(body, request_id))
}

/// GET /api/v1/posts/{id}/revisions/{revision_id} — 修订详情（管理查看写审计）。
async fn get_post_revision(
    State(state): State<AppState>,
    auth: AuthSession,
    Path((id, revision_id)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let request_id = "get_post_revision";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(
        pool,
        &user.id,
        "post.read_revision",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "post.read_revision permission required",
            request_id,
        ));
    }
    let Some((post_author_id, status)) = load_post_visibility(pool, &id, request_id).await? else {
        return Err(AppError::not_found("post not found", request_id));
    };
    if !matches!(status.as_str(), "published" | "hidden") {
        return Err(AppError::not_found("post not found", request_id));
    }
    let revision = crate::content::repository::get_post_revision(pool, &revision_id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .filter(|r| r.post_id == id)
        .ok_or_else(|| AppError::not_found("revision not found", request_id))?;

    let is_author = user.id == post_author_id;
    let is_moderator =
        !is_author && can_view_revision_body(pool, &user.id, &post_author_id, request_id).await?;
    // 管理查看写审计（M04-POSTS-11）
    if is_moderator {
        AuditEntry::user_action(&user.id, "post.revision.read")
            .with_target("post_revision", &revision.id)
            .with_effective_role("moderator")
            .record(pool)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    }

    let can_body = is_author || is_moderator;
    let mut v = json!({
        "id": revision.id,
        "resource_id": revision.post_id,
        "version": revision.version,
        "editor": { "id": revision.editor_id },
        "reason": revision.change_reason,
        "created_at": revision.created_at,
    });
    if can_body {
        v["body_html"] = Value::String(revision.body_html);
    }
    Ok(read_response(v, request_id))
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
        // M04-VISIBILITY-04：编辑时作者等级重检被阻断 → 稳定 422
        PublishError::Blocked(PublishBlocked::VisibilityExceedsLevel {
            requested,
            author_level,
        }) => AppError::visibility_level_exceeds_author(
            format!("visibility_level {requested} exceeds author level {author_level}"),
            request_id,
        ),
        PublishError::Blocked(b) => AppError::conflict(format!("edit blocked: {b}"), request_id),
        PublishError::Risk(e) => {
            AppError::internal(format!("risk evaluation failed: {e}"), request_id)
        }
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

/// GET /api/v1/posts/{id}/comments — 列出评论（keyset 分页 + 软删占位，M04-COMMENTS-04）
///
/// 稳定排序 `floor ASC, id ASC`；`after` 为不透明游标 `base64url("floor:id")`
/// （[`CommentCursor`]）；fetch limit+1 判定 `has_more`。软删/隐藏评论返回
/// 占位投影（`body_html:null`，不泄漏正文；占位保留楼层）。匿名可读（OpenAPI
/// `security: *2` = 可选会话）。响应 `Cache-Control: public, max-age=60` + ETag。
async fn list_comments(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Query(query): Query<ListQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_comments";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);
    let after = match query.after.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(CommentCursor::decode(raw).map_err(|_| {
            AppError::bad_request("after must be a valid comment cursor", request_id, None)
        })?),
    };

    // 主题存在性（published/hidden 且未删除），否则 404
    let post_exists: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT 1 FROM posts WHERE id = ? AND status IN ('published', 'hidden') AND deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT 1 FROM posts WHERE id = ? AND status IN ('published', 'hidden') AND deleted_at IS NULL",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    if post_exists != Some(1) {
        return Err(AppError::not_found("post not found", request_id));
    }

    let (rows, has_more) = list_comments_page(pool, &id, after.as_ref(), limit)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let items: Vec<Value> = rows.iter().map(comment_json).collect();
    let next_cursor = if has_more {
        rows.last()
            .map(|r| CommentCursor::new(r.floor, &r.id).encode())
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
    Ok(read_response(body, request_id))
}

/// POST /api/v1/posts/{id}/comments — 创建回复（M04-COMMENTS-01/02/03，幂等）。
///
/// 服务端权威流程（与 `create_post` 同一模式）：auth → 邮箱门 → `comment.create`
/// 权限（含账号状态门）→ 内容/幂等键校验 → 主题 + 板块 + 锁帖（closed_at 即
/// 回复开关）重检 → parent 存在性 + 同主题 + 可见性（status published 且
/// `deleted_at IS NULL`，M04-COMMENTS-02）重检 → 幂等门（scope `comment.create`，
/// 同 key+摘要重放返回原评论、不同摘要 409）→ 事务内原子楼层分配
/// （MAX(floor)+1，UNIQUE 兜底，M04-COMMENTS-03）→ `complete` → 201 +
/// `Cache-Control: private, no-store`。
///
/// 响应满足 OpenAPI Comment 投影；`body_html` 读取时经
/// [`crate::content::markdown::render_and_sanitize`] 计算。
async fn create_comment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, AppError> {
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

    let decision = authorize_action(pool, &user.id, "comment.create", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "comment.create permission required",
            request_id,
        ));
    }

    let req: CreateCommentRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let hash = crate::idempotency::request_hash(&body);

    // 内容校验（domain 层单一维护；契约 1..300000，domain 权威 1..10000）
    let content = CommentContent::parse(&req.markdown)
        .map_err(|detail| AppError::bad_request(detail, request_id, None))?;
    // client_request_id 长度校验（契约 16-200）
    let crl = req.client_request_id.chars().count();
    if !(16..=200).contains(&crl) {
        return Err(AppError::bad_request(
            "client_request_id must be 16-200 characters",
            request_id,
            None,
        ));
    }

    // 主题 + 板块 + 锁帖 重新检查（closed_at 即回复开关）
    let topic: Option<(String, String, Option<i64>, Option<i64>)> = match pool {
        // (board_id, status, closed_at, deleted_at)
        Either::Left(p) => {
            sqlx::query_as("SELECT board_id, status, closed_at, deleted_at FROM posts WHERE id = ?")
                .bind(&id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT board_id, status, closed_at, deleted_at FROM posts WHERE id = ?")
                .bind(&id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };
    let Some((board_id, status, closed_at, deleted_at)) = topic else {
        return Err(AppError::not_found("post not found", request_id));
    };
    if deleted_at.is_some() || !matches!(status.as_str(), "published" | "hidden") {
        return Err(AppError::not_found("post not found", request_id));
    }
    if closed_at.is_some() {
        return Err(AppError::conflict(
            "post is closed for new replies",
            request_id,
        ));
    }
    // 板块启用
    let board_active: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT is_active FROM boards WHERE id = ? AND deleted_at IS NULL")
                .bind(&board_id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT is_active FROM boards WHERE id = ? AND deleted_at IS NULL")
                .bind(&board_id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };
    if board_active != Some(1) {
        return Err(AppError::conflict(
            "board is not accepting replies",
            request_id,
        ));
    }

    // parent 存在性 + 同主题 + 可见性（M04-COMMENTS-02 防跨主题引用泄漏；
    // 隐藏/已删 parent 返回稳定 400，不泄漏 deleted vs hidden）
    if let Some(parent_id) = req.parent_id.as_deref() {
        let parent: Option<(String, String, Option<i64>)> = match pool {
            // (post_id, status, deleted_at)
            Either::Left(p) => {
                sqlx::query_as("SELECT post_id, status, deleted_at FROM comments WHERE id = ?")
                    .bind(parent_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
            }
            Either::Right(p) => {
                sqlx::query_as("SELECT post_id, status, deleted_at FROM comments WHERE id = ?")
                    .bind(parent_id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
            }
        };
        match parent {
            Some((pid, parent_status, parent_deleted_at))
                if pid == id && parent_status == "published" && parent_deleted_at.is_none() =>
            {
                // 同主题断言（复用 Comment::validate_quote_scope）
                validate_parent_scope(&id, &pid)
                    .map_err(|detail| AppError::bad_request(detail, request_id, None))?;
            }
            Some((pid, _, _)) if pid != id => {
                return Err(AppError::bad_request(
                    "parent comment must belong to the same post",
                    request_id,
                    None,
                ))
            }
            _ => {
                return Err(AppError::bad_request(
                    "parent comment not found or not visible",
                    request_id,
                    None,
                ))
            }
        }
    }

    // 幂等门（M04-COMMENTS-01，镜像 create_post 模式）
    let idem_key =
        crate::idempotency::IdempotencyKey::new("comment.create", &req.client_request_id)
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let outcome = crate::idempotency::begin_or_replay(
        pool,
        &idem_key,
        &hash,
        24 * 60 * 60 * 1000,
        crate::idempotency::FailureCachePolicy::Cache,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match outcome {
        crate::idempotency::IdempotencyOutcome::Created { record_id } => {
            let comment_id = uuid::Uuid::now_v7().to_string();
            let now = now_millis();
            let created = service_create_comment(
                pool,
                &CreateCommentInput {
                    comment_id: comment_id.clone(),
                    post_id: &id,
                    author_id: &user.id,
                    parent_id: req.parent_id.as_deref(),
                    markdown: content.as_str(),
                    now,
                },
            )
            .await
            .map_err(|e| match e {
                CreateCommentError::FloorContended => AppError::conflict(
                    "floor allocation raced with a concurrent reply; retry with a new idempotency key",
                    request_id,
                ),
                CreateCommentError::Db(msg) => AppError::internal(msg, request_id),
            })?;
            let _ = crate::idempotency::complete(pool, &record_id, &comment_id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            // 重读投影（含作者卡 display_name/level）组装响应
            let projection = load_comment_projection(pool, &comment_id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .ok_or_else(|| AppError::internal("comment not found after insert", request_id))?;
            let mut resp_body = comment_json(&projection);
            resp_body["floor"] = json!(created.floor);
            Ok(private_no_store_response(
                (StatusCode::CREATED, Json(resp_body)).into_response(),
            ))
        }
        crate::idempotency::IdempotencyOutcome::Replay { response_reference } => {
            // 同 key+摘要重放：返回原评论（按引用读取）
            if let Some(comment_id) = response_reference {
                if let Ok(Some(projection)) = load_comment_projection(pool, &comment_id).await {
                    return Ok(private_no_store_response(
                        (StatusCode::CREATED, Json(comment_json(&projection))).into_response(),
                    ));
                }
            }
            Err(AppError::conflict(
                "idempotent replay but original comment not found",
                request_id,
            ))
        }
        crate::idempotency::IdempotencyOutcome::InProgress => Err(AppError::conflict(
            "request already in progress",
            request_id,
        )),
        crate::idempotency::IdempotencyOutcome::Conflict => Err(AppError::conflict(
            "idempotency key reused with different request",
            request_id,
        )),
        crate::idempotency::IdempotencyOutcome::Failed { .. } => Err(AppError::conflict(
            "previous attempt failed; retry with a new idempotency key",
            request_id,
        )),
    }
}

/// 写响应：`Cache-Control: private, no-store`。
fn private_no_store_response(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// POST /api/v1/posts/{id}/reactions — 切换反应（M07-SHOP-08，user_reactions）
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
    let reaction = "like";
    // toggle：先尝试添加；已存在则移除。
    match crate::reactions::service::add_reaction(pool, &user.id, "post", &id, reaction, false)
        .await
    {
        Ok(summary) => Ok(Json(summary)),
        Err(crate::reactions::ReactionError::AlreadyExists) => {
            crate::reactions::service::remove_reaction(pool, &user.id, "post", &id, reaction)
                .await
                .map(Json)
                .map_err(|e| map_reaction_error(e, request_id))
        }
        Err(e) => Err(map_reaction_error(e, request_id)),
    }
}

/// DELETE /api/v1/posts/{id}/reactions/{reaction} — 移除帖子反应
/// （OpenAPI `delete_posts_id_reactions_reaction_`，与 comments 对齐）。
async fn delete_post_reaction(
    State(state): State<AppState>,
    auth: AuthSession,
    Path((id, reaction)): Path<(String, String)>,
) -> Result<Json<Value>, AppError> {
    let request_id = "delete_posts_id_reactions_reaction";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    crate::reactions::service::remove_reaction(pool, &user.id, "post", &id, &reaction)
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
