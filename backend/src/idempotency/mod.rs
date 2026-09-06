//! 幂等记录数据模型与判定（M01-AUDIT-03/04）。
//!
//! 数据模型（表 `idempotency_records`，迁移 0010）：
//! - `scope` + `key` 唯一标识一次业务操作（唯一约束兜底并发首请求，
//!   M01-AUDIT-05）；
//! - `request_hash`：请求摘要（SHA-256 hex），用于"相同 key+摘要返回原结果、
//!   不同摘要稳定返回 409"（M01-AUDIT-04）；
//! - `status`：`in_progress` / `completed` / `failed`；
//! - `response_reference`：已存储响应/结果的引用（如 job id）；
//! - `expires_at`：保留窗口，过期记录可清理/重试。

use sha2::{Digest, Sha256};
use sqlx::Either;

use crate::db::pool::DatabasePool;

/// 当前 Unix 毫秒（跨库时间约定 SCHEMA §2.2）。
fn now_millis() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 幂等记录状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IdempotencyStatus {
    InProgress,
    Completed,
    Failed,
}

impl IdempotencyStatus {
    pub const ALL: [IdempotencyStatus; 3] = [
        IdempotencyStatus::InProgress,
        IdempotencyStatus::Completed,
        IdempotencyStatus::Failed,
    ];

    /// 数据库表示（与 idempotency_records.status 一致）。
    pub fn as_str(&self) -> &'static str {
        match self {
            IdempotencyStatus::InProgress => "in_progress",
            IdempotencyStatus::Completed => "completed",
            IdempotencyStatus::Failed => "failed",
        }
    }

    pub fn parse(value: &str) -> Option<IdempotencyStatus> {
        Self::ALL
            .iter()
            .find(|status| status.as_str() == value)
            .copied()
    }
}

impl std::fmt::Display for IdempotencyStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 幂等键（scope + key），带校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyKey {
    pub scope: String,
    pub key: String,
}

impl IdempotencyKey {
    /// 校验并构造幂等键。
    ///
    /// - `scope`：非空，≤ 50 字符（如 `pay`、`download`、`purchase`）；
    /// - `key`：非空，≤ 200 字符（客户端幂等键）。
    pub fn new(scope: impl Into<String>, key: impl Into<String>) -> Result<Self, IdempotencyError> {
        let scope = scope.into();
        let key = key.into();
        if scope.is_empty() || scope.len() > 50 {
            return Err(IdempotencyError::InvalidScope);
        }
        if key.is_empty() || key.len() > 200 {
            return Err(IdempotencyError::InvalidKey);
        }
        Ok(Self { scope, key })
    }
}

/// 幂等记录（对应 `idempotency_records` 一行）。
#[derive(Debug, Clone)]
pub struct IdempotencyRecord {
    pub id: String,
    pub scope: String,
    pub key: String,
    /// 请求摘要（SHA-256 hex，64 字符）。
    pub request_hash: String,
    pub status: IdempotencyStatus,
    /// 已存储响应/结果的引用（如 job id）。
    pub response_reference: Option<String>,
    /// 保留窗口截止（Unix 毫秒）。
    pub expires_at: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 幂等键/哈希校验错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyError {
    InvalidScope,
    InvalidKey,
    InvalidRequestHash,
}

impl std::fmt::Display for IdempotencyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IdempotencyError::InvalidScope => write!(f, "idempotency scope must be 1..=50 chars"),
            IdempotencyError::InvalidKey => write!(f, "idempotency key must be 1..=200 chars"),
            IdempotencyError::InvalidRequestHash => {
                write!(f, "request_hash must be 64-char hex")
            }
        }
    }
}

impl std::error::Error for IdempotencyError {}

/// 计算请求摘要（SHA-256 hex）。
///
/// 用于 M01-AUDIT-04：相同 key+摘要返回原结果；相同 key+不同摘要返回 409。
pub fn request_hash(payload: &[u8]) -> String {
    hex::encode(Sha256::digest(payload))
}

/// 校验请求摘要是否为 64 字符 hex。
pub fn validate_request_hash(hash: &str) -> Result<(), IdempotencyError> {
    if hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(IdempotencyError::InvalidRequestHash)
    }
}

/// 幂等判定结果（M01-AUDIT-04）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdempotencyOutcome {
    /// 首次请求：已创建 `in_progress` 记录，调用方执行操作后调用
    /// [`complete`] / [`mark_failed`]。
    Created { record_id: String },
    /// 重放：相同 key+摘要且上次已完成，返回原结果引用（原响应）。
    Replay { response_reference: Option<String> },
    /// 相同 key+摘要但仍在处理中（并发）：调用方不得执行，等待或返回进行中。
    InProgress,
    /// 相同 key 但摘要不同：调用方必须稳定返回 409。
    Conflict,
    /// 相同 key+摘要但上次执行失败且按 [`FailureCachePolicy::Cache`] 缓存：
    /// 返回已存储的失败结果，不重新执行。
    Failed { response_reference: Option<String> },
}

/// 失败是否缓存（M01-AUDIT-05）：由 operation 契约显式指定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureCachePolicy {
    /// 缓存失败：同 key+摘要且上次 `failed` → 返回 `Failed`，不重新执行。
    Cache,
    /// 不缓存失败：同 key+摘要且上次 `failed` → 重置为 `in_progress`
    /// 并返回 `Created`，允许重新执行。
    Retry,
}

/// `in_progress` 视为崩溃残留的阈值（M01-AUDIT-05）：
/// 超过该时长未推进（`updated_at` 停滞）时，允许同 key+摘要的新执行者
/// 接管记录——进程在创建与完成之间崩溃后，幂等键不会卡死到 TTL 过期
/// （checkout 场景为 24h）。10 分钟远大于请求超时（30s）与最长业务链路，
/// 不会误接管仍在执行的请求；业务侧二次执行由各自的条件更新/唯一键兜底。
const IN_PROGRESS_STALE_MS: i64 = 10 * 60 * 1000;

/// 并发插入冲突后记录又被并发清理（过期删除）时的有限重试次数；
/// 超过后按 `InProgress` 退避，避免任何路径 panic。
const MAX_INSERT_ATTEMPTS: u32 = 3;

/// 按现有记录判定结果（M01-AUDIT-04/05）。
///
/// 返回 `Some(outcome)`：直接返回该结果；返回 `None`：记录可由当前
/// 执行者接管（`failed` + Retry 策略，或 `in_progress` 崩溃残留），
/// 见 [`take_over_record`]。
fn outcome_for_record(
    record: &IdempotencyRecord,
    request_hash: &str,
    failure_policy: FailureCachePolicy,
    now: i64,
) -> Option<IdempotencyOutcome> {
    if record.request_hash != request_hash {
        return Some(IdempotencyOutcome::Conflict);
    }
    match record.status {
        IdempotencyStatus::Completed => Some(IdempotencyOutcome::Replay {
            response_reference: record.response_reference.clone(),
        }),
        IdempotencyStatus::Failed => match failure_policy {
            FailureCachePolicy::Cache => Some(IdempotencyOutcome::Failed {
                response_reference: record.response_reference.clone(),
            }),
            // Retry 策略：重置后由当前执行者接管
            FailureCachePolicy::Retry => None,
        },
        IdempotencyStatus::InProgress => {
            if now - record.updated_at > IN_PROGRESS_STALE_MS {
                // 崩溃残留：接管，防止幂等键卡死到 TTL
                None
            } else {
                Some(IdempotencyOutcome::InProgress)
            }
        }
    }
}

/// 接管记录：`failed`（Retry 策略）重置为 `in_progress`；`in_progress`
/// 崩溃残留刷新 `updated_at` 作为新执行者心跳。返回 `Created`。
async fn take_over_record(
    pool: &DatabasePool,
    record: &IdempotencyRecord,
    now: i64,
) -> Result<IdempotencyOutcome, sqlx::Error> {
    if record.status == IdempotencyStatus::Failed {
        reset_to_in_progress(pool, &record.id).await?;
    } else {
        heartbeat_in_progress(pool, &record.id, now).await?;
    }
    Ok(IdempotencyOutcome::Created {
        record_id: record.id.clone(),
    })
}

/// 刷新 `in_progress` 记录的 `updated_at`（接管心跳）。
async fn heartbeat_in_progress(
    pool: &DatabasePool,
    record_id: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE idempotency_records SET updated_at = ?
             WHERE id = ? AND status = 'in_progress'",
        )
        .bind(now)
        .bind(record_id)
        .execute(p)
        .await
        .map(|_| ()),
        Either::Right(p) => sqlx::query(
            "UPDATE idempotency_records SET updated_at = ?
             WHERE id = ? AND status = 'in_progress'",
        )
        .bind(now)
        .bind(record_id)
        .execute(p)
        .await
        .map(|_| ()),
    }
}

/// 开始一次幂等操作：首次创建 `in_progress` 记录；重复投递返回原结果；
/// 摘要不一致返回冲突（M01-AUDIT-04）；并发首请求只有一个执行者
/// （M01-AUDIT-05，唯一约束兜底）；失败是否缓存按 `failure_policy` 明确处理；
/// `in_progress` 崩溃残留超过阈值后允许接管（不卡死到 TTL）。
///
/// - 相同 key+摘要且 `completed` → `Replay`（原结果）；
/// - 相同 key+摘要且 `in_progress`（未超时）→ `InProgress`（并发，不执行）；
/// - 相同 key+摘要且 `in_progress` 超过 [`IN_PROGRESS_STALE_MS`] → 接管重执行；
/// - 相同 key+摘要且 `failed` → 按 `failure_policy`：`Cache` → `Failed`；
///   `Retry` → 重置并 `Created`；
/// - 相同 key+不同摘要 → 稳定 `Conflict`（409）；
/// - 记录已过期（`expires_at < now`）→ 删除并以 `Created` 重新开始。
pub async fn begin_or_replay(
    pool: &DatabasePool,
    key: &IdempotencyKey,
    request_hash: &str,
    ttl_ms: i64,
    failure_policy: FailureCachePolicy,
) -> Result<IdempotencyOutcome, sqlx::Error> {
    validate_request_hash(request_hash).map_err(|e| sqlx::Error::Protocol(e.to_string()))?;
    let now = now_millis();

    if let Some(record) = fetch_record(pool, key).await? {
        if record.expires_at < now {
            // 记录已过期：删除后按首次请求重新开始
            delete_record(pool, &record.id).await?;
        } else if let Some(outcome) = outcome_for_record(&record, request_hash, failure_policy, now)
        {
            return Ok(outcome);
        } else {
            return take_over_record(pool, &record, now).await;
        }
    }

    // 首次请求：插入 in_progress 记录。并发唯一冲突 → 按现有记录判定；
    // 记录恰在冲突与重查之间被并发清理（过期删除）→ 有限重试插入，
    // 不再 `expect`（原实现在此路径可能 panic，M01-AUDIT-05）。
    for _ in 0..MAX_INSERT_ATTEMPTS {
        let id = uuid::Uuid::now_v7().to_string();
        let expires_at = now.saturating_add(ttl_ms.max(0));
        match insert_in_progress(pool, &id, key, request_hash, expires_at, now).await {
            Ok(()) => return Ok(IdempotencyOutcome::Created { record_id: id }),
            Err(err) if is_unique_violation(&err) => match fetch_record(pool, key).await? {
                Some(record) => {
                    return match outcome_for_record(&record, request_hash, failure_policy, now) {
                        Some(outcome) => Ok(outcome),
                        None => take_over_record(pool, &record, now).await,
                    };
                }
                // 记录已被并发请求按过期清理：重试插入
                None => continue,
            },
            Err(err) => return Err(err),
        }
    }
    // 极端并发下每轮插入都被抢先且记录又被清理：按进行中退避，
    // 调用方稍后重试（不 panic、不错误执行）
    Ok(IdempotencyOutcome::InProgress)
}

/// 标记幂等操作完成（`in_progress → completed`），保存响应引用。
///
/// 仅 `in_progress` 可完成；返回 `false` 表示记录状态已变化（不应发生）。
pub async fn complete(
    pool: &DatabasePool,
    record_id: &str,
    response_reference: &str,
) -> Result<bool, sqlx::Error> {
    let now = now_millis();
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE idempotency_records
                 SET status = 'completed', response_reference = ?, updated_at = ?
                 WHERE id = ? AND status = 'in_progress'",
        )
        .bind(response_reference)
        .bind(now)
        .bind(record_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE idempotency_records
                 SET status = 'completed', response_reference = ?, updated_at = ?
                 WHERE id = ? AND status = 'in_progress'",
        )
        .bind(response_reference)
        .bind(now)
        .bind(record_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(rows == 1)
}

/// 标记幂等操作失败（`in_progress → failed`），供后续同摘要重试。
pub async fn mark_failed(pool: &DatabasePool, record_id: &str) -> Result<bool, sqlx::Error> {
    let now = now_millis();
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE idempotency_records
                 SET status = 'failed', updated_at = ?
                 WHERE id = ? AND status = 'in_progress'",
        )
        .bind(now)
        .bind(record_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE idempotency_records
                 SET status = 'failed', updated_at = ?
                 WHERE id = ? AND status = 'in_progress'",
        )
        .bind(now)
        .bind(record_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(rows == 1)
}

/// 把 `failed` 记录重置为 `in_progress`（Retry 策略：允许重新执行）。
async fn reset_to_in_progress(pool: &DatabasePool, record_id: &str) -> Result<(), sqlx::Error> {
    let now = now_millis();
    match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE idempotency_records
                 SET status = 'in_progress', response_reference = NULL, updated_at = ?
                 WHERE id = ? AND status = 'failed'",
        )
        .bind(now)
        .bind(record_id)
        .execute(p)
        .await
        .map(|_| ()),
        Either::Right(p) => sqlx::query(
            "UPDATE idempotency_records
                 SET status = 'in_progress', response_reference = NULL, updated_at = ?
                 WHERE id = ? AND status = 'failed'",
        )
        .bind(now)
        .bind(record_id)
        .execute(p)
        .await
        .map(|_| ()),
    }
}

/// 删除过期记录（expires_at 已到）。
async fn delete_record(pool: &DatabasePool, record_id: &str) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => sqlx::query("DELETE FROM idempotency_records WHERE id = ?")
            .bind(record_id)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query("DELETE FROM idempotency_records WHERE id = ?")
            .bind(record_id)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

async fn fetch_record(
    pool: &DatabasePool,
    key: &IdempotencyKey,
) -> Result<Option<IdempotencyRecord>, sqlx::Error> {
    // `key` 为 MySQL/MariaDB 保留字：反引号转义（SQLite 同样接受反引号，
    // 三库语法一致；见迁移 0010）
    const SELECT: &str = "SELECT id, scope, `key`, request_hash, status, response_reference, expires_at, created_at, updated_at
        FROM idempotency_records WHERE scope = ? AND `key` = ?";
    let row = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, RecordRow>(SELECT)
                .bind(&key.scope)
                .bind(&key.key)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, RecordRow>(SELECT)
                .bind(&key.scope)
                .bind(&key.key)
                .fetch_optional(p)
                .await?
        }
    };
    Ok(row.map(IdempotencyRecord::from))
}

async fn insert_in_progress(
    pool: &DatabasePool,
    id: &str,
    key: &IdempotencyKey,
    request_hash: &str,
    expires_at: i64,
    now: i64,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO idempotency_records (id, scope, `key`, request_hash, status, expires_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'in_progress', ?, ?, ?)",
            )
            .bind(id)
            .bind(&key.scope)
            .bind(&key.key)
            .bind(request_hash)
            .bind(expires_at)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO idempotency_records (id, scope, `key`, request_hash, status, expires_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'in_progress', ?, ?, ?)",
            )
            .bind(id)
            .bind(&key.scope)
            .bind(&key.key)
            .bind(request_hash)
            .bind(expires_at)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
        }
    }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db) if db.is_unique_violation()
    )
}

#[derive(sqlx::FromRow)]
struct RecordRow {
    id: String,
    scope: String,
    key: String,
    request_hash: String,
    status: String,
    response_reference: Option<String>,
    expires_at: i64,
    created_at: i64,
    updated_at: i64,
}

impl From<RecordRow> for IdempotencyRecord {
    fn from(row: RecordRow) -> Self {
        Self {
            id: row.id,
            scope: row.scope,
            key: row.key,
            request_hash: row.request_hash,
            status: IdempotencyStatus::parse(&row.status).expect("status 由 CHECK 约束保证合法"),
            response_reference: row.response_reference,
            expires_at: row.expires_at,
            created_at: row.created_at,
            updated_at: row.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_round_trips() {
        for status in IdempotencyStatus::ALL {
            assert_eq!(IdempotencyStatus::parse(status.as_str()), Some(status));
        }
        assert_eq!(IdempotencyStatus::parse("unknown"), None);
    }

    #[test]
    fn idempotency_key_validation() {
        assert!(IdempotencyKey::new("pay", "order-123").is_ok());
        assert_eq!(
            IdempotencyKey::new("", "order-123"),
            Err(IdempotencyError::InvalidScope)
        );
        assert_eq!(
            IdempotencyKey::new("pay", ""),
            Err(IdempotencyError::InvalidKey)
        );
        assert_eq!(
            IdempotencyKey::new("pay", "x".repeat(201)),
            Err(IdempotencyError::InvalidKey)
        );
        assert_eq!(
            IdempotencyKey::new("x".repeat(51), "order-123"),
            Err(IdempotencyError::InvalidScope)
        );
    }

    #[test]
    fn request_hash_is_deterministic_sha256_hex() {
        let a = request_hash(b"hello");
        let b = request_hash(b"hello");
        let c = request_hash(b"hello!");
        assert_eq!(a, b, "相同请求摘要一致");
        assert_ne!(a, c, "不同请求摘要不同");
        assert_eq!(a.len(), 64);
        assert!(validate_request_hash(&a).is_ok());
        assert_eq!(
            validate_request_hash("short"),
            Err(IdempotencyError::InvalidRequestHash)
        );
        assert_eq!(
            validate_request_hash(&"z".repeat(64)),
            Err(IdempotencyError::InvalidRequestHash)
        );
    }
}
