//! 社交域 API 密钥路由（GAP-FIX 社交域）。
//!
//! - `GET /api/v1/me/api-keys`：本人密钥列表（不含明文，只含 prefix）；
//! - `POST /api/v1/me/api-keys`：创建密钥 → 201（**明文 key 仅此一次返回**）；
//! - `DELETE /api/v1/me/api-keys/{id}`：软删除（`revoked_at` 置位，立即失效）
//!   → 204。
//!
//! 安全设计：
//! - 明文格式 `bblbb_<32 位随机 base62>`（熵来自 `OsRng`，与 auth token
//!   同源）；数据库只存 SHA-256 hex（复用 `crate::auth::token::hash_token`），
//!   `prefix` = 明文前 12 字符（`bblbb_` + 6 位随机段，供人工识别）；
//! - scopes 白名单：`posts:read | drafts:write | notifications:read | me:read`
//!   的子集，存 JSON 数组文本；
//! - 幂等：`client_request_id` 走 idempotency_records（scope
//!   `apikey.create`）。**重放取舍**：明文 key 只在首次创建时返回一次，
//!   同 key 重放无法重新出示明文——返回 409 并说明「密钥已创建，明文仅
//!   首次返回」，避免误导客户端（也不产生重复行）。

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use rand::{rngs::OsRng, RngCore};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::{
    app::AppState,
    auth::session::AuthSession,
    auth::token::hash_token,
    error::AppError,
    idempotency::{
        begin_or_replay, complete, request_hash, FailureCachePolicy, IdempotencyKey,
        IdempotencyOutcome,
    },
    outbox::now_millis,
};

/// 幂等记录保留窗口（创建密钥，24h）。
const IDEMPOTENCY_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// 允许的 scope 白名单。
pub const ALLOWED_SCOPES: &[&str] = &[
    "posts:read",
    "drafts:write",
    "notifications:read",
    "me:read",
];
/// 明文 key 随机段长度（base62）。
const KEY_RANDOM_LEN: usize = 32;
/// prefix 长度（明文前 12 字符 = `bblbb_` + 6 位随机段）。
const PREFIX_LEN: usize = 12;
/// 密钥名长度。
const NAME_MIN: usize = 1;
const NAME_MAX: usize = 64;

/// API 密钥路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/me/api-keys",
            get(list_my_api_keys).post(create_api_key),
        )
        .route(
            "/api/v1/me/api-keys/{id}",
            axum::routing::delete(revoke_api_key),
        )
}

#[derive(Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    #[serde(default)]
    scopes: Vec<String>,
    client_request_id: String,
}

/// 私有响应统一 `Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 生成明文 API key：`bblbb_` + 32 位 base62 随机段（OsRng）。
fn generate_api_key() -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
    let mut random = [0u8; KEY_RANDOM_LEN];
    OsRng.fill_bytes(&mut random);
    let mut key = String::with_capacity(6 + KEY_RANDOM_LEN);
    key.push_str("bblbb_");
    for byte in random {
        key.push(ALPHABET[(byte as usize) % ALPHABET.len()] as char);
    }
    key
}

/// 校验 scopes：必须为白名单子集（去重、保序）。
#[allow(clippy::result_large_err)] // AppError 为路由层统一错误载体（体积固定可接受）
fn validate_scopes(scopes: &[String], request_id: &str) -> Result<Vec<String>, AppError> {
    let mut seen = std::collections::HashSet::new();
    let mut result = Vec::new();
    for scope in scopes {
        if !ALLOWED_SCOPES.contains(&scope.as_str()) {
            return Err(AppError::bad_request(
                format!("unsupported scope: {scope}"),
                request_id,
                None,
            ));
        }
        if seen.insert(scope.clone()) {
            result.push(scope.clone());
        }
    }
    Ok(result)
}

/// 本人密钥行投影。
#[derive(sqlx::FromRow)]
struct ApiKeyRow {
    id: String,
    name: String,
    prefix: String,
    scopes: String,
    created_at: i64,
    last_used_at: Option<i64>,
    revoked_at: Option<i64>,
}

/// 行 → JSON（scopes 反序列化为数组；secret_hash 绝不返回）。
fn api_key_json(r: &ApiKeyRow) -> Value {
    let scopes: Vec<String> = serde_json::from_str(&r.scopes).unwrap_or_default();
    json!({
        "id": r.id,
        "name": r.name,
        "prefix": r.prefix,
        "scopes": scopes,
        "created_at": r.created_at,
        "last_used_at": r.last_used_at,
        "revoked_at": r.revoked_at,
    })
}

/// GET /api/v1/me/api-keys — 本人密钥列表（created_at DESC）。
async fn list_my_api_keys(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "list_my_api_keys";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let sql = "SELECT id, name, prefix, scopes, created_at, last_used_at, revoked_at
               FROM api_keys WHERE user_id = ? ORDER BY created_at DESC, id DESC";
    let rows: Vec<ApiKeyRow> = match pool {
        Either::Left(p) => sqlx::query_as(sql).bind(&user.id).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as(sql).bind(&user.id).fetch_all(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (
        StatusCode::OK,
        Json(json!({ "items": rows.iter().map(api_key_json).collect::<Vec<Value>>() })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// POST /api/v1/me/api-keys — 创建密钥（幂等；明文仅此一次返回）。
async fn create_api_key(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "create_api_key";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let req: CreateApiKeyRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let name_chars = req.name.trim().chars().count();
    if !(NAME_MIN..=NAME_MAX).contains(&name_chars) {
        return Err(AppError::bad_request(
            "name must be 1-64 characters",
            request_id,
            None,
        ));
    }
    let scopes = validate_scopes(&req.scopes, request_id)?;
    let crl = req.client_request_id.chars().count();
    if !(1..=200).contains(&crl) {
        return Err(AppError::bad_request(
            "client_request_id must be 1-200 characters",
            request_id,
            None,
        ));
    }

    // 幂等门（scope `apikey.create`）。
    let hash = request_hash(&body);
    let idem_key = IdempotencyKey::new("apikey.create", &req.client_request_id)
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

    match outcome {
        IdempotencyOutcome::Created { record_id } => {
            let key = generate_api_key();
            let secret_hash = hash_token(&key);
            let prefix: String = key.chars().take(PREFIX_LEN).collect();
            let id = uuid::Uuid::now_v7().to_string();
            let now = now_millis();
            let scopes_json = serde_json::to_string(&scopes)
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            let sql = "INSERT INTO api_keys (id, user_id, name, secret_hash, prefix, scopes, created_at, last_used_at, revoked_at)
                       VALUES (?, ?, ?, ?, ?, ?, ?, NULL, NULL)";
            match pool {
                Either::Left(p) => {
                    sqlx::query(sql)
                        .bind(&id)
                        .bind(&user.id)
                        .bind(req.name.trim())
                        .bind(&secret_hash)
                        .bind(&prefix)
                        .bind(&scopes_json)
                        .bind(now)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                }
                Either::Right(p) => {
                    sqlx::query(sql)
                        .bind(&id)
                        .bind(&user.id)
                        .bind(req.name.trim())
                        .bind(&secret_hash)
                        .bind(&prefix)
                        .bind(&scopes_json)
                        .bind(now)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                }
            }

            let _ = complete(pool, &record_id, &id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            // 明文 key 仅此一次返回。
            let resp = (
                StatusCode::CREATED,
                Json(json!({
                    "id": id,
                    "name": req.name.trim(),
                    "scopes": scopes,
                    "created_at": now,
                    "key": key,
                    "prefix": prefix,
                })),
            )
                .into_response();
            Ok(private_no_store(resp))
        }
        IdempotencyOutcome::Replay { .. } => Err(AppError::conflict(
            "api key already created with this request id; the plaintext key is shown only once at creation",
            request_id,
        )),
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

/// DELETE /api/v1/me/api-keys/{id} — 软删除（revoked_at 置位，立即失效）。
async fn revoke_api_key(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "revoke_api_key";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // 只作用于本人密钥（他人/不存在一律 404，不泄露存在性）；
    // 已撤销幂等返回 204。
    let now = now_millis();
    let affected = match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE api_keys SET revoked_at = ? WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(&id)
            .bind(&user.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE api_keys SET revoked_at = ? WHERE id = ? AND user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(&id)
            .bind(&user.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected()
        }
    };
    if affected == 0 {
        // 区分：本人已撤销（幂等 204）vs 不存在/他人（404）。
        let exists: Option<i64> = match pool {
            Either::Left(p) => {
                sqlx::query_scalar("SELECT 1 FROM api_keys WHERE id = ? AND user_id = ?")
                    .bind(&id)
                    .bind(&user.id)
                    .fetch_optional(p)
                    .await
            }
            Either::Right(p) => {
                sqlx::query_scalar("SELECT 1 FROM api_keys WHERE id = ? AND user_id = ?")
                    .bind(&id)
                    .bind(&user.id)
                    .fetch_optional(p)
                    .await
            }
        }
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        if exists != Some(1) {
            return Err(AppError::not_found("api key not found", request_id));
        }
    }

    let resp = (StatusCode::NO_CONTENT, Json(json!({}))).into_response();
    Ok(private_no_store(resp))
}
