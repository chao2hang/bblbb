use axum::{
    extract::{Path, State},
    response::Json,
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::download::service::{download, get_authorization, sign_url, DownloadError};
use crate::error::AppError;

/// 下载授权与抵扣路由（M06-DOWNLOAD）。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/attachments/{id}/download", post(post_download))
        .route(
            "/api/v1/attachments/{id}/download-policy",
            get(get_download_policy),
        )
        .route(
            "/api/v1/download-authorizations/{id}",
            get(get_authorization_route),
        )
        .route(
            "/api/v1/download-authorizations/{id}/sign-url",
            post(sign_url_route),
        )
        .route(
            "/api/v1/me/download-transactions",
            get(get_me_download_transactions),
        )
}

#[derive(Deserialize)]
struct DownloadBody {
    idempotency_key: Option<String>,
}

fn download_error_to_app(e: DownloadError, request_id: &str) -> AppError {
    match e {
        DownloadError::NotFound(m) => AppError::not_found(m, request_id),
        DownloadError::Invalid(m) => AppError::bad_request(m, request_id, None),
        DownloadError::Forbidden(m) => AppError::forbidden(m, request_id),
        DownloadError::IdempotencyConflict => {
            AppError::conflict("idempotency conflict", request_id)
        }
        DownloadError::Unavailable(m) => {
            AppError::bad_request(format!("download_url_unavailable: {m}"), request_id, None)
        }
        DownloadError::Db(m) => AppError::internal(m, request_id),
    }
}

async fn post_download(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<DownloadBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_download";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let storage = state
        .storage
        .as_deref()
        .ok_or_else(|| AppError::internal("storage not configured", request_id))?;
    let decision = authorize_action(
        pool,
        &user.id,
        "download.create",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("download not allowed", request_id));
    }
    let key = body.idempotency_key.unwrap_or_default();
    download(pool, storage, &user.id, &id, &key)
        .await
        .map(Json)
        .map_err(|e| download_error_to_app(e, request_id))
}

/// 公开展示该附件生效下载策略摘要（不泄漏价格决策细节以外信息）。
async fn get_download_policy(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_download_policy";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    // 读取最新有效策略（附件级），仅暴露展示字段。
    match pool {
        sqlx::Either::Left(p) => {
            let row = sqlx::query(
                "SELECT mode, amount, currency_id FROM download_billing_policies \
                 WHERE scope_type = 'attachment' AND scope_id = ? AND is_enabled = 1 \
                 ORDER BY version DESC LIMIT 1",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            match row {
                Some(r) => Ok(Json(json!({
                    "attachment_id": id,
                    "mode": r.get::<String,_>("mode"),
                    "amount": r.get::<i64,_>("amount"),
                    "currency_id": r.get::<Option<String>,_>("currency_id"),
                }))),
                None => Ok(Json(
                    json!({ "attachment_id": id, "mode": "free", "amount": 0 }),
                )),
            }
        }
        sqlx::Either::Right(p) => {
            let row = sqlx::query(
                "SELECT mode, amount, currency_id FROM download_billing_policies \
                 WHERE scope_type = 'attachment' AND scope_id = ? AND is_enabled = 1 \
                 ORDER BY version DESC LIMIT 1",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            match row {
                Some(r) => Ok(Json(json!({
                    "attachment_id": id,
                    "mode": r.get::<String,_>("mode"),
                    "amount": r.get::<i64,_>("amount"),
                    "currency_id": r.get::<Option<String>,_>("currency_id"),
                }))),
                None => Ok(Json(
                    json!({ "attachment_id": id, "mode": "free", "amount": 0 }),
                )),
            }
        }
    }
}

async fn get_authorization_route(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_authorization";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    get_authorization(pool, &user.id, &id)
        .await
        .map(Json)
        .map_err(|e| download_error_to_app(e, request_id))
}

async fn sign_url_route(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "sign_url";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let storage = state
        .storage
        .as_deref()
        .ok_or_else(|| AppError::internal("storage not configured", request_id))?;
    sign_url(pool, storage, &user.id, &id)
        .await
        .map(Json)
        .map_err(|e| download_error_to_app(e, request_id))
}

/// 当前用户下载扣费流水（关联账本 point_transactions）。
async fn get_me_download_transactions(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_download_transactions";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(
        pool,
        &user.id,
        "download.read_own",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden("not allowed", request_id));
    }
    match pool {
        sqlx::Either::Left(p) => {
            let rows = sqlx::query(
                "SELECT pt.id, pt.operation_id, pt.currency_id, pt.delta_balance, pt.balance_after, pt.created_at
                 FROM point_transactions pt
                 JOIN point_operations po ON po.id = pt.operation_id
                 WHERE pt.user_id = ? AND po.source_type = 'attachment' AND po.kind = 'consume'
                 ORDER BY pt.created_at DESC",
            )
            .bind(&user.id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let items: Vec<Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "id": r.get::<String,_>("id"),
                        "operation_id": r.get::<String,_>("operation_id"),
                        "currency_id": r.get::<String,_>("currency_id"),
                        "delta_balance": r.get::<i64,_>("delta_balance"),
                        "balance_after": r.get::<i64,_>("balance_after"),
                        "created_at": r.get::<i64,_>("created_at"),
                    })
                })
                .collect();
            Ok(Json(json!({ "transactions": items })))
        }
        sqlx::Either::Right(p) => {
            let rows = sqlx::query(
                "SELECT pt.id, pt.operation_id, pt.currency_id, pt.delta_balance, pt.balance_after, pt.created_at
                 FROM point_transactions pt
                 JOIN point_operations po ON po.id = pt.operation_id
                 WHERE pt.user_id = ? AND po.source_type = 'attachment' AND po.kind = 'consume'
                 ORDER BY pt.created_at DESC",
            )
            .bind(&user.id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let items: Vec<Value> = rows
                .iter()
                .map(|r| {
                    json!({
                        "id": r.get::<String,_>("id"),
                        "operation_id": r.get::<String,_>("operation_id"),
                        "currency_id": r.get::<String,_>("currency_id"),
                        "delta_balance": r.get::<i64,_>("delta_balance"),
                        "balance_after": r.get::<i64,_>("balance_after"),
                        "created_at": r.get::<i64,_>("created_at"),
                    })
                })
                .collect();
            Ok(Json(json!({ "transactions": items })))
        }
    }
}
