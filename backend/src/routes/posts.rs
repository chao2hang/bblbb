use std::collections::{HashMap, HashSet};

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
    content::attachments::{extract_attachment_content_ids, load_unavailable_attachment_ids},
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
        .route(
            "/api/v1/posts/{id}/reactions",
            post(toggle_reaction).get(get_post_reactions),
        )
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
    /// 付费定价（金币；access_policy=paid 时必填 1-1000，否则 422）。
    /// 存 posts.price_coin（0061 列）。
    #[serde(default)]
    price_coin: Option<u32>,
    /// 作者手写摘要（≤300 字符；文章类型才有意义）。存 posts.summary
    /// （0062 列；与 post_contents.excerpt 自动摘录语义不同）。
    #[serde(default)]
    summary: Option<String>,
    /// 标签（slug 或名称，≤8 个、每个 1-32 字符）。写入 post_tags 关联
    /// （0003 既有表，与 GET /posts 的 tag= 筛选同源）。
    #[serde(default)]
    tags: Option<Vec<String>>,
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
    #[serde(default)]
    tags: Option<Vec<String>>,
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
    /// 作者用户名过滤（username_normalized 精确匹配；GAP-FIX 筛选补齐）。
    #[serde(default)]
    author_username: Option<String>,
    /// 排序/过滤：latest（默认）| popular（浏览量）| featured（精华过滤）|
    /// unanswered（无回复过滤）| following（已关注用户，M18-HOME-01 原型对齐）。
    #[serde(default)]
    sort: Option<String>,
    /// 类型过滤（query 参数 `type`）：article | topic——topic 映射既有
    /// post_type='discussion'（0032 CHECK 值域 article/discussion）。
    #[serde(default, rename = "type")]
    type_filter: Option<String>,
    /// 标签过滤（tags.slug 精确匹配，post_tags 关联）。
    #[serde(default)]
    tag: Option<String>,
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

    // GAP-FIX 付费解锁/摘要/标签字段校验（服务端权威）。
    // price_coin：paid 必填 1-1000（422 invalid_price_coin），非 paid 禁带
    // （付费定价只对 paid 策略有意义，避免双源歧义）。
    match req.price_coin {
        Some(p) if req.access_policy == "paid" => {
            if !(1..=1000).contains(&p) {
                return Err(AppError::with_code(
                    axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                    "invalid_price_coin",
                    "Unprocessable Entity",
                    "price_coin must be between 1 and 1000",
                    request_id,
                ));
            }
        }
        Some(_) => {
            return Err(AppError::with_code(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_price_coin",
                "Unprocessable Entity",
                "price_coin is only allowed for paid access policy",
                request_id,
            ));
        }
        None if req.access_policy == "paid" => {
            return Err(AppError::with_code(
                axum::http::StatusCode::UNPROCESSABLE_ENTITY,
                "invalid_price_coin",
                "Unprocessable Entity",
                "price_coin is required for paid access policy",
                request_id,
            ));
        }
        None => {}
    }
    // summary：≤300 字符（trim 后可空；空串按未提供处理）。
    if let Some(summary) = req.summary.as_deref() {
        let trimmed = summary.trim();
        if trimmed.chars().count() > 300 {
            return Err(AppError::bad_request(
                "summary must be at most 300 characters",
                request_id,
                None,
            ));
        }
    }
    // tags：≤8 个，每个 1-32 字符（trim 后；空项直接拒绝，避免静默丢数据）。
    if let Some(tags) = req.tags.as_deref() {
        if tags.len() > 8 {
            return Err(AppError::bad_request(
                "tags must contain at most 8 items",
                request_id,
                None,
            ));
        }
        for tag in tags {
            let len = tag.trim().chars().count();
            if len == 0 || len > 32 {
                return Err(AppError::bad_request(
                    "each tag must be 1-32 characters",
                    request_id,
                    None,
                ));
            }
        }
    }

    let level: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let author_level = level.unwrap_or(0).clamp(0, u32::MAX as i64) as u32;

    // GAP-FIX extras 输入（CreatePostInput 构造会 move req 字段，先取出）。
    let extras = PostExtras {
        access_policy: req.access_policy.clone(),
        visibility_level: req.visibility_level,
        price_coin: req.price_coin,
        summary: req.summary.clone(),
        tags: req.tags.clone(),
    };

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
            // GAP-FIX 付费解锁/摘要/标签落库（发布事务外的补充写：均为
            // 新增可选字段，失败即整体 5xx——帖子行已建，幂等重放会返回
            // 原帖 id，重试同一请求不会再走 Created 分支，因此这里失败必须
            // 显式报错而不是静默丢弃定价/标签）。
            let linked_tags =
                apply_post_extras(pool, &published.post.id, &extras, &user.id, request_id).await?;
            // 成就钩子（best-effort）：post_count 类成就；失败只 warn 不阻断。
            if let Err(e) = crate::achievements::evaluate(pool, &user.id).await {
                tracing::warn!(user_id = %user.id, error = %e, "achievement evaluate failed (post)");
            }
            // 活跃奖励钩子（best-effort，M07-LEVELS）：post 类 activity_rules 规则
            // （管理端「积分规则」页配置）；未配置规则/总闸关闭/每日上限/冷却命中
            // 时引擎返回 NotEligible，静默跳过；失败只 warn 不阻断发帖响应。
            if let Err(e) = crate::economy::activity::service::claim_content_reward(
                pool,
                &user.id,
                "post",
                &published.post.id,
                now_millis(),
            )
            .await
            {
                tracing::warn!(user_id = %user.id, error = %e, "activity content reward failed (post)");
            }
            let mut body = post_created_json(&published.post);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("price_coin".to_string(), serde_json::json!(req.price_coin));
                obj.insert(
                    "summary".to_string(),
                    serde_json::json!(req
                        .summary
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())),
                );
                obj.insert("tags".to_string(), serde_json::json!(linked_tags));
            }
            Ok((StatusCode::CREATED, Json(body)))
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

/// GAP-FIX：CreatePostRequest 的扩展字段（发布事务外的补充写输入）。
struct PostExtras {
    access_policy: String,
    visibility_level: Option<u32>,
    price_coin: Option<u32>,
    summary: Option<String>,
    tags: Option<Vec<String>>,
}

/// GAP-FIX：发布后补充写入 price_coin/summary/标签关联与非公开策略行。
///
/// - posts.price_coin（0061）/ posts.summary（0062）直接 UPDATE；
/// - access_policy!=public 时创建 content_access_policies 行（支持 paid/after_reply/level/logged_in）并回填 posts.access_policy_id——
///   可见性评估（GET /posts/{id} 走 pol.kind）与解锁 grant
///   （content_access_grants.policy_id NOT NULL）都依赖该行；
/// - tags 写入 post_tags 关联并 bump tags.usage_count（详见
///   [`link_post_tags`]）；
/// - 标签写入后重入索引 Job（best-effort：发布事务内的首次入队在标签
///   关联建立之前，search_documents.tags_json 以本次重建为准；索引 Job
///   幂等合并，重复入队无副作用）。
///
/// 失败语义：均为新增可选字段的补充写——失败返回 5xx 而不是静默丢弃
/// （帖子行已建，幂等重放返回原帖 id；调用方需要知道定价/标签是否落库）。
async fn apply_post_extras(
    pool: &DatabasePool,
    post_id: &str,
    extras: &PostExtras,
    author_id: &str,
    request_id: &str,
) -> Result<Vec<String>, AppError> {
    let now = now_millis();
    let price = extras.price_coin.map(i64::from);
    let summary = extras
        .summary
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    // 1) 定价与摘要。
    let update_sql = "UPDATE posts SET price_coin = ?, summary = ? WHERE id = ?";
    match pool {
        Either::Left(p) => sqlx::query(update_sql)
            .bind(price)
            .bind(summary)
            .bind(post_id)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query(update_sql)
            .bind(price)
            .bind(summary)
            .bind(post_id)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    // 2) 访问策略行（可见性评估 + 解锁 grant 的外键来源）。
    if extras.access_policy != "public" {
        let policy_id = uuid::Uuid::now_v7().to_string();
        let (kind, currency, amount, min_level) = match extras.access_policy.as_str() {
            "paid" => (
                "paid",
                Some(crate::economy::ledger::service::CURRENCY_COIN),
                price,
                None,
            ),
            "after_reply" => ("after_reply", None, None, None),
            "logged_in" => ("logged_in", None, None, None),
            "level" => ("level", None, None, extras.visibility_level.map(i64::from)),
            _ => ("public", None, None, None),
        };
        let insert_policy = "INSERT INTO content_access_policies
             (id, kind, min_level, currency_id, amount, reply_grant_persists, policy_version, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, 0, 1, ?, ?)";
        let inserted = match pool {
            Either::Left(p) => sqlx::query(insert_policy)
                .bind(&policy_id)
                .bind(kind)
                .bind(min_level)
                .bind(currency)
                .bind(amount)
                .bind(author_id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected(),
            Either::Right(p) => sqlx::query(insert_policy)
                .bind(&policy_id)
                .bind(kind)
                .bind(min_level)
                .bind(currency)
                .bind(amount)
                .bind(author_id)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected(),
        };
        if inserted == 0 {
            return Err(AppError::internal(
                "failed to create access policy row",
                request_id,
            ));
        }
        let link_sql = "UPDATE posts SET access_policy_id = ? WHERE id = ?";
        match pool {
            Either::Left(p) => sqlx::query(link_sql)
                .bind(&policy_id)
                .bind(post_id)
                .execute(p)
                .await
                .map(|_| ())
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            Either::Right(p) => sqlx::query(link_sql)
                .bind(&policy_id)
                .bind(post_id)
                .execute(p)
                .await
                .map(|_| ())
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        };
    }

    // 3) 标签关联。
    let linked = link_post_tags(pool, post_id, extras.tags.as_deref(), now, request_id).await?;

    // 4) 索引重建（best-effort）。
    if !linked.is_empty() {
        let _ = crate::search::index_job::enqueue_index_job(pool, "post", post_id).await;
    }
    Ok(linked)
}

/// 解析并写入帖子标签关联（GAP-FIX：CreatePostRequest.tags）。
///
/// 输入为标签 slug 或名称（解析口径与 GET /search?tag= 一致：slug 或
/// name 匹配）。只链接存在、启用（is_active=1）且未合并（status IS NULL）
/// 的标签——**未知标签跳过**（标签创建是 tag.manage 管理操作，普通用户
/// 只能选用现有标签；返回值回显实际链接的标签名，前端可据此提示）。
/// 输入去重后写 post_tags（复合主键冲突幂等跳过）并 bump usage_count。
async fn link_post_tags(
    pool: &DatabasePool,
    post_id: &str,
    tags: Option<&[String]>,
    now: i64,
    request_id: &str,
) -> Result<Vec<String>, AppError> {
    let Some(tags) = tags else {
        return Ok(Vec::new());
    };
    let mut linked_ids: Vec<String> = Vec::new();
    let mut linked_names: Vec<String> = Vec::new();
    for (idx, raw) in tags.iter().enumerate() {
        let tag = raw.trim();
        if tag.is_empty() {
            continue;
        }
        let row: Option<(String, String)> = match pool {
            Either::Left(p) => sqlx::query_as(
                "SELECT id, name FROM tags
                     WHERE is_active = 1 AND status IS NULL AND (slug = ? OR name = ?) LIMIT 1",
            )
            .bind(tag)
            .bind(tag)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            Either::Right(p) => sqlx::query_as(
                "SELECT id, name FROM tags
                     WHERE is_active = 1 AND status IS NULL AND (slug = ? OR name = ?) LIMIT 1",
            )
            .bind(tag)
            .bind(tag)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        };
        let Some((tag_id, name)) = row else {
            continue;
        };
        if linked_ids.contains(&tag_id) {
            continue;
        }
        let insert_sql =
            "INSERT OR IGNORE INTO post_tags (post_id, tag_id, created_at) VALUES (?, ?, ?)";
        let bump_sql = "UPDATE tags SET usage_count = usage_count + 1 WHERE id = ?";
        let tag_now = now + idx as i64;
        match pool {
            Either::Left(p) => {
                sqlx::query(insert_sql)
                    .bind(post_id)
                    .bind(&tag_id)
                    .bind(tag_now)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                sqlx::query(bump_sql)
                    .bind(&tag_id)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT IGNORE INTO post_tags (post_id, tag_id, created_at) VALUES (?, ?, ?)",
                )
                .bind(post_id)
                .bind(&tag_id)
                .bind(tag_now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                sqlx::query(bump_sql)
                    .bind(&tag_id)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
        }
        linked_ids.push(tag_id);
        linked_names.push(name);
    }
    Ok(linked_names)
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
/// 可选项：`board_id`、`author_id`/`author_username`（作者列表）、`sort`
/// （latest/popular/featured/unanswered）、`type`（article/topic）、`tag`（slug）。
async fn list_posts(
    State(state): State<AppState>,
    auth: AuthSession,
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
    // 类型映射：topic → discussion（既有 post_type 值域）。
    let post_type = match query.type_filter.as_deref().filter(|s| !s.is_empty()) {
        Some("article") => Some("article".to_string()),
        Some("topic") => Some("discussion".to_string()),
        Some(other) => {
            return Err(AppError::bad_request(
                format!("type must be article or topic, got: {other}"),
                request_id,
                None,
            ))
        }
        None => None,
    };
    let is_following = query.sort.as_deref() == Some("following");
    let viewer_id = auth.user.as_ref().map(|u| u.id.clone());
    let empty_results = is_following && viewer_id.is_none();
    let filter = PostsFilter {
        board_id: query.board_id.filter(|s| !s.is_empty()),
        author_id: query.author_id.filter(|s| !s.is_empty()),
        author_username: query.author_username.filter(|s| !s.is_empty()),
        tag_slug: query.tag.filter(|s| !s.is_empty()),
        post_type,
        // featured/unanswered/following 实为过滤条件（排序仍按 latest 键序，保证
        // created_at keyset 游标一致）；popular 维持既有浏览量排序。
        featured_only: query.sort.as_deref() == Some("featured"),
        unanswered_only: query.sort.as_deref() == Some("unanswered"),
        popular: query.sort.as_deref() == Some("popular"),
        following_user_id: if is_following { viewer_id } else { None },
        empty_results,
        after,
        limit,
    };

    let (rows, has_more) = list_posts_page(pool, &filter, request_id).await?;

    // 参与者预览（一页一查询）：楼主之外已发布回复的不同作者，每帖 ≤2 个。
    let post_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
    let participants = fetch_post_participants(pool, &post_ids, request_id).await?;
    // 作者装扮投影（作者去重后逐个查询）：参与者列楼主头像的已装备头像框。
    let author_ids: Vec<String> = rows.iter().map(|r| r.author_id.clone()).collect();
    let author_tokens = fetch_author_presentation_tokens(pool, &author_ids).await;
    let empty_participants: Vec<PostParticipantRow> = Vec::new();
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            post_summary_json(
                r,
                participants
                    .get(&r.id)
                    .map(|v| v.as_slice())
                    .unwrap_or(&empty_participants),
                author_tokens.get(&r.author_id),
            )
        })
        .collect();
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
/// 字段 pub(crate)：推荐流（routes::recommendations）复用同一投影。
#[derive(sqlx::FromRow)]
pub(crate) struct PostListRow {
    pub(crate) id: String,
    pub(crate) board_id: String,
    pub(crate) author_id: String,
    pub(crate) post_type: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) reply_count: i64,
    pub(crate) view_count: i64,
    pub(crate) created_at: i64,
    pub(crate) updated_at: i64,
    pub(crate) last_reply_at: Option<i64>,
    pub(crate) pinned_at: Option<i64>,
    /// 0003 既有布尔列（is_pinned 的同义列，见 0061 迁移注释）。
    pub(crate) pinned: i64,
    /// 精选时间戳（is_featured = featured_at IS NOT NULL）。
    pub(crate) featured_at: Option<i64>,
    pub(crate) author_name: Option<String>,
    /// 作者昵称（users.display_name；前台列表优先显示昵称，缺省回退用户名）。
    pub(crate) author_display_name: Option<String>,
    /// 作者上传头像附件 id（公开引用；参与者列楼主头像渲染用，可空）。
    pub(crate) author_avatar_attachment_id: Option<String>,
    /// 作者手写摘要（列表卡片展示；与前端首页线程卡对齐，见 posts 表同名列）。
    pub(crate) summary: Option<String>,
    /// 点赞计数（M18-HOME-01，原型帖子卡 ♥ 计数）。
    pub(crate) like_count: i64,
}

/// 帖子列表参与者预览行（GET /posts、GET /boards/{slug}/posts、推荐流共用）：
/// 话题参与者 = 已发布回复的不同作者（不含楼主），按首个回复楼层排序。
/// 仅含公开字段（id/username/display_name/avatar_attachment_id/
/// presentation_tokens），与作者投影同构。
#[derive(Clone, Debug)]
pub(crate) struct PostParticipantRow {
    #[allow(dead_code)]
    pub(crate) post_id: String,
    pub(crate) user_id: String,
    pub(crate) username: Option<String>,
    pub(crate) display_name: Option<String>,
    /// 用户上传头像附件 id（公开引用；附件内容经 /attachments/{id} 鉴权下发）。
    pub(crate) avatar_attachment_id: Option<String>,
    #[allow(dead_code)]
    pub(crate) presentation_tokens: Option<crate::users::dto::PublicPresentationTokens>,
}

/// 每帖返回的参与者上限：前台参与者列最多展示 5 个头像（楼主 + 至多 4 个回复者），
/// 超过 5 个时最后头像后显示省略符号（返回至多 5 个回复者供前台判断是否超限）。
pub(crate) const PARTICIPANT_PREVIEW_LIMIT: usize = 5;

/// `fetch_post_participants` 查询行：
/// (post_id, user_id, username, display_name, avatar_attachment_id)。
type ParticipantQueryRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

/// 一次取回一页帖子的参与者预览（单查询避免 N+1）：
/// `comments(status='published')` 与楼主去重后按 (post_id, author_id) 分组，
/// 以首个回复楼层（MIN(floor)）排序——先参与者先展示；GROUP BY 显式包含
/// 所选非聚合列，SQLite/MySQL(ONLY_FULL_GROUP_BY)/MariaDB 三方言一致。
pub(crate) async fn fetch_post_participants(
    pool: &DatabasePool,
    post_ids: &[String],
    request_id: &'static str,
) -> Result<HashMap<String, Vec<PostParticipantRow>>, AppError> {
    if post_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let placeholders = vec!["?"; post_ids.len()].join(", ");
    let sql = format!(
        "SELECT c.post_id, c.author_id AS user_id, u.username_normalized AS username, \
                u.display_name AS display_name, u.avatar_attachment_id AS avatar_attachment_id \
         FROM comments c \
         JOIN posts p ON p.id = c.post_id AND c.author_id <> p.author_id \
         LEFT JOIN users u ON u.id = c.author_id \
         WHERE c.status = 'published' AND c.post_id IN ({placeholders}) \
         GROUP BY c.post_id, c.author_id, u.username_normalized, u.display_name, u.avatar_attachment_id \
         ORDER BY c.post_id, MIN(c.floor), c.author_id"
    );
    let rows: Vec<ParticipantQueryRow> = match pool {
        Either::Left(p) => {
            let mut query = sqlx::query_as::<_, ParticipantQueryRow>(&sql);
            for id in post_ids {
                query = query.bind(id);
            }
            query
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
        Either::Right(p) => {
            let mut query = sqlx::query_as::<_, ParticipantQueryRow>(&sql);
            for id in post_ids {
                query = query.bind(id);
            }
            query
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };

    // Decorated participant previews are enriched below.
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut grouped: HashMap<String, Vec<PostParticipantRow>> = HashMap::new();
    for (post_id, user_id, username, display_name, avatar_attachment_id) in rows {
        let count = counts.entry(post_id.clone()).or_insert(0);
        if *count >= PARTICIPANT_PREVIEW_LIMIT {
            continue;
        }
        *count += 1;
        let presentation_tokens =
            crate::shop::service::get_public_presentation_tokens(pool, &user_id)
                .await
                .unwrap_or(None);
        grouped
            .entry(post_id.clone())
            .or_default()
            .push(PostParticipantRow {
                post_id,
                user_id,
                username,
                display_name,
                avatar_attachment_id,
                presentation_tokens,
            });
    }
    Ok(grouped)
}

/// 一页帖子的作者装扮投影（按 author_id 去重后逐作者查询；列表「参与者」
/// 列楼主头像渲染已装备头像框用）。作者未装备/权益失效时不写入映射，
/// 投影保持与参与者一致：无装扮不出 `presentation_tokens` 键。
pub(crate) async fn fetch_author_presentation_tokens(
    pool: &DatabasePool,
    author_ids: &[String],
) -> HashMap<String, crate::users::dto::PublicPresentationTokens> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut tokens: HashMap<String, crate::users::dto::PublicPresentationTokens> = HashMap::new();
    for author_id in author_ids {
        if author_id.is_empty() || !seen.insert(author_id.as_str()) {
            continue;
        }
        if let Ok(Some(value)) =
            crate::shop::service::get_public_presentation_tokens(pool, author_id).await
        {
            tokens.insert(author_id.clone(), value);
        }
    }
    tokens
}

pub(crate) fn post_summary_json(
    p: &PostListRow,
    participants: &[PostParticipantRow],
    author_presentation_tokens: Option<&crate::users::dto::PublicPresentationTokens>,
) -> Value {
    let participants_json: Vec<Value> = participants
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
    // 作者公开投影（可选项）：已装备头像框等安全 Token 与上传头像附件引用
    // 随列表行下发，参与者列楼主头像与悬浮资料卡同源渲染。
    let mut author = json!({
        "id": p.author_id,
        "username": p.author_name,
        "display_name": p.author_display_name,
    });
    if let Some(attachment_id) = &p.author_avatar_attachment_id {
        author["avatar_attachment_id"] = json!(attachment_id);
    }
    if let Some(tokens) = author_presentation_tokens {
        author["presentation_tokens"] = json!(tokens);
    }
    json!({
        "id": p.id,
        "board_id": p.board_id,
        "author": author,
        "participants": participants_json,
        "post_type": p.post_type,
        "title": p.title,
        "status": p.status,
        "summary": p.summary,
        "reply_count": p.reply_count,
        "view_count": p.view_count,
        "like_count": p.like_count,
        "pinned_at": p.pinned_at,
        "is_pinned": p.pinned != 0,
        "is_featured": p.featured_at.is_some(),
        "created_at": p.created_at,
        "updated_at": p.updated_at,
        "last_reply_at": p.last_reply_at,
    })
}

/// 帖子列表过滤参数（GAP-FIX 筛选补齐：sort/type/tag/author_username）。
struct PostsFilter {
    board_id: Option<String>,
    author_id: Option<String>,
    author_username: Option<String>,
    tag_slug: Option<String>,
    post_type: Option<String>,
    featured_only: bool,
    unanswered_only: bool,
    popular: bool,
    /// 关注过滤：当前登录用户的 id（未登录时 empty_results 恒为 true）。
    following_user_id: Option<String>,
    empty_results: bool,
    after: Option<i64>,
    limit: i64,
}

/// keyset 分页查询（published 帖子；cursor=created_at）。
///
/// 过滤说明：
/// - featured：featured_at 非空（精选标记，同义列见 0061 迁移注释）；
/// - unanswered：reply_count = 0（无回复缓存列，即无评论——既有列的最佳
///   近似：posts.reply_count 由评论路径维护）；
/// - tag：post_tags × tags.slug 精确关联（EXISTS 子查询，三方言一致）；
/// - author_username：users.username_normalized 精确匹配。
async fn list_posts_page(
    pool: &DatabasePool,
    f: &PostsFilter,
    request_id: &'static str,
) -> Result<(Vec<PostListRow>, bool), AppError> {
    if f.empty_results {
        return Ok((Vec::new(), false));
    }
    let order = if f.popular {
        "p.view_count DESC, p.reply_count DESC, p.id DESC"
    } else {
        "p.created_at DESC, p.id DESC"
    };
    let mut sql = String::from(
        "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status,
                p.reply_count, p.view_count, p.created_at, p.updated_at, p.last_reply_at,
                p.pinned_at, p.pinned, p.featured_at, u.username_normalized as author_name,
                u.display_name as author_display_name,
                u.avatar_attachment_id as author_avatar_attachment_id,
                p.summary,
                (SELECT COUNT(*) FROM post_reactions pr WHERE pr.post_id = p.id AND pr.reaction = 'like') AS like_count
         FROM posts p
         LEFT JOIN users u ON u.id = p.author_id
         WHERE p.status = 'published' AND p.deleted_at IS NULL
           AND (? IS NULL OR p.board_id = ?)
           AND (? IS NULL OR p.author_id = ?)
           AND (? IS NULL OR p.author_id = (SELECT id FROM users WHERE username_normalized = ?))
           AND (? IS NULL OR p.post_type = ?)
           AND (? IS NULL OR EXISTS (
                SELECT 1 FROM post_tags pt JOIN tags t ON t.id = pt.tag_id
                WHERE pt.post_id = p.id AND t.slug = ?))
           AND (? IS NULL OR p.author_id IN (SELECT followee_id FROM user_follows WHERE follower_id = ?))
           AND (? IS NULL OR p.created_at < ?)",
    );
    if f.featured_only {
        sql.push_str(" AND p.featured_at IS NOT NULL");
    }
    if f.unanswered_only {
        sql.push_str(" AND p.reply_count = 0");
    }
    sql.push_str(&format!(" ORDER BY {order} LIMIT ?"));

    let fetch_limit = f.limit + 1;
    let rows: Vec<PostListRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, PostListRow>(&sql)
            .bind(&f.board_id)
            .bind(&f.board_id)
            .bind(&f.author_id)
            .bind(&f.author_id)
            .bind(&f.author_username)
            .bind(&f.author_username)
            .bind(&f.post_type)
            .bind(&f.post_type)
            .bind(&f.tag_slug)
            .bind(&f.tag_slug)
            .bind(&f.following_user_id)
            .bind(&f.following_user_id)
            .bind(f.after)
            .bind(f.after)
            .bind(fetch_limit)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, PostListRow>(&sql)
            .bind(&f.board_id)
            .bind(&f.board_id)
            .bind(&f.author_id)
            .bind(&f.author_id)
            .bind(&f.author_username)
            .bind(&f.author_username)
            .bind(&f.post_type)
            .bind(&f.post_type)
            .bind(&f.tag_slug)
            .bind(&f.tag_slug)
            .bind(&f.following_user_id)
            .bind(&f.following_user_id)
            .bind(f.after)
            .bind(f.after)
            .bind(fetch_limit)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let has_more = rows.len() as i64 > f.limit;
    let rows = rows.into_iter().take(f.limit as usize).collect();
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
            "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status, p.deleted_at,
                    p.review_status, p.reply_count, p.view_count, p.created_at, p.updated_at, p.version, p.last_reply_at,
                    p.pinned_at, p.scheduled_at, p.published_at, p.slug, p.closed_at,
                    u.username_normalized as author_name, u.display_name as author_display_name,
                    u.trust_level as author_level,
                    u.avatar_attachment_id as author_avatar_attachment_id,
                    pol.kind as policy_kind, pol.min_level as policy_min_level,
                    c.body_html, c.body_markdown, c.excerpt, c.renderer_version
             FROM posts p
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN post_contents c ON c.post_id = p.id
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             WHERE p.id = ?",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, PostDetailProjection>(
            "SELECT p.id, p.board_id, p.author_id, p.post_type, p.title, p.status, p.deleted_at,
                    p.review_status, p.reply_count, p.view_count, p.created_at, p.updated_at, p.version, p.last_reply_at,
                    p.pinned_at, p.scheduled_at, p.published_at, p.slug, p.closed_at,
                    u.username_normalized as author_name, u.display_name as author_display_name,
                    u.trust_level as author_level,
                    u.avatar_attachment_id as author_avatar_attachment_id,
                    pol.kind as policy_kind, pol.min_level as policy_min_level,
                    c.body_html, c.body_markdown, c.excerpt, c.renderer_version
             FROM posts p
             LEFT JOIN users u ON u.id = p.author_id
             LEFT JOIN post_contents c ON c.post_id = p.id
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             WHERE p.id = ?",
        )
        .bind(&id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let Some(r) = row else {
        return Err(AppError::not_found("post not found", request_id));
    };

    // 权威管理员/版主判断（成熟论坛规范：具备 post.moderate 或管理角色的管理员可前台查看任意状态帖子）
    let is_moderator = if let Some(u) = auth.user.as_ref() {
        let has_role = u
            .roles
            .iter()
            .any(|role| role == "admin" || role == "moderator" || role == "administrator");
        if has_role {
            true
        } else {
            authorize_action(pool, &u.id, "post.moderate", None, AUTHZ_POLICY_VERSION)
                .await
                .map(|d| d.is_allowed())
                .unwrap_or(false)
        }
    } else {
        false
    };

    let is_deleted = r.deleted_at.is_some();
    // M05-RISK-03/06：pending_review（status='draft' + review_status）只对
    // 作者本人或管理员可见，且投影为安全的审核状态（不含举报人/内部 note/规则细节）。
    let pending_review =
        r.status == "draft" && r.review_status.as_deref() == Some("pending_review");
    let requester_is_author = auth.user.as_ref().is_some_and(|u| u.id == r.author_id);

    if is_deleted {
        if !is_moderator {
            return Err(AppError::not_found("post not found", request_id));
        }
    } else if !matches!(r.status.as_str(), "published" | "hidden")
        && !(pending_review && requester_is_author)
        && !is_moderator
    {
        return Err(AppError::not_found("post not found", request_id));
    }
    let is_pending_author_view = pending_review && requester_is_author;
    let is_mod_preview = is_moderator && (is_deleted || r.status != "published");

    // pending_review 或管理员预览未公开/已删除内容不计数浏览量。
    if !is_pending_author_view && !is_mod_preview {
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
        .map(|lv| lv.clamp(0, i64::from(u32::MAX)) as u32);
    let key = post_grant_key(&id);
    let actor = auth.user.as_ref().map(|u| Actor {
        id: &u.id,
        level: u.level.clamp(0, i64::from(u32::MAX)) as u32,
        username: &u.username,
    });
    let author_level = r.author_level.unwrap_or(0).clamp(0, i64::from(u32::MAX)) as u32;
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
        moderator_override: is_moderator,
    };
    let grant = evaluate(actor.as_ref(), &content, &ctx).await;

    let has_inline = r
        .body_markdown
        .as_deref()
        .is_some_and(crate::content::markdown::inline_reply::has_inline_reply);

    let is_inline_unlocked = if has_inline {
        if grant.unlocked {
            true
        } else {
            let is_author = auth.user.as_ref().is_some_and(|u| u.id == r.author_id);
            let is_mod = auth.user.as_ref().is_some_and(|u| {
                u.roles
                    .iter()
                    .any(|role| role == "admin" || role == "moderator" || role == "administrator")
            });
            if is_author || is_mod {
                true
            } else if let Some(u) = auth.user.as_ref() {
                let has_replied: bool = match pool {
                    Either::Left(p) => {
                        sqlx::query_scalar::<_, i64>(
                            "SELECT 1 FROM comments WHERE post_id = ? AND author_id = ? AND status = 'published' LIMIT 1",
                        )
                        .bind(&id)
                        .bind(&u.id)
                        .fetch_optional(p)
                        .await
                        .unwrap_or(None)
                        .is_some()
                    }
                    Either::Right(p) => {
                        sqlx::query_scalar::<_, i64>(
                            "SELECT 1 FROM comments WHERE post_id = ? AND author_id = ? AND status = 'published' LIMIT 1",
                        )
                        .bind(&id)
                        .bind(&u.id)
                        .fetch_optional(p)
                        .await
                        .unwrap_or(None)
                        .is_some()
                    }
                };
                has_replied
            } else {
                false
            }
        }
    } else {
        false
    };

    let base_html = if has_inline {
        r.body_markdown.as_deref().map(|md| {
            crate::content::markdown::inline_reply::render_with_inline_reply(md, is_inline_unlocked)
        })
    } else {
        r.body_html
    };

    // 附件占位（M06-QUOTA-09 展示端）：正文引用的附件已删除/不可用时，
    // 读取时替换为「附件已删除」占位（落库 HTML 与 Markdown 不变）。
    let body_html = match (base_html, r.body_markdown.as_deref()) {
        (Some(html), Some(markdown)) if !html.is_empty() => {
            let candidate_ids = extract_attachment_content_ids(markdown);
            let unavailable = load_unavailable_attachment_ids(pool, &candidate_ids)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            if unavailable.is_empty() {
                Some(html)
            } else {
                Some(
                    crate::content::attachments::replace_unavailable_attachments(
                        &html,
                        &unavailable,
                    ),
                )
            }
        }
        (html, _) => html,
    };

    let author_presentation_tokens =
        crate::shop::service::get_public_presentation_tokens(pool, &r.author_id)
            .await
            .unwrap_or(None);

    let fields = PostFields {
        id: r.id,
        title: r.title,
        author_id: r.author_id,
        author_username: r.author_name,
        author_display_name: r.author_display_name,
        author_level: r.author_level.unwrap_or(0),
        author_presentation_tokens,
        author_avatar_attachment_id: r.author_avatar_attachment_id.clone(),
        post_type: r.post_type,
        status: r.status.clone(),
        board_id: r.board_id,
        slug: r.slug,
        reply_count: r.reply_count,
        view_count: r.view_count + 1,
        created_at: r.created_at,
        updated_at: r.updated_at,
        version: r.version,
        pinned_at: r.pinned_at,
        scheduled_at: r.scheduled_at,
        published_at: r.published_at,
        last_reply_at: r.last_reply_at,
        closed_at: r.closed_at,
        body_html,
        body_markdown: r.body_markdown,
        excerpt: r.excerpt,
        attachments: Vec::new(),
        search_highlight: None,
        restricted_html: None,
    };
    let mut body = project_post(fields, grant, author_level);

    // 收藏聚合（GAP-FIX 社交域）：favorite_count 子查询计数 +
    // viewer_favorited（登录时 EXISTS 判定，匿名恒 false）。
    let favorite_count = load_post_favorite_count(pool, &id, request_id).await?;
    let viewer_favorited = match auth.user.as_ref() {
        Some(u) => {
            let exists: Option<i64> = match pool {
                Either::Left(p) => {
                    sqlx::query_scalar("SELECT 1 FROM favorites WHERE post_id = ? AND user_id = ?")
                        .bind(&id)
                        .bind(&u.id)
                        .fetch_optional(p)
                        .await
                }
                Either::Right(p) => {
                    sqlx::query_scalar("SELECT 1 FROM favorites WHERE post_id = ? AND user_id = ?")
                        .bind(&id)
                        .bind(&u.id)
                        .fetch_optional(p)
                        .await
                }
            }
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            exists == Some(1)
        }
        None => false,
    };
    let post_tags = load_post_tags(pool, &id, request_id).await?;
    if let Some(map) = body.as_object_mut() {
        map.insert("favorite_count".into(), json!(favorite_count));
        map.insert("viewer_favorited".into(), json!(viewer_favorited));
        map.insert("tags".into(), json!(post_tags));
        if is_deleted {
            map.insert("status".into(), json!("deleted"));
            map.insert("deleted_at".into(), json!(r.deleted_at));
        } else if r.status == "draft" && r.review_status.as_deref() == Some("pending_review") {
            map.insert("status".into(), json!("pending_review"));
        }
        if is_moderator {
            map.insert("is_moderator_view".into(), json!(true));
        }
    }

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

    if is_pending_author_view || is_mod_preview {
        // 未公开/管理预览内容：禁止任何缓存（private, no-store）。
        let mut resp = (StatusCode::OK, Json(body)).into_response();
        resp.headers_mut().insert(
            header::CACHE_CONTROL,
            HeaderValue::from_static("private, no-store"),
        );
        return Ok(resp);
    }

    // 信任等级钩子（best-effort，M20-TRUST）：登录用户浏览即「进入话题」，
    // user×post 唯一幂等；触发惰性评估。放在缓存头构造前，失败不影响响应。
    if let Some(u) = auth.user.as_ref() {
        crate::trust::on_topic_viewed(pool, &u.id, &id).await;
    }

    let ch = if has_inline {
        crate::content::visibility::cache::CacheHeaders {
            cache_control: "private, no-store",
            vary: None,
            etag: None,
        }
    } else {
        cache_headers_for(&grant, &body.to_string())
    };

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
    deleted_at: Option<i64>,
    review_status: Option<String>,
    reply_count: i64,
    view_count: i64,
    created_at: i64,
    updated_at: i64,
    version: i64,
    last_reply_at: Option<i64>,
    pinned_at: Option<i64>,
    scheduled_at: Option<i64>,
    published_at: Option<i64>,
    slug: Option<String>,
    closed_at: Option<i64>,
    author_name: Option<String>,
    author_display_name: Option<String>,
    author_level: Option<i64>,
    author_avatar_attachment_id: Option<String>,
    policy_kind: Option<String>,
    policy_min_level: Option<i64>,
    body_html: Option<String>,
    body_markdown: Option<String>,
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

/// 帖子收藏计数（详情聚合；GAP-FIX 社交域）。
async fn load_post_favorite_count(
    pool: &DatabasePool,
    post_id: &str,
    request_id: &'static str,
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

/// 读取帖子标签名（按关联顺序；供详情聚合与反显）。
async fn load_post_tags(
    pool: &DatabasePool,
    post_id: &str,
    request_id: &'static str,
) -> Result<Vec<String>, AppError> {
    let sql = "SELECT t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id WHERE pt.post_id = ? ORDER BY pt.created_at ASC, pt.tag_id ASC";
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, String>(sql)
            .bind(post_id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id)),
        Either::Right(p) => sqlx::query_scalar::<_, String>(sql)
            .bind(post_id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id)),
    }
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

    // 加载帖子（含作者与所属板块）
    let post = get_post_row(pool, &id, request_id).await?;
    let post = post.ok_or_else(|| AppError::not_found("post not found", request_id))?;
    let (post_author_id, post_board_id, _post_status, _post_version, _post_updated_at) = post;

    // 权限判定：作者本人；管理员或当前板块版主（post.moderate / admin.manage）
    let is_owner = post_author_id == user.id;
    let is_admin = if !is_owner {
        let is_admin = authorize_action(pool, &user.id, "admin.manage", None, AUTHZ_POLICY_VERSION)
            .await
            .map(|d| d.is_allowed())
            .unwrap_or(false);
        let is_moderator = authorize_action(
            pool,
            &user.id,
            "post.moderate",
            Some(&post_board_id),
            AUTHZ_POLICY_VERSION,
        )
        .await
        .map(|d| d.is_allowed())
        .unwrap_or(false);

        if !is_admin && !is_moderator {
            return Err(AppError::forbidden(
                "post.moderate permission required for delegated edit",
                request_id,
            ));
        }
        is_admin
    } else {
        false
    };

    // If-Match 版本校验
    let expected_version: i64 = headers
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header required", request_id, None))?
        .parse()
        .map_err(|_| {
            AppError::bad_request("If-Match must be an integer version", request_id, None)
        })?;

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

    // 标签校验（PATCH 语义：仅当提供时校验与更新）
    if let Some(tags) = req.tags.as_deref() {
        if tags.len() > 8 {
            return Err(AppError::bad_request(
                "tags must contain at most 8 items",
                request_id,
                None,
            ));
        }
        for tag in tags {
            let len = tag.trim().chars().count();
            if len == 0 || len > 32 {
                return Err(AppError::bad_request(
                    "each tag must be 1-32 characters",
                    request_id,
                    None,
                ));
            }
        }
    }

    if !is_owner {
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
        let role_name = if is_admin {
            "administrator"
        } else {
            "moderator"
        };
        AuditEntry::delegated_admin_action(&user.id, role_name, "post.update", "post", &id, reason)
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

    if let Some(tags) = req.tags.as_deref() {
        let old_tag_ids: Vec<String> = match pool {
            Either::Left(p) => sqlx::query_scalar("SELECT tag_id FROM post_tags WHERE post_id = ?")
                .bind(&id)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            Either::Right(p) => {
                sqlx::query_scalar("SELECT tag_id FROM post_tags WHERE post_id = ?")
                    .bind(&id)
                    .fetch_all(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
            }
        };
        for tag_id in &old_tag_ids {
            let decr_sql = "UPDATE tags SET usage_count = CASE WHEN usage_count > 0 THEN usage_count - 1 ELSE 0 END WHERE id = ?";
            match pool {
                Either::Left(p) => {
                    let _ = sqlx::query(decr_sql).bind(tag_id).execute(p).await;
                }
                Either::Right(p) => {
                    let _ = sqlx::query(decr_sql).bind(tag_id).execute(p).await;
                }
            }
        }
        match pool {
            Either::Left(p) => {
                let _ = sqlx::query("DELETE FROM post_tags WHERE post_id = ?")
                    .bind(&id)
                    .execute(p)
                    .await;
            }
            Either::Right(p) => {
                let _ = sqlx::query("DELETE FROM post_tags WHERE post_id = ?")
                    .bind(&id)
                    .execute(p)
                    .await;
            }
        }
        let now = now_millis();
        let _ = link_post_tags(pool, &id, Some(tags), now, request_id).await?;
        let _ = crate::search::index_job::enqueue_index_job(pool, "post", &id).await;
    }

    let post_tags = load_post_tags(pool, &id, request_id).await?;
    let body = json!({
        "id": refreshed.id,
        "title": refreshed.title,
        "status": refreshed.status.as_str(),
        "version": refreshed.version,
        "updated_at": refreshed.updated_at,
        "tags": post_tags,
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

/// 读取帖子元数据行（含作者与板块）。
async fn get_post_row(
    pool: &DatabasePool,
    id: &str,
    request_id: &'static str,
) -> Result<Option<(String, String, String, i64, i64)>, AppError> {
    // (author_id, board_id, status, version, updated_at)
    let row: Option<(String, String, String, i64, i64)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT author_id, board_id, status, version, updated_at FROM posts WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT author_id, board_id, status, version, updated_at FROM posts WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let Some((author_id, board_id, status, version, updated_at)) = row else {
        return Ok(None);
    };
    Ok(Some((author_id, board_id, status, version, updated_at)))
}

/// GET /api/v1/posts/{id}/comments — 列出评论（keyset 分页 + 软删占位，M04-COMMENTS-04）
///
/// 稳定排序 `floor ASC, id ASC`；`after` 为不透明游标 `base64url("floor:id")`
/// （[`CommentCursor`]）；fetch limit+1 判定 `has_more`。软删/隐藏评论返回
/// 占位投影（`body_html:null`，不泄漏正文；占位保留楼层）。匿名可读（OpenAPI
/// `security: *2` = 可选会话）。响应 `Cache-Control: public, max-age=60` + ETag。
async fn list_comments(
    State(state): State<AppState>,
    auth: AuthSession,
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
    // 附件占位（M06-QUOTA-09 展示端）：本页评论引用的附件统一查一次状态，
    // 行缺失/非 ready（含已删除）的引用读取时替换为「附件已删除」占位。
    let candidate_ids: Vec<String> = rows
        .iter()
        .flat_map(|r| extract_attachment_content_ids(&r.content))
        .collect();
    let unavailable = load_unavailable_attachment_ids(pool, &candidate_ids)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let items: Vec<Value> = rows.iter().map(|r| comment_json(r, &unavailable)).collect();
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

    // 信任等级钩子（best-effort，M20-TRUST）：登录用户翻阅楼层即「阅读楼层」，
    // user×comment 唯一幂等；触发惰性评估。
    if let Some(u) = auth.user.as_ref() {
        let comment_ids: Vec<String> = rows.iter().map(|r| r.id.clone()).collect();
        crate::trust::on_comments_read(pool, &u.id, &id, &comment_ids).await;
    }

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
            // 成就钩子（best-effort）：comment_count 类成就；失败只 warn。
            if let Err(e) = crate::achievements::evaluate(pool, &user.id).await {
                tracing::warn!(user_id = %user.id, error = %e, "achievement evaluate failed (comment)");
            }
            // 活跃奖励钩子（best-effort，M07-LEVELS）：comment 类 activity_rules
            // 规则；同评论去重（幂等重放不重复奖励）；失败只 warn 不阻断。
            if let Err(e) = crate::economy::activity::service::claim_content_reward(
                pool,
                &user.id,
                "comment",
                &comment_id,
                now,
            )
            .await
            {
                tracing::warn!(user_id = %user.id, error = %e, "activity content reward failed (comment)");
            }
            // @提及通知（M05-NOTIFY-10，best-effort）：解析正文 @用户，为每个
            // 被提及的真实用户创建 mention 通知；失败只 warn，不影响回复创建
            // （幂等重放同评论不会重复通知，见去重键细化到 comment）。
            let mentioned = crate::content::mentions::extract_mentions(content.as_str());
            if !mentioned.is_empty() {
                let actor_name = user
                    .display_name
                    .clone()
                    .filter(|n| !n.trim().is_empty())
                    .unwrap_or_else(|| user.username.clone());
                if let Err(e) = crate::notifications::service::create_mention_notifications(
                    pool,
                    &user.id,
                    &actor_name,
                    &id,
                    &comment_id,
                    &mentioned,
                    now,
                )
                .await
                {
                    tracing::warn!(comment_id = %comment_id, error = %e, "mention notification failed");
                }
            }
            let candidate_ids = extract_attachment_content_ids(&projection.content);
            let unavailable = load_unavailable_attachment_ids(pool, &candidate_ids)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let mut resp_body = comment_json(&projection, &unavailable);
            resp_body["floor"] = json!(created.floor);
            Ok(private_no_store_response(
                (StatusCode::CREATED, Json(resp_body)).into_response(),
            ))
        }
        crate::idempotency::IdempotencyOutcome::Replay { response_reference } => {
            // 同 key+摘要重放：返回原评论（按引用读取）
            if let Some(comment_id) = response_reference {
                if let Ok(Some(projection)) = load_comment_projection(pool, &comment_id).await {
                    let candidate_ids = extract_attachment_content_ids(&projection.content);
                    // 幂等重放路径保持既有错误语义（409），占位查询失败降级为不替换
                    let unavailable = load_unavailable_attachment_ids(pool, &candidate_ids)
                        .await
                        .unwrap_or_default();
                    return Ok(private_no_store_response(
                        (
                            StatusCode::CREATED,
                            Json(comment_json(&projection, &unavailable)),
                        )
                            .into_response(),
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

#[derive(Debug, serde::Deserialize)]
pub struct ReactionPayload {
    #[serde(default = "default_reaction")]
    pub reaction: String,
}

fn default_reaction() -> String {
    "like".to_string()
}

/// GET /api/v1/posts/{id}/reactions — 获取帖子收到的表情与用户明细
async fn get_post_reactions(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_post_reactions";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let viewer_id = auth.user.as_ref().map(|u| u.id.as_str());
    let detail = crate::reactions::service::get_reactions_detail(pool, "post", &id, viewer_id)
        .await
        .map_err(|e| map_reaction_error(e, request_id))?;
    Ok(Json(detail))
}

/// POST /api/v1/posts/{id}/reactions — 切换反应（M07-SHOP-08，user_reactions）
async fn toggle_reaction(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    payload: Option<Json<ReactionPayload>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "toggle_reaction";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let reaction = payload
        .map(|Json(p)| p.reaction)
        .unwrap_or_else(default_reaction);
    // 基础点赞（like / 👍）为全员免费互动；仅特殊反应包才消耗 reaction_pack
    let require_pack = !matches!(reaction.as_str(), "like" | "👍");
    // toggle：先尝试添加；已存在则移除。
    match crate::reactions::service::add_reaction(
        pool,
        &user.id,
        "post",
        &id,
        &reaction,
        require_pack,
    )
    .await
    {
        Ok(summary) => {
            // 作者查询（被赞方；供活跃奖励与成就钩子共用）。
            let author: Option<String> = match pool {
                Either::Left(p) => sqlx::query_scalar("SELECT author_id FROM posts WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?,
                Either::Right(p) => sqlx::query_scalar("SELECT author_id FROM posts WHERE id = ?")
                    .bind(&id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            };
            // 活跃奖励钩子（best-effort，M07-LEVELS）：reaction 类规则奖励**表态方**
            // （引擎内排除自赞、同目标+反应去重、撤赞重赞不重复奖励）；失败只 warn。
            if let Some(author_id) = author.as_deref() {
                if let Err(e) = crate::economy::activity::service::claim_reaction_reward(
                    pool,
                    &user.id,
                    author_id,
                    "post",
                    &id,
                    &reaction,
                    now_millis(),
                )
                .await
                {
                    tracing::warn!(user_id = %user.id, error = %e, "activity reaction reward failed (post)");
                }
            }
            // 信任等级钩子（best-effort，M20-TRUST）：反应变化影响表态方与
            // 被赞方统计（送出/收到的赞）；失败只 warn。
            crate::trust::on_reaction_changed(pool, &user.id, author.as_deref()).await;
            // 成就钩子（best-effort）：reaction_received 类成就按**帖子作者**
            // 判定（被赞方）；失败只 warn。
            if let Some(author_id) = author {
                if let Err(e) = crate::achievements::evaluate(pool, &author_id).await {
                    tracing::warn!(user_id = %author_id, error = %e, "achievement evaluate failed (reaction)");
                }
            }
            Ok(Json(summary))
        }
        Err(crate::reactions::ReactionError::AlreadyExists) => {
            let removed =
                crate::reactions::service::remove_reaction(pool, &user.id, "post", &id, &reaction)
                    .await
                    .map_err(|e| map_reaction_error(e, request_id))?;
            let author = post_author(pool, &id, request_id).await?;
            crate::trust::on_reaction_changed(pool, &user.id, author.as_deref()).await;
            Ok(Json(removed))
        }
        Err(e) => Err(map_reaction_error(e, request_id)),
    }
}

/// 帖子作者查询（信任等级钩子用；查不到返回 None，不阻塞主流程）。
async fn post_author(
    pool: &DatabasePool,
    post_id: &str,
    request_id: &str,
) -> Result<Option<String>, AppError> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT author_id FROM posts WHERE id = ?")
                .bind(post_id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT author_id FROM posts WHERE id = ?")
                .bind(post_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))
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
    let removed =
        crate::reactions::service::remove_reaction(pool, &user.id, "post", &id, &reaction)
            .await
            .map_err(|e| map_reaction_error(e, request_id))?;
    // 信任等级钩子（best-effort）：撤赞同样影响双方统计。
    let author = post_author(pool, &id, request_id).await?;
    crate::trust::on_reaction_changed(pool, &user.id, author.as_deref()).await;
    Ok(Json(removed))
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
            // 稳定码 self_reaction（400）：前端映射为「不能对自己发布的内容表态」。
            AppError::with_code(
                StatusCode::BAD_REQUEST,
                "self_reaction",
                "Self Reaction",
                "cannot react to own content",
                request_id,
            )
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
