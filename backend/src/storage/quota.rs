//! M06-QUOTA：等级容量、reserved/charged/released 字节与保留期。
//!
//! 容量口径（M06-QUOTA-04）：
//! - create 阶段 `bytes_reserved += size`（预留，complete 后净效果 0）；
//! - complete 成功 `bytes_charged += size`、`bytes_released += reserved`、
//!   `bytes_reserved -= reserved`；
//! - quarantined 回滚 `bytes_released += reserved`、`bytes_reserved -= reserved`；
//! - 物理删除（30 天保留期后）`bytes_released += charged`、`bytes_charged -= charged`。
//!
//! 并发安全（M06-QUOTA-05）：SQLite 用 `BEGIN IMMEDIATE` 整体写锁
//! （`&mut *conn` 处需要 `#[allow(clippy::explicit_auto_deref)]`）；
//! MySQL/MariaDB 用 `SELECT ... FOR UPDATE` 行锁 + 事务，防止预留超卖与负数释放。
//! 固定锁顺序：所有操作先锁 `user_quota_counters` 行，再执行校验/更新。

use sqlx::Either;

use crate::db::DatabasePool;
use crate::outbox::now_millis;
use crate::storage::adapter::StorageService;
use crate::storage::error::StorageError;
use crate::storage::model::{AttachmentStatus, QuotaCounters, QuotaPolicy};

/// 站点级容量硬上限（所有用户 charged 容量之和的上界，M06-QUOTA-01）。
pub const SITE_TOTAL_HARD_LIMIT_BYTES: i64 = 8 * 1024 * 1024 * 1024; // 8 GiB
/// 单文件绝对硬上限（任何等级策略都不得超过）。
pub const SITE_SINGLE_FILE_HARD_LIMIT_BYTES: i64 = 128 * 1024 * 1024; // 128 MiB
/// 默认保留期（天，M06-QUOTA-09）。
pub const DEFAULT_RETENTION_DAYS: i64 = 30;
/// 预签名 URL 默认 TTL（秒；S3 直传/下载用，M06-QUOTA-08）。
pub const PRESIGN_TTL_SECS: u64 = 300;
/// 每日上传窗口（毫秒；滚动 24 小时口径）。
pub const DAILY_WINDOW_MS: i64 = 24 * 60 * 60 * 1000;
/// 孤儿对象保护宽限期（毫秒；未绑定附件的对象在该窗口内不清理）。
pub const ORPHAN_GRACE_MS: i64 = 24 * 60 * 60 * 1000;

/// 站点级默认等级配额（M06-QUOTA-01 seed；总容量恒不小于单文件上限）。
/// 返回 `(single_file_max_bytes, total_bytes, daily_upload_bytes, retention_days)`。
pub fn default_policy_for_level(level: i64) -> (i64, i64, i64, i64) {
    const MB: i64 = 1024 * 1024;
    let (single, total, daily) = match level {
        i64::MIN..=1 => (2 * MB, 100 * MB, 20 * MB),
        2 => (5 * MB, 250 * MB, 50 * MB),
        3 => (10 * MB, 500 * MB, 100 * MB),
        4 => (20 * MB, 1024 * MB, 200 * MB),
        _ => (20 * MB, 2 * 1024 * MB, 400 * MB),
    };
    (single, total, daily, DEFAULT_RETENTION_DAYS)
}

/// 读取某等级当前生效（最新 policy_version）的策略。
///
/// 等级尚无任何策略修订时，以站点默认值 seed 一条（actor 作为写入者，
/// `quota_policy_revisions.created_by` 外键要求真实用户）。
pub async fn get_policy_for_level(
    pool: &DatabasePool,
    level: i64,
    actor_id: &str,
) -> Result<QuotaPolicy, StorageError> {
    if let Some(policy) = latest_revision(pool, level).await? {
        return Ok(policy);
    }
    let (single, total, daily, retention) = default_policy_for_level(level);
    let policy_version = 1;
    insert_revision(
        pool,
        level,
        single,
        total,
        daily,
        retention,
        policy_version,
        actor_id,
        now_millis(),
    )
    .await?;
    Ok(QuotaPolicy {
        level,
        single_file_max_bytes: single,
        total_bytes: total,
        daily_upload_bytes: daily,
        retention_days: retention,
        policy_version,
    })
}

/// 某等级的全部策略修订（按 policy_version 升序；管理读取用）。
pub async fn get_policy_revisions(
    pool: &DatabasePool,
    level: i64,
) -> Result<Vec<QuotaPolicy>, StorageError> {
    let rows: Vec<PolicyRevisionRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, PolicyRevisionRow>(
                "SELECT level, single_file_max_bytes, total_bytes, daily_upload_bytes,
                        retention_days, policy_version
                 FROM quota_policy_revisions WHERE level = ?
                 ORDER BY policy_version ASC",
            )
            .bind(level)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, PolicyRevisionRow>(
                "SELECT level, single_file_max_bytes, total_bytes, daily_upload_bytes,
                        retention_days, policy_version
                 FROM quota_policy_revisions WHERE level = ?
                 ORDER BY policy_version ASC",
            )
            .bind(level)
            .fetch_all(p)
            .await?
        }
    };
    Ok(rows.into_iter().map(QuotaPolicy::from).collect())
}

/// 管理员更新等级配额（M06-QUOTA-02）：创建**新** policy_version，不修改旧行。
///
/// - `expected_version`：当前生效版本（If-Match 乐观锁）；不一致返回
///   [`StorageError::Conflict`]；
/// - 校验：所有值非负、单文件上限 ≤ 总容量、总容量 ≤ 站点硬上限；
/// - 事务内读最新版本（SQLite `BEGIN IMMEDIATE` / MySQL 行锁），
///   版本号 = max+1，并发更新不产生重复版本。
#[allow(clippy::too_many_arguments)]
pub async fn update_level_quota(
    pool: &DatabasePool,
    level: i64,
    single_file_max_bytes: i64,
    total_bytes: i64,
    daily_upload_bytes: i64,
    retention_days: i64,
    expected_version: i64,
    actor_id: &str,
    now: i64,
) -> Result<QuotaPolicy, StorageError> {
    validate_policy_values(
        single_file_max_bytes,
        total_bytes,
        daily_upload_bytes,
        retention_days,
    )?;

    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            #[allow(clippy::explicit_auto_deref)]
            let result = update_level_quota_locked(
                &mut *conn,
                level,
                single_file_max_bytes,
                total_bytes,
                daily_upload_bytes,
                retention_days,
                expected_version,
                actor_id,
                now,
            )
            .await;
            match result {
                Ok(policy) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(policy)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            #[allow(clippy::explicit_auto_deref)]
            let current: Option<PolicyRevisionRow> = sqlx::query_as::<_, PolicyRevisionRow>(
                "SELECT level, single_file_max_bytes, total_bytes, daily_upload_bytes,
                        retention_days, policy_version
                 FROM quota_policy_revisions
                 WHERE level = ? ORDER BY policy_version DESC LIMIT 1 FOR UPDATE",
            )
            .bind(level)
            .fetch_optional(&mut *tx)
            .await?;
            let current_version = current.as_ref().map(|r| r.policy_version).unwrap_or(0);
            if current_version != expected_version {
                return Err(StorageError::Conflict(format!(
                    "quota policy version mismatch: expected {expected_version}, current {current_version}"
                )));
            }
            let next_version = current_version + 1;
            let policy = insert_revision_mysql(
                &mut tx,
                level,
                single_file_max_bytes,
                total_bytes,
                daily_upload_bytes,
                retention_days,
                next_version,
                actor_id,
                now,
            )
            .await?;
            tx.commit().await?;
            Ok(policy)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn update_level_quota_locked(
    conn: &mut sqlx::SqliteConnection,
    level: i64,
    single_file_max_bytes: i64,
    total_bytes: i64,
    daily_upload_bytes: i64,
    retention_days: i64,
    expected_version: i64,
    actor_id: &str,
    now: i64,
) -> Result<QuotaPolicy, StorageError> {
    #[allow(clippy::explicit_auto_deref)]
    let current: Option<PolicyRevisionRow> = sqlx::query_as::<_, PolicyRevisionRow>(
        "SELECT level, single_file_max_bytes, total_bytes, daily_upload_bytes,
                retention_days, policy_version
         FROM quota_policy_revisions
         WHERE level = ? ORDER BY policy_version DESC LIMIT 1",
    )
    .bind(level)
    .fetch_optional(&mut *conn)
    .await?;
    let current_version = current.as_ref().map(|r| r.policy_version).unwrap_or(0);
    if current_version != expected_version {
        return Err(StorageError::Conflict(format!(
            "quota policy version mismatch: expected {expected_version}, current {current_version}"
        )));
    }
    let next_version = current_version + 1;
    let policy = QuotaPolicy {
        level,
        single_file_max_bytes,
        total_bytes,
        daily_upload_bytes,
        retention_days,
        policy_version: next_version,
    };
    #[allow(clippy::explicit_auto_deref)]
    sqlx::query(
        "INSERT INTO quota_policy_revisions
            (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes,
             retention_days, policy_version, created_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(level)
    .bind(single_file_max_bytes)
    .bind(total_bytes)
    .bind(daily_upload_bytes)
    .bind(retention_days)
    .bind(next_version)
    .bind(actor_id)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(policy)
}

/// 校验配额策略值（M06-QUOTA-02：单文件上限 ≤ 总容量 ≤ 站点硬上限）。
pub fn validate_policy_values(
    single_file_max_bytes: i64,
    total_bytes: i64,
    daily_upload_bytes: i64,
    retention_days: i64,
) -> Result<(), StorageError> {
    if single_file_max_bytes <= 0 || total_bytes <= 0 || daily_upload_bytes <= 0 {
        return Err(StorageError::Invalid(
            "quota policy values must be positive".to_string(),
        ));
    }
    if single_file_max_bytes > total_bytes {
        return Err(StorageError::Invalid(
            "single_file_max_bytes must not exceed total_bytes".to_string(),
        ));
    }
    if total_bytes > SITE_TOTAL_HARD_LIMIT_BYTES {
        return Err(StorageError::Invalid(
            "total_bytes exceeds site hard limit".to_string(),
        ));
    }
    if daily_upload_bytes > total_bytes {
        return Err(StorageError::Invalid(
            "daily_upload_bytes must not exceed total_bytes".to_string(),
        ));
    }
    if !(0..=3650).contains(&retention_days) {
        return Err(StorageError::Invalid(
            "retention_days must be in 0..=3650".to_string(),
        ));
    }
    Ok(())
}

// ────────────────────────── 用户配额计数 ───────────────────────────────────

#[derive(sqlx::FromRow)]
struct CounterRow {
    bytes_reserved: i64,
    bytes_charged: i64,
    bytes_released: i64,
}

/// 读取用户配额计数（无行时返回全零，不创建行）。
pub async fn get_counters(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<QuotaCounters, StorageError> {
    let row: Option<CounterRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, CounterRow>(
                "SELECT bytes_reserved, bytes_charged, bytes_released
                 FROM user_quota_counters WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, CounterRow>(
                "SELECT bytes_reserved, bytes_charged, bytes_released
                 FROM user_quota_counters WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row.map(QuotaCounters::from).unwrap_or_default())
}

/// create 阶段预留容量（M06-QUOTA-03/05）：原子校验并增加 reserved。
///
/// 校验（在同一把锁内）：`reserved + charged + size <= total_bytes` 与
/// `daily_upload（滚动 24h）+ size <= daily_upload_bytes`；任一超限返回
/// [`StorageError::Quota`]，不修改任何计数。
pub async fn reserve_bytes(
    pool: &DatabasePool,
    user_id: &str,
    size: i64,
    policy: &QuotaPolicy,
    now: i64,
) -> Result<(), StorageError> {
    if size <= 0 {
        return Err(StorageError::Invalid(
            "attachment size must be positive".to_string(),
        ));
    }
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            #[allow(clippy::explicit_auto_deref)]
            let result = reserve_bytes_locked(&mut *conn, user_id, size, policy, now).await;
            match result {
                Ok(()) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(())
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            // 行锁：确保并发上传/替换不会超卖（M06-QUOTA-05）
            #[allow(clippy::explicit_auto_deref)]
            sqlx::query(
                "INSERT IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
                 VALUES (?, 0, 0, 0, ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            #[allow(clippy::explicit_auto_deref)]
            let row: CounterRow = sqlx::query_as::<_, CounterRow>(
                "SELECT bytes_reserved, bytes_charged, bytes_released
                 FROM user_quota_counters WHERE user_id = ? FOR UPDATE",
            )
            .bind(user_id)
            .fetch_one(&mut *tx)
            .await?;
            let counters = QuotaCounters::from(row);
            #[allow(clippy::explicit_auto_deref)]
            let daily = daily_upload_locked_mysql(&mut *tx, user_id, now).await?;
            check_quota_available(&counters, daily, size, policy, user_id)?;
            sqlx::query(
                "UPDATE user_quota_counters SET bytes_reserved = bytes_reserved + ?, updated_at = ? WHERE user_id = ?",
            )
            .bind(size)
            .bind(now)
            .bind(user_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(())
        }
    }
}

async fn reserve_bytes_locked(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    size: i64,
    policy: &QuotaPolicy,
    now: i64,
) -> Result<(), StorageError> {
    #[allow(clippy::explicit_auto_deref)]
    sqlx::query(
        "INSERT OR IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
         VALUES (?, 0, 0, 0, ?)",
    )
    .bind(user_id)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    #[allow(clippy::explicit_auto_deref)]
    let row: CounterRow = sqlx::query_as::<_, CounterRow>(
        "SELECT bytes_reserved, bytes_charged, bytes_released
         FROM user_quota_counters WHERE user_id = ?",
    )
    .bind(user_id)
    .fetch_one(&mut *conn)
    .await?;
    let counters = QuotaCounters::from(row);
    let daily = daily_upload_locked(&mut *conn, user_id, now).await?;
    check_quota_available(&counters, daily, size, policy, user_id)?;
    #[allow(clippy::explicit_auto_deref)]
    sqlx::query(
        "UPDATE user_quota_counters SET bytes_reserved = bytes_reserved + ?, updated_at = ? WHERE user_id = ?",
    )
    .bind(size)
    .bind(now)
    .bind(user_id)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// 总容量与每日上传量的纯校验（在计数器锁内调用）。
fn check_quota_available(
    counters: &QuotaCounters,
    daily: i64,
    size: i64,
    policy: &QuotaPolicy,
    user_id: &str,
) -> Result<(), StorageError> {
    let committed_after = counters.bytes_charged + counters.bytes_reserved + size;
    if committed_after > policy.total_bytes {
        return Err(StorageError::Quota(format!(
            "total quota exceeded for user {user_id}: committed {committed_after} > limit {}",
            policy.total_bytes
        )));
    }
    let daily_after = daily + size;
    if daily_after > policy.daily_upload_bytes {
        return Err(StorageError::Quota(format!(
            "daily upload limit exceeded for user {user_id}: {daily_after} > {}",
            policy.daily_upload_bytes
        )));
    }
    Ok(())
}

/// 用户滚动 24h 内已上传字节（锁内/锁外通用）。
/// 锁内每日上传量查询（SQLite executor；具体连接类型避免
/// `Executor` 高阶生命周期导致的 Send 推断失败）。
async fn daily_upload_locked(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    now: i64,
) -> Result<i64, StorageError> {
    let since = now - DAILY_WINDOW_MS;
    let sum: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(size_bytes) FROM attachments
         WHERE owner_id = ? AND created_at >= ? AND status != 'deleted'",
    )
    .bind(user_id)
    .bind(since)
    .fetch_one(conn)
    .await?;
    Ok(sum.unwrap_or(0))
}

/// 锁内每日上传量查询（MySQL executor）。
async fn daily_upload_locked_mysql(
    conn: &mut sqlx::MySqlConnection,
    user_id: &str,
    now: i64,
) -> Result<i64, StorageError> {
    let since = now - DAILY_WINDOW_MS;
    let sum: Option<i64> = sqlx::query_scalar(
        "SELECT SUM(size_bytes) FROM attachments
         WHERE owner_id = ? AND created_at >= ? AND status != 'deleted'",
    )
    .bind(user_id)
    .bind(since)
    .fetch_one(conn)
    .await?;
    Ok(sum.unwrap_or(0))
}

/// 用户今日（滚动 24h）已上传字节（公开查询；供管理/投影展示）。
pub async fn daily_upload_bytes(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<i64, StorageError> {
    let since = now - DAILY_WINDOW_MS;
    let sum: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT SUM(size_bytes) FROM attachments
                 WHERE owner_id = ? AND created_at >= ? AND status != 'deleted'",
            )
            .bind(user_id)
            .bind(since)
            .fetch_one(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT SUM(size_bytes) FROM attachments
                 WHERE owner_id = ? AND created_at >= ? AND status != 'deleted'",
            )
            .bind(user_id)
            .bind(since)
            .fetch_one(p)
            .await?
        }
    };
    Ok(sum.unwrap_or(0))
}

/// 计数器行更新类型（配合 [`apply_counter_update`] 统一三库两分支）。
#[derive(Clone, Copy)]
enum CounterUpdate {
    /// complete 成功：`charged += charged_size`、`released += reserved`、
    /// `reserved -= reserved`。
    Charge { charged_size: i64, reserved: i64 },
    /// 释放预留：`released += amount`、`reserved -= amount`。
    ReleaseReserved { amount: i64 },
    /// 物理删除：`released += amount`、`charged -= amount`。
    ReleaseCharged { amount: i64 },
}

/// 在计数器锁内应用一次更新（SQLite `BEGIN IMMEDIATE` / MySQL 事务）。
async fn apply_counter_update(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
    update: CounterUpdate,
) -> Result<(), StorageError> {
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            #[allow(clippy::explicit_auto_deref)]
            let result = counter_update_sqlite(&mut *conn, user_id, now, update).await;
            match result {
                Ok(()) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(())
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            #[allow(clippy::explicit_auto_deref)]
            sqlx::query(
                "INSERT IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
                 VALUES (?, 0, 0, 0, ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            let (sql, args) = counter_update_sql(update, now);
            let mut q = sqlx::query(sql);
            for a in args {
                q = q.bind(a);
            }
            #[allow(clippy::explicit_auto_deref)]
            q.bind(user_id).execute(&mut *tx).await?;
            tx.commit().await?;
            Ok(())
        }
    }
}

async fn counter_update_sqlite(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    now: i64,
    update: CounterUpdate,
) -> Result<(), StorageError> {
    #[allow(clippy::explicit_auto_deref)]
    sqlx::query(
        "INSERT OR IGNORE INTO user_quota_counters (user_id, bytes_reserved, bytes_charged, bytes_released, updated_at)
         VALUES (?, 0, 0, 0, ?)",
    )
    .bind(user_id)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    let (sql, args) = counter_update_sql(update, now);
    let mut q = sqlx::query(sql);
    for a in args {
        q = q.bind(a);
    }
    #[allow(clippy::explicit_auto_deref)]
    q.bind(user_id).execute(&mut *conn).await?;
    Ok(())
}

/// 构造计数更新 SQL 的参数（`?` 占位与 CASE 语法三库通用；
/// user_id 由调用方最后绑定）。
fn counter_update_sql(update: CounterUpdate, now: i64) -> (&'static str, Vec<i64>) {
    match update {
        CounterUpdate::Charge {
            charged_size,
            reserved,
        } => (
            "UPDATE user_quota_counters
             SET bytes_charged = bytes_charged + ?,
                 bytes_released = bytes_released + ?,
                 bytes_reserved = CASE WHEN bytes_reserved < ? THEN 0 ELSE bytes_reserved - ? END,
                 updated_at = ?
             WHERE user_id = ?",
            vec![charged_size, reserved, reserved, reserved, now],
        ),
        CounterUpdate::ReleaseReserved { amount } => (
            "UPDATE user_quota_counters
             SET bytes_released = bytes_released + ?,
                 bytes_reserved = CASE WHEN bytes_reserved < ? THEN 0 ELSE bytes_reserved - ? END,
                 updated_at = ?
             WHERE user_id = ?",
            vec![amount, amount, amount, now],
        ),
        CounterUpdate::ReleaseCharged { amount } => (
            "UPDATE user_quota_counters
             SET bytes_released = bytes_released + ?,
                 bytes_charged = CASE WHEN bytes_charged < ? THEN 0 ELSE bytes_charged - ? END,
                 updated_at = ?
             WHERE user_id = ?",
            vec![amount, amount, amount, now],
        ),
    }
}

/// complete 成功：`charged += size`、`released += reserved`、`reserved -= reserved`
/// （M06-QUOTA-04 净效果 0）。`reserved_amount` 为 create 阶段预留的字节数。
pub async fn charge_reserved(
    pool: &DatabasePool,
    user_id: &str,
    reserved_amount: i64,
    charged_size: i64,
    now: i64,
) -> Result<(), StorageError> {
    if reserved_amount < 0 || charged_size < 0 {
        return Err(StorageError::Invalid(
            "reserved_amount and charged_size must be non-negative".to_string(),
        ));
    }
    apply_counter_update(
        pool,
        user_id,
        now,
        CounterUpdate::Charge {
            charged_size,
            reserved: reserved_amount,
        },
    )
    .await
}

/// quarantined / 中止上传：释放预留字节（`released += amount`、`reserved -= amount`，
/// 负数释放用 CASE 钳制为 0，M06-QUOTA-05）。
pub async fn release_reserved(
    pool: &DatabasePool,
    user_id: &str,
    amount: i64,
    now: i64,
) -> Result<(), StorageError> {
    if amount < 0 {
        return Err(StorageError::Invalid(
            "release amount must be non-negative".to_string(),
        ));
    }
    apply_counter_update(
        pool,
        user_id,
        now,
        CounterUpdate::ReleaseReserved { amount },
    )
    .await
}

/// 物理删除成功：`released += charged`、`charged -= charged`（M06-QUOTA-09）。
pub async fn release_charged(
    pool: &DatabasePool,
    user_id: &str,
    amount: i64,
    now: i64,
) -> Result<(), StorageError> {
    if amount < 0 {
        return Err(StorageError::Invalid(
            "release amount must be non-negative".to_string(),
        ));
    }
    apply_counter_update(pool, user_id, now, CounterUpdate::ReleaseCharged { amount }).await
}

// ────────────────────────── 引用与引用完整性 ───────────────────────────────

/// 记录附件引用（attachment_links）并递增 `attachments.ref_count`
/// （M06-QUOTA-06/07；Cover/头像/封面/正文图/普通附件统一走 quota）。
pub async fn link_attachment(
    pool: &DatabasePool,
    attachment_id: &str,
    target_type: &str,
    target_id: &str,
    purpose: &str,
    now: i64,
) -> Result<(), StorageError> {
    if target_type.is_empty() || target_id.is_empty() || purpose.is_empty() {
        return Err(StorageError::Invalid(
            "target_type/target_id/purpose must be non-empty".to_string(),
        ));
    }
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin().await?),
        Either::Right(p) => Either::Right(p.begin().await?),
    };
    let exists: Option<i64> = match &mut tx {
        Either::Left(t) => {
            sqlx::query_scalar("SELECT 1 FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_optional(&mut **t)
                .await?
        }
        Either::Right(t) => {
            sqlx::query_scalar("SELECT 1 FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_optional(&mut **t)
                .await?
        }
    };
    if exists != Some(1) {
        return Err(StorageError::NotFound(format!(
            "attachment {attachment_id}"
        )));
    }
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO attachment_links (id, attachment_id, target_type, target_id, purpose, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(attachment_id)
            .bind(target_type)
            .bind(target_id)
            .bind(purpose)
            .bind(now)
            .execute(&mut **t)
            .await?;
            sqlx::query("UPDATE attachments SET ref_count = ref_count + 1 WHERE id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO attachment_links (id, attachment_id, target_type, target_id, purpose, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(attachment_id)
            .bind(target_type)
            .bind(target_id)
            .bind(purpose)
            .bind(now)
            .execute(&mut **t)
            .await?;
            sqlx::query("UPDATE attachments SET ref_count = ref_count + 1 WHERE id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
        }
    }
    match tx {
        Either::Left(t) => t.commit().await?,
        Either::Right(t) => t.commit().await?,
    }
    Ok(())
}

/// 解除附件引用（Cover 移除只解除引用，不删附件，M06-QUOTA-07）。
pub async fn unlink_attachment(
    pool: &DatabasePool,
    attachment_id: &str,
    target_type: &str,
    target_id: &str,
) -> Result<(), StorageError> {
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin().await?),
        Either::Right(p) => Either::Right(p.begin().await?),
    };
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "DELETE FROM attachment_links WHERE attachment_id = ? AND target_type = ? AND target_id = ?",
            )
            .bind(attachment_id)
            .bind(target_type)
            .bind(target_id)
            .execute(&mut **t)
            .await?;
            sqlx::query(
                "UPDATE attachments SET ref_count = CASE WHEN ref_count < 1 THEN 0 ELSE ref_count - 1 END WHERE id = ?",
            )
            .bind(attachment_id)
            .execute(&mut **t)
            .await?;
        }
        Either::Right(t) => {
            sqlx::query(
                "DELETE FROM attachment_links WHERE attachment_id = ? AND target_type = ? AND target_id = ?",
            )
            .bind(attachment_id)
            .bind(target_type)
            .bind(target_id)
            .execute(&mut **t)
            .await?;
            sqlx::query(
                "UPDATE attachments SET ref_count = CASE WHEN ref_count < 1 THEN 0 ELSE ref_count - 1 END WHERE id = ?",
            )
            .bind(attachment_id)
            .execute(&mut **t)
            .await?;
        }
    }
    match tx {
        Either::Left(t) => t.commit().await?,
        Either::Right(t) => t.commit().await?,
    }
    Ok(())
}

/// 解除附件的全部引用（软删除时调用；`ref_count` 归零后可进入保留期清理）。
pub async fn unlink_all_references(
    pool: &DatabasePool,
    attachment_id: &str,
) -> Result<(), StorageError> {
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin().await?),
        Either::Right(p) => Either::Right(p.begin().await?),
    };
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("DELETE FROM attachment_links WHERE attachment_id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
            sqlx::query("UPDATE attachments SET ref_count = 0 WHERE id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
        }
        Either::Right(t) => {
            sqlx::query("DELETE FROM attachment_links WHERE attachment_id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
            sqlx::query("UPDATE attachments SET ref_count = 0 WHERE id = ?")
                .bind(attachment_id)
                .execute(&mut **t)
                .await?;
        }
    }
    match tx {
        Either::Left(t) => t.commit().await?,
        Either::Right(t) => t.commit().await?,
    }
    Ok(())
}

/// Cover/头像/封面引用校验（M06-QUOTA-07）：只允许引用**本人**已 `ready`
/// 且安全处理通过的附件；未 ready 返回 [`StorageError::State`]
/// （`storage_state_error`，禁止关联公开内容）。
pub async fn verify_reference_candidate(
    pool: &DatabasePool,
    owner_id: &str,
    attachment_id: &str,
) -> Result<(), StorageError> {
    let row: Option<(String, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT owner_id, status FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT owner_id, status FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((row_owner, status)) = row else {
        return Err(StorageError::NotFound(format!(
            "attachment {attachment_id}"
        )));
    };
    if row_owner != owner_id {
        return Err(StorageError::Forbidden(
            "attachment belongs to another user".to_string(),
        ));
    }
    if AttachmentStatus::parse(&status) != Some(AttachmentStatus::Ready) {
        return Err(StorageError::State(
            "attachment is not ready for reference".to_string(),
        ));
    }
    Ok(())
}

// ────────────────────────── 保留期清理与孤儿回收 ──────────────────────────

/// 延迟清理结果摘要。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PurgeSummary {
    /// 已物理删除的附件数。
    pub purged: usize,
    /// 释放的 charged 字节。
    pub released_bytes: i64,
    /// 有引用被跳过的附件数。
    pub skipped_referenced: usize,
}

/// 30 天保留期清理（M06-QUOTA-09）：扫描 `status='deleted'` 且
/// `deleted_at + retention_days` 已到期的附件；**无引用**（ref_count==0 且
/// attachment_links 为空）且对象校验通过后才物理删除并释放容量。
///
/// `retention_days` 取附件所有者当前等级的生效策略；等级无策略时回退站点默认。
pub async fn purge_expired_deleted(
    pool: &DatabasePool,
    storage: &StorageService,
    now: i64,
) -> Result<PurgeSummary, StorageError> {
    let mut summary = PurgeSummary::default();
    let candidates: Vec<PurgeRow> =
        match pool {
            Either::Left(p) => sqlx::query_as::<_, PurgeRow>(
                "SELECT id, owner_id, storage_backend, storage_key, quota_bytes_charged, deleted_at
                 FROM attachments
                 WHERE status = 'deleted' AND deleted_at IS NOT NULL",
            )
            .fetch_all(p)
            .await?,
            Either::Right(p) => sqlx::query_as::<_, PurgeRow>(
                "SELECT id, owner_id, storage_backend, storage_key, quota_bytes_charged, deleted_at
                 FROM attachments
                 WHERE status = 'deleted' AND deleted_at IS NOT NULL",
            )
            .fetch_all(p)
            .await?,
        };
    for row in candidates {
        let retention_days = retention_days_for_owner(pool, &row.owner_id).await?;
        let purge_at = row
            .deleted_at
            .unwrap_or(0)
            .saturating_add(retention_days.saturating_mul(86_400_000));
        if purge_at > now {
            continue;
        }
        // 引用完整性：仍被引用则跳过（不释放容量，M06-QUOTA-09）
        if reference_count(pool, &row.id).await? != 0 {
            summary.skipped_referenced += 1;
            continue;
        }
        let backend = match crate::storage::model::StorageBackend::parse(&row.storage_backend) {
            Some(b) => b,
            None => continue,
        };
        let adapter = storage.adapter(backend)?;
        // 对象校验：head 确认存在（不存在视为容量已物理释放，直接结算）
        match adapter.head_object(&row.storage_key).await {
            Ok(head) if head.exists => {
                adapter.delete_object(&row.storage_key).await?;
            }
            Ok(_) => {}
            Err(_) => continue, // head 失败：跳过本次清理，等待重试
        }
        release_charged(pool, &row.owner_id, row.quota_bytes_charged, now).await?;
        delete_attachment_row(pool, &row.id).await?;
        summary.purged += 1;
        summary.released_bytes += row.quota_bytes_charged;
    }
    Ok(summary)
}

/// 孤儿 mark-and-sweep（M06-QUOTA-10）：列出对象存储中全部 key，与
/// `attachments.storage_key` 比对；**无对应行**且对象创建已超过宽限期
/// （由 key 内嵌 UUIDv7 时间戳判定，24h）才物理删除——在用文件与
/// 进行中上传（行先于对象存在）绝不误删。
pub async fn sweep_orphans(
    pool: &DatabasePool,
    storage: &StorageService,
    now: i64,
) -> Result<usize, StorageError> {
    let known: Vec<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT storage_key FROM attachments")
                .fetch_all(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT storage_key FROM attachments")
                .fetch_all(p)
                .await?
        }
    };
    let mut known_set: std::collections::HashSet<String> = known.into_iter().collect();

    let mut backends = vec![crate::storage::model::StorageBackend::Local];
    if storage.default_backend() == crate::storage::model::StorageBackend::S3 {
        backends.push(crate::storage::model::StorageBackend::S3);
    }
    let mut purged = 0;
    for backend in backends {
        let Ok(adapter) = storage.adapter(backend) else {
            continue;
        };
        let Ok(keys) = adapter.list_objects("u").await else {
            continue;
        };
        for key in keys {
            if known_set.contains(&key) {
                continue;
            }
            // UUIDv7 时间戳 → 对象年龄（宽限期保护；解析失败也跳过）
            let Some(created_ms) = key_created_millis(&key) else {
                continue;
            };
            if now.saturating_sub(created_ms) < ORPHAN_GRACE_MS {
                continue;
            }
            if adapter.delete_object(&key).await.is_ok() {
                purged += 1;
            }
            known_set.insert(key); // 幂等：同一对象只清理一次
        }
    }
    Ok(purged)
}

/// 从 object key `u/<owner>/<uuidv7>/<safe>` 中解析 UUIDv7 的创建毫秒。
fn key_created_millis(key: &str) -> Option<i64> {
    let mut parts = key.split('/');
    let _ = parts.next()?; // "u"
    let _ = parts.next()?; // owner
    let uuid_part = parts.next()?;
    let parsed = uuid::Uuid::parse_str(uuid_part).ok()?;
    let ts = parsed.get_timestamp()?;
    let (secs, _nanos) = ts.to_unix();
    Some((secs as i64).saturating_mul(1000))
}

#[derive(sqlx::FromRow)]
struct PurgeRow {
    id: String,
    owner_id: String,
    storage_backend: String,
    storage_key: String,
    quota_bytes_charged: i64,
    deleted_at: Option<i64>,
}

/// 附件当前引用数（`ref_count` 与 attachment_links 计数取非零较大者）。
async fn reference_count(pool: &DatabasePool, attachment_id: &str) -> Result<i64, StorageError> {
    let (cached, links): (i64, i64) = match pool {
        Either::Left(p) => {
            let cached: i64 = sqlx::query_scalar("SELECT ref_count FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_one(p)
                .await?;
            let links: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM attachment_links WHERE attachment_id = ?")
                    .bind(attachment_id)
                    .fetch_one(p)
                    .await?;
            (cached, links)
        }
        Either::Right(p) => {
            let cached: i64 = sqlx::query_scalar("SELECT ref_count FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .fetch_one(p)
                .await?;
            let links: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM attachment_links WHERE attachment_id = ?")
                    .bind(attachment_id)
                    .fetch_one(p)
                    .await?;
            (cached, links)
        }
    };
    Ok(if links > 0 { links.max(cached) } else { cached })
}

/// 附件所有者当前等级的保留期（天）；无策略回退默认 30 天。
async fn retention_days_for_owner(
    pool: &DatabasePool,
    owner_id: &str,
) -> Result<i64, StorageError> {
    let level: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
                .bind(owner_id)
                .fetch_one(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
                .bind(owner_id)
                .fetch_one(p)
                .await?
        }
    };
    Ok(latest_revision(pool, level)
        .await?
        .map(|p| p.retention_days)
        .unwrap_or(DEFAULT_RETENTION_DAYS))
}

/// 物理删除附件行（清理完成；对象已删除、容量已释放）。
async fn delete_attachment_row(
    pool: &DatabasePool,
    attachment_id: &str,
) -> Result<(), StorageError> {
    match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .execute(p)
                .await?;
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM attachments WHERE id = ?")
                .bind(attachment_id)
                .execute(p)
                .await?;
        }
    }
    Ok(())
}

// ────────────────────────── 内部 helpers ───────────────────────────────────

#[derive(sqlx::FromRow)]
struct PolicyRevisionRow {
    level: i64,
    single_file_max_bytes: i64,
    total_bytes: i64,
    daily_upload_bytes: i64,
    retention_days: i64,
    policy_version: i64,
}

impl From<PolicyRevisionRow> for QuotaPolicy {
    fn from(r: PolicyRevisionRow) -> Self {
        Self {
            level: r.level,
            single_file_max_bytes: r.single_file_max_bytes,
            total_bytes: r.total_bytes,
            daily_upload_bytes: r.daily_upload_bytes,
            retention_days: r.retention_days,
            policy_version: r.policy_version,
        }
    }
}

impl From<CounterRow> for QuotaCounters {
    fn from(r: CounterRow) -> Self {
        Self {
            bytes_reserved: r.bytes_reserved,
            bytes_charged: r.bytes_charged,
            bytes_released: r.bytes_released,
        }
    }
}

async fn latest_revision(
    pool: &DatabasePool,
    level: i64,
) -> Result<Option<QuotaPolicy>, StorageError> {
    let row: Option<PolicyRevisionRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, PolicyRevisionRow>(
                "SELECT level, single_file_max_bytes, total_bytes, daily_upload_bytes,
                        retention_days, policy_version
                 FROM quota_policy_revisions
                 WHERE level = ? ORDER BY policy_version DESC LIMIT 1",
            )
            .bind(level)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, PolicyRevisionRow>(
                "SELECT level, single_file_max_bytes, total_bytes, daily_upload_bytes,
                        retention_days, policy_version
                 FROM quota_policy_revisions
                 WHERE level = ? ORDER BY policy_version DESC LIMIT 1",
            )
            .bind(level)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row.map(QuotaPolicy::from))
}

#[allow(clippy::too_many_arguments)]
async fn insert_revision(
    pool: &DatabasePool,
    level: i64,
    single_file_max_bytes: i64,
    total_bytes: i64,
    daily_upload_bytes: i64,
    retention_days: i64,
    policy_version: i64,
    actor_id: &str,
    now: i64,
) -> Result<(), StorageError> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO quota_policy_revisions
                    (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes,
                     retention_days, policy_version, created_by, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(level)
            .bind(single_file_max_bytes)
            .bind(total_bytes)
            .bind(daily_upload_bytes)
            .bind(retention_days)
            .bind(policy_version)
            .bind(actor_id)
            .bind(now)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO quota_policy_revisions
                    (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes,
                     retention_days, policy_version, created_by, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(level)
            .bind(single_file_max_bytes)
            .bind(total_bytes)
            .bind(daily_upload_bytes)
            .bind(retention_days)
            .bind(policy_version)
            .bind(actor_id)
            .bind(now)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_revision_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    level: i64,
    single_file_max_bytes: i64,
    total_bytes: i64,
    daily_upload_bytes: i64,
    retention_days: i64,
    policy_version: i64,
    actor_id: &str,
    now: i64,
) -> Result<QuotaPolicy, StorageError> {
    sqlx::query(
        "INSERT INTO quota_policy_revisions
            (id, level, single_file_max_bytes, total_bytes, daily_upload_bytes,
             retention_days, policy_version, created_by, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(level)
    .bind(single_file_max_bytes)
    .bind(total_bytes)
    .bind(daily_upload_bytes)
    .bind(retention_days)
    .bind(policy_version)
    .bind(actor_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(QuotaPolicy {
        level,
        single_file_max_bytes,
        total_bytes,
        daily_upload_bytes,
        retention_days,
        policy_version,
    })
}
