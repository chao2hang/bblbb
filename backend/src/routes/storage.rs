//! M06 附件路由：list/create/upload(stream)/get/delete/complete/content。
//!
//! OpenAPI 路由契约（与 openapi.yaml 一致）：
//! - `POST /api/v1/attachments`（createAttachment，x-permission: attachment.upload）
//! - `GET/DELETE /api/v1/attachments/{id}`（get/delete_attachments_id_）
//! - `POST /api/v1/attachments/{id}/complete`（post_attachments_id_complete）
//! - `GET /api/v1/attachments/{id}/content`（公共内容端点：ready+is_public 或已授权）
//!
//! 扩展端点（非冻结契约，GAP-FIX）：`GET /api/v1/attachments`——本人附件
//! 列表 + 当前等级容量摘要（前台「我的附件」页数据源；注册于
//! scripts/check-route-coverage.rb `DOCUMENTED_NON_CONTRACT`，docs/API.md §12）。
//!
//! 另提供 Rust stream 传输端点 `PUT /api/v1/attachments/{id}`（local 后端；
//! S3 后端经 create 返回的预签名 PUT URL 直传，M06-UPLOAD-03）。
//!
//! 下载策略/授权/签名 URL 路由归属 download 域 agent（routes/download.rs）。

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{
    app::AppState,
    auth::session::AuthSession,
    authz::decision::AUTHZ_POLICY_VERSION,
    authz::enforce::authorize_action,
    db::DatabasePool,
    error::{AppError, Problem},
    idempotency::{
        begin_or_replay, complete, request_hash, FailureCachePolicy, IdempotencyKey,
        IdempotencyOutcome,
    },
    outbox::now_millis,
    storage::{
        error::StorageError,
        model::{AttachmentRecord, AttachmentStatus, QuotaCounters, QuotaPolicy, StorageBackend},
        quota::{daily_upload_bytes, get_counters, get_policy_for_level, PRESIGN_TTL_SECS},
        upload::{
            self, complete_attachment as complete_attachment_service,
            create_attachment as create_attachment_service,
            delete_attachment as delete_attachment_service, list_attachments_for_owner,
            stream_upload as stream_upload_service, CreateAttachmentInput, CreateOutcome,
            UploadTransport,
        },
    },
};

/// 附件路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/attachments",
            get(list_my_attachments).post(create_attachment),
        )
        .route(
            "/api/v1/attachments/{id}",
            get(get_attachment)
                .delete(delete_attachment)
                .put(upload_attachment),
        )
        .route(
            "/api/v1/attachments/{id}/complete",
            post(complete_attachment),
        )
        .route(
            "/api/v1/attachments/{id}/content",
            get(get_attachment_content),
        )
}

// ────────────────────────── 本人附件列表 + 容量摘要 ────────────────────────

/// GET /api/v1/attachments 查询参数（`limit` 缺省 100，钳制 1..=100）。
#[derive(Debug, Deserialize)]
struct ListAttachmentsQuery {
    limit: Option<i64>,
}

/// 容量摘要投影（M06-QUOTA 口径；前台「我的附件」页与上传组件共用形状）。
///
/// - `used_bytes` = `bytes_charged`（已就绪计费字节）；
/// - `remaining_bytes` = max(0, total − charged − reserved)（与预留校验一致）；
/// - `daily_used_bytes` 为滚动 24h 已上传字节。
fn quota_summary_json(
    level: i64,
    policy: &QuotaPolicy,
    counters: &QuotaCounters,
    daily_used: i64,
    upload_types: &[&str],
) -> Value {
    let remaining = policy
        .total_bytes
        .saturating_sub(counters.bytes_charged)
        .saturating_sub(counters.bytes_reserved)
        .max(0);
    json!({
        "level": level,
        "max_file_bytes": policy.single_file_max_bytes,
        "total_bytes": policy.total_bytes,
        "used_bytes": counters.bytes_charged,
        "remaining_bytes": remaining,
        "reserved_bytes": counters.bytes_reserved,
        "charged_bytes": counters.bytes_charged,
        "daily_upload_bytes": policy.daily_upload_bytes,
        "daily_used_bytes": daily_used,
        "retention_days": policy.retention_days,
        // 站点上传类型策略（管理后台可配置）：前端 accept/预校验数据源。
        "allowed_media_types": upload_types,
    })
}

/// 读取用户当前信任等级（users.trust_level，TL0–4；缺省 0，与
/// upload::current_trust_level 同口径——配额档位键 2026-09 起为信任等级）。
async fn requester_level(pool: &DatabasePool, user_id: &str) -> Result<i64, AppError> {
    let level: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), "listMyAttachments"))?,
        Either::Right(p) => sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), "listMyAttachments"))?,
    };
    Ok(level.unwrap_or(0).max(0))
}

/// GET /api/v1/attachments — 本人附件列表 + 容量摘要（前台「我的附件」页）。
///
/// 非冻结契约的扩展端点（同 admin/levels 先例：注册于
/// `scripts/check-route-coverage.rb` 的 `DOCUMENTED_NON_CONTRACT`，记录于
/// docs/API.md §12）。auth 后返回本人非 deleted 附件（created_at 倒序，
/// ≤100 条）与当前等级生效配额摘要（等级无修订时以站点默认 seed，与上传
/// 路径同源）。不泄漏 storage_key/sha256 之外的存储细节（与 get_attachment
/// 同脱敏口径）。
async fn list_my_attachments(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(q): Query<ListAttachmentsQuery>,
) -> Result<Response, AppError> {
    let request_id = "listMyAttachments";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = q.limit.unwrap_or(100).clamp(1, 100);
    let records = list_attachments_for_owner(pool, &user.id, limit)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let items: Vec<Value> = records
        .iter()
        .filter(|a| a.status != AttachmentStatus::Deleted)
        .map(attachment_json)
        .collect();

    let level = requester_level(pool, &user.id).await?;
    let policy = get_policy_for_level(pool, level, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let counters = get_counters(pool, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let daily_used = daily_upload_bytes(pool, &user.id, now_millis())
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let upload_types = upload::load_upload_type_policy(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .allowed_media_types();

    let mut response = Json(json!({
        "items": items,
        "quota": quota_summary_json(level, &policy, &counters, daily_used, &upload_types),
    }))
    .into_response();
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(response)
}

/// POST /api/v1/attachments — 创建附件（两阶段上传第一步，M06-UPLOAD-01/02）。
///
/// 服务端权威流程：auth → 邮箱门 → `attachment.upload` 权限（含账号状态门：
/// 未验证/冷静期/封禁/mute 实时裁决）→ 解析声明（filename/size/媒体类型，
/// `additionalProperties: false`）→ 幂等门（scope `attachment.create`，
/// 同 key+摘要重放返回原附件）→ create 服务：等级配额策略重读 + 预留容量 +
/// pending 行 + object key → 传输通道（local Rust stream / S3 预签名 PUT）。
async fn create_attachment(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "createAttachment";
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
    let storage = state
        .storage
        .as_deref()
        .ok_or_else(|| AppError::feature_disabled("storage is not configured", request_id))?;

    let decision = authorize_action(
        pool,
        &user.id,
        "attachment.upload",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(deny_to_app_error(decision, request_id));
    }

    let req: CreateAttachmentRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let hash = request_hash(&body);
    let idem_key = idempotency_key_from_headers(&headers, request_id)?;

    let outcome = begin_or_replay(
        pool,
        &idem_key,
        &hash,
        24 * 60 * 60 * 1000,
        FailureCachePolicy::Cache,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match outcome {
        IdempotencyOutcome::Created { record_id } => {
            let created = match create_attachment_service(
                pool,
                storage,
                &user.id,
                CreateAttachmentInput {
                    owner_id: user.id.clone(),
                    original_name: req.filename.clone(),
                    media_type: req.declared_media_type.clone(),
                    size_bytes: req.size,
                    // 图片附件（编辑器插入正文/回复展示）创建即公开：内容端点
                    // `GET /attachments/{id}/content` 对非作者要求 is_public，
                    // 否则同帖其他读者全部 403（非图片附件保持私有，仍走下载授权）。
                    is_public: req.declared_media_type.starts_with("image/"),
                },
                now_millis(),
            )
            .await
            {
                Ok(created) => created,
                Err(e) => return Ok(storage_error_response(e, request_id)),
            };

            // 若声明了 target（封面/图库等预绑定意图），仅校验目标存在，
            // 不在此建立引用（未 ready 附件禁止关联公开内容，M06-UPLOAD-07）。
            if let (Some(tt), Some(tid)) = (&req.target_type, &req.target_id) {
                if let Err(e) = validate_target_exists(pool, tt, tid, request_id).await {
                    let _ = delete_attachment_service(
                        pool,
                        &user.id,
                        &created.attachment.id,
                        now_millis(),
                    )
                    .await;
                    return Err(e);
                }
            }

            let _ = complete(pool, &record_id, &created.attachment.id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(created_response(created, request_id))
        }
        IdempotencyOutcome::Replay { response_reference } => {
            if let Some(attachment_id) = response_reference {
                if let Ok(Some(attachment)) = upload::load_attachment(pool, &attachment_id).await {
                    if attachment.owner_id != user.id {
                        return Err(AppError::not_found("attachment not found", request_id));
                    }
                    // 重放：重新组装传输通道（S3 重新签发短 TTL PUT URL）
                    let transport = match attachment.storage_backend {
                        StorageBackend::S3 => {
                            let adapter = match storage.adapter(StorageBackend::S3) {
                                Ok(a) => a,
                                Err(e) => return Ok(storage_error_response(e, request_id)),
                            };
                            let key = attachment.storage_key.clone();
                            let media_type = attachment.media_type.clone();
                            match adapter
                                .presign_upload(&key, &media_type, PRESIGN_TTL_SECS)
                                .await
                            {
                                Ok(p) => UploadTransport::Presigned {
                                    url: p.url,
                                    method: p.method,
                                    expires_at: p.expires_at,
                                },
                                Err(e) => return Ok(storage_error_response(e, request_id)),
                            }
                        }
                        StorageBackend::Local => UploadTransport::Stream,
                    };
                    return Ok(create_response_json(
                        attachment,
                        Some(transport),
                        StatusCode::CREATED,
                        request_id,
                    ));
                }
            }
            Err(AppError::conflict(
                "idempotent replay but original attachment not found",
                request_id,
            ))
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

/// PUT /api/v1/attachments/{id} — Rust stream 上传（local 后端，M06-UPLOAD-03）。
///
/// 服务端复检：Content-Length 与 create 声明大小一致、Content-Type 与声明
/// 类型一致（octet-stream 放行）；写入本地对象后状态 pending → processing。
async fn upload_attachment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "uploadAttachmentStream";
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
    let storage = state
        .storage
        .as_deref()
        .ok_or_else(|| AppError::feature_disabled("storage is not configured", request_id))?;

    let decision = authorize_action(
        pool,
        &user.id,
        "attachment.upload",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(deny_to_app_error(decision, request_id));
    }

    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(str::to_string);

    let updated =
        match stream_upload_service(pool, storage, &id, &user.id, &body, content_type.as_deref())
            .await
        {
            Ok(updated) => updated,
            Err(e) => {
                // M15-OBSERVE-05：上传失败指标
                crate::observability::metrics::registry()
                    .counter_inc("bblbb_uploads_failed_total", 1);
                return Ok(storage_error_response(e, request_id));
            }
        };

    Ok(attachment_json_response(updated, request_id))
}

/// POST /api/v1/attachments/{id}/complete — 两阶段上传第二步（M06-UPLOAD-04/05/08）。
///
/// 服务端 HEAD 复检（存在性/大小/metadata 与 create 声明一致）→ 内容安全
/// worker（magic/hash/图片重解码 + EXIF 剥离）→ ready（容量结算 + Outbox）
/// 或 quarantined（安全摘要 + reserved 回滚）。幂等：ready 重放直接返回。
async fn complete_attachment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "postAttachmentsIdComplete";
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
    let storage = state
        .storage
        .as_deref()
        .ok_or_else(|| AppError::feature_disabled("storage is not configured", request_id))?;

    let decision = authorize_action(
        pool,
        &user.id,
        "attachment.upload",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(deny_to_app_error(decision, request_id));
    }

    let req: CompleteAttachmentRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let hash = request_hash(&body);
    // Idempotency-Key 头优先；缺失时回退到 body 的 client_request_id（契约字段）
    let idem_key = match headers.get("idempotency-key").and_then(|v| v.to_str().ok()) {
        Some(key) => IdempotencyKey::new("attachment.complete", key)
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?,
        None => IdempotencyKey::new("attachment.complete", &req.client_request_id)
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?,
    };

    let outcome = begin_or_replay(
        pool,
        &idem_key,
        &hash,
        24 * 60 * 60 * 1000,
        FailureCachePolicy::Cache,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match outcome {
        IdempotencyOutcome::Created { record_id } => {
            let result =
                complete_attachment_service(pool, storage, &id, &user.id, now_millis()).await;
            match result {
                Ok(upload::CompleteOutcome::Ready) => {
                    let _ = complete(pool, &record_id, &id)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    let attachment = upload::load_attachment(pool, &id)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?
                        .ok_or_else(|| {
                            AppError::internal("attachment missing after complete", request_id)
                        })?;
                    Ok(attachment_json_response(attachment, request_id))
                }
                Ok(upload::CompleteOutcome::Quarantined) => {
                    // M15-OBSERVE-05：上传被内容安全检查隔离视为失败
                    crate::observability::metrics::registry()
                        .counter_inc("bblbb_uploads_failed_total", 1);
                    let _ = complete(pool, &record_id, &id)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    let attachment = upload::load_attachment(pool, &id)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?
                        .ok_or_else(|| {
                            AppError::internal("attachment missing after quarantine", request_id)
                        })?;
                    Ok(attachment_json_response(attachment, request_id))
                }
                Err(e) => {
                    // M15-OBSERVE-05：上传失败指标
                    crate::observability::metrics::registry()
                        .counter_inc("bblbb_uploads_failed_total", 1);
                    Ok(storage_error_response(e, request_id))
                }
            }
        }
        IdempotencyOutcome::Replay { response_reference } => {
            let attachment_id = response_reference.unwrap_or(id.clone());
            let attachment = upload::load_attachment(pool, &attachment_id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .ok_or_else(|| AppError::not_found("attachment not found", request_id))?;
            if attachment.owner_id != user.id {
                return Err(AppError::not_found("attachment not found", request_id));
            }
            Ok(attachment_json_response(attachment, request_id))
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

/// GET /api/v1/attachments/{id} — 附件状态投影（脱敏：不泄漏 storage_key）。
///
/// 可见性：本人（任意状态）或公开已 ready 附件（M06-UPLOAD-07）。
async fn get_attachment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "getAttachmentsId";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let attachment = upload::load_attachment(pool, &id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("attachment not found", request_id))?;

    let is_owner = auth
        .user
        .as_ref()
        .is_some_and(|u| u.id == attachment.owner_id);
    let is_public_ready = attachment.status == AttachmentStatus::Ready && attachment.is_public;
    if !is_owner && !is_public_ready {
        return Err(AppError::not_found("attachment not found", request_id));
    }
    Ok(attachment_json_response(attachment, request_id))
}

/// DELETE /api/v1/attachments/{id} — 软删除进入 30 天保留（M06-QUOTA-09）。
async fn delete_attachment(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    _body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "deleteAttachmentsId";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let deleted = match delete_attachment_service(pool, &user.id, &id, now_millis()).await {
        Ok(deleted) => deleted,
        Err(e) => return Ok(storage_error_response(e, request_id)),
    };
    Ok(attachment_json_response(deleted, request_id))
}

/// GET /api/v1/attachments/{id}/content — 公共内容端点（M06-QUOTA-08）。
///
/// 授权：`ready + is_public` 或本人或持有有效下载授权。local 后端流式返回
/// 字节；S3 后端签发短 TTL 预签名 GET 后 302 跳转（URL 到期只使 URL 失效，
/// 不改变 ready/对象生命周期/引用/容量）。
async fn get_attachment_content(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "getAttachmentsIdContent";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let storage = state
        .storage
        .as_deref()
        .ok_or_else(|| AppError::feature_disabled("storage is not configured", request_id))?;

    let attachment = upload::load_attachment(pool, &id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .ok_or_else(|| AppError::not_found("attachment not found", request_id))?;

    // 未 ready 附件禁止公开读取（不泄漏状态，M06-UPLOAD-07）
    if attachment.status != AttachmentStatus::Ready {
        return Err(AppError::not_found("attachment not found", request_id));
    }
    let requester = auth.user.as_ref();
    let is_owner = requester.is_some_and(|u| u.id == attachment.owner_id);
    let authorized = is_owner || attachment.is_public;
    if !authorized {
        // 有效下载授权（download 域写入的表；active + 未过期）
        if let Some(user) = requester {
            if has_active_download_authorization(pool, user.id.as_str(), &attachment.id, request_id)
                .await?
            {
                return serve_attachment(storage, &attachment, request_id).await;
            }
        }
        return Err(AppError::forbidden(
            "attachment is not public or authorized",
            request_id,
        ));
    }
    serve_attachment(storage, &attachment, request_id).await
}

/// 是否存在有效下载授权（download_authorizations：active 且窗口内）。
async fn has_active_download_authorization(
    pool: &DatabasePool,
    user_id: &str,
    attachment_id: &str,
    request_id: &str,
) -> Result<bool, AppError> {
    let now = now_millis();
    let exists: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT 1 FROM download_authorizations
             WHERE user_id = ? AND attachment_id = ? AND status = 'active'
               AND valid_from <= ? AND expires_at > ?",
        )
        .bind(user_id)
        .bind(attachment_id)
        .bind(now)
        .bind(now)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT 1 FROM download_authorizations
             WHERE user_id = ? AND attachment_id = ? AND status = 'active'
               AND valid_from <= ? AND expires_at > ?",
        )
        .bind(user_id)
        .bind(attachment_id)
        .bind(now)
        .bind(now)
        .fetch_optional(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    Ok(exists == Some(1))
}

/// 本地流式 / S3 302 跳转。
async fn serve_attachment(
    storage: &crate::storage::StorageService,
    attachment: &AttachmentRecord,
    request_id: &str,
) -> Result<Response, AppError> {
    let adapter = match storage.adapter(attachment.storage_backend) {
        Ok(a) => a,
        Err(e) => return Ok(storage_error_response(e, request_id)),
    };
    match attachment.storage_backend {
        StorageBackend::Local => {
            let data = match adapter.read_object(&attachment.storage_key).await {
                Ok(d) => d,
                Err(e) => return Ok(storage_error_response(e, request_id)),
            };
            let mut resp = (StatusCode::OK, data).into_response();
            resp.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_str(&attachment.media_type)
                    .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
            );
            if let Some(name) = &attachment.original_name {
                if let Ok(v) = HeaderValue::from_str(&format!(
                    "inline; filename*=UTF-8''{}",
                    percent_encode_name(name)
                )) {
                    resp.headers_mut().insert(header::CONTENT_DISPOSITION, v);
                }
            }
            resp.headers_mut().insert(
                header::CACHE_CONTROL,
                if attachment.is_public {
                    HeaderValue::from_static("public, max-age=300")
                } else {
                    HeaderValue::from_static("private, no-store")
                },
            );
            resp.headers_mut().insert(
                header::X_CONTENT_TYPE_OPTIONS,
                HeaderValue::from_static("nosniff"),
            );
            Ok(resp)
        }
        StorageBackend::S3 => {
            let presigned = match adapter
                .presign_download(&attachment.storage_key, PRESIGN_TTL_SECS)
                .await
            {
                Ok(p) => p,
                Err(e) => return Ok(storage_error_response(e, request_id)),
            };
            let mut resp = Response::new(axum::body::Body::empty());
            *resp.status_mut() = StatusCode::FOUND;
            resp.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("private, no-store"),
            );
            resp.headers_mut().insert(
                header::LOCATION,
                HeaderValue::from_str(&presigned.url).map_err(|e| {
                    AppError::internal(format!("invalid presigned url: {e}"), request_id)
                })?,
            );
            Ok(resp)
        }
    }
}

/// 文件名百分号编码（Content-Disposition filename* 用）。
fn percent_encode_name(name: &str) -> String {
    let mut out = String::new();
    for b in name.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b' ') {
            if b == b' ' {
                out.push('%');
                out.push_str("20");
            } else {
                out.push(b as char);
            }
        } else {
            out.push('%');
            out.push_str(&format!("{b:02X}"));
        }
    }
    out
}

// ────────────────────────── 请求/响应映射 ──────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateAttachmentRequest {
    filename: Option<String>,
    size: i64,
    declared_media_type: String,
    target_type: Option<String>,
    target_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CompleteAttachmentRequest {
    client_request_id: String,
}

/// 校验 create 声明的 target 存在性（post/comment/user；不建立引用）。
async fn validate_target_exists(
    pool: &DatabasePool,
    target_type: &str,
    target_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let table = match target_type {
        "post" => "posts",
        "comment" => "comments",
        "user" => "users",
        other => {
            return Err(AppError::bad_request(
                format!("unsupported target_type: {other}"),
                request_id,
                None,
            ));
        }
    };
    let sql = format!("SELECT 1 FROM {table} WHERE id = ?");
    let exists: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(&sql)
            .bind(target_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar(&sql)
            .bind(target_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    if exists != Some(1) {
        return Err(AppError::bad_request(
            "target does not exist",
            request_id,
            None,
        ));
    }
    Ok(())
}

/// 从 Idempotency-Key 头解析幂等键（契约 16-200）。
#[allow(clippy::result_large_err)] // AppError 为统一错误类型（与 auth/session 同约定）
fn idempotency_key_from_headers(
    headers: &HeaderMap,
    request_id: &str,
) -> Result<IdempotencyKey, AppError> {
    let key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            AppError::bad_request("Idempotency-Key header is required", request_id, None)
        })?;
    IdempotencyKey::new("attachment.create", key)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))
}

/// create 响应：附件投影 + 传输通道。
fn created_response(created: CreateOutcome, request_id: &str) -> Response {
    create_response_json(
        created.attachment,
        Some(created.transport),
        StatusCode::CREATED,
        request_id,
    )
}

fn create_response_json(
    attachment: AttachmentRecord,
    transport: Option<UploadTransport>,
    status: StatusCode,
    request_id: &str,
) -> Response {
    let mut upload = json!({ "mode": "stream" });
    if let Some(t) = transport {
        upload = match t {
            UploadTransport::Presigned {
                url,
                method,
                expires_at,
            } => json!({
                "mode": "presigned",
                "url": url,
                "method": method,
                "expires_at": expires_at,
            }),
            UploadTransport::Stream => json!({ "mode": "stream" }),
        };
    }
    let body = json!({
        "id": attachment.id,
        "attachment": attachment_json(&attachment),
        "upload": upload,
    });
    let mut resp = (status, Json(body)).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    let _ = request_id;
    resp
}

/// 附件状态投影（脱敏：不泄漏 storage_key；sha256 仅 ready 返回）。
fn attachment_json(attachment: &AttachmentRecord) -> Value {
    let mut v = json!({
        "id": attachment.id,
        "owner_id": attachment.owner_id,
        "status": attachment.status.as_str(),
        "media_type": attachment.media_type,
        "size_bytes": attachment.size_bytes,
        "original_name": attachment.original_name,
        "width": attachment.width,
        "height": attachment.height,
        "is_public": attachment.is_public,
        "quota_bytes_charged": attachment.quota_bytes_charged,
        "ref_count": attachment.ref_count,
        "processing_version": attachment.processing_version,
        "processing_error": attachment.processing_error,
        "created_at": attachment.created_at,
        "deleted_at": attachment.deleted_at,
    });
    if attachment.status == AttachmentStatus::Ready {
        v["sha256"] = json!(attachment.sha256);
    }
    v
}

fn attachment_json_response(attachment: AttachmentRecord, request_id: &str) -> Response {
    let body = json!({ "attachment": attachment_json(&attachment) });
    let mut resp = (StatusCode::OK, Json(body)).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    let _ = request_id;
    resp
}

/// DenyReason → AppError（deny_to_error 的别名，保持签名一致）。
fn deny_to_app_error(decision: crate::authz::decision::Decision, request_id: &str) -> AppError {
    crate::authz::enforce::deny_to_error(
        crate::authz::enforce::denied_reason(&decision)
            .unwrap_or(crate::authz::decision::DenyReason::DefaultDeny),
        request_id,
    )
}

/// StorageError → 稳定 Problem 响应（storage_verification_failed / quota_exceeded /
/// storage_state_error 等，ERROR-CODES.md / OpenAPI ProblemResponse）。
fn storage_error_response(e: StorageError, request_id: &str) -> Response {
    // M15-OBSERVE-05：存储适配器/处理错误指标
    crate::observability::metrics::registry().counter_inc("bblbb_storage_errors_total", 1);
    let (status, code, title) = match &e {
        StorageError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found", "Not Found"),
        StorageError::Quota(_) => (StatusCode::CONFLICT, "quota_exceeded", "Quota Exceeded"),
        StorageError::State(_) => (StatusCode::CONFLICT, "storage_state_error", "Conflict"),
        StorageError::Verification(_) => (
            StatusCode::CONFLICT,
            "storage_verification_failed",
            "Conflict",
        ),
        StorageError::Conflict(_) => (StatusCode::CONFLICT, "storage_conflict", "Conflict"),
        StorageError::RateLimited(_) => (
            StatusCode::TOO_MANY_REQUESTS,
            "storage_rate_limited",
            "Too Many Requests",
        ),
        StorageError::Forbidden(_) => (StatusCode::FORBIDDEN, "storage_forbidden", "Forbidden"),
        StorageError::Auth(_) => (
            StatusCode::UNAUTHORIZED,
            "storage_auth_failed",
            "Unauthorized",
        ),
        StorageError::PartialUpload(_) => (
            StatusCode::BAD_REQUEST,
            "storage_partial_upload",
            "Bad Request",
        ),
        StorageError::Invalid(_) => (
            StatusCode::BAD_REQUEST,
            "invalid_storage_request",
            "Bad Request",
        ),
        StorageError::Network(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_network_error",
            "Service Unavailable",
        ),
        StorageError::Upstream(_) => (
            StatusCode::SERVICE_UNAVAILABLE,
            "storage_upstream_error",
            "Service Unavailable",
        ),
        _ => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal_error",
            "Internal Server Error",
        ),
    };
    let problem = Problem {
        type_uri: "about:blank",
        title,
        status: status.as_u16(),
        code,
        detail: e.to_string(),
        instance: None,
        request_id: request_id.to_string(),
        errors: None,
    };
    (status, Json(problem)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::model::QuotaCounters;

    #[test]
    fn quota_summary_uses_charged_and_clamps_remaining() {
        // MB 常量便于阅读。
        const MB: i64 = 1024 * 1024;
        let policy = QuotaPolicy {
            level: 2,
            single_file_max_bytes: 5 * MB,
            total_bytes: 250 * MB,
            daily_upload_bytes: 50 * MB,
            retention_days: 30,
            policy_version: 3,
        };

        // 常规：used=charged，remaining=total-charged（无预留）。
        let counters = QuotaCounters {
            bytes_reserved: 0,
            bytes_charged: 100 * MB,
            bytes_released: 0,
        };
        let v = quota_summary_json(2, &policy, &counters, 10 * MB, &["image", "pdf", "text"]);
        assert_eq!(v["max_file_bytes"], 5 * MB);
        assert_eq!(v["total_bytes"], 250 * MB);
        assert_eq!(v["used_bytes"], 100 * MB);
        assert_eq!(v["remaining_bytes"], 150 * MB);
        assert_eq!(v["daily_upload_bytes"], 50 * MB);
        assert_eq!(v["daily_used_bytes"], 10 * MB);
        assert_eq!(v["retention_days"], 30);

        // 预留占用剩余额度：remaining = total − charged − reserved。
        let with_reserved = QuotaCounters {
            bytes_reserved: 20 * MB,
            bytes_charged: 200 * MB,
            bytes_released: 0,
        };
        let v = quota_summary_json(2, &policy, &with_reserved, 0, &[]);
        assert_eq!(v["remaining_bytes"], 30 * MB);

        // 超卖防御：charged+reserved > total 时 remaining 钳制为 0（不为负）。
        let over = QuotaCounters {
            bytes_reserved: 30 * MB,
            bytes_charged: 240 * MB,
            bytes_released: 0,
        };
        let v = quota_summary_json(2, &policy, &over, 0, &[]);
        assert_eq!(v["remaining_bytes"], 0);
    }
}
