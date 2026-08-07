//! M12-REFUND-05/06：Webhook 签名、投递记录与重放。
//!
//! - 事件在业务事务提交后由 Outbox 登记（`webhook_deliveries`），worker 异步
//!   投递；投递不改变已提交购买结果（MARKETPLACE.md §8）。
//! - 签名：HMAC-SHA-256，密钥为该 Client 独立的可轮换 Webhook Secret；
//!   明文只在创建/轮换时返回一次，库里存 AES-256-GCM 密文（用
//!   `marketplace_webhook_encryption_key` 主密钥加密），签名时解密。
//! - 接收方必须校验 5 分钟时间窗并按 `event_id` 去重；重放保持原 `event_id`。
//! - 非 2xx 指数退避重试；超过 `max_attempts` 进入 dead-letter，保留手动重放。
//! - payload 最小化：只含 event_id/event_type/created_at/client_id/purchase_id/
//!   status/金额字段，不含 Token、用户邮箱或完整余额。

use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{Either, Row};

use crate::auth::token::hash_token;
use crate::db::DatabasePool;
use crate::marketplace::now_millis;
use crate::marketplace::{MarketplaceError, WEBHOOK_MAX_ATTEMPTS, WEBHOOK_TTL_MS};

/// AES-256-GCM 派生密钥（主密钥 + 用途标签）。
fn aes_key(master: &str) -> Result<[u8; 32], MarketplaceError> {
    if master.is_empty() {
        return Err(MarketplaceError::Db(
            "marketplace_webhook_encryption_key is not configured".into(),
        ));
    }
    let digest = Sha256::digest(format!("bblbb-webhook-secret:{master}").as_bytes());
    Ok(digest.into())
}

/// 加密 Webhook Secret（AES-256-GCM；返回 `nonce||ciphertext` 的 hex）。
pub fn encrypt_webhook_secret(master: &str, secret: &str) -> Result<String, MarketplaceError> {
    use aes_gcm::aead::{Aead, KeyInit};
    let key = aes_key(master)?;
    let cipher = aes_gcm::Aes256Gcm::new_from_slice(&key)
        .map_err(|e| MarketplaceError::Db(e.to_string()))?;
    let nonce = {
        use rand::RngCore;
        let mut n = [0u8; 12];
        rand::rngs::OsRng.fill_bytes(&mut n);
        n
    };
    let ct = cipher
        .encrypt(aes_gcm::Nonce::from_slice(&nonce), secret.as_bytes())
        .map_err(|_| MarketplaceError::Db("webhook secret encryption failed".into()))?;
    let mut out = Vec::with_capacity(nonce.len() + ct.len());
    out.extend_from_slice(&nonce);
    out.extend_from_slice(&ct);
    Ok(hex::encode(out))
}

/// 解密 Webhook Secret。
pub fn decrypt_webhook_secret(master: &str, stored: &str) -> Result<String, MarketplaceError> {
    use aes_gcm::aead::{Aead, KeyInit};
    let key = aes_key(master)?;
    let cipher = aes_gcm::Aes256Gcm::new_from_slice(&key)
        .map_err(|e| MarketplaceError::Db(e.to_string()))?;
    let raw = hex::decode(stored).map_err(|_| MarketplaceError::WebhookInvalidSignature)?;
    if raw.len() < 13 {
        return Err(MarketplaceError::WebhookInvalidSignature);
    }
    let (nonce, ct) = raw.split_at(12);
    let pt = cipher
        .decrypt(aes_gcm::Nonce::from_slice(nonce), ct)
        .map_err(|_| MarketplaceError::WebhookInvalidSignature)?;
    String::from_utf8(pt).map_err(|_| MarketplaceError::WebhookInvalidSignature)
}

/// 规范化签名输入：`timestamp.event_id.body`（原始请求体字节）。
pub fn signature_input(timestamp: i64, event_id: &str, body: &[u8]) -> Vec<u8> {
    let mut out = format!("{timestamp}.{event_id}.").into_bytes();
    out.extend_from_slice(body);
    out
}

/// HMAC-SHA-256 签名（密钥 = 解密后的 Webhook Secret）。
pub fn hmac_sign(
    master: &str,
    stored_secret: &str,
    input: &[u8],
) -> Result<String, MarketplaceError> {
    let secret = decrypt_webhook_secret(master, stored_secret)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| MarketplaceError::Db(e.to_string()))?;
    mac.update(input);
    Ok(hex::encode(mac.finalize().into_bytes()))
}

/// 常量时间校验签名。
pub fn signature_valid(expected: &str, provided: &str) -> bool {
    if expected.len() != provided.len() {
        return false;
    }
    let mut diff = 0u8;
    for (a, b) in expected.bytes().zip(provided.bytes()) {
        diff |= a ^ b;
    }
    diff == 0
}

/// Webhook 投递出站客户端抽象（测试用 mock，真实部署注入）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebhookResponse {
    pub status: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WebhookSendError {
    Transport(String),
    EgressUnavailable,
}

pub trait WebhookClient: Send + Sync {
    fn post(
        &self,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<WebhookResponse, WebhookSendError>> + Send + '_,
        >,
    >;
}

/// 默认客户端：egress 未配置时安全拒绝（与 video/egress 同模式）。
pub struct UnavailableWebhookClient;

impl WebhookClient for UnavailableWebhookClient {
    fn post(
        &self,
        _url: &str,
        _headers: Vec<(String, String)>,
        _body: Vec<u8>,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = Result<WebhookResponse, WebhookSendError>> + Send + '_,
        >,
    > {
        Box::pin(async { Err(WebhookSendError::EgressUnavailable) })
    }
}

/// 最小 Webhook payload（不含 Token/用户邮箱/完整余额）。
pub fn minimal_payload(event_id: &str, event_type: &str, details: &Value) -> Value {
    json!({
        "event_id": event_id,
        "event_type": event_type,
        "created_at": now_millis(),
        "client_id": details.get("client_id").cloned().unwrap_or(Value::Null),
        "purchase_id": details.get("purchase_id").cloned().unwrap_or(Value::Null),
        "status": details.get("status").cloned().unwrap_or(Value::Null),
        "amount": details.get("amount").cloned().unwrap_or(Value::Null),
        "currency_id": details.get("currency_id").cloned().unwrap_or(Value::Null),
        "merchant_order_id": details.get("merchant_order_id").cloned().unwrap_or(Value::Null),
    })
}

/// 在业务事务后登记 Webhook 投递记录（post-commit；Outbox 已入队）。
pub async fn register_delivery(
    pool: &DatabasePool,
    client_id: &str,
    event_id: &str,
    event_type: &str,
    payload: &Value,
    now: i64,
) -> Result<(), MarketplaceError> {
    let id = uuid::Uuid::now_v7().to_string();
    let payload_str = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "INSERT OR IGNORE INTO webhook_deliveries
             (id, event_id, client_id, event_type, payload, status, attempts, max_attempts, next_retry_at, last_status_code, last_error, delivered_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'pending', 0, ?, ?, NULL, NULL, NULL, ?, ?)",
        )
        .bind(&id)
        .bind(event_id)
        .bind(client_id)
        .bind(event_type)
        .bind(&payload_str)
        .bind(WEBHOOK_MAX_ATTEMPTS)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "INSERT IGNORE INTO webhook_deliveries
             (id, event_id, client_id, event_type, payload, status, attempts, max_attempts, next_retry_at, last_status_code, last_error, delivered_at, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, 'pending', 0, ?, ?, NULL, NULL, NULL, ?, ?)",
        )
        .bind(&id)
        .bind(event_id)
        .bind(client_id)
        .bind(event_type)
        .bind(&payload_str)
        .bind(WEBHOOK_MAX_ATTEMPTS)
        .bind(now)
        .bind(now)
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        // 重复事件（outbox 至少一次投递）：幂等忽略。
        return Ok(());
    }
    Ok(())
}

/// 在业务事务内登记投递记录（SQLite 连接）。
pub async fn register_delivery_sqlite(
    conn: &mut sqlx::SqliteConnection,
    client_id: &str,
    event_id: &str,
    event_type: &str,
    payload: &Value,
    now: i64,
) -> Result<(), MarketplaceError> {
    let id = uuid::Uuid::now_v7().to_string();
    let payload_str = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT OR IGNORE INTO webhook_deliveries
         (id, event_id, client_id, event_type, payload, status, attempts, max_attempts, next_retry_at, last_status_code, last_error, delivered_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 'pending', 0, ?, ?, NULL, NULL, NULL, ?, ?)",
    )
    .bind(&id)
    .bind(event_id)
    .bind(client_id)
    .bind(event_type)
    .bind(&payload_str)
    .bind(WEBHOOK_MAX_ATTEMPTS)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// 在业务事务内登记投递记录（MySQL 事务）。
pub async fn register_delivery_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    client_id: &str,
    event_id: &str,
    event_type: &str,
    payload: &Value,
    now: i64,
) -> Result<(), MarketplaceError> {
    let id = uuid::Uuid::now_v7().to_string();
    let payload_str = serde_json::to_string(payload).unwrap_or_else(|_| "{}".into());
    sqlx::query(
        "INSERT IGNORE INTO webhook_deliveries
         (id, event_id, client_id, event_type, payload, status, attempts, max_attempts, next_retry_at, last_status_code, last_error, delivered_at, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 'pending', 0, ?, ?, NULL, NULL, NULL, ?, ?)",
    )
    .bind(&id)
    .bind(event_id)
    .bind(client_id)
    .bind(event_type)
    .bind(&payload_str)
    .bind(WEBHOOK_MAX_ATTEMPTS)
    .bind(now)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 投递记录行。
#[derive(Debug, Clone)]
pub struct DeliveryRow {
    pub id: String,
    pub event_id: String,
    pub client_id: String,
    pub event_type: String,
    pub payload: Value,
    pub status: String,
    pub attempts: i64,
    pub max_attempts: i64,
    pub next_retry_at: i64,
    pub last_status_code: Option<i64>,
    pub last_error: Option<String>,
    pub delivered_at: Option<i64>,
    pub created_at: i64,
}

const DELIVERY_COLUMNS: &str = "id, event_id, client_id, event_type, payload, status, attempts, max_attempts, next_retry_at, last_status_code, last_error, delivered_at, created_at";

fn delivery_from_sqlite(row: &sqlx::sqlite::SqliteRow) -> DeliveryRow {
    DeliveryRow {
        id: row.get("id"),
        event_id: row.get("event_id"),
        client_id: row.get("client_id"),
        event_type: row.get("event_type"),
        payload: serde_json::from_str(&row.get::<String, _>("payload")).unwrap_or(Value::Null),
        status: row.get("status"),
        attempts: row.get("attempts"),
        max_attempts: row.get("max_attempts"),
        next_retry_at: row.get("next_retry_at"),
        last_status_code: row.get("last_status_code"),
        last_error: row.get("last_error"),
        delivered_at: row.get("delivered_at"),
        created_at: row.get("created_at"),
    }
}

fn delivery_from_mysql(row: &sqlx::mysql::MySqlRow) -> DeliveryRow {
    DeliveryRow {
        id: row.get("id"),
        event_id: row.get("event_id"),
        client_id: row.get("client_id"),
        event_type: row.get("event_type"),
        payload: serde_json::from_str(&row.get::<String, _>("payload")).unwrap_or(Value::Null),
        status: row.get("status"),
        attempts: row.get("attempts"),
        max_attempts: row.get("max_attempts"),
        next_retry_at: row.get("next_retry_at"),
        last_status_code: row.get("last_status_code"),
        last_error: row.get("last_error"),
        delivered_at: row.get("delivered_at"),
        created_at: row.get("created_at"),
    }
}

/// 列出投递记录（Client 隔离 + 状态过滤）。
pub async fn list_deliveries(
    pool: &DatabasePool,
    client_id: Option<&str>,
    status: Option<&str>,
    after: Option<&str>,
    limit: i64,
) -> Result<Vec<DeliveryRow>, MarketplaceError> {
    let limit = limit.clamp(1, 100);
    let (clause, n_binds) = match (client_id, status) {
        (Some(_), Some(_)) => ("WHERE client_id = ? AND status = ? AND id > ?", 3),
        (Some(_), None) => ("WHERE client_id = ? AND id > ?", 2),
        (None, Some(_)) => ("WHERE status = ? AND id > ?", 2),
        (None, None) => ("WHERE id > ?", 1),
    };
    let sql = format!(
        "SELECT {DELIVERY_COLUMNS} FROM webhook_deliveries {clause} ORDER BY id ASC LIMIT ?"
    );
    let args: Vec<String> = match (client_id, status) {
        (Some(c), Some(s)) => vec![
            c.to_string(),
            s.to_string(),
            after.unwrap_or("").to_string(),
        ],
        (Some(c), None) => vec![c.to_string(), after.unwrap_or("").to_string()],
        (None, Some(s)) => vec![s.to_string(), after.unwrap_or("").to_string()],
        (None, None) => vec![after.unwrap_or("").to_string()],
    };
    let rows: Vec<DeliveryRow> = match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(&sql);
            for a in &args {
                q = q.bind(a);
            }
            q.bind(limit + 1)
                .fetch_all(p)
                .await?
                .iter()
                .map(delivery_from_sqlite)
                .collect()
        }
        Either::Right(p) => {
            let mut q = sqlx::query(&sql);
            for a in &args {
                q = q.bind(a);
            }
            q.bind(limit + 1)
                .fetch_all(p)
                .await?
                .iter()
                .map(delivery_from_mysql)
                .collect()
        }
    };
    let _ = n_binds;
    Ok(rows.into_iter().take(limit as usize).collect())
}

pub fn delivery_json(d: &DeliveryRow) -> Value {
    json!({
        "id": d.id,
        "event_id": d.event_id,
        "client_id": d.client_id,
        "event_type": d.event_type,
        "payload": d.payload,
        "status": d.status,
        "attempts": d.attempts,
        "next_retry_at": d.next_retry_at,
        "last_status_code": d.last_status_code,
        "last_error": d.last_error,
        "delivered_at": d.delivered_at,
        "created_at": d.created_at,
    })
}

/// 指数退避间隔（秒）：2^n * 30s，上限 10 分钟。
pub fn backoff_delay_ms(attempts: i64) -> i64 {
    let exp = (attempts.max(1) as u32).min(5);
    (30_000u64 << exp).min(600_000) as i64
}

/// 投递一条记录（SSRF 二次校验 + HMAC 签名 + 时间戳头）。
///
/// 返回 Ok(true) 表示投递成功（2xx）。非 2xx/传输错误更新退避重试；
/// 超过 max_attempts → dead_letter。Webhook 结果不改变购买事实。
#[allow(clippy::too_many_arguments)]
pub async fn deliver_one(
    pool: &DatabasePool,
    client: &crate::marketplace::clients::MarketplaceClient,
    delivery: &DeliveryRow,
    master_key: &str,
    http: &dyn WebhookClient,
    now: i64,
) -> Result<bool, MarketplaceError> {
    let Some(url) = client.webhook_url.as_deref() else {
        mark_failed_delivery(pool, &delivery.id, "no webhook_url configured", now).await?;
        return Ok(false);
    };
    // 发送前 SSRF 校验（DNS 重绑定由真实 egress 连接时复核）。
    crate::marketplace::clients::validate_webhook_url(url)?;
    let Some(stored) = client.webhook_secret_hash.as_deref() else {
        mark_failed_delivery(pool, &delivery.id, "no webhook secret configured", now).await?;
        return Ok(false);
    };
    let body = serde_json::to_vec(&delivery.payload).unwrap_or_else(|_| b"{}".to_vec());
    let event_id = delivery.event_id.clone();
    let timestamp = now;
    let sig = hmac_sign(
        master_key,
        stored,
        &signature_input(timestamp, &event_id, &body),
    )?;
    let headers = vec![
        ("content-type".to_string(), "application/json".to_string()),
        (
            "x-bblbb-webhook-timestamp".to_string(),
            timestamp.to_string(),
        ),
        ("x-bblbb-webhook-event-id".to_string(), event_id),
        ("x-bblbb-signature".to_string(), sig),
    ];
    match http.post(url, headers, body).await {
        Ok(resp) if (200..300).contains(&resp.status) => {
            let rows = match pool {
                Either::Left(p) => sqlx::query(
                    "UPDATE webhook_deliveries
                     SET status = 'sent', attempts = attempts + 1, last_status_code = ?,
                         last_error = NULL, delivered_at = ?, updated_at = ?
                     WHERE id = ?",
                )
                .bind(resp.status as i64)
                .bind(now)
                .bind(now)
                .bind(&delivery.id)
                .execute(p)
                .await?
                .rows_affected(),
                Either::Right(p) => sqlx::query(
                    "UPDATE webhook_deliveries
                     SET status = 'sent', attempts = attempts + 1, last_status_code = ?,
                         last_error = NULL, delivered_at = ?, updated_at = ?
                     WHERE id = ?",
                )
                .bind(resp.status as i64)
                .bind(now)
                .bind(now)
                .bind(&delivery.id)
                .execute(p)
                .await?
                .rows_affected(),
            };
            let _ = rows;
            Ok(true)
        }
        Ok(resp) => {
            // 非 2xx：指数退避重试；超过上限 dead-letter。
            let attempts = delivery.attempts + 1;
            let status = if attempts >= delivery.max_attempts {
                "dead_letter"
            } else {
                "pending"
            };
            let next = now + backoff_delay_ms(attempts);
            let err = format!("http {}", resp.status);
            match pool {
                Either::Left(p) => {
                    sqlx::query(
                        "UPDATE webhook_deliveries
                         SET status = ?, attempts = ?, next_retry_at = ?, last_status_code = ?, last_error = ?, updated_at = ?
                         WHERE id = ?",
                    )
                    .bind(status)
                    .bind(attempts)
                    .bind(next)
                    .bind(resp.status as i64)
                    .bind(&err)
                    .bind(now)
                    .bind(&delivery.id)
                    .execute(p)
                    .await?;
                }
                Either::Right(p) => {
                    sqlx::query(
                        "UPDATE webhook_deliveries
                         SET status = ?, attempts = ?, next_retry_at = ?, last_status_code = ?, last_error = ?, updated_at = ?
                         WHERE id = ?",
                    )
                    .bind(status)
                    .bind(attempts)
                    .bind(next)
                    .bind(resp.status as i64)
                    .bind(&err)
                    .bind(now)
                    .bind(&delivery.id)
                    .execute(p)
                    .await?;
                }
            }
            Ok(false)
        }
        Err(e) => {
            mark_failed_delivery(pool, &delivery.id, &format!("{e:?}"), now).await?;
            Ok(false)
        }
    }
}

async fn mark_failed_delivery(
    pool: &DatabasePool,
    delivery_id: &str,
    error: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE webhook_deliveries
                 SET status = 'pending', attempts = attempts + 1, last_error = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(error)
            .bind(now)
            .bind(delivery_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE webhook_deliveries
                 SET status = 'pending', attempts = attempts + 1, last_error = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(error)
            .bind(now)
            .bind(delivery_id)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

/// 投递一条记录（公开入口；含加载 client 与 delivery）。
#[allow(clippy::too_many_arguments)]
pub async fn replay_delivery(
    pool: &DatabasePool,
    delivery_id: &str,
    actor_client_id: Option<&str>,
    master_key: &str,
    http: &dyn WebhookClient,
    now: i64,
) -> Result<Value, MarketplaceError> {
    let delivery = fetch_delivery(pool, delivery_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("webhook delivery not found".into()))?;
    if let Some(actor_client) = actor_client_id {
        if delivery.client_id != actor_client {
            return Err(MarketplaceError::Forbidden(
                "delivery belongs to another client".into(),
            ));
        }
    }
    let client =
        crate::marketplace::clients::fetch_client_by_internal_id(pool, &delivery.client_id)
            .await?
            .ok_or_else(|| MarketplaceError::NotFound("marketplace client".into()))?;
    deliver_one(pool, &client, &delivery, master_key, http, now).await?;
    let updated = fetch_delivery(pool, delivery_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("webhook delivery".into()))?;
    Ok(delivery_json(&updated))
}

async fn fetch_delivery(
    pool: &DatabasePool,
    delivery_id: &str,
) -> Result<Option<DeliveryRow>, MarketplaceError> {
    let sql = format!("SELECT {DELIVERY_COLUMNS} FROM webhook_deliveries WHERE id = ?");
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(delivery_id)
            .fetch_optional(p)
            .await?
            .map(|r| delivery_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(delivery_id)
            .fetch_optional(p)
            .await?
            .map(|r| delivery_from_mysql(&r)),
    };
    Ok(row)
}

/// 验证 Webhook 请求签名（接收方语义；供外部接入文档与测试向量使用）。
pub fn verify_webhook_request(
    secret: &str,
    timestamp: i64,
    event_id: &str,
    body: &[u8],
    provided: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    if now.abs_diff(timestamp) > WEBHOOK_TTL_MS as u64 {
        return Err(MarketplaceError::WebhookInvalidSignature);
    }
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
        .map_err(|e| MarketplaceError::Db(e.to_string()))?;
    mac.update(&signature_input(timestamp, event_id, body));
    let expected = hex::encode(mac.finalize().into_bytes());
    if !signature_valid(&expected, provided) {
        return Err(MarketplaceError::WebhookInvalidSignature);
    }
    Ok(())
}

/// event_id 重放保护摘要（接收方按 event_id 去重的哈希存储）。
pub fn event_id_hash(event_id: &str) -> String {
    hash_token(event_id)
}
