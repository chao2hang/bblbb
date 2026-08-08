//! M06-DOWNLOAD 服务层：策略解析、授权、扣费与 URL 重签。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::audit::AuditEntry;
use crate::db::DatabasePool;
use crate::economy::ledger::service as ledger;
use crate::economy::ledger::service::{LedgerCommand, LedgerError, LedgerKind, CURRENCY_COIN};
use crate::events::types::DOWNLOAD_AUTHORIZATION_CREATED;
use crate::outbox::now_millis;
use crate::storage::model::{AttachmentStatus, StorageBackend};
use crate::storage::StorageService;

/// 下载错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    Db(String),
    NotFound(String),
    Invalid(String),
    Forbidden(String),
    /// 幂等键同键不同摘要。
    IdempotencyConflict,
    /// URL 签发失败（不重复扣款）。
    Unavailable(String),
}

impl From<sqlx::Error> for DownloadError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<LedgerError> for DownloadError {
    fn from(e: LedgerError) -> Self {
        match e {
            LedgerError::InsufficientBalance => Self::Forbidden("insufficient balance".into()),
            LedgerError::IdempotencyConflict => Self::IdempotencyConflict,
            other => Self::Db(other.to_string()),
        }
    }
}

impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(m) => write!(f, "download db error: {m}"),
            Self::NotFound(m) => write!(f, "download not found: {m}"),
            Self::Invalid(m) => write!(f, "invalid download request: {m}"),
            Self::Forbidden(m) => write!(f, "download forbidden: {m}"),
            Self::IdempotencyConflict => write!(f, "idempotency key conflict"),
            Self::Unavailable(m) => write!(f, "download url unavailable: {m}"),
        }
    }
}

impl std::error::Error for DownloadError {}

impl DownloadError {
    /// 稳定错误码（docs/ERROR-CODES.md；M16-HARNESS-04 路由层按此输出 Problem code）。
    pub fn code(&self) -> &'static str {
        match self {
            Self::Db(_) => "internal_error",
            Self::NotFound(_) => "not_found",
            Self::Invalid(_) => "invalid_request",
            Self::Forbidden(_) => "forbidden",
            Self::IdempotencyConflict => "idempotency_conflict",
            Self::Unavailable(_) => "download_url_unavailable",
        }
    }
}

/// 下载计费策略行（download_billing_policies）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DownloadPolicy {
    pub id: String,
    pub scope_type: String,
    pub scope_id: Option<String>,
    pub mode: String,
    pub currency_id: Option<String>,
    pub amount: i64,
    pub authorization_ttl_seconds: i64,
    pub daily_user_limit: Option<i64>,
    pub single_charge_limit: Option<i64>,
    pub attachment_revenue_limit: Option<i64>,
    pub grace_on_disable: bool,
    pub version: i64,
    pub is_enabled: bool,
}

/// 附件行（用于策略解析所需的轻量字段）。
pub struct AttachmentView {
    pub id: String,
    pub owner_id: String,
    pub storage_backend: StorageBackend,
    pub storage_key: String,
    pub status: AttachmentStatus,
    pub size_bytes: i64,
    pub is_public: bool,
}

fn policy_from_sqlite(row: &sqlx::sqlite::SqliteRow) -> DownloadPolicy {
    DownloadPolicy {
        id: row.get("id"),
        scope_type: row.get("scope_type"),
        scope_id: row.get("scope_id"),
        mode: row.get("mode"),
        currency_id: row.get("currency_id"),
        amount: row.get("amount"),
        authorization_ttl_seconds: row.get("authorization_ttl_seconds"),
        daily_user_limit: row.get("daily_user_limit"),
        single_charge_limit: row.get("single_charge_limit"),
        attachment_revenue_limit: row.get("attachment_revenue_limit"),
        grace_on_disable: row.get("grace_on_disable"),
        version: row.get("version"),
        is_enabled: row.get("is_enabled"),
    }
}

fn policy_from_mysql(row: &sqlx::mysql::MySqlRow) -> DownloadPolicy {
    DownloadPolicy {
        id: row.get("id"),
        scope_type: row.get("scope_type"),
        scope_id: row.get("scope_id"),
        mode: row.get("mode"),
        currency_id: row.get("currency_id"),
        amount: row.get("amount"),
        authorization_ttl_seconds: row.get("authorization_ttl_seconds"),
        daily_user_limit: row.get("daily_user_limit"),
        single_charge_limit: row.get("single_charge_limit"),
        attachment_revenue_limit: row.get("attachment_revenue_limit"),
        grace_on_disable: row.get("grace_on_disable"),
        version: row.get("version"),
        is_enabled: row.get("is_enabled"),
    }
}

const POLICY_COLUMNS: &str = "id, scope_type, scope_id, mode, currency_id, amount, \
     authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, \
     grace_on_disable, version, is_enabled";

async fn load_attachment_sqlite(
    conn: &mut sqlx::SqliteConnection,
    attachment_id: &str,
) -> Result<AttachmentView, DownloadError> {
    let row = sqlx::query(
        "SELECT id, owner_id, storage_backend, storage_key, status, size_bytes, is_public \
         FROM attachments WHERE id = ?",
    )
    .bind(attachment_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DownloadError::NotFound("attachment not found".into()))?;
    Ok(AttachmentView {
        id: row.get("id"),
        owner_id: row.get("owner_id"),
        storage_backend: StorageBackend::parse(row.get("storage_backend"))
            .ok_or_else(|| DownloadError::Invalid("invalid backend".into()))?,
        storage_key: row.get("storage_key"),
        status: AttachmentStatus::parse(row.get("status"))
            .ok_or_else(|| DownloadError::Invalid("invalid status".into()))?,
        size_bytes: row.get("size_bytes"),
        is_public: row.get("is_public"),
    })
}

async fn load_attachment_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    attachment_id: &str,
) -> Result<AttachmentView, DownloadError> {
    let row = sqlx::query(
        "SELECT id, owner_id, storage_backend, storage_key, status, size_bytes, is_public \
         FROM attachments WHERE id = ?",
    )
    .bind(attachment_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| DownloadError::NotFound("attachment not found".into()))?;
    Ok(AttachmentView {
        id: row.get("id"),
        owner_id: row.get("owner_id"),
        storage_backend: StorageBackend::parse(row.get("storage_backend"))
            .ok_or_else(|| DownloadError::Invalid("invalid backend".into()))?,
        storage_key: row.get("storage_key"),
        status: AttachmentStatus::parse(row.get("status"))
            .ok_or_else(|| DownloadError::Invalid("invalid status".into()))?,
        size_bytes: row.get("size_bytes"),
        is_public: row.get("is_public"),
    })
}

async fn board_of_attachment_sqlite(
    conn: &mut sqlx::SqliteConnection,
    attachment_id: &str,
) -> Result<Option<String>, DownloadError> {
    let board: Option<String> = sqlx::query_scalar(
        "SELECT target_id FROM attachment_links WHERE attachment_id = ? AND target_type = 'post' LIMIT 1",
    )
    .bind(attachment_id)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(post_id) = board {
        let board_id: Option<String> =
            sqlx::query_scalar("SELECT board_id FROM posts WHERE id = ?")
                .bind(post_id)
                .fetch_optional(&mut *conn)
                .await?;
        return Ok(board_id);
    }
    Ok(None)
}

async fn board_of_attachment_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    attachment_id: &str,
) -> Result<Option<String>, DownloadError> {
    let post_id: Option<String> = sqlx::query_scalar(
        "SELECT target_id FROM attachment_links WHERE attachment_id = ? AND target_type = 'post' LIMIT 1",
    )
    .bind(attachment_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(post_id) = post_id {
        let board_id: Option<String> =
            sqlx::query_scalar("SELECT board_id FROM posts WHERE id = ?")
                .bind(post_id)
                .fetch_optional(&mut **tx)
                .await?;
        return Ok(board_id);
    }
    Ok(None)
}

/// 按作用域解析策略：附件 → 板块 → 站点（M06-DOWNLOAD-01）。
async fn resolve_policy_sqlite(
    conn: &mut sqlx::SqliteConnection,
    attachment: &AttachmentView,
) -> Result<DownloadPolicy, DownloadError> {
    let board_id = board_of_attachment_sqlite(conn, &attachment.id).await?;
    // 附件级
    let row = sqlx::query(&format!(
        "SELECT {POLICY_COLUMNS} FROM download_billing_policies \
         WHERE scope_type = 'attachment' AND scope_id = ? AND is_enabled = 1 \
         ORDER BY version DESC LIMIT 1"
    ))
    .bind(&attachment.id)
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(row) = row {
        return Ok(policy_from_sqlite(&row));
    }
    // 板块级
    if let Some(board) = board_id {
        let row = sqlx::query(&format!(
            "SELECT {POLICY_COLUMNS} FROM download_billing_policies \
             WHERE scope_type = 'board' AND scope_id = ? AND is_enabled = 1 \
             ORDER BY version DESC LIMIT 1"
        ))
        .bind(&board)
        .fetch_optional(&mut *conn)
        .await?;
        if let Some(row) = row {
            return Ok(policy_from_sqlite(&row));
        }
    }
    // 站点级
    let row = sqlx::query(&format!(
        "SELECT {POLICY_COLUMNS} FROM download_billing_policies \
         WHERE scope_type = 'site' AND scope_id IS NULL AND is_enabled = 1 \
         ORDER BY version DESC LIMIT 1"
    ))
    .fetch_optional(&mut *conn)
    .await?;
    if let Some(row) = row {
        return Ok(policy_from_sqlite(&row));
    }
    // 默认：free（站点未配置时不收费）
    Ok(DownloadPolicy {
        id: "default".into(),
        scope_type: "site".into(),
        scope_id: None,
        mode: "free".into(),
        currency_id: Some(CURRENCY_COIN.to_string()),
        amount: 0,
        authorization_ttl_seconds: 3600,
        daily_user_limit: None,
        single_charge_limit: None,
        attachment_revenue_limit: None,
        grace_on_disable: true,
        version: 0,
        is_enabled: true,
    })
}

async fn resolve_policy_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    attachment: &AttachmentView,
) -> Result<DownloadPolicy, DownloadError> {
    let board_id = board_of_attachment_mysql(tx, &attachment.id).await?;
    let row = sqlx::query(&format!(
        "SELECT {POLICY_COLUMNS} FROM download_billing_policies \
         WHERE scope_type = 'attachment' AND scope_id = ? AND is_enabled = 1 \
         ORDER BY version DESC LIMIT 1"
    ))
    .bind(&attachment.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = row {
        return Ok(policy_from_mysql(&row));
    }
    if let Some(board) = board_id {
        let row = sqlx::query(&format!(
            "SELECT {POLICY_COLUMNS} FROM download_billing_policies \
             WHERE scope_type = 'board' AND scope_id = ? AND is_enabled = 1 \
             ORDER BY version DESC LIMIT 1"
        ))
        .bind(&board)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(row) = row {
            return Ok(policy_from_mysql(&row));
        }
    }
    let row = sqlx::query(&format!(
        "SELECT {POLICY_COLUMNS} FROM download_billing_policies \
         WHERE scope_type = 'site' AND scope_id IS NULL AND is_enabled = 1 \
         ORDER BY version DESC LIMIT 1"
    ))
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = row {
        return Ok(policy_from_mysql(&row));
    }
    Ok(DownloadPolicy {
        id: "default".into(),
        scope_type: "site".into(),
        scope_id: None,
        mode: "free".into(),
        currency_id: Some(CURRENCY_COIN.to_string()),
        amount: 0,
        authorization_ttl_seconds: 3600,
        daily_user_limit: None,
        single_charge_limit: None,
        attachment_revenue_limit: None,
        grace_on_disable: true,
        version: 0,
        is_enabled: true,
    })
}

/// 计算该策略下应付金额（mode 决定）。
fn charge_amount(policy: &DownloadPolicy) -> Option<i64> {
    match policy.mode.as_str() {
        "fixed" => Some(policy.amount),
        "forced_paid" => Some(policy.amount),
        "free" | "forced_free" => Some(0),
        "disabled" => None,
        _ => Some(0), // inherit 在解析后已展开；兜底免费
    }
}

/// 创建下载授权（首次授权；免费也写授权）。
#[allow(clippy::explicit_auto_deref)]
pub async fn download(
    pool: &DatabasePool,
    storage: &StorageService,
    user_id: &str,
    attachment_id: &str,
    idempotency_key: &str,
) -> Result<Value, DownloadError> {
    let now = now_millis();
    let key = if idempotency_key.is_empty() {
        uuid::Uuid::now_v7().simple().to_string()
    } else {
        idempotency_key.to_string()
    };

    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, DownloadError> = async {
                let attachment = load_attachment_sqlite(&mut *conn, attachment_id).await?;
                // 未 ready 不泄漏（统一 NotFound）。
                if attachment.status != AttachmentStatus::Ready {
                    return Err(DownloadError::NotFound("attachment not available".into()));
                }
                let policy = resolve_policy_sqlite(&mut *conn, &attachment).await?;
                if policy.mode == "disabled" {
                    return Err(DownloadError::Forbidden("download disabled".into()));
                }
                let amount = charge_amount(&policy).unwrap_or(0);

                // 幂等记录：同键同摘要重放原授权；不同摘要冲突。
                let request_hash = hash_request(user_id, attachment_id, &key);
                if let Some(existing) = sqlx::query(
                    "SELECT authorization_id FROM download_idempotency_records \
                     WHERE scope = 'download' AND user_id = ? AND idempotency_key = ?",
                )
                .bind(user_id)
                .bind(&key)
                .fetch_optional(&mut *conn)
                .await?
                {
                    let auth_id: String = existing.get("authorization_id");
                    let stored_hash: String = sqlx::query_scalar(
                        "SELECT request_hash FROM download_idempotency_records \
                         WHERE scope = 'download' AND user_id = ? AND idempotency_key = ?",
                    )
                    .bind(user_id)
                    .bind(&key)
                    .fetch_one(&mut *conn)
                    .await?;
                    if stored_hash != request_hash {
                        return Err(DownloadError::IdempotencyConflict);
                    }
                    return sign_url_impl(&mut *conn, storage, user_id, &auth_id, &policy).await;
                }

                // 扣款（如需）：账本 in-tx。
                let (operation_id, charged_amount) = if amount > 0 {
                    let currency_id = policy
                        .currency_id
                        .clone()
                        .unwrap_or_else(|| CURRENCY_COIN.to_string());
                    let cmd = LedgerCommand {
                        idempotency_scope: "download".to_string(),
                        idempotency_key: uuid::Uuid::now_v7().to_string(),
                        kind: LedgerKind::Consume,
                        actor_id: Some(user_id.to_string()),
                        user_id: user_id.to_string(),
                        currency_id,
                        delta_balance: -amount,
                        delta_frozen: 0,
                        source_type: Some("attachment".to_string()),
                        source_id: Some(attachment.id.clone()),
                        memo: format!("download attachment {}", attachment.id),
                        reverses_operation_id: None,
                    };
                    let op = ledger::apply_operation_in_sqlite_tx(&mut *conn, cmd, now).await?;
                    (Some(op.operation_id), amount)
                } else {
                    (None, 0)
                };

                // 创建授权。
                let auth_id = uuid::Uuid::now_v7().to_string();
                let valid_from = now;
                let expires_at = now + policy.authorization_ttl_seconds * 1000;
                sqlx::query(
                    "INSERT INTO download_authorizations
                         (id, attachment_id, user_id, policy_version, point_operation_id, status, charged_amount, currency_id, valid_from, expires_at, created_at)
                     VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, ?)",
                )
                .bind(&auth_id)
                .bind(&attachment.id)
                .bind(user_id)
                .bind(policy.version)
                .bind(&operation_id)
                .bind(charged_amount)
                .bind(&policy.currency_id)
                .bind(valid_from)
                .bind(expires_at)
                .bind(now)
                .execute(&mut *conn)
                .await?;

                // 幂等记录。
                sqlx::query(
                    "INSERT INTO download_idempotency_records
                         (scope, user_id, idempotency_key, request_hash, authorization_id, response_status, created_at, completed_at)
                     VALUES ('download', ?, ?, ?, ?, 'authorized', ?, ?)",
                )
                .bind(user_id)
                .bind(&key)
                .bind(&request_hash)
                .bind(&auth_id)
                .bind(now)
                .bind(now)
                .execute(&mut *conn)
                .await?;

                // 审计 + Outbox 同事务。
                AuditEntry::user_action(user_id, "download.authorize")
                    .with_target("attachment", &attachment.id)
                    .with_target("authorization", &auth_id)
                    .with_reason("attachment download")
                    .record_into_sqlite(&mut *conn)
                    .await?;
                enqueue_sqlite(
                    &mut *conn,
                    DOWNLOAD_AUTHORIZATION_CREATED,
                    json!({
                        "authorization_id": auth_id,
                        "attachment_id": attachment.id,
                        "user_id": user_id,
                        "charged_amount": charged_amount,
                    }),
                )
                .await?;

                sign_url_impl(&mut *conn, storage, user_id, &auth_id, &policy).await
            }
            .await;
            match outcome {
                Ok(v) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let outcome: Result<Value, DownloadError> = async {
                let attachment = load_attachment_mysql(&mut tx, attachment_id).await?;
                if attachment.status != AttachmentStatus::Ready {
                    return Err(DownloadError::NotFound("attachment not available".into()));
                }
                let policy = resolve_policy_mysql(&mut tx, &attachment).await?;
                if policy.mode == "disabled" {
                    return Err(DownloadError::Forbidden("download disabled".into()));
                }
                let amount = charge_amount(&policy).unwrap_or(0);
                let request_hash = hash_request(user_id, attachment_id, &key);
                if let Some(existing) = sqlx::query(
                    "SELECT authorization_id FROM download_idempotency_records \
                     WHERE scope = 'download' AND user_id = ? AND idempotency_key = ?",
                )
                .bind(user_id)
                .bind(&key)
                .fetch_optional(&mut *tx)
                .await?
                {
                    let auth_id: String = existing.get("authorization_id");
                    let stored_hash: String = sqlx::query_scalar(
                        "SELECT request_hash FROM download_idempotency_records \
                         WHERE scope = 'download' AND user_id = ? AND idempotency_key = ?",
                    )
                    .bind(user_id)
                    .bind(&key)
                    .fetch_one(&mut *tx)
                    .await?;
                    if stored_hash != request_hash {
                        return Err(DownloadError::IdempotencyConflict);
                    }
                    return sign_url_mysql(&mut tx, storage, user_id, &auth_id).await;
                }
                let (operation_id, charged_amount) = if amount > 0 {
                    let currency_id = policy
                        .currency_id
                        .clone()
                        .unwrap_or_else(|| CURRENCY_COIN.to_string());
                    let cmd = LedgerCommand {
                        idempotency_scope: "download".to_string(),
                        idempotency_key: uuid::Uuid::now_v7().to_string(),
                        kind: LedgerKind::Consume,
                        actor_id: Some(user_id.to_string()),
                        user_id: user_id.to_string(),
                        currency_id,
                        delta_balance: -amount,
                        delta_frozen: 0,
                        source_type: Some("attachment".to_string()),
                        source_id: Some(attachment.id.clone()),
                        memo: format!("download attachment {}", attachment.id),
                        reverses_operation_id: None,
                    };
                    let op = ledger::apply_operation_in_mysql_tx(&mut tx, cmd, now).await?;
                    (Some(op.operation_id), amount)
                } else {
                    (None, 0)
                };
                let auth_id = uuid::Uuid::now_v7().to_string();
                let valid_from = now;
                let expires_at = now + policy.authorization_ttl_seconds * 1000;
                sqlx::query(
                    "INSERT INTO download_authorizations
                         (id, attachment_id, user_id, policy_version, point_operation_id, status, charged_amount, currency_id, valid_from, expires_at, created_at)
                     VALUES (?, ?, ?, ?, ?, 'active', ?, ?, ?, ?, ?)",
                )
                .bind(&auth_id)
                .bind(&attachment.id)
                .bind(user_id)
                .bind(policy.version)
                .bind(&operation_id)
                .bind(charged_amount)
                .bind(&policy.currency_id)
                .bind(valid_from)
                .bind(expires_at)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                sqlx::query(
                    "INSERT INTO download_idempotency_records
                         (scope, user_id, idempotency_key, request_hash, authorization_id, response_status, created_at, completed_at)
                     VALUES ('download', ?, ?, ?, ?, 'authorized', ?, ?)",
                )
                .bind(user_id)
                .bind(&key)
                .bind(&request_hash)
                .bind(&auth_id)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                AuditEntry::user_action(user_id, "download.authorize")
                    .with_target("attachment", &attachment.id)
                    .with_target("authorization", &auth_id)
                    .with_reason("attachment download")
                    .record_into_mysql(&mut tx)
                    .await?;
                enqueue_mysql(
                    &mut tx,
                    DOWNLOAD_AUTHORIZATION_CREATED,
                    json!({
                        "authorization_id": auth_id,
                        "attachment_id": attachment.id,
                        "user_id": user_id,
                        "charged_amount": charged_amount,
                    }),
                )
                .await?;
                sign_url_mysql(&mut tx, storage, user_id, &auth_id).await
            }
            .await;
            match outcome {
                Ok(v) => {
                    tx.commit().await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            }
        }
    }
}

/// 读取授权（本人）。
pub async fn get_authorization(
    pool: &DatabasePool,
    user_id: &str,
    auth_id: &str,
) -> Result<Value, DownloadError> {
    match pool {
        Either::Left(p) => {
            let row = sqlx::query(
                "SELECT id, attachment_id, policy_version, status, charged_amount, currency_id, valid_from, expires_at, created_at \
                 FROM download_authorizations WHERE id = ? AND user_id = ?",
            )
            .bind(auth_id)
            .bind(user_id)
            .fetch_optional(p)
            .await?
            .ok_or_else(|| DownloadError::NotFound("authorization not found".into()))?;
            Ok(json!({
                "id": row.get::<String,_>("id"),
                "attachment_id": row.get::<String,_>("attachment_id"),
                "policy_version": row.get::<i64,_>("policy_version"),
                "status": row.get::<String,_>("status"),
                "charged_amount": row.get::<i64,_>("charged_amount"),
                "currency_id": row.get::<Option<String>,_>("currency_id"),
                "valid_from": row.get::<i64,_>("valid_from"),
                "expires_at": row.get::<i64,_>("expires_at"),
                "created_at": row.get::<i64,_>("created_at"),
            }))
        }
        Either::Right(p) => {
            let row = sqlx::query(
                "SELECT id, attachment_id, policy_version, status, charged_amount, currency_id, valid_from, expires_at, created_at \
                 FROM download_authorizations WHERE id = ? AND user_id = ?",
            )
            .bind(auth_id)
            .bind(user_id)
            .fetch_optional(p)
            .await?
            .ok_or_else(|| DownloadError::NotFound("authorization not found".into()))?;
            Ok(json!({
                "id": row.get::<String,_>("id"),
                "attachment_id": row.get::<String,_>("attachment_id"),
                "policy_version": row.get::<i64,_>("policy_version"),
                "status": row.get::<String,_>("status"),
                "charged_amount": row.get::<i64,_>("charged_amount"),
                "currency_id": row.get::<Option<String>,_>("currency_id"),
                "valid_from": row.get::<i64,_>("valid_from"),
                "expires_at": row.get::<i64,_>("expires_at"),
                "created_at": row.get::<i64,_>("created_at"),
            }))
        }
    }
}

/// 有效授权重签 URL（不重复扣款；M06-DOWNLOAD-05）。
pub async fn sign_url(
    pool: &DatabasePool,
    storage: &StorageService,
    user_id: &str,
    auth_id: &str,
) -> Result<Value, DownloadError> {
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            let auth = sqlx::query(
                "SELECT id, attachment_id, status, valid_from, expires_at, policy_version FROM download_authorizations WHERE id = ? AND user_id = ?",
            )
            .bind(auth_id)
            .bind(user_id)
            .fetch_optional(&mut *conn)
            .await?
            .ok_or_else(|| DownloadError::NotFound("authorization not found".into()))?;
            let status: String = auth.get("status");
            let expires_at: i64 = auth.get("expires_at");
            let now = now_millis();
            if status != "active" || expires_at <= now {
                return Err(DownloadError::Forbidden("authorization expired".into()));
            }
            let attachment_id: String = auth.get("attachment_id");
            let attachment = load_attachment_sqlite(&mut conn, &attachment_id).await?;
            let policy = resolve_policy_sqlite(&mut conn, &attachment).await?;
            sign_url_impl(&mut conn, storage, user_id, auth_id, &policy).await
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let auth = sqlx::query(
                "SELECT id, attachment_id, status, valid_from, expires_at FROM download_authorizations WHERE id = ? AND user_id = ?",
            )
            .bind(auth_id)
            .bind(user_id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or_else(|| DownloadError::NotFound("authorization not found".into()))?;
            let status: String = auth.get("status");
            let expires_at: i64 = auth.get("expires_at");
            let now = now_millis();
            if status != "active" || expires_at <= now {
                return Err(DownloadError::Forbidden("authorization expired".into()));
            }
            let attachment_id: String = auth.get("attachment_id");
            let attachment = load_attachment_mysql(&mut tx, &attachment_id).await?;
            let _policy = resolve_policy_mysql(&mut tx, &attachment).await?;
            sign_url_mysql(&mut tx, storage, user_id, auth_id).await
        }
    }
}

/// 签名 URL（SQLite 事务内）。
async fn sign_url_impl(
    conn: &mut sqlx::SqliteConnection,
    storage: &StorageService,
    user_id: &str,
    auth_id: &str,
    policy: &DownloadPolicy,
) -> Result<Value, DownloadError> {
    let row = sqlx::query(
        "SELECT a.id, a.attachment_id, a.status, a.expires_at, at.storage_backend, at.storage_key \
         FROM download_authorizations a JOIN attachments at ON at.id = a.attachment_id \
         WHERE a.id = ? AND a.user_id = ?",
    )
    .bind(auth_id)
    .bind(user_id)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| DownloadError::NotFound("authorization not found".into()))?;
    let status: String = row.get("status");
    let expires_at: i64 = row.get("expires_at");
    if status != "active" {
        return Err(DownloadError::Forbidden("authorization not active".into()));
    }
    let backend_str: String = row.get("storage_backend");
    let backend = StorageBackend::parse(&backend_str).unwrap_or(StorageBackend::Local);
    let storage_key: String = row.get("storage_key");
    let ttl = policy
        .authorization_ttl_seconds
        .min(expires_at.saturating_sub(now_millis()) / 1000)
        .max(60);
    let adapter = storage
        .adapter(backend)
        .map_err(|e| DownloadError::Db(e.to_string()))?;
    if adapter.supports_presign() {
        let url = adapter
            .presign_download(&storage_key, ttl as u64)
            .await
            .map_err(|e| DownloadError::Unavailable(e.code().to_string()))?;
        Ok(json!({
            "authorization_id": auth_id,
            "url": url.url,
            "url_expires_at": url.expires_at,
            "method": "GET",
            "attachment_id": row.get::<String,_>("attachment_id"),
        }))
    } else {
        // local：内容端点流式（Range 由 content 端点处理）。
        Ok(json!({
            "authorization_id": auth_id,
            "url": format!("/api/v1/attachments/{}/content", row.get::<String,_>("attachment_id")),
            "attachment_id": row.get::<String,_>("attachment_id"),
            "local": true,
        }))
    }
}

/// 签名 URL（MySQL 事务内）。
async fn sign_url_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    storage: &StorageService,
    user_id: &str,
    auth_id: &str,
) -> Result<Value, DownloadError> {
    let row = sqlx::query(
        "SELECT a.id, a.attachment_id, a.status, a.expires_at, at.storage_backend, at.storage_key \
         FROM download_authorizations a JOIN attachments at ON at.id = a.attachment_id \
         WHERE a.id = ? AND a.user_id = ?",
    )
    .bind(auth_id)
    .bind(user_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| DownloadError::NotFound("authorization not found".into()))?;
    let status: String = row.get("status");
    let expires_at: i64 = row.get("expires_at");
    if status != "active" {
        return Err(DownloadError::Forbidden("authorization not active".into()));
    }
    let backend_str: String = row.get("storage_backend");
    let backend = StorageBackend::parse(&backend_str).unwrap_or(StorageBackend::Local);
    let storage_key: String = row.get("storage_key");
    let ttl = (expires_at.saturating_sub(now_millis()) / 1000).max(60);
    let adapter = storage
        .adapter(backend)
        .map_err(|e| DownloadError::Db(e.to_string()))?;
    if adapter.supports_presign() {
        let url = adapter
            .presign_download(&storage_key, ttl as u64)
            .await
            .map_err(|e| DownloadError::Unavailable(e.code().to_string()))?;
        Ok(json!({
            "authorization_id": auth_id,
            "url": url.url,
            "url_expires_at": url.expires_at,
            "method": "GET",
            "attachment_id": row.get::<String,_>("attachment_id"),
        }))
    } else {
        Ok(json!({
            "authorization_id": auth_id,
            "url": format!("/api/v1/attachments/{}/content", row.get::<String,_>("attachment_id")),
            "attachment_id": row.get::<String,_>("attachment_id"),
            "local": true,
        }))
    }
}

fn hash_request(user_id: &str, attachment_id: &str, key: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(format!("{user_id}|{attachment_id}|{key}"));
    hex::encode(hasher.finalize())
}

async fn enqueue_sqlite(
    conn: &mut sqlx::SqliteConnection,
    event_type: &str,
    payload: Value,
) -> Result<(), DownloadError> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    sqlx::query(
        "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
         VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(&payload_str)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn enqueue_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    event_type: &str,
    payload: Value,
) -> Result<(), DownloadError> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    sqlx::query(
        "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
         VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(&payload_str)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}
