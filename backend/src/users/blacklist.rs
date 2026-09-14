//! 昵称黑名单与随机昵称服务模块。
//!
//! 提供：
//! - 昵称黑名单写入、删除、查询与校验；
//! - 规范化随机昵称生成（格式如 `用户_a1b2c3d4`）；
//! - 管理员一键随机用户昵称：原昵称自动存入黑名单，并更新目标用户昵称。

use serde::{Deserialize, Serialize};
use sqlx::{Either, Row};

use crate::db::DatabasePool;

/// 昵称黑名单条目。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlacklistEntry {
    pub id: String,
    pub nickname: String,
    pub nickname_normalized: String,
    pub reason: Option<String>,
    pub created_by: Option<String>,
    pub created_at: i64,
}

/// 随机昵称执行结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomizeNicknameOutcome {
    pub user_id: String,
    pub old_nickname: String,
    pub new_nickname: String,
    pub new_version: i64,
}

/// 随机昵称错误。
#[derive(Debug)]
pub enum RandomizeNicknameError {
    NotFound,
    Database(String),
}

impl std::fmt::Display for RandomizeNicknameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "user not found"),
            Self::Database(msg) => write!(f, "database error: {msg}"),
        }
    }
}

impl std::error::Error for RandomizeNicknameError {}

/// 归一化昵称：trim 并在大小写不敏感比对时转为小写。
pub fn normalize_nickname(name: &str) -> String {
    name.trim().to_lowercase()
}

/// 检查昵称是否处于黑名单中。
pub async fn is_nickname_blacklisted(
    pool: &DatabasePool,
    nickname: &str,
) -> Result<bool, sqlx::Error> {
    let normalized = normalize_nickname(nickname);
    if normalized.is_empty() {
        return Ok(false);
    }
    let sql = "SELECT 1 FROM nickname_blacklist WHERE nickname_normalized = ?";
    let exists: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(sql)
                .bind(&normalized)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(sql)
                .bind(&normalized)
                .fetch_optional(p)
                .await?
        }
    };
    Ok(exists.is_some())
}

/// 添加昵称到黑名单（若已存在则忽略，幂等操作）。返回是否新插入。
pub async fn add_to_nickname_blacklist(
    pool: &DatabasePool,
    nickname: &str,
    reason: Option<&str>,
    created_by: Option<&str>,
    now: i64,
) -> Result<bool, sqlx::Error> {
    let trimmed = nickname.trim();
    if trimmed.is_empty() {
        return Ok(false);
    }
    let normalized = normalize_nickname(trimmed);
    let id = uuid::Uuid::now_v7().to_string();

    let sql = match pool {
        Either::Left(_) => {
            "INSERT OR IGNORE INTO nickname_blacklist (id, nickname, nickname_normalized, reason, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        }
        Either::Right(_) => {
            "INSERT IGNORE INTO nickname_blacklist (id, nickname, nickname_normalized, reason, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, ?)"
        }
    };

    let affected = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&id)
            .bind(trimmed)
            .bind(&normalized)
            .bind(reason)
            .bind(created_by)
            .bind(now)
            .execute(p)
            .await?
            .rows_affected(),
        Either::Right(p) => sqlx::query(sql)
            .bind(&id)
            .bind(trimmed)
            .bind(&normalized)
            .bind(reason)
            .bind(created_by)
            .bind(now)
            .execute(p)
            .await?
            .rows_affected(),
    };

    Ok(affected > 0)
}

/// 从黑名单移除记录。
pub async fn remove_from_nickname_blacklist(
    pool: &DatabasePool,
    id: &str,
) -> Result<bool, sqlx::Error> {
    let sql = "DELETE FROM nickname_blacklist WHERE id = ?";
    let affected = match pool {
        Either::Left(p) => sqlx::query(sql).bind(id).execute(p).await?.rows_affected(),
        Either::Right(p) => sqlx::query(sql).bind(id).execute(p).await?.rows_affected(),
    };
    Ok(affected > 0)
}

/// 查询昵称黑名单列表与总数。
pub async fn list_nickname_blacklist(
    pool: &DatabasePool,
    limit: i64,
    offset: i64,
    q: Option<&str>,
) -> Result<(Vec<BlacklistEntry>, i64), sqlx::Error> {
    let limit = limit.clamp(1, 200);
    let offset = offset.max(0);

    let (items, total) = match (pool, q.map(str::trim).filter(|s| !s.is_empty())) {
        (Either::Left(p), Some(query)) => {
            let pattern = format!("%{}%", query.to_lowercase());
            let count_sql =
                "SELECT COUNT(*) FROM nickname_blacklist WHERE nickname_normalized LIKE ?";
            let total: i64 = sqlx::query_scalar(count_sql)
                .bind(&pattern)
                .fetch_one(p)
                .await?;
            let list_sql =
                "SELECT id, nickname, nickname_normalized, reason, created_by, created_at
                            FROM nickname_blacklist
                            WHERE nickname_normalized LIKE ?
                            ORDER BY created_at DESC
                            LIMIT ? OFFSET ?";
            let rows = sqlx::query(list_sql)
                .bind(&pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await?;
            let entries = rows
                .into_iter()
                .map(|r| BlacklistEntry {
                    id: r.get("id"),
                    nickname: r.get("nickname"),
                    nickname_normalized: r.get("nickname_normalized"),
                    reason: r.get("reason"),
                    created_by: r.get("created_by"),
                    created_at: r.get("created_at"),
                })
                .collect();
            (entries, total)
        }
        (Either::Left(p), None) => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nickname_blacklist")
                .fetch_one(p)
                .await?;
            let list_sql =
                "SELECT id, nickname, nickname_normalized, reason, created_by, created_at
                            FROM nickname_blacklist
                            ORDER BY created_at DESC
                            LIMIT ? OFFSET ?";
            let rows = sqlx::query(list_sql)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await?;
            let entries = rows
                .into_iter()
                .map(|r| BlacklistEntry {
                    id: r.get("id"),
                    nickname: r.get("nickname"),
                    nickname_normalized: r.get("nickname_normalized"),
                    reason: r.get("reason"),
                    created_by: r.get("created_by"),
                    created_at: r.get("created_at"),
                })
                .collect();
            (entries, total)
        }
        (Either::Right(p), Some(query)) => {
            let pattern = format!("%{}%", query.to_lowercase());
            let count_sql =
                "SELECT COUNT(*) FROM nickname_blacklist WHERE nickname_normalized LIKE ?";
            let total: i64 = sqlx::query_scalar(count_sql)
                .bind(&pattern)
                .fetch_one(p)
                .await?;
            let list_sql =
                "SELECT id, nickname, nickname_normalized, reason, created_by, created_at
                            FROM nickname_blacklist
                            WHERE nickname_normalized LIKE ?
                            ORDER BY created_at DESC
                            LIMIT ? OFFSET ?";
            let rows = sqlx::query(list_sql)
                .bind(&pattern)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await?;
            let entries = rows
                .into_iter()
                .map(|r| BlacklistEntry {
                    id: r.get("id"),
                    nickname: r.get("nickname"),
                    nickname_normalized: r.get("nickname_normalized"),
                    reason: r.get("reason"),
                    created_by: r.get("created_by"),
                    created_at: r.get("created_at"),
                })
                .collect();
            (entries, total)
        }
        (Either::Right(p), None) => {
            let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM nickname_blacklist")
                .fetch_one(p)
                .await?;
            let list_sql =
                "SELECT id, nickname, nickname_normalized, reason, created_by, created_at
                            FROM nickname_blacklist
                            ORDER BY created_at DESC
                            LIMIT ? OFFSET ?";
            let rows = sqlx::query(list_sql)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await?;
            let entries = rows
                .into_iter()
                .map(|r| BlacklistEntry {
                    id: r.get("id"),
                    nickname: r.get("nickname"),
                    nickname_normalized: r.get("nickname_normalized"),
                    reason: r.get("reason"),
                    created_by: r.get("created_by"),
                    created_at: r.get("created_at"),
                })
                .collect();
            (entries, total)
        }
    };

    Ok((items, total))
}

/// 生成合规且不在黑名单中的随机昵称。
pub async fn generate_random_nickname(pool: &DatabasePool) -> Result<String, sqlx::Error> {
    for _ in 0..10 {
        let rand_val: u32 = rand::random();
        let candidate = format!("用户_{:08x}", rand_val);
        if !is_nickname_blacklisted(pool, &candidate).await? {
            return Ok(candidate);
        }
    }
    // 极端情况下追加纳秒防重
    let candidate = format!("用户_{:x}", crate::outbox::now_millis());
    Ok(candidate)
}

/// 管理员一键随机重置用户昵称：
/// 1. 获取用户当前昵称（display_name 缺省时回退 username）；
/// 2. 将原昵称存入黑名单；
/// 3. 生成全新合规随机昵称并更新用户表（version+1）。
pub async fn randomize_user_nickname(
    pool: &DatabasePool,
    user_id: &str,
    admin_id: &str,
    reason: &str,
    now: i64,
) -> Result<RandomizeNicknameOutcome, RandomizeNicknameError> {
    // 读取当前用户
    let row: Option<(String, Option<String>, i64)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT username_normalized, display_name, version FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(user_id)
                .fetch_optional(p)
                .await
                .map_err(|e| RandomizeNicknameError::Database(e.to_string()))?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT username_normalized, display_name, version FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(user_id)
                .fetch_optional(p)
                .await
                .map_err(|e| RandomizeNicknameError::Database(e.to_string()))?
        }
    };

    let (username_normalized, display_name, current_version) = match row {
        Some(data) => data,
        None => return Err(RandomizeNicknameError::NotFound),
    };

    let old_nickname = display_name.unwrap_or(username_normalized);

    // 1. 将原昵称加入黑名单
    add_to_nickname_blacklist(pool, &old_nickname, Some(reason), Some(admin_id), now)
        .await
        .map_err(|e| RandomizeNicknameError::Database(e.to_string()))?;

    // 2. 生成新随机昵称
    let new_nickname = generate_random_nickname(pool)
        .await
        .map_err(|e| RandomizeNicknameError::Database(e.to_string()))?;

    let new_version = current_version + 1;

    // 3. 更新用户表
    let update_sql = "UPDATE users SET display_name = ?, version = ?, updated_at = ? WHERE id = ?";
    let affected = match pool {
        Either::Left(p) => sqlx::query(update_sql)
            .bind(&new_nickname)
            .bind(new_version)
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map_err(|e| RandomizeNicknameError::Database(e.to_string()))?
            .rows_affected(),
        Either::Right(p) => sqlx::query(update_sql)
            .bind(&new_nickname)
            .bind(new_version)
            .bind(now)
            .bind(user_id)
            .execute(p)
            .await
            .map_err(|e| RandomizeNicknameError::Database(e.to_string()))?
            .rows_affected(),
    };

    if affected == 0 {
        return Err(RandomizeNicknameError::NotFound);
    }

    Ok(RandomizeNicknameOutcome {
        user_id: user_id.to_string(),
        old_nickname,
        new_nickname,
        new_version,
    })
}
