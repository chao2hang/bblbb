//! M04-POSTS-02/03：草稿 CRUD——创建（幂等）、读取、cursor 列表、更新
//! （If-Match 版本冲突）、软删除。
//!
//! - `POST /api/v1/drafts`：创建草稿——经 [`validate_draft_create`] 服务端
//!   校验（不信任客户端），author 取会话、board 可空、version 从 1 起；
//!   `client_request_id` 为幂等键（scope `draft.create`）：相同 key+请求摘要
//!   重放返回原草稿，摘要不一致 → 409；
//! - `GET /api/v1/drafts/{id}`：读取**自己**的草稿（他人/不存在一律 404）；
//! - `PATCH /api/v1/drafts/{id}`：更新草稿——`If-Match` 版本号必须与当前
//!   version 一致（冲突 → 409），部分更新，`update_draft` 递增 version；
//! - `DELETE /api/v1/drafts/{id}`：软删除（`deleted_at` 置位，行保留供
//!   审计/恢复）；可选 `If-Match` 防并发覆盖；
//! - `GET /api/v1/drafts`：owner 维度 keyset 分页（`after` 游标 =
//!   上一页最后一条 updated_at，按 updated_at DESC 取下一页）。
//!
//! 响应均为私有数据：`Cache-Control: private, no-store`。

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::content::markdown::rerender::render_content as render_for_preview;
use crate::content::model::Draft;
use crate::content::posts::command::{
    validate_draft_create, validate_draft_patch, CreateDraftInput, DraftPatchInput, PostCreateError,
};
use crate::content::repository::{
    delete_draft as repo_delete_draft, get_draft, insert_draft, list_drafts_cursor,
    update_draft as repo_update_draft,
};
use crate::db::DatabasePool;
use crate::error::AppError;
use crate::idempotency::{
    begin_or_replay, complete, request_hash, FailureCachePolicy, IdempotencyKey, IdempotencyOutcome,
};
use crate::outbox::now_millis;
use sqlx::Either;

/// 幂等记录保留窗口（草稿创建/更新，24h）。
const IDEMPOTENCY_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// 草稿路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/drafts", get(list_drafts).post(create_draft))
        .route(
            "/api/v1/drafts/{id}",
            get(get_owned_draft)
                .patch(update_draft)
                .delete(delete_draft),
        )
        .route("/api/v1/drafts/{id}/preview", post(preview_draft))
}

#[derive(Deserialize)]
struct CreateDraftRequest {
    r#type: String,
    title: String,
    markdown: String,
    #[serde(default)]
    board_id: Option<String>,
    #[serde(default)]
    visibility_level: Option<u32>,
    access_policy: String,
    #[serde(default)]
    scheduled_at: Option<i64>,
    client_request_id: String,
}

#[derive(Deserialize)]
struct UpdateDraftRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    markdown: Option<String>,
    #[serde(default)]
    board_id: Option<String>,
    #[serde(default)]
    visibility_level: Option<u32>,
    #[serde(default)]
    access_policy: Option<String>,
    #[serde(default)]
    scheduled_at: Option<Option<i64>>,
}

#[derive(Deserialize)]
struct ListDraftsQuery {
    /// keyset 游标：上一页最后一条的 `updated_at`（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    30
}

/// 草稿 → API JSON（契约 Draft = ResourceMeta + 内容字段）。
fn draft_json(d: &Draft) -> Value {
    json!({
        "id": d.id,
        "version": d.version,
        "created_at": d.created_at,
        "updated_at": d.updated_at,
        "type": d.post_type.as_str(),
        "title": d.title,
        "markdown": d.markdown,
        "board_id": d.board_id,
        "visibility_level": d.visibility_level,
        "access_policy": d.access_policy,
        "scheduled_at": d.scheduled_at,
    })
}

fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 读取作者当前等级（真实来源 M7 经验账户；缓存重建前 default 1）。
async fn author_level(pool: &DatabasePool, user_id: &str) -> Result<u32, AppError> {
    let request_id = "author_level";
    let level: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(level.unwrap_or(1).clamp(1, u32::MAX as i64) as u32)
}

/// 权限判定辅助（异步）。
async fn check_permission(
    pool: &DatabasePool,
    user_id: &str,
    permission: &str,
    request_id: &'static str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, permission, None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            format!("{permission} permission required"),
            request_id,
        ));
    }
    Ok(())
}

/// POST /api/v1/drafts — 创建草稿（幂等：client_request_id）。
async fn create_draft(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "create_draft";
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
    check_permission(pool, &user.id, "post.create", request_id).await?;

    let req: CreateDraftRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let hash = request_hash(&body);

    let level = author_level(pool, &user.id).await?;
    let now = now_millis();
    let input = CreateDraftInput {
        post_type: req.r#type,
        title: req.title,
        markdown: req.markdown,
        board_id: req.board_id,
        visibility_level: req.visibility_level,
        access_policy: req.access_policy,
        scheduled_at: req.scheduled_at,
        client_request_id: req.client_request_id.clone(),
    };
    let cmd = validate_draft_create(input, level, now).map_err(map_create_error)?;

    // 幂等：scope `draft.create`，key = client_request_id
    let idem_key = IdempotencyKey::new("draft.create", &req.client_request_id)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let outcome = begin_or_replay(
        pool,
        &idem_key,
        &hash,
        IDEMPOTENCY_TTL_MS,
        FailureCachePolicy::Cache,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let draft = Draft {
        id: uuid::Uuid::now_v7().to_string(),
        owner_id: user.id.clone(),
        board_id: cmd.board_id.map(|b| b.to_string()),
        post_type: cmd.post_type,
        title: cmd.title.to_string(),
        markdown: cmd.markdown.to_string(),
        visibility_level: cmd.visibility_level.map(i64::from),
        access_policy: Some(cmd.access_policy.as_str().to_string()),
        scheduled_at: cmd.scheduled_at,
        version: 1,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };

    match outcome {
        IdempotencyOutcome::Created { record_id } => {
            insert_draft(pool, &draft)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let _ = complete(pool, &record_id, &draft.id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let resp = (StatusCode::CREATED, Json(draft_json(&draft))).into_response();
            Ok(private_no_store(resp))
        }
        IdempotencyOutcome::Replay { response_reference } => {
            // 相同 key+摘要重放：返回原草稿
            match response_reference {
                Some(draft_id) => {
                    let existing = get_draft(pool, &draft_id, &user.id)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    match existing {
                        Some(d) => {
                            let resp = (StatusCode::CREATED, Json(draft_json(&d))).into_response();
                            Ok(private_no_store(resp))
                        }
                        None => Err(AppError::conflict(
                            "idempotent replay but original draft not found",
                            request_id,
                        )),
                    }
                }
                None => Err(AppError::conflict(
                    "idempotent replay but no original reference",
                    request_id,
                )),
            }
        }
        IdempotencyOutcome::InProgress => Err(AppError::conflict(
            "request already in progress",
            request_id,
        )),
        IdempotencyOutcome::Conflict => Err(AppError::conflict(
            "idempotency key reused with different request",
            request_id,
        )),
        IdempotencyOutcome::Failed { .. } => Err(AppError::conflict(
            "previous attempt failed; retry with a new idempotency key",
            request_id,
        )),
    }
}

/// GET /api/v1/drafts/{id} — 读取自己的草稿（他人/不存在一律 404）。
async fn get_owned_draft(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "get_draft";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    check_permission(pool, &user.id, "post.read_own", request_id).await?;

    let draft = get_draft(pool, &id, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("draft not found", request_id))?;

    let resp = (StatusCode::OK, Json(draft_json(&draft))).into_response();
    Ok(private_no_store(resp))
}

/// PATCH /api/v1/drafts/{id} — 更新草稿（If-Match 版本冲突 → 409）。
async fn update_draft(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    if_match: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "update_draft";
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
    check_permission(pool, &user.id, "post.edit_own", request_id).await?;

    let req: UpdateDraftRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;

    let current = get_draft(pool, &id, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("draft not found", request_id))?;

    // If-Match 版本校验
    let expected_version = if_match
        .get(header::IF_MATCH)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header required", request_id, None))?;
    let expected: i64 = expected_version.parse().map_err(|_| {
        AppError::bad_request("If-Match must be an integer version", request_id, None)
    })?;
    if expected != current.version {
        return Err(AppError::version_conflict(
            "draft version mismatch",
            request_id,
        ));
    }

    let level = author_level(pool, &user.id).await?;
    let now = now_millis();
    let patch = validate_draft_patch(
        DraftPatchInput {
            title: req.title,
            markdown: req.markdown,
            board_id: req.board_id,
            visibility_level: req.visibility_level,
            access_policy: req.access_policy,
            scheduled_at: req.scheduled_at,
        },
        level,
        now,
    )
    .map_err(map_create_error)?;

    let updated = Draft {
        id: current.id,
        owner_id: current.owner_id,
        board_id: match patch.board_id {
            Some(b) => Some(b.to_string()),
            None => current.board_id,
        },
        post_type: current.post_type,
        title: match patch.title {
            Some(t) => t.to_string(),
            None => current.title,
        },
        markdown: match patch.markdown {
            Some(m) => m.to_string(),
            None => current.markdown,
        },
        visibility_level: match patch.visibility_level {
            Some(lv) => Some(i64::from(lv)),
            None => current.visibility_level,
        },
        access_policy: match patch.access_policy {
            Some(p) => Some(p.as_str().to_string()),
            None => current.access_policy,
        },
        scheduled_at: match patch.scheduled_at {
            Some(ts) => ts,
            None => current.scheduled_at,
        },
        version: current.version,
        created_at: current.created_at,
        updated_at: now,
        deleted_at: None,
    };
    repo_update_draft(pool, &updated)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    // 重新读取（version 已递增）以返回准确结果
    let saved = get_draft(pool, &id, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("draft not found", request_id))?;
    let resp = (StatusCode::OK, Json(draft_json(&saved))).into_response();
    Ok(private_no_store(resp))
}

/// DELETE /api/v1/drafts/{id} — 软删除（可选 If-Match 防并发覆盖）。
async fn delete_draft(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    if_match: axum::http::HeaderMap,
) -> Result<Response, AppError> {
    let request_id = "delete_draft";
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
    check_permission(pool, &user.id, "post.edit_own", request_id).await?;

    let current = get_draft(pool, &id, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("draft not found", request_id))?;

    // 可选 If-Match：存在则必须匹配
    if let Some(raw) = if_match.get(header::IF_MATCH).and_then(|v| v.to_str().ok()) {
        let expected: i64 = raw.parse().map_err(|_| {
            AppError::bad_request("If-Match must be an integer version", request_id, None)
        })?;
        if expected != current.version {
            return Err(AppError::version_conflict(
                "draft version mismatch",
                request_id,
            ));
        }
    }

    let now = now_millis();
    repo_delete_draft(pool, &id, &user.id, now)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (StatusCode::NO_CONTENT, Json(json!({}))).into_response();
    Ok(private_no_store(resp))
}

#[derive(Deserialize)]
struct PreviewDraftRequest {
    markdown: String,
    #[serde(default)]
    restricted_markdown: Option<String>,
}

/// POST /api/v1/drafts/{id}/preview — 当前用户临时安全 HTML 预览。
///
/// - 只渲染**当前用户自己的草稿**（他人/不存在 → 404）；
/// - 经 [`render_for_preview`] 全管线（CommonMark → allowlist 清洗 → 公开
///   摘要）产出临时 HTML，**不写任何数据库行**（不落 post_contents、不入
///   搜索索引、不写缓存）；响应 `Cache-Control: private, no-store`。
async fn preview_draft(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    Json(req): Json<PreviewDraftRequest>,
) -> Result<Response, AppError> {
    let request_id = "preview_draft";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    check_permission(pool, &user.id, "post.read_own", request_id).await?;

    // 归属校验：预览只能作用于自己的草稿（不泄露他人草稿存在性）
    let _draft = get_draft(pool, &id, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("draft not found", request_id))?;

    let rendered = render_for_preview(&req.markdown, req.restricted_markdown.as_deref());
    let body = Json(json!({
        "html": rendered.body_html,
        "restricted_html": rendered.restricted_html,
        "excerpt": rendered.excerpt,
    }));
    let resp = (StatusCode::OK, body).into_response();
    Ok(private_no_store(resp))
}

/// GET /api/v1/drafts — 自己的草稿 cursor 列表（keyset on updated_at DESC）。
async fn list_drafts(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<ListDraftsQuery>,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_drafts";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    check_permission(pool, &user.id, "post.read_own", request_id).await?;

    let limit = query.limit.clamp(1, 100);
    // `after` 游标 = 上一页最后一条 updated_at（keyset 取 updated_at < after）
    let before = match query.after.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(raw.parse::<i64>().map_err(|_| {
            AppError::bad_request("after must be an integer cursor", request_id, None)
        })?),
    };

    let items = list_drafts_cursor(pool, &user.id, before, limit)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let next_cursor = if items.len() as i64 == limit {
        items
            .last()
            .map(|d| d.updated_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    Ok(Json(json!({
        "items": items.iter().map(draft_json).collect::<Vec<Value>>(),
        "next_cursor": next_cursor,
    })))
}

/// 创建命令校验错误 → Problem detail（稳定消息，无原始输入回显）。
fn map_create_error(err: PostCreateError) -> AppError {
    AppError::bad_request(err.to_string(), "create_draft", None)
}
