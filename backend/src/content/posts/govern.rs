//! M04-POSTS-09：pin/feature/close/move/merge 治理命令接口。
//!
//! 本模块提供**治理命令**（类型化、服务端校验）与应用函数；具体治理权限
//! （post.moderate / reason / 审计）由 M5 接入路由层时执行——本层只保证：
//! - 命令字段合法（UUID、语义约束，如 merge 目标不得为自身）；
//! - 状态变更服务端权威（pinned_at/featured_at/closed_at 置 now 或置空、
//!   板块变更、合并迁移评论）；
//! - 全部为原子写（事务）。

use uuid::Uuid;

use crate::db::DatabasePool;
use crate::error::AppError;

/// 治理命令校验/应用错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GovernError {
    InvalidPostId,
    InvalidBoardId,
    TargetBoardNotActive,
    MergeIntoSelf,
    PostNotFound,
    Db(String),
}

impl std::fmt::Display for GovernError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPostId => write!(f, "post id must be a valid UUID"),
            Self::InvalidBoardId => write!(f, "board id must be a valid UUID"),
            Self::TargetBoardNotActive => write!(f, "target board is not active"),
            Self::MergeIntoSelf => write!(f, "merge source and target must differ"),
            Self::PostNotFound => write!(f, "post not found"),
            Self::Db(msg) => write!(f, "govern db error: {msg}"),
        }
    }
}

impl std::error::Error for GovernError {}

// ---------------------------------------------------------------------------
// 命令（类型化 + 校验）
// ---------------------------------------------------------------------------

/// 置顶/取消置顶：`pin=true` 置 `pinned_at=now`；`false` 置空。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinCommand {
    pub post_id: String,
    pub pin: bool,
}

impl PinCommand {
    pub fn validate(self) -> Result<Self, GovernError> {
        Uuid::parse_str(&self.post_id).map_err(|_| GovernError::InvalidPostId)?;
        Ok(self)
    }
}

/// 精选/取消精选：`feature=true` 置 `featured_at=now`；`false` 置空。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureCommand {
    pub post_id: String,
    pub feature: bool,
}

impl FeatureCommand {
    pub fn validate(self) -> Result<Self, GovernError> {
        Uuid::parse_str(&self.post_id).map_err(|_| GovernError::InvalidPostId)?;
        Ok(self)
    }
}

/// 关闭/重开回复：`close=true` 置 `closed_at=now`；`false` 置空。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloseCommand {
    pub post_id: String,
    pub close: bool,
}

impl CloseCommand {
    pub fn validate(self) -> Result<Self, GovernError> {
        Uuid::parse_str(&self.post_id).map_err(|_| GovernError::InvalidPostId)?;
        Ok(self)
    }
}

/// 移帖：把帖子移到目标板块（目标板块必须存在且启用）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MoveCommand {
    pub post_id: String,
    pub target_board_id: String,
}

impl MoveCommand {
    pub fn validate(self) -> Result<Self, GovernError> {
        Uuid::parse_str(&self.post_id).map_err(|_| GovernError::InvalidPostId)?;
        Uuid::parse_str(&self.target_board_id).map_err(|_| GovernError::InvalidBoardId)?;
        Ok(self)
    }
}

/// 合并：把源帖评论迁移到目标帖，软删除源帖（重定向/permalink 随 SEO 里程碑）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MergeCommand {
    pub source_post_id: String,
    pub target_post_id: String,
}

impl MergeCommand {
    pub fn validate(self) -> Result<Self, GovernError> {
        Uuid::parse_str(&self.source_post_id).map_err(|_| GovernError::InvalidPostId)?;
        Uuid::parse_str(&self.target_post_id).map_err(|_| GovernError::InvalidPostId)?;
        if self.source_post_id == self.target_post_id {
            return Err(GovernError::MergeIntoSelf);
        }
        Ok(self)
    }
}

// ---------------------------------------------------------------------------
// 应用（原子写；权限由 M5 路由层接入）
// ---------------------------------------------------------------------------

/// 应用置顶命令。
pub async fn apply_pin(pool: &DatabasePool, cmd: &PinCommand, now: i64) -> Result<(), GovernError> {
    let pinned_at = if cmd.pin { Some(now) } else { None };
    let affected = update_timestamp(pool, &cmd.post_id, "pinned_at", pinned_at, now).await?;
    if affected == 0 {
        return Err(GovernError::PostNotFound);
    }
    Ok(())
}

/// 应用精选命令。
pub async fn apply_feature(
    pool: &DatabasePool,
    cmd: &FeatureCommand,
    now: i64,
) -> Result<(), GovernError> {
    let featured_at = if cmd.feature { Some(now) } else { None };
    let affected = update_timestamp(pool, &cmd.post_id, "featured_at", featured_at, now).await?;
    if affected == 0 {
        return Err(GovernError::PostNotFound);
    }
    Ok(())
}

/// 应用关闭命令。
pub async fn apply_close(
    pool: &DatabasePool,
    cmd: &CloseCommand,
    now: i64,
) -> Result<(), GovernError> {
    let closed_at = if cmd.close { Some(now) } else { None };
    let affected = update_timestamp(pool, &cmd.post_id, "closed_at", closed_at, now).await?;
    if affected == 0 {
        return Err(GovernError::PostNotFound);
    }
    Ok(())
}

async fn update_timestamp(
    pool: &DatabasePool,
    post_id: &str,
    column: &str,
    value: Option<i64>,
    now: i64,
) -> Result<u64, GovernError> {
    // column 为内部固定白名单值（pinned_at/featured_at/closed_at），拼接安全
    let sql = format!(
        "UPDATE posts SET {column} = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL"
    );
    let affected = match pool {
        crate::db::DatabasePool::Left(p) => sqlx::query(&sql)
            .bind(value)
            .bind(now)
            .bind(post_id)
            .execute(p)
            .await
            .map_err(|e| GovernError::Db(e.to_string()))?
            .rows_affected(),
        crate::db::DatabasePool::Right(p) => sqlx::query(&sql)
            .bind(value)
            .bind(now)
            .bind(post_id)
            .execute(p)
            .await
            .map_err(|e| GovernError::Db(e.to_string()))?
            .rows_affected(),
    };
    Ok(affected)
}

/// 应用移帖命令：目标板块存在且启用 → 更新 board_id（slug 冲突由调用方
/// 处理，M5/SEO 里程碑收口）。
pub async fn apply_move(
    pool: &DatabasePool,
    cmd: &MoveCommand,
    now: i64,
) -> Result<(), GovernError> {
    let active: Option<i64> = match pool {
        crate::db::DatabasePool::Left(p) => {
            sqlx::query_scalar("SELECT is_active FROM boards WHERE id = ? AND deleted_at IS NULL")
                .bind(&cmd.target_board_id)
                .fetch_optional(p)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?
        }
        crate::db::DatabasePool::Right(p) => {
            sqlx::query_scalar("SELECT is_active FROM boards WHERE id = ? AND deleted_at IS NULL")
                .bind(&cmd.target_board_id)
                .fetch_optional(p)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?
        }
    };
    match active {
        None => return Err(GovernError::TargetBoardNotActive),
        Some(0) => return Err(GovernError::TargetBoardNotActive),
        _ => {}
    }
    let affected = match pool {
        crate::db::DatabasePool::Left(p) => sqlx::query(
            "UPDATE posts SET board_id = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(&cmd.target_board_id)
        .bind(now)
        .bind(&cmd.post_id)
        .execute(p)
        .await
        .map_err(|e| GovernError::Db(e.to_string()))?
        .rows_affected(),
        crate::db::DatabasePool::Right(p) => sqlx::query(
            "UPDATE posts SET board_id = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL",
        )
        .bind(&cmd.target_board_id)
        .bind(now)
        .bind(&cmd.post_id)
        .execute(p)
        .await
        .map_err(|e| GovernError::Db(e.to_string()))?
        .rows_affected(),
    };
    if affected == 0 {
        return Err(GovernError::PostNotFound);
    }
    Ok(())
}

/// 应用合并命令：事务内把源帖评论迁移到目标帖、软删源帖、目标帖
/// reply_count 累加源帖回复数。
pub async fn apply_merge(
    pool: &DatabasePool,
    cmd: &MergeCommand,
    now: i64,
) -> Result<(), GovernError> {
    // 源/目标都必须存在且未删除
    let src: Option<(String,)> = match pool {
        crate::db::DatabasePool::Left(p) => {
            sqlx::query_as("SELECT id FROM posts WHERE id = ? AND deleted_at IS NULL")
                .bind(&cmd.source_post_id)
                .fetch_optional(p)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?
        }
        crate::db::DatabasePool::Right(p) => {
            sqlx::query_as("SELECT id FROM posts WHERE id = ? AND deleted_at IS NULL")
                .bind(&cmd.source_post_id)
                .fetch_optional(p)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?
        }
    };
    if src.is_none() {
        return Err(GovernError::PostNotFound);
    }
    let tgt: Option<(String,)> = match pool {
        crate::db::DatabasePool::Left(p) => {
            sqlx::query_as("SELECT id FROM posts WHERE id = ? AND deleted_at IS NULL")
                .bind(&cmd.target_post_id)
                .fetch_optional(p)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?
        }
        crate::db::DatabasePool::Right(p) => {
            sqlx::query_as("SELECT id FROM posts WHERE id = ? AND deleted_at IS NULL")
                .bind(&cmd.target_post_id)
                .fetch_optional(p)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?
        }
    };
    if tgt.is_none() {
        return Err(GovernError::PostNotFound);
    }

    let src_replies: i64 = match pool {
        crate::db::DatabasePool::Left(p) => {
            sqlx::query_scalar("SELECT reply_count FROM posts WHERE id = ?")
                .bind(&cmd.source_post_id)
                .fetch_one(p)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?
        }
        crate::db::DatabasePool::Right(p) => {
            sqlx::query_scalar("SELECT reply_count FROM posts WHERE id = ?")
                .bind(&cmd.source_post_id)
                .fetch_one(p)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?
        }
    };

    match pool {
        crate::db::DatabasePool::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?;
            sqlx::query("UPDATE comments SET post_id = ? WHERE post_id = ?")
                .bind(&cmd.target_post_id)
                .bind(&cmd.source_post_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?;
            sqlx::query(
                "UPDATE posts SET status = 'deleted', deleted_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(now)
            .bind(&cmd.source_post_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| GovernError::Db(e.to_string()))?;
            sqlx::query(
                "UPDATE posts SET reply_count = reply_count + ?, updated_at = ? WHERE id = ?",
            )
            .bind(src_replies)
            .bind(now)
            .bind(&cmd.target_post_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| GovernError::Db(e.to_string()))?;
            tx.commit()
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?;
        }
        crate::db::DatabasePool::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?;
            sqlx::query("UPDATE comments SET post_id = ? WHERE post_id = ?")
                .bind(&cmd.target_post_id)
                .bind(&cmd.source_post_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?;
            sqlx::query(
                "UPDATE posts SET status = 'deleted', deleted_at = ?, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(now)
            .bind(&cmd.source_post_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| GovernError::Db(e.to_string()))?;
            sqlx::query(
                "UPDATE posts SET reply_count = reply_count + ?, updated_at = ? WHERE id = ?",
            )
            .bind(src_replies)
            .bind(now)
            .bind(&cmd.target_post_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| GovernError::Db(e.to_string()))?;
            tx.commit()
                .await
                .map_err(|e| GovernError::Db(e.to_string()))?;
        }
    }
    Ok(())
}

// 供 M5 路由层复用的错误映射（避免路由层直接依赖 sqlx）。
impl From<GovernError> for AppError {
    fn from(e: GovernError) -> Self {
        match e {
            GovernError::PostNotFound => AppError::not_found(e.to_string(), "govern"),
            GovernError::Db(msg) => AppError::internal(msg, "govern"),
            other => AppError::bad_request(other.to_string(), "govern", None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn uuid() -> String {
        uuid::Uuid::now_v7().to_string()
    }

    #[test]
    fn pin_command_validates_uuid() {
        let ok = PinCommand {
            post_id: uuid(),
            pin: true,
        }
        .validate();
        assert!(ok.is_ok());
        let bad = PinCommand {
            post_id: "nope".into(),
            pin: true,
        }
        .validate();
        assert_eq!(bad.unwrap_err(), GovernError::InvalidPostId);
    }

    #[test]
    fn feature_and_close_validate_uuid() {
        assert!(FeatureCommand {
            post_id: uuid(),
            feature: false
        }
        .validate()
        .is_ok());
        assert!(CloseCommand {
            post_id: uuid(),
            close: true
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn move_command_validates_both_ids() {
        assert!(MoveCommand {
            post_id: uuid(),
            target_board_id: uuid()
        }
        .validate()
        .is_ok());
        assert_eq!(
            MoveCommand {
                post_id: "bad".into(),
                target_board_id: uuid()
            }
            .validate()
            .unwrap_err(),
            GovernError::InvalidPostId
        );
        assert_eq!(
            MoveCommand {
                post_id: uuid(),
                target_board_id: "bad".into()
            }
            .validate()
            .unwrap_err(),
            GovernError::InvalidBoardId
        );
    }

    #[test]
    fn merge_rejects_self_and_bad_ids() {
        let id = uuid();
        assert_eq!(
            MergeCommand {
                source_post_id: id.clone(),
                target_post_id: id
            }
            .validate()
            .unwrap_err(),
            GovernError::MergeIntoSelf
        );
        assert!(MergeCommand {
            source_post_id: uuid(),
            target_post_id: uuid()
        }
        .validate()
        .is_ok());
    }
}
