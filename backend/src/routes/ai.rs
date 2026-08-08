//! AI 路由（M09-GATEWAY/TASKS/SUGGESTIONS 用户侧）。
//!
//! OpenAPI 契约（冻结 193 ops）：
//! - `GET /api/v1/ai/capabilities`（get_ai_capabilities）
//! - `POST /api/v1/ai/consent` / `DELETE /api/v1/ai/consent`
//! - `POST /api/v1/ai/drafts/{draft_id}/format`
//! - `POST /api/v1/ai/posts/{post_id}/moderation-suggestion`
//! - `POST /api/v1/ai/posts/{post_id}/seo-suggestion`
//! - `GET /api/v1/ai/suggestions/{id}` / `POST /api/v1/ai/suggestions/{id}/accept`
//! - `GET /api/v1/ai/tasks/{id}` / `POST /api/v1/ai/tasks/{id}/cancel`
//!
//! 路由前缀 `/api/v1/ai/` 由 Feature Gate（FeatureName::Ai，默认关闭）门控。

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::ai::consent::{grant_consent, revoke_consent};
use crate::ai::gateway::RedactionMode;
use crate::ai::suggestions::{accept_suggestion, get_suggestion};
use crate::ai::tasks::{cancel_task, enqueue_task, task_state};
use crate::ai::TaskKind;
use crate::app::AppState;
use crate::error::AppError;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/ai/capabilities", get(get_ai_capabilities))
        .route(
            "/api/v1/ai/consent",
            post(post_ai_consent).delete(delete_ai_consent),
        )
        .route("/api/v1/ai/drafts/{draft_id}/format", post(format_draft))
        .route(
            "/api/v1/ai/posts/{post_id}/moderation-suggestion",
            post(moderation_suggestion),
        )
        .route(
            "/api/v1/ai/posts/{post_id}/seo-suggestion",
            post(seo_suggestion),
        )
        .route("/api/v1/ai/suggestions/{id}", get(get_ai_suggestion))
        .route(
            "/api/v1/ai/suggestions/{id}/accept",
            post(accept_ai_suggestion),
        )
        .route("/api/v1/ai/tasks/{id}", get(get_ai_task))
        .route("/api/v1/ai/tasks/{id}/cancel", post(cancel_ai_task))
}

fn ai_error_to_app(e: crate::ai::consent::ConsentError, request_id: &str) -> AppError {
    use crate::ai::consent::ConsentError as CE;
    match e {
        // M16-HARNESS-04：缺少 AI 数据发送独立同意 → 403 `ai_consent_required`。
        CE::NotFound(m) => AppError::with_code(
            axum::http::StatusCode::FORBIDDEN,
            "ai_consent_required",
            "Forbidden",
            m,
            request_id,
        ),
        CE::Invalid(m) => AppError::bad_request(m, request_id, None),
        CE::AlreadyGranted => AppError::conflict("consent already granted", request_id),
        CE::Db(m) => AppError::internal(m, request_id),
    }
}

fn task_error_to_app(e: crate::ai::tasks::TaskError, request_id: &str) -> AppError {
    use crate::ai::tasks::TaskError as TE;
    match e {
        TE::NotFound(m) => AppError::not_found(m, request_id),
        TE::Invalid(m) => AppError::bad_request(m, request_id, None),
        // M16-HARNESS-04：执行前重确认失败（revision/consent 变化）→ `ai_suggestion_stale`。
        TE::Stale { reason } => AppError::with_code(
            axum::http::StatusCode::CONFLICT,
            "ai_suggestion_stale",
            "Conflict",
            reason,
            request_id,
        ),
        TE::Cancelled => AppError::conflict("task cancelled", request_id),
        // M16-HARNESS-04：预算熔断 → 409 `ai_budget_exceeded`；其余 Provider
        // 网关错误保持 `bad_request` + 脱敏 detail（既有行为）。
        TE::Provider(crate::ai::gateway::GatewayError::BudgetExceeded(m)) => AppError::with_code(
            axum::http::StatusCode::CONFLICT,
            "ai_budget_exceeded",
            "Conflict",
            m,
            request_id,
        ),
        TE::Provider(g) => AppError::bad_request(g.code(), request_id, None),
        TE::Consent(ce) => ai_error_to_app(ce, request_id),
        TE::Db(m) => AppError::internal(m, request_id),
    }
}

fn suggestion_error_to_app(
    e: crate::ai::suggestions::SuggestionError,
    request_id: &str,
) -> AppError {
    use crate::ai::suggestions::SuggestionError as SE;
    match e {
        SE::NotFound(m) => AppError::not_found(m, request_id),
        SE::Invalid(m) => AppError::bad_request(m, request_id, None),
        // M16-HARNESS-04：目标 revision 已变化 → 409 `ai_suggestion_stale`。
        SE::VersionConflict { .. } => AppError::with_code(
            axum::http::StatusCode::CONFLICT,
            "ai_suggestion_stale",
            "Conflict",
            "target revision changed; regenerate the suggestion",
            request_id,
        ),
        SE::Forbidden(m) => AppError::forbidden(m, request_id),
        SE::AlreadyAccepted => AppError::conflict("suggestion already accepted", request_id),
        SE::Db(m) => AppError::internal(m, request_id),
    }
}

/// GET /api/v1/ai/capabilities — 能力声明（默认关闭的 Flag 状态 + Provider 脱敏状态）。
async fn get_ai_capabilities(State(state): State<AppState>) -> Result<Json<Value>, AppError> {
    let request_id = "get_ai_capabilities";
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    let enabled = state
        .flags
        .is_enabled(crate::config::flags::FeatureName::Ai, now);
    let pool = state.db.as_deref();
    let providers = match pool {
        Some(pool) => list_providers_redacted(pool, request_id).await?,
        None => vec![],
    };
    Ok(Json(json!({
        "enabled": enabled,
        "capabilities": {
            "formatting": enabled,
            "moderation": enabled,
            "seo": enabled,
            "tagging": enabled,
        },
        "providers": providers,
        "consent_required": true,
    })))
}

async fn list_providers_redacted(
    pool: &crate::db::DatabasePool,
    request_id: &str,
) -> Result<Vec<Value>, AppError> {
    let items: Vec<Value> = match pool {
        Either::Left(p) => sqlx::query(
            "SELECT id, name, adapter_type, base_url, default_model, status, secret_configured, data_mode, region, timeout_ms
             FROM ai_providers ORDER BY name",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(redact_provider_row)
        .collect(),
        Either::Right(p) => sqlx::query(
            "SELECT id, name, adapter_type, base_url, default_model, status, secret_configured, data_mode, region, timeout_ms
             FROM ai_providers ORDER BY name",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(redact_provider_row_mysql)
        .collect(),
    };
    Ok(items)
}

fn redact_provider_row(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "name": r.get::<String,_>("name"),
        "adapter_type": r.get::<String,_>("adapter_type"),
        "default_model": r.get::<String,_>("default_model"),
        "status": r.get::<String,_>("status"),
        "secret_configured": r.get::<i64,_>("secret_configured") != 0,
        "data_mode": r.get::<String,_>("data_mode"),
        "region": r.get::<Option<String>,_>("region"),
        // Secret 永不外发；base_url host 部分可展示。
        "base_url_host": r.get::<String,_>("base_url"),
    })
}

fn redact_provider_row_mysql(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "name": r.get::<String,_>("name"),
        "adapter_type": r.get::<String,_>("adapter_type"),
        "default_model": r.get::<String,_>("default_model"),
        "status": r.get::<String,_>("status"),
        "secret_configured": r.get::<i64,_>("secret_configured") != 0,
        "data_mode": r.get::<String,_>("data_mode"),
        "region": r.get::<Option<String>,_>("region"),
        // Secret 永不外发；base_url host 部分可展示。
        "base_url_host": r.get::<String,_>("base_url"),
    })
}

#[derive(Deserialize)]
struct ConsentBody {
    provider_id: String,
    purpose: String,
    data_mode: String,
    disclosure_version: i64,
    disclosure_hash: String,
}

/// POST /api/v1/ai/consent — 授予逐次同意（full_with_consent）。
async fn post_ai_consent(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Json(body): Json<ConsentBody>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "post_ai_consent";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    if body.data_mode != "full_with_consent" {
        return Err(AppError::bad_request(
            "data_mode must be full_with_consent",
            request_id,
            None,
        ));
    }
    let purpose = TaskKind::parse(&body.purpose)
        .ok_or_else(|| AppError::bad_request("invalid purpose", request_id, None))?;
    let id = grant_consent(
        pool,
        &user.id,
        &body.provider_id,
        purpose,
        body.disclosure_version,
        &body.disclosure_hash,
        "consent accepted",
        "per_task",
        crate::ai::consent::now(),
    )
    .await
    .map_err(|e| ai_error_to_app(e, request_id))?;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "consent_id": id, "status": "granted" })),
    ))
}

/// DELETE /api/v1/ai/consent — 撤回同意。
async fn delete_ai_consent(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Json(body): Json<ConsentBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "delete_ai_consent";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let purpose = TaskKind::parse(&body.purpose)
        .ok_or_else(|| AppError::bad_request("invalid purpose", request_id, None))?;
    let affected = revoke_consent(
        pool,
        &user.id,
        &body.provider_id,
        purpose,
        "user requested",
        crate::ai::consent::now(),
    )
    .await
    .map_err(|e| ai_error_to_app(e, request_id))?;
    Ok(Json(
        json!({ "revoked": affected, "purpose": body.purpose }),
    ))
}

/// 生成任务输入投影（脱敏 + 最小化）。`body` 为调用方提交的文本。
fn build_input_projection(raw: &str, mode: RedactionMode) -> String {
    crate::ai::gateway::Redactor::redact(raw, mode)
}

#[derive(Deserialize)]
struct GenerateBody {
    content: String,
    #[serde(default)]
    idempotency_key: String,
}

/// POST /api/v1/ai/drafts/{draft_id}/format — 格式化建议任务（202）。
async fn format_draft(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(draft_id): Path<String>,
    Json(body): Json<GenerateBody>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "format_draft";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let provider_id = default_provider(pool, request_id).await?;
    let content_revision = current_draft_revision(pool, &user.id, &draft_id, request_id).await?;
    let key = if body.idempotency_key.is_empty() {
        format!("format-{}-{}", draft_id, content_revision)
    } else {
        body.idempotency_key.clone()
    };
    let task = enqueue_task(
        pool,
        TaskKind::Formatting,
        "draft",
        &draft_id,
        &user.id,
        &provider_id,
        content_revision,
        1,
        None,
        &key,
        1000,
        crate::ai::tasks::now(),
    )
    .await
    .map_err(|e| task_error_to_app(e, request_id))?;
    let _ = build_input_projection(&body.content, RedactionMode::Redacted);
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "task_id": task.id,
            "status": task.status,
            "poll_url": format!("/api/v1/ai/tasks/{}", task.id),
            "cancel_url": format!("/api/v1/ai/tasks/{}/cancel", task.id),
            "source_revision": content_revision,
        })),
    ))
}

/// POST /api/v1/ai/posts/{post_id}/moderation-suggestion — 审核建议任务（202）。
async fn moderation_suggestion(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(post_id): Path<String>,
    Json(body): Json<GenerateBody>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "moderation_suggestion";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    // 审核建议需要 moderation.review 权限。
    let decision = crate::authz::enforce::authorize_action(
        pool,
        &user.id,
        "moderation.review",
        None,
        crate::authz::decision::AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "moderation.review required",
            request_id,
        ));
    }
    let provider_id = default_provider(pool, request_id).await?;
    let content_revision = current_post_revision(pool, &post_id, request_id).await?;
    let key = if body.idempotency_key.is_empty() {
        format!("mod-{}-{}", post_id, content_revision)
    } else {
        body.idempotency_key.clone()
    };
    let task = enqueue_task(
        pool,
        TaskKind::Moderation,
        "post",
        &post_id,
        &user.id,
        &provider_id,
        content_revision,
        1,
        None,
        &key,
        2000,
        crate::ai::tasks::now(),
    )
    .await
    .map_err(|e| task_error_to_app(e, request_id))?;
    let _ = build_input_projection(&body.content, RedactionMode::MetadataOnly);
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "task_id": task.id,
            "status": task.status,
            "poll_url": format!("/api/v1/ai/tasks/{}", task.id),
            "cancel_url": format!("/api/v1/ai/tasks/{}/cancel", task.id),
            "source_revision": content_revision,
        })),
    ))
}

/// POST /api/v1/ai/posts/{post_id}/seo-suggestion — SEO 建议任务（202）。
async fn seo_suggestion(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(post_id): Path<String>,
    Json(body): Json<GenerateBody>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "seo_suggestion";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let provider_id = default_provider(pool, request_id).await?;
    let content_revision = current_post_revision(pool, &post_id, request_id).await?;
    let key = if body.idempotency_key.is_empty() {
        format!("seo-{}-{}", post_id, content_revision)
    } else {
        body.idempotency_key.clone()
    };
    let task = enqueue_task(
        pool,
        TaskKind::Seo,
        "post",
        &post_id,
        &user.id,
        &provider_id,
        content_revision,
        1,
        None,
        &key,
        1000,
        crate::ai::tasks::now(),
    )
    .await
    .map_err(|e| task_error_to_app(e, request_id))?;
    let _ = build_input_projection(&body.content, RedactionMode::Redacted);
    Ok((
        StatusCode::ACCEPTED,
        Json(json!({
            "task_id": task.id,
            "status": task.status,
            "poll_url": format!("/api/v1/ai/tasks/{}", task.id),
            "cancel_url": format!("/api/v1/ai/tasks/{}/cancel", task.id),
            "source_revision": content_revision,
        })),
    ))
}

/// GET /api/v1/ai/tasks/{id} — 用户本人任务投影。
async fn get_ai_task(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_ai_task";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let t = task_state(pool, &user.id, &id)
        .await
        .map_err(|e| task_error_to_app(e, request_id))?;
    Ok(Json(json!({
        "task_id": t.id,
        "task_type": t.task_type,
        "status": t.status,
        "attempt": t.attempt,
        "error_class": t.error_class,
        "created_at": t.created_at,
    })))
}

/// POST /api/v1/ai/tasks/{id}/cancel — 取消。
async fn cancel_ai_task(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "cancel_ai_task";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let t = cancel_task(pool, &user.id, &id, crate::ai::tasks::now())
        .await
        .map_err(|e| task_error_to_app(e, request_id))?;
    Ok(Json(json!({ "task_id": t.id, "status": t.status })))
}

/// GET /api/v1/ai/suggestions/{id} — 建议投影（作者/授权审核员）。
async fn get_ai_suggestion(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_ai_suggestion";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let s = get_suggestion(pool, &user.id, &id)
        .await
        .map_err(|e| suggestion_error_to_app(e, request_id))?;
    Ok(Json(s))
}

#[derive(Deserialize)]
struct AcceptBody {
    expected_base_version: i64,
    #[serde(default)]
    selected_fields: Option<Vec<String>>,
}

/// POST /api/v1/ai/suggestions/{id}/accept — 采纳（版本校验 + 幂等）。
async fn accept_ai_suggestion(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(id): Path<String>,
    Json(body): Json<AcceptBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "accept_ai_suggestion";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    // 当前内容 revision：取建议的 base_revision（目标内容未被新编辑覆盖时相等）。
    let current = current_target_revision(pool, &id, request_id).await?;
    let r = accept_suggestion(
        pool,
        &user.id,
        &id,
        body.expected_base_version,
        current,
        body.selected_fields.as_deref(),
        crate::ai::suggestions::now(),
    )
    .await
    .map_err(|e| suggestion_error_to_app(e, request_id))?;
    Ok(Json(r))
}

// ── 辅助 ────────────────────────────────────────────────────────────────

async fn default_provider(
    pool: &crate::db::DatabasePool,
    request_id: &str,
) -> Result<String, AppError> {
    let id: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT id FROM ai_providers WHERE status = 'enabled' ORDER BY name LIMIT 1",
        )
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT id FROM ai_providers WHERE status = 'enabled' ORDER BY name LIMIT 1",
        )
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    id.ok_or_else(|| AppError::feature_disabled("no ai provider configured", request_id))
}

async fn current_draft_revision(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    draft_id: &str,
    request_id: &str,
) -> Result<i64, AppError> {
    let rev: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT version FROM drafts WHERE id = ? AND author_id = ?")
                .bind(draft_id)
                .bind(user_id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT version FROM drafts WHERE id = ? AND author_id = ?")
                .bind(draft_id)
                .bind(user_id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };
    Ok(rev.unwrap_or(0))
}

async fn current_post_revision(
    pool: &crate::db::DatabasePool,
    post_id: &str,
    request_id: &str,
) -> Result<i64, AppError> {
    let rev: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT version FROM posts WHERE id = ?")
            .bind(post_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT version FROM posts WHERE id = ?")
            .bind(post_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    Ok(rev.unwrap_or(0))
}

async fn current_target_revision(
    pool: &crate::db::DatabasePool,
    suggestion_id: &str,
    request_id: &str,
) -> Result<i64, AppError> {
    let rev: Option<(String, String, i64)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT target_type, target_id, base_revision FROM ai_suggestions WHERE id = ?",
        )
        .bind(suggestion_id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT target_type, target_id, base_revision FROM ai_suggestions WHERE id = ?",
        )
        .bind(suggestion_id)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let Some((target_type, target_id, base)) = rev else {
        return Err(AppError::not_found("suggestion not found", request_id));
    };
    match target_type.as_str() {
        "post" => Ok(current_post_revision(pool, &target_id, request_id)
            .await
            .unwrap_or(base)),
        "draft" => Ok(base),
        _ => Ok(base),
    }
}
