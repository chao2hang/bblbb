//! 社交域私信路由（GAP-FIX 社交域）。
//!
//! - `GET /api/v1/conversations?after=&limit=30`：本人会话列表（按
//!   `last_message_at DESC` keyset；只返回双人会话，`other` = 对方投影）；
//! - `POST /api/v1/conversations`：与指定用户开（或复用既有）会话 → 201
//!   `{id, other:{...}}`（与自己开会话 422；目标不存在 404）；
//! - `GET /api/v1/conversations/{id}/messages?after=&limit=50`：消息线程
//!   （`created_at ASC`，`after` = 上一页最后一条 created_at；非参与者
//!   404）；
//! - `POST /api/v1/conversations/{id}/messages`：发消息 → 201（幂等：
//!   `client_request_id` 走 idempotency_records，同 key+摘要重放返回原
//!   消息）；消息落库后给对方插 notifications（type='mention'，
//!   link='/messages'，best-effort 不阻断）；
//! - `POST /api/v1/conversations/{id}/read`：标记已读（更新本人
//!   `last_read_at`）→ 204。
//!
//! 会话创建幂等取舍：与 favorite/follow 不同，会话有独立业务产物（新
//! conversation 行），但「两人已有会话则返回既有 id」本身就是稳定的
//! find-or-create 语义——事务内 SELECT 后 INSERT（SQLite `BEGIN
//! IMMEDIATE` 原子），不引入 idempotency_records（重复请求得到的响应
//! 与首次完全一致）。**发消息**才是真正的写操作，走完整的
//! begin_or_replay/complete 幂等门。

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
use sqlx::Either;

use crate::{
    app::AppState,
    auth::session::AuthSession,
    error::AppError,
    idempotency::{
        begin_or_replay, complete, request_hash, FailureCachePolicy, IdempotencyKey,
        IdempotencyOutcome,
    },
    outbox::now_millis,
};

/// 幂等记录保留窗口（发消息，24h，与其他写路径一致）。
const IDEMPOTENCY_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// 会话列表 / 消息线程分页上限。
const MAX_CONV_LIMIT: i64 = 100;
const DEFAULT_CONV_LIMIT: i64 = 30;
const DEFAULT_MSG_LIMIT: i64 = 50;
const MAX_MSG_LIMIT: i64 = 100;
/// 消息体长度（字符数）。
const MSG_BODY_MIN: usize = 1;
const MSG_BODY_MAX: usize = 2000;

/// 私信路由。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/conversations",
            get(list_conversations).post(create_conversation),
        )
        .route(
            "/api/v1/conversations/{id}/messages",
            get(list_messages).post(send_message),
        )
        .route("/api/v1/conversations/{id}/read", post(mark_read))
}

#[derive(Deserialize)]
struct CreateConversationRequest {
    username: String,
}

#[derive(Deserialize)]
struct SendMessageRequest {
    body: String,
    client_request_id: String,
}

#[derive(Deserialize)]
struct ListConversationsQuery {
    /// keyset 游标：上一页最后一条 last_message_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_conv_limit")]
    limit: i64,
}

#[derive(Deserialize)]
struct ListMessagesQuery {
    /// keyset 游标：上一页最后一条 created_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_msg_limit")]
    limit: i64,
}

fn default_conv_limit() -> i64 {
    DEFAULT_CONV_LIMIT
}

fn default_msg_limit() -> i64 {
    DEFAULT_MSG_LIMIT
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

/// 会话列表行（other 投影 + 最后一条消息 + 未读数）。
#[derive(sqlx::FromRow)]
struct ConversationRow {
    id: String,
    last_message_at: i64,
    other_username: String,
    other_display_name: Option<String>,
    other_level: i64,
    last_body: Option<String>,
    last_created_at: Option<i64>,
    last_sender_username: Option<String>,
    unread_count: i64,
}

/// GET /api/v1/conversations — 本人会话列表（last_message_at DESC keyset）。
async fn list_conversations(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<ListConversationsQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_conversations";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, MAX_CONV_LIMIT);
    let before = match query.after.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(raw.parse::<i64>().map_err(|_| {
            AppError::bad_request("after must be an integer cursor", request_id, None)
        })?),
    };

    // 双人会话：me + other 两个参与者 JOIN；最后一条消息与未读数用子查询
    // 聚合（避免 N+1；未读 = 对方发的、晚于本人 last_read_at 的未删消息）。
    let sql = "SELECT c.id, c.last_message_at,
                      ou.username_normalized AS other_username,
                      ou.display_name AS other_display_name,
                      ou.level AS other_level,
                      lm.body AS last_body,
                      lm.created_at AS last_created_at,
                      lu.username_normalized AS last_sender_username,
                      (SELECT COUNT(*) FROM messages m
                        WHERE m.conversation_id = c.id AND m.sender_id <> ?
                          AND m.created_at > COALESCE(me.last_read_at, 0)
                          AND m.deleted_at IS NULL) AS unread_count
               FROM conversations c
               JOIN conversation_participants me ON me.conversation_id = c.id AND me.user_id = ?
               JOIN conversation_participants op ON op.conversation_id = c.id AND op.user_id <> ?
               JOIN users ou ON ou.id = op.user_id
               LEFT JOIN messages lm ON lm.id = (
                        SELECT m2.id FROM messages m2
                        WHERE m2.conversation_id = c.id AND m2.deleted_at IS NULL
                        ORDER BY m2.created_at DESC LIMIT 1)
               LEFT JOIN users lu ON lu.id = lm.sender_id
               WHERE (? IS NULL OR c.last_message_at < ?)
               ORDER BY c.last_message_at DESC, c.id DESC
               LIMIT ?";
    let fetch_limit = limit + 1;
    let rows: Vec<ConversationRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as(sql)
                .bind(&user.id)
                .bind(&user.id)
                .bind(&user.id)
                .bind(before)
                .bind(before)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as(sql)
                .bind(&user.id)
                .bind(&user.id)
                .bind(&user.id)
                .bind(before)
                .bind(before)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = rows.len() as i64 > limit;
    let page: Vec<ConversationRow> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| r.last_message_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let items: Vec<Value> = page
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "other": {
                    "username": r.other_username,
                    "display_name": r.other_display_name,
                    "level": r.other_level,
                },
                "last_message": match (&r.last_body, r.last_created_at, &r.last_sender_username) {
                    (Some(body), Some(created_at), Some(sender)) => json!({
                        "body": body,
                        "created_at": created_at,
                        "sender_username": sender,
                    }),
                    _ => Value::Null,
                },
                "unread_count": r.unread_count,
                "updated_at": r.last_message_at,
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

/// 按用户名（normalized）查目标用户；不存在/已注销 → None。
async fn user_id_by_username(
    pool: &crate::db::DatabasePool,
    username: &str,
    request_id: &str,
) -> Result<Option<(String, Option<String>, i64)>, AppError> {
    // (id, display_name, level)
    let row: Option<(String, Option<String>, i64)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id, display_name, level FROM users WHERE username_normalized = ? AND status <> 'deleted'",
            )
            .bind(username.to_lowercase())
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, display_name, level FROM users WHERE username_normalized = ? AND status <> 'deleted'",
            )
            .bind(username.to_lowercase())
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row)
}

/// POST /api/v1/conversations — 与指定用户开（或复用）双人会话。
async fn create_conversation(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "create_conversation";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let req: CreateConversationRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    if req.username.trim().is_empty() {
        return Err(AppError::bad_request(
            "username is required",
            request_id,
            None,
        ));
    }

    let (target_id, target_display_name, target_level) =
        user_id_by_username(pool, req.username.trim(), request_id)
            .await?
            .ok_or_else(|| AppError::not_found("user not found", request_id))?;

    // 不能与自己开会话（422 稳定错误码）。
    if target_id == user.id {
        return Err(AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "cannot_message_self",
            "Unprocessable Entity",
            "cannot start a conversation with yourself",
            request_id,
        ));
    }

    let conversation_id =
        find_or_create_conversation(pool, &user.id, &target_id, request_id).await?;

    let resp = (
        StatusCode::CREATED,
        Json(json!({
            "id": conversation_id,
            "other": {
                "username": req.username.trim().to_lowercase(),
                "display_name": target_display_name,
                "level": target_level,
            },
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// 事务内 find-or-create 双人会话：两人已有会话则返回既有 id。
async fn find_or_create_conversation(
    pool: &crate::db::DatabasePool,
    me: &str,
    other: &str,
    request_id: &str,
) -> Result<String, AppError> {
    let now = now_millis();
    let find_sql = "SELECT c.id FROM conversations c
                    JOIN conversation_participants a ON a.conversation_id = c.id AND a.user_id = ?
                    JOIN conversation_participants b ON b.conversation_id = c.id AND b.user_id = ?
                    LIMIT 1";
    match pool {
        Either::Left(p) => {
            let mut conn = p
                .acquire()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut *conn)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let r: Result<String, sqlx::Error> = async {
                let existing: Option<String> = sqlx::query_scalar(find_sql)
                    .bind(me)
                    .bind(other)
                    .fetch_optional(&mut *conn)
                    .await?;
                if let Some(id) = existing {
                    return Ok(id);
                }
                let id = uuid::Uuid::now_v7().to_string();
                sqlx::query(
                    "INSERT INTO conversations (id, created_at, last_message_at) VALUES (?, ?, ?)",
                )
                .bind(&id)
                .bind(now)
                .bind(now)
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    "INSERT INTO conversation_participants (conversation_id, user_id, last_read_at) VALUES (?, ?, NULL)",
                )
                .bind(&id)
                .bind(me)
                .execute(&mut *conn)
                .await?;
                sqlx::query(
                    "INSERT INTO conversation_participants (conversation_id, user_id, last_read_at) VALUES (?, ?, NULL)",
                )
                .bind(&id)
                .bind(other)
                .execute(&mut *conn)
                .await?;
                Ok(id)
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
                    Err(AppError::internal(e.to_string(), request_id))
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let r: Result<String, sqlx::Error> = async {
                let existing: Option<String> = sqlx::query_scalar(find_sql)
                    .bind(me)
                    .bind(other)
                    .fetch_optional(&mut *tx)
                    .await?;
                if let Some(id) = existing {
                    return Ok(id);
                }
                let id = uuid::Uuid::now_v7().to_string();
                sqlx::query(
                    "INSERT INTO conversations (id, created_at, last_message_at) VALUES (?, ?, ?)",
                )
                .bind(&id)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO conversation_participants (conversation_id, user_id, last_read_at) VALUES (?, ?, NULL)",
                )
                .bind(&id)
                .bind(me)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO conversation_participants (conversation_id, user_id, last_read_at) VALUES (?, ?, NULL)",
                )
                .bind(&id)
                .bind(other)
                .execute(&mut *tx)
                .await?;
                Ok(id)
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
                    Err(AppError::internal(e.to_string(), request_id))
                }
            }
        }
    }
}

/// 会话参与者校验：本人是否参与该会话（不存在同样 false → 404 语义）。
async fn is_participant(
    pool: &crate::db::DatabasePool,
    conversation_id: &str,
    user_id: &str,
    request_id: &str,
) -> Result<bool, AppError> {
    let row: Option<i64> =
        match pool {
            Either::Left(p) => sqlx::query_scalar(
                "SELECT 1 FROM conversation_participants WHERE conversation_id = ? AND user_id = ?",
            )
            .bind(conversation_id)
            .bind(user_id)
            .fetch_optional(p)
            .await,
            Either::Right(p) => sqlx::query_scalar(
                "SELECT 1 FROM conversation_participants WHERE conversation_id = ? AND user_id = ?",
            )
            .bind(conversation_id)
            .bind(user_id)
            .fetch_optional(p)
            .await,
        }
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row == Some(1))
}

/// 会话另一方参与者 id（双人会话恒存在）。
async fn other_participant(
    pool: &crate::db::DatabasePool,
    conversation_id: &str,
    user_id: &str,
    request_id: &str,
) -> Result<Option<String>, AppError> {
    let row: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT user_id FROM conversation_participants WHERE conversation_id = ? AND user_id <> ? LIMIT 1",
            )
            .bind(conversation_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT user_id FROM conversation_participants WHERE conversation_id = ? AND user_id <> ? LIMIT 1",
            )
            .bind(conversation_id)
            .bind(user_id)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row)
}

/// 消息线程行投影。
#[derive(sqlx::FromRow)]
struct MessageRow {
    id: String,
    sender_username: String,
    body: String,
    created_at: i64,
}

/// GET /api/v1/conversations/{id}/messages — 消息线程（ASC + keyset）。
async fn list_messages(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    Query(query): Query<ListMessagesQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_messages";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // 非参与者/不存在一律 404（不泄露存在性）。
    if !is_participant(pool, &id, &user.id, request_id).await? {
        return Err(AppError::not_found("conversation not found", request_id));
    }

    let limit = query.limit.clamp(1, MAX_MSG_LIMIT);
    let after = match query.after.as_deref() {
        None | Some("") => None,
        Some(raw) => Some(raw.parse::<i64>().map_err(|_| {
            AppError::bad_request("after must be an integer cursor", request_id, None)
        })?),
    };

    let sql = "SELECT m.id, u.username_normalized AS sender_username, m.body, m.created_at
               FROM messages m
               JOIN users u ON u.id = m.sender_id
               WHERE m.conversation_id = ? AND m.deleted_at IS NULL
                 AND (? IS NULL OR m.created_at > ?)
               ORDER BY m.created_at ASC, m.id ASC
               LIMIT ?";
    let fetch_limit = limit + 1;
    let rows: Vec<MessageRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as(sql)
                .bind(&id)
                .bind(after)
                .bind(after)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as(sql)
                .bind(&id)
                .bind(after)
                .bind(after)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = rows.len() as i64 > limit;
    let page: Vec<MessageRow> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|m| m.created_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let items: Vec<Value> = page
        .iter()
        .map(|m| {
            json!({
                "id": m.id,
                "sender_username": m.sender_username,
                "body": m.body,
                "created_at": m.created_at,
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

/// POST /api/v1/conversations/{id}/messages — 发送消息（幂等）。
async fn send_message(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "send_message";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let req: SendMessageRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let char_count = req.body.chars().count();
    if !(MSG_BODY_MIN..=MSG_BODY_MAX).contains(&char_count) {
        return Err(AppError::bad_request(
            "body must be 1-2000 characters",
            request_id,
            None,
        ));
    }
    let crl = req.client_request_id.chars().count();
    if !(1..=200).contains(&crl) {
        return Err(AppError::bad_request(
            "client_request_id must be 1-200 characters",
            request_id,
            None,
        ));
    }

    // 非参与者/不存在一律 404。
    if !is_participant(pool, &id, &user.id, request_id).await? {
        return Err(AppError::not_found("conversation not found", request_id));
    }

    // 幂等门（scope `conversation.message`，key = client_request_id）。
    let hash = request_hash(&body);
    let idem_key = IdempotencyKey::new("conversation.message", &req.client_request_id)
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
            let message_id = insert_message(pool, &id, &user.id, &req.body, request_id).await?;
            let _ = complete(pool, &record_id, &message_id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            // 通知对方（type='mention'，link='/messages'；best-effort）。
            if let Some(other_id) = other_participant(pool, &id, &user.id, request_id).await? {
                if let Err(e) =
                    crate::achievements::notify_private_message(pool, &other_id, &user.username)
                        .await
                {
                    tracing::warn!(conversation_id = %id, error = %e, "dm notification failed");
                }
            }

            let now = now_millis();
            let resp = (
                StatusCode::CREATED,
                Json(json!({
                    "id": message_id,
                    "sender_username": user.username,
                    "body": req.body,
                    "created_at": now,
                })),
            )
                .into_response();
            Ok(private_no_store(resp))
        }
        IdempotencyOutcome::Replay { response_reference } => {
            // 同 key+摘要重放：返回原消息。
            if let Some(message_id) = response_reference {
                if let Some((sender_username, body_text, created_at)) =
                    load_message(pool, &message_id, request_id).await?
                {
                    let resp = (
                        StatusCode::CREATED,
                        Json(json!({
                            "id": message_id,
                            "sender_username": sender_username,
                            "body": body_text,
                            "created_at": created_at,
                        })),
                    )
                        .into_response();
                    return Ok(private_no_store(resp));
                }
            }
            Err(AppError::conflict(
                "idempotent replay but original message not found",
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

/// 事务内插入消息：消息行 + 会话 last_message_at + 发送者 last_read_at
/// （自己发的消息视为已读）。返回消息 id。
async fn insert_message(
    pool: &crate::db::DatabasePool,
    conversation_id: &str,
    sender_id: &str,
    body: &str,
    request_id: &str,
) -> Result<String, AppError> {
    let now = now_millis();
    let message_id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            let mut conn = p
                .acquire()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut *conn)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let r: Result<(), sqlx::Error> = async {
                sqlx::query(
                    "INSERT INTO messages (id, conversation_id, sender_id, body, created_at, deleted_at)
                     VALUES (?, ?, ?, ?, ?, NULL)",
                )
                .bind(&message_id)
                .bind(conversation_id)
                .bind(sender_id)
                .bind(body)
                .bind(now)
                .execute(&mut *conn)
                .await?;
                sqlx::query("UPDATE conversations SET last_message_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(conversation_id)
                    .execute(&mut *conn)
                    .await?;
                sqlx::query(
                    "UPDATE conversation_participants SET last_read_at = ? WHERE conversation_id = ? AND user_id = ?",
                )
                .bind(now)
                .bind(conversation_id)
                .bind(sender_id)
                .execute(&mut *conn)
                .await?;
                Ok(())
            }
            .await;
            match r {
                Ok(()) => {
                    sqlx::query("COMMIT")
                        .execute(&mut *conn)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    Ok(message_id)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(AppError::internal(e.to_string(), request_id))
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let r: Result<(), sqlx::Error> = async {
                sqlx::query(
                    "INSERT INTO messages (id, conversation_id, sender_id, body, created_at, deleted_at)
                     VALUES (?, ?, ?, ?, ?, NULL)",
                )
                .bind(&message_id)
                .bind(conversation_id)
                .bind(sender_id)
                .bind(body)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                sqlx::query("UPDATE conversations SET last_message_at = ? WHERE id = ?")
                    .bind(now)
                    .bind(conversation_id)
                    .execute(&mut *tx)
                    .await?;
                sqlx::query(
                    "UPDATE conversation_participants SET last_read_at = ? WHERE conversation_id = ? AND user_id = ?",
                )
                .bind(now)
                .bind(conversation_id)
                .bind(sender_id)
                .execute(&mut *tx)
                .await?;
                Ok(())
            }
            .await;
            match r {
                Ok(()) => {
                    tx.commit()
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    Ok(message_id)
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(AppError::internal(e.to_string(), request_id))
                }
            }
        }
    }
}

/// 读取单条消息（幂等重放回显用）：(sender_username, body, created_at)。
async fn load_message(
    pool: &crate::db::DatabasePool,
    message_id: &str,
    request_id: &str,
) -> Result<Option<(String, String, i64)>, AppError> {
    let row: Option<(String, String, i64)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT u.username_normalized, m.body, m.created_at FROM messages m
                 JOIN users u ON u.id = m.sender_id WHERE m.id = ?",
            )
            .bind(message_id)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT u.username_normalized, m.body, m.created_at FROM messages m
                 JOIN users u ON u.id = m.sender_id WHERE m.id = ?",
            )
            .bind(message_id)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row)
}

/// POST /api/v1/conversations/{id}/read — 标记会话已读（本人 last_read_at）。
async fn mark_read(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "mark_read";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    if !is_participant(pool, &id, &user.id, request_id).await? {
        return Err(AppError::not_found("conversation not found", request_id));
    }

    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE conversation_participants SET last_read_at = ? WHERE conversation_id = ? AND user_id = ?",
            )
            .bind(now)
            .bind(&id)
            .bind(&user.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE conversation_participants SET last_read_at = ? WHERE conversation_id = ? AND user_id = ?",
            )
            .bind(now)
            .bind(&id)
            .bind(&user.id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let resp = (StatusCode::NO_CONTENT, Json(json!({}))).into_response();
    Ok(private_no_store(resp))
}
