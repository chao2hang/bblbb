//! M07-REACTIONS：互动 Reaction 服务（user_reactions 表）。
//!
//! - add/remove 基于 (user_id, target_type, target_id, reaction) 复合唯一键。
//! - reaction_pack 从权益 remaining_quantity 原子扣减（M07-SHOP-08）。
//! - 反应不改变可见性、审核、排序或现金价值（M07-SHOP-06）。
//! - 排除自赞、重复、批量刷（限流窗口，M07-LEVELS-07）。
//! - 通知偏好：给目标 owner 写 Outbox（若开启）。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::db::DatabasePool;
use crate::events::types::{REACTION_CREATED, REACTION_REMOVED};
use crate::outbox::now_millis;

/// 反应错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionError {
    Db(String),
    NotFound(String),
    Invalid(String),
    /// 目标不存在或无权访问。
    Forbidden(String),
    /// 不允许对自己目标反应。
    SelfReaction,
    /// 限流（重试时间窗）。
    RateLimited {
        retry_after_ms: i64,
    },
    /// 反应包余额不足。
    PackExhausted,
    /// 重复反应（已存在且非 toggle 语义）。
    AlreadyExists,
    /// 未找到反应（删除不存在）。
    NotFoundReaction,
}

impl From<sqlx::Error> for ReactionError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for ReactionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "reaction db error: {msg}"),
            Self::NotFound(msg) => write!(f, "reaction target not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid reaction: {msg}"),
            Self::Forbidden(msg) => write!(f, "reaction forbidden: {msg}"),
            Self::SelfReaction => write!(f, "self reaction not allowed"),
            Self::RateLimited { .. } => write!(f, "reaction rate limited"),
            Self::PackExhausted => write!(f, "reaction pack exhausted"),
            Self::AlreadyExists => write!(f, "reaction already exists"),
            Self::NotFoundReaction => write!(f, "reaction not found"),
        }
    }
}

impl std::error::Error for ReactionError {}

/// 可反应目标类型（封闭枚举）。
pub const TARGET_TYPES: &[&str] = &["post", "comment"];

/// 反应名白名单（当前只有 like；扩展时在此枚举）。
pub const REACTIONS: &[&str] = &["like"];

/// 校验目标类型与反应名。
pub fn validate_reaction(target_type: &str, reaction: &str) -> Result<(), ReactionError> {
    if !TARGET_TYPES.contains(&target_type) {
        return Err(ReactionError::Invalid(format!(
            "unsupported target_type {target_type}"
        )));
    }
    if !REACTIONS.contains(&reaction) || reaction.len() > 32 {
        return Err(ReactionError::Invalid(format!(
            "unsupported reaction {reaction}"
        )));
    }
    Ok(())
}

/// 目标 owner 查询（用于自赞排除与通知）。
async fn target_owner(
    conn: &mut sqlx::SqliteConnection,
    target_type: &str,
    target_id: &str,
) -> Result<Option<String>, ReactionError> {
    match target_type {
        "post" => {
            let owner: Option<String> =
                sqlx::query_scalar("SELECT author_id FROM posts WHERE id = ?")
                    .bind(target_id)
                    .fetch_optional(&mut *conn)
                    .await?;
            Ok(owner)
        }
        "comment" => {
            let owner: Option<String> =
                sqlx::query_scalar("SELECT author_id FROM comments WHERE id = ?")
                    .bind(target_id)
                    .fetch_optional(&mut *conn)
                    .await?;
            Ok(owner)
        }
        _ => Ok(None),
    }
}

async fn target_owner_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    target_type: &str,
    target_id: &str,
) -> Result<Option<String>, ReactionError> {
    match target_type {
        "post" => {
            let owner: Option<String> =
                sqlx::query_scalar("SELECT author_id FROM posts WHERE id = ?")
                    .bind(target_id)
                    .fetch_optional(&mut **tx)
                    .await?;
            Ok(owner)
        }
        "comment" => {
            let owner: Option<String> =
                sqlx::query_scalar("SELECT author_id FROM comments WHERE id = ?")
                    .bind(target_id)
                    .fetch_optional(&mut **tx)
                    .await?;
            Ok(owner)
        }
        _ => Ok(None),
    }
}

/// 检查用户是否启用"他人反应"通知。
async fn notify_pref_enabled(
    conn: &mut sqlx::SqliteConnection,
    owner_id: &str,
) -> Result<bool, ReactionError> {
    // 通知偏好：默认开；preferences 表 reaction_notifications=false 时关闭。
    let pref: Option<i64> =
        sqlx::query_scalar("SELECT reaction_notifications FROM user_preferences WHERE user_id = ?")
            .bind(owner_id)
            .fetch_optional(&mut *conn)
            .await?;
    Ok(pref.map(|v| v != 0).unwrap_or(true))
}

async fn notify_pref_enabled_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    owner_id: &str,
) -> Result<bool, ReactionError> {
    let pref: Option<i64> =
        sqlx::query_scalar("SELECT reaction_notifications FROM user_preferences WHERE user_id = ?")
            .bind(owner_id)
            .fetch_optional(&mut **tx)
            .await?;
    Ok(pref.map(|v| v != 0).unwrap_or(true))
}

/// 当前用户最近 60 秒内对同一目标类型的反应次数（限流判断）。
async fn recent_reaction_count(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    target_type: &str,
    now: i64,
) -> Result<i64, ReactionError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_reactions WHERE user_id = ? AND target_type = ? AND created_at >= ?",
    )
    .bind(user_id)
    .bind(target_type)
    .bind(now - 60_000)
    .fetch_one(&mut *conn)
    .await?;
    Ok(count)
}

async fn recent_reaction_count_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    target_type: &str,
    now: i64,
) -> Result<i64, ReactionError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_reactions WHERE user_id = ? AND target_type = ? AND created_at >= ?",
    )
    .bind(user_id)
    .bind(target_type)
    .bind(now - 60_000)
    .fetch_one(&mut **tx)
    .await?;
    Ok(count)
}

/// 添加反应（含 reaction_pack 消耗与自赞/限流排除）。
///
/// `require_pack`: 该反应类型需要 reaction_pack 权益时传 true（由路由层根据
/// 商品 kind=reaction_pack 决定）。调用方需提供 pool。
#[allow(clippy::explicit_auto_deref)]
pub async fn add_reaction(
    pool: &DatabasePool,
    user_id: &str,
    target_type: &str,
    target_id: &str,
    reaction: &str,
    require_pack: bool,
) -> Result<Value, ReactionError> {
    validate_reaction(target_type, reaction)?;
    let now = now_millis();

    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, ReactionError> = async {
                // 目标存在性 + 自赞排除。
                let owner = target_owner(&mut *conn, target_type, target_id).await?;
                let Some(owner) = owner else {
                    return Err(ReactionError::NotFound(format!("{target_type} {target_id}")));
                };
                if owner == user_id {
                    return Err(ReactionError::SelfReaction);
                }
                // 限流：同一目标类型 60 秒内最多 20 次。
                let recent = recent_reaction_count(&mut *conn, user_id, target_type, now).await?;
                if recent >= 20 {
                    return Err(ReactionError::RateLimited {
                        retry_after_ms: 60_000,
                    });
                }
                // 重复反应：复合唯一键已存在 → 冲突（toggle 由路由层先 remove）。
                let dup: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM user_reactions WHERE user_id = ? AND target_type = ? AND target_id = ? AND reaction = ?",
                )
                .bind(user_id)
                .bind(target_type)
                .bind(target_id)
                .bind(reaction)
                .fetch_one(&mut *conn)
                .await?;
                if dup > 0 {
                    return Err(ReactionError::AlreadyExists);
                }
                // reaction_pack 消耗：从最新 reaction_pack entitlement 扣减。
                // （SQLite 不支持 UPDATE...JOIN 与 UPDATE...ORDER BY...LIMIT，
                // 统一先选最新行再按 id 扣减，事务内安全）
                if require_pack {
                    let pack_id: Option<String> = sqlx::query_scalar(
                        "SELECT id FROM user_entitlements
                         WHERE user_id = ? AND status = 'owned' AND remaining_quantity > 0
                           AND product_id IN (SELECT id FROM shop_products WHERE kind = 'reaction_pack')
                         ORDER BY expires_at IS NOT NULL, expires_at ASC LIMIT 1",
                    )
                    .bind(user_id)
                    .fetch_optional(&mut *conn)
                    .await?;
                    if let Some(pack_id) = pack_id {
                        let consumed = sqlx::query(
                            "UPDATE user_entitlements
                             SET remaining_quantity = remaining_quantity - 1, updated_at = ?
                             WHERE id = ? AND remaining_quantity > 0",
                        )
                        .bind(now)
                        .bind(&pack_id)
                        .execute(&mut *conn)
                        .await?
                        .rows_affected();
                        if consumed != 1 {
                            return Err(ReactionError::PackExhausted);
                        }
                    } else {
                        return Err(ReactionError::PackExhausted);
                    }
                }
                sqlx::query(
                    "INSERT INTO user_reactions (user_id, target_type, target_id, reaction, created_at)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(user_id)
                .bind(target_type)
                .bind(target_id)
                .bind(reaction)
                .bind(now)
                .execute(&mut *conn)
                .await?;
                // 通知（Outbox 同事务）：owner 开启偏好才发。
                if notify_pref_enabled(&mut *conn, &owner).await? {
                    enqueue_sqlite(
                        &mut *conn,
                        REACTION_CREATED,
                        json!({
                            "target_type": target_type,
                            "target_id": target_id,
                            "actor_user_id": user_id,
                            "owner_user_id": owner,
                            "reaction": reaction,
                        }),
                    )
                    .await?;
                }
                let summary = reaction_summary_sqlite(&mut *conn, target_type, target_id).await?;
                Ok(summary)
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
            let outcome: Result<Value, ReactionError> = async {
                let owner = target_owner_mysql(&mut tx, target_type, target_id).await?;
                let Some(owner) = owner else {
                    return Err(ReactionError::NotFound(format!("{target_type} {target_id}")));
                };
                if owner == user_id {
                    return Err(ReactionError::SelfReaction);
                }
                let recent = recent_reaction_count_mysql(&mut tx, user_id, target_type, now).await?;
                if recent >= 20 {
                    return Err(ReactionError::RateLimited {
                        retry_after_ms: 60_000,
                    });
                }
                let dup: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM user_reactions WHERE user_id = ? AND target_type = ? AND target_id = ? AND reaction = ?",
                )
                .bind(user_id)
                .bind(target_type)
                .bind(target_id)
                .bind(reaction)
                .fetch_one(&mut *tx)
                .await?;
                if dup > 0 {
                    return Err(ReactionError::AlreadyExists);
                }
                if require_pack {
                    let pack_id: Option<String> = sqlx::query_scalar(
                        "SELECT id FROM user_entitlements
                         WHERE user_id = ? AND status = 'owned' AND remaining_quantity > 0
                           AND product_id IN (SELECT id FROM shop_products WHERE kind = 'reaction_pack')
                         ORDER BY expires_at IS NOT NULL, expires_at ASC LIMIT 1",
                    )
                    .bind(user_id)
                    .fetch_optional(&mut *tx)
                    .await?;
                    if let Some(pack_id) = pack_id {
                        let consumed = sqlx::query(
                            "UPDATE user_entitlements
                             SET remaining_quantity = remaining_quantity - 1, updated_at = ?
                             WHERE id = ? AND remaining_quantity > 0",
                        )
                        .bind(now)
                        .bind(&pack_id)
                        .execute(&mut *tx)
                        .await?
                        .rows_affected();
                        if consumed != 1 {
                            return Err(ReactionError::PackExhausted);
                        }
                    } else {
                        return Err(ReactionError::PackExhausted);
                    }
                }
                sqlx::query(
                    "INSERT INTO user_reactions (user_id, target_type, target_id, reaction, created_at)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(user_id)
                .bind(target_type)
                .bind(target_id)
                .bind(reaction)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                if notify_pref_enabled_mysql(&mut tx, &owner).await? {
                    enqueue_mysql(
                        &mut tx,
                        REACTION_CREATED,
                        json!({
                            "target_type": target_type,
                            "target_id": target_id,
                            "actor_user_id": user_id,
                            "owner_user_id": owner,
                            "reaction": reaction,
                        }),
                    )
                    .await?;
                }
                let summary = reaction_summary_mysql(&mut *tx, target_type, target_id).await?;
                Ok(summary)
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

/// 移除反应。
#[allow(clippy::explicit_auto_deref)]
pub async fn remove_reaction(
    pool: &DatabasePool,
    user_id: &str,
    target_type: &str,
    target_id: &str,
    reaction: &str,
) -> Result<Value, ReactionError> {
    validate_reaction(target_type, reaction)?;
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, ReactionError> = async {
                let affected = sqlx::query(
                    "DELETE FROM user_reactions WHERE user_id = ? AND target_type = ? AND target_id = ? AND reaction = ?",
                )
                .bind(user_id)
                .bind(target_type)
                .bind(target_id)
                .bind(reaction)
                .execute(&mut *conn)
                .await?
                .rows_affected();
                if affected != 1 {
                    return Err(ReactionError::NotFoundReaction);
                }
                enqueue_sqlite(
                    &mut *conn,
                    REACTION_REMOVED,
                    json!({
                        "target_type": target_type,
                        "target_id": target_id,
                        "user_id": user_id,
                        "reaction": reaction,
                    }),
                )
                .await?;
                let summary = reaction_summary_sqlite(&mut *conn, target_type, target_id).await?;
                Ok(summary)
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
            let outcome: Result<Value, ReactionError> = async {
                let affected = sqlx::query(
                    "DELETE FROM user_reactions WHERE user_id = ? AND target_type = ? AND target_id = ? AND reaction = ?",
                )
                .bind(user_id)
                .bind(target_type)
                .bind(target_id)
                .bind(reaction)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if affected != 1 {
                    return Err(ReactionError::NotFoundReaction);
                }
                enqueue_mysql(
                    &mut tx,
                    REACTION_REMOVED,
                    json!({
                        "target_type": target_type,
                        "target_id": target_id,
                        "user_id": user_id,
                        "reaction": reaction,
                    }),
                )
                .await?;
                let summary = reaction_summary_mysql(&mut *tx, target_type, target_id).await?;
                Ok(summary)
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

/// 反应汇总（按 reaction 统计，供详情页展示；不含用户私密信息）。
pub async fn summarize(
    pool: &DatabasePool,
    target_type: &str,
    target_id: &str,
) -> Result<Value, ReactionError> {
    validate_reaction(target_type, "like")
        .map_err(|_| ReactionError::Invalid("target_type".into()))?;
    match pool {
        Either::Left(p) => {
            let summary =
                reaction_summary_sqlite(&mut *p.acquire().await?, target_type, target_id).await?;
            Ok(summary)
        }
        Either::Right(p) => {
            let summary =
                reaction_summary_mysql(&mut *p.acquire().await?, target_type, target_id).await?;
            Ok(summary)
        }
    }
}

async fn reaction_summary_sqlite(
    conn: &mut sqlx::SqliteConnection,
    target_type: &str,
    target_id: &str,
) -> Result<Value, ReactionError> {
    let rows = sqlx::query(
        "SELECT reaction, COUNT(*) AS count FROM user_reactions \
         WHERE target_type = ? AND target_id = ? GROUP BY reaction",
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut counts = serde_json::Map::new();
    let mut total: i64 = 0;
    for row in &rows {
        let reaction: String = row.get("reaction");
        let count: i64 = row.get("count");
        counts.insert(reaction, json!(count));
        total += count;
    }
    Ok(json!({
        "target_type": target_type,
        "target_id": target_id,
        "total": total,
        "counts": Value::Object(counts),
    }))
}

async fn reaction_summary_mysql(
    conn: &mut sqlx::MySqlConnection,
    target_type: &str,
    target_id: &str,
) -> Result<Value, ReactionError> {
    let rows = sqlx::query(
        "SELECT reaction, COUNT(*) AS count FROM user_reactions \
         WHERE target_type = ? AND target_id = ? GROUP BY reaction",
    )
    .bind(target_type)
    .bind(target_id)
    .fetch_all(&mut *conn)
    .await?;
    let mut counts = serde_json::Map::new();
    let mut total: i64 = 0;
    for row in &rows {
        let reaction: String = row.get("reaction");
        let count: i64 = row.get("count");
        counts.insert(reaction, json!(count));
        total += count;
    }
    Ok(json!({
        "target_type": target_type,
        "target_id": target_id,
        "total": total,
        "counts": Value::Object(counts),
    }))
}

async fn enqueue_sqlite(
    conn: &mut sqlx::SqliteConnection,
    event_type: &str,
    payload: Value,
) -> Result<String, ReactionError> {
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
    Ok(id)
}

async fn enqueue_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    event_type: &str,
    payload: Value,
) -> Result<String, ReactionError> {
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
    Ok(id)
}
