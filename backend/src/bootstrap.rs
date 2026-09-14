//! 首管理员引导（P0 整改）。
//!
//! 流程（docs/OPERATIONS.md §15 / docs/AUTHORIZATION.md §10）：
//!
//! 1. `bblbb-backend --bootstrap`：生成一次性 bootstrap token（高熵随机），
//!    数据库只存 SHA-256 哈希，明文 token 仅打印到终端一次；
//! 2. `POST /api/v1/auth/bootstrap`：匿名携带 token 创建首个 administrator；
//!    校验顺序：token 有效性 → 实例未初始化（无 active administrator）→
//!    注册形状（用户名/邮箱/密码强度）；
//! 3. 消费成功后 token 永久失效（consumed_at 置位），重复使用/重复初始化
//!    一律拒绝；用户创建 + 角色授予 + token 消费 + 审计在同一事务提交。
//!
//! 最后管理员保护（`is_last_active_admin`）：撤销 administrator 角色或把
//! 最后一名 active administrator 停用/封禁 → 调用方 409 拒绝。

use serde_json::json;
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::auth::token::{generate_token, hash_token};
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::outbox::OutboxTx;

/// bootstrap token 有效期：24 小时（Unix 毫秒）。
pub const BOOTSTRAP_TOKEN_TTL_MS: i64 = 24 * 60 * 60 * 1000;

/// 生成一次性 bootstrap token：返回 `(token_id, 明文 token)`。
///
/// 明文只在创建时返回一次；数据库只存 SHA-256 哈希。已有 active
/// administrator 时拒绝生成（实例已初始化）。
pub async fn create_bootstrap_token(
    pool: &DatabasePool,
    now: i64,
) -> Result<(String, String), String> {
    if has_active_administrator(pool)
        .await
        .map_err(|e| e.to_string())?
    {
        return Err("administrator already exists; bootstrap disabled".to_string());
    }
    let token = generate_token();
    let token_id = uuid::Uuid::now_v7().to_string();
    let token_hash = hash_token(&token);
    let sql = "INSERT INTO bootstrap_tokens (id, token_hash, created_at, expires_at)
               VALUES (?, ?, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&token_id)
                .bind(&token_hash)
                .bind(now)
                .bind(now + BOOTSTRAP_TOKEN_TTL_MS)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(&token_id)
                .bind(&token_hash)
                .bind(now)
                .bind(now + BOOTSTRAP_TOKEN_TTL_MS)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok((token_id, token))
}

/// 校验一次性 bootstrap token：存在、未消费、未过期。
/// 返回 token 行 id（消费时使用）；无效返回 `None`（调用方统一 422 防枚举）。
pub async fn validate_bootstrap_token(
    pool: &DatabasePool,
    token: &str,
    now: i64,
) -> Result<Option<String>, sqlx::Error> {
    if token.trim().is_empty() {
        return Ok(None);
    }
    let token_hash = hash_token(token.trim());
    let sql = "SELECT id FROM bootstrap_tokens
               WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?";
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar(sql)
                .bind(&token_hash)
                .bind(now)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(sql)
                .bind(&token_hash)
                .bind(now)
                .fetch_optional(p)
                .await
        }
    }
}

/// 实例是否已有至少一名 active administrator（未删除用户）。
pub async fn has_active_administrator(pool: &DatabasePool) -> Result<bool, sqlx::Error> {
    let sql = "SELECT COUNT(*) FROM user_roles ur
               JOIN roles r ON r.id = ur.role_id
               JOIN users u ON u.id = ur.user_id
               WHERE r.name = 'administrator'
                 AND u.status = 'active'
                 AND u.deleted_at IS NULL";
    let count: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await?,
        Either::Right(p) => sqlx::query_scalar(sql).fetch_one(p).await?,
    };
    Ok(count > 0)
}

/// 目标用户是否是最后一名 active administrator（撤销/停用前检查）。
pub async fn is_last_active_admin(pool: &DatabasePool, user_id: &str) -> Result<bool, sqlx::Error> {
    let now = crate::outbox::now_millis();
    let sql = "SELECT COUNT(*) FROM user_roles ur
               JOIN roles r ON r.id = ur.role_id
               JOIN users u ON u.id = ur.user_id
               WHERE r.name = 'administrator'
                 AND u.status = 'active'
                 AND u.deleted_at IS NULL
                 AND ur.granted_at <= ?
                 AND (ur.expires_at IS NULL OR ur.expires_at > ?)
                 AND ur.user_id = ?";
    let target_is_admin: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(sql)
                .bind(now)
                .bind(now)
                .bind(user_id)
                .fetch_one(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(sql)
                .bind(now)
                .bind(now)
                .bind(user_id)
                .fetch_one(p)
                .await?
        }
    };
    if target_is_admin == 0 {
        return Ok(false);
    }
    let others: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM user_roles ur
             JOIN roles r ON r.id = ur.role_id
             JOIN users u ON u.id = ur.user_id
             WHERE r.name = 'administrator'
               AND u.status = 'active'
               AND u.deleted_at IS NULL
               AND ur.granted_at <= ?
               AND (ur.expires_at IS NULL OR ur.expires_at > ?)
               AND ur.user_id != ?",
            )
            .bind(now)
            .bind(now)
            .bind(user_id)
            .fetch_one(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM user_roles ur
             JOIN roles r ON r.id = ur.role_id
             JOIN users u ON u.id = ur.user_id
             WHERE r.name = 'administrator'
               AND u.status = 'active'
               AND u.deleted_at IS NULL
               AND ur.granted_at <= ?
               AND (ur.expires_at IS NULL OR ur.expires_at > ?)
               AND ur.user_id != ?",
            )
            .bind(now)
            .bind(now)
            .bind(user_id)
            .fetch_one(p)
            .await?
        }
    };
    Ok(others == 0)
}

#[derive(Debug)]
pub enum GuardedUserUpdateError {
    NotFound,
    VersionConflict,
    LastAdministrator,
    Database(sqlx::Error),
}

/// 在事务内更新用户状态/昵称，并把最后管理员不变量与版本检查绑定到同一锁域。
/// SQLite 使用 BEGIN IMMEDIATE；MySQL/MariaDB 锁住当前有效管理员集合，避免两个
/// 并发管理请求同时通过“最后管理员”检查。
pub async fn update_user_guarded(
    pool: &DatabasePool,
    user_id: &str,
    expected_version: i64,
    status: Option<&str>,
    display_name: Option<&str>,
    now: i64,
) -> Result<(), GuardedUserUpdateError> {
    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin_with("BEGIN IMMEDIATE")
                .await
                .map_err(GuardedUserUpdateError::Database)?;
            let row: Option<(i64, String)> =
                sqlx::query_as("SELECT version, status FROM users WHERE id = ?")
                    .bind(user_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(GuardedUserUpdateError::Database)?;
            let Some((current_version, current_status)) = row else {
                return Err(GuardedUserUpdateError::NotFound);
            };
            if current_version != expected_version {
                return Err(GuardedUserUpdateError::VersionConflict);
            }
            if status.is_some_and(|next| next != "active") && current_status == "active" {
                let admins: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON r.id = ur.role_id
                     JOIN users u ON u.id = ur.user_id WHERE r.name = 'administrator'
                     AND u.status = 'active' AND u.deleted_at IS NULL AND ur.granted_at <= ?
                     AND (ur.expires_at IS NULL OR ur.expires_at > ?)",
                )
                .bind(now)
                .bind(now)
                .fetch_one(&mut *tx)
                .await
                .map_err(GuardedUserUpdateError::Database)?;
                let target_is_admin: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON r.id = ur.role_id
                     WHERE ur.user_id = ? AND r.name = 'administrator' AND ur.granted_at <= ?
                     AND (ur.expires_at IS NULL OR ur.expires_at > ?)",
                )
                .bind(user_id)
                .bind(now)
                .bind(now)
                .fetch_one(&mut *tx)
                .await
                .map_err(GuardedUserUpdateError::Database)?;
                if target_is_admin > 0 && admins <= 1 {
                    return Err(GuardedUserUpdateError::LastAdministrator);
                }
            }
            sqlx::query(
                "UPDATE users SET version = ?, updated_at = ?, status = COALESCE(?, status),
                 display_name = COALESCE(?, display_name) WHERE id = ? AND version = ?",
            )
            .bind(expected_version + 1)
            .bind(now)
            .bind(status)
            .bind(display_name)
            .bind(user_id)
            .bind(expected_version)
            .execute(&mut *tx)
            .await
            .map_err(GuardedUserUpdateError::Database)?;
            tx.commit()
                .await
                .map_err(GuardedUserUpdateError::Database)?;
        }
        Either::Right(p) => {
            let mut tx = p.begin().await.map_err(GuardedUserUpdateError::Database)?;
            if status.is_some_and(|next| next != "active") {
                sqlx::query(
                    "SELECT u.id FROM user_roles ur JOIN roles r ON r.id = ur.role_id
                     JOIN users u ON u.id = ur.user_id WHERE r.name = 'administrator'
                     AND u.status = 'active' AND u.deleted_at IS NULL AND ur.granted_at <= ?
                     AND (ur.expires_at IS NULL OR ur.expires_at > ?) ORDER BY u.id FOR UPDATE",
                )
                .bind(now)
                .bind(now)
                .fetch_all(&mut *tx)
                .await
                .map_err(GuardedUserUpdateError::Database)?;
            }
            let row: Option<(i64, String)> =
                sqlx::query_as("SELECT version, status FROM users WHERE id = ? FOR UPDATE")
                    .bind(user_id)
                    .fetch_optional(&mut *tx)
                    .await
                    .map_err(GuardedUserUpdateError::Database)?;
            let Some((current_version, current_status)) = row else {
                return Err(GuardedUserUpdateError::NotFound);
            };
            if current_version != expected_version {
                return Err(GuardedUserUpdateError::VersionConflict);
            }
            if status.is_some_and(|next| next != "active") && current_status == "active" {
                let admins: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON r.id = ur.role_id
                     JOIN users u ON u.id = ur.user_id WHERE r.name = 'administrator'
                     AND u.status = 'active' AND u.deleted_at IS NULL AND ur.granted_at <= ?
                     AND (ur.expires_at IS NULL OR ur.expires_at > ?)",
                )
                .bind(now)
                .bind(now)
                .fetch_one(&mut *tx)
                .await
                .map_err(GuardedUserUpdateError::Database)?;
                let target_is_admin: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM user_roles ur JOIN roles r ON r.id = ur.role_id
                     WHERE ur.user_id = ? AND r.name = 'administrator' AND ur.granted_at <= ?
                     AND (ur.expires_at IS NULL OR ur.expires_at > ?)",
                )
                .bind(user_id)
                .bind(now)
                .bind(now)
                .fetch_one(&mut *tx)
                .await
                .map_err(GuardedUserUpdateError::Database)?;
                if target_is_admin > 0 && admins <= 1 {
                    return Err(GuardedUserUpdateError::LastAdministrator);
                }
            }
            sqlx::query(
                "UPDATE users SET version = ?, updated_at = ?, status = COALESCE(?, status),
                 display_name = COALESCE(?, display_name) WHERE id = ? AND version = ?",
            )
            .bind(expected_version + 1)
            .bind(now)
            .bind(status)
            .bind(display_name)
            .bind(user_id)
            .bind(expected_version)
            .execute(&mut *tx)
            .await
            .map_err(GuardedUserUpdateError::Database)?;
            tx.commit()
                .await
                .map_err(GuardedUserUpdateError::Database)?;
        }
    }
    Ok(())
}

/// 首管理员输入（规范化与密码强度校验由调用方完成，与注册同规则）。
pub struct BootstrapAdminInput {
    pub username: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
}

/// 在同一事务中：创建 active administrator → 授予 administrator 角色 →
/// 消费 bootstrap token → 写审计。任一步失败整体回滚。返回新用户 id。
pub async fn consume_and_create_admin(
    pool: &DatabasePool,
    token_row_id: &str,
    input: &BootstrapAdminInput,
    now: i64,
) -> Result<String, String> {
    let user_id = uuid::Uuid::now_v7().to_string();
    let role_id: String = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT id FROM roles WHERE name = 'administrator'")
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT id FROM roles WHERE name = 'administrator'")
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| format!("administrator role missing (run migrations): {e}"))?;

    match pool {
        Either::Left(p) => {
            let mut tx = OutboxTx::Left(p.begin().await.map_err(|e| e.to_string())?);
            run_in_tx(&mut tx, &user_id, token_row_id, input, &role_id, now).await?;
            match tx {
                Either::Left(t) => t.commit().await.map_err(|e| e.to_string())?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = OutboxTx::Right(p.begin().await.map_err(|e| e.to_string())?);
            run_in_tx(&mut tx, &user_id, token_row_id, input, &role_id, now).await?;
            match tx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await.map_err(|e| e.to_string())?,
            }
        }
    }
    Ok(user_id)
}

async fn run_in_tx(
    tx: &mut OutboxTx<'_>,
    user_id: &str,
    token_row_id: &str,
    input: &BootstrapAdminInput,
    role_id: &str,
    now: i64,
) -> Result<(), String> {
    let insert_user = "INSERT INTO users (id, username_normalized, email_normalized, password_hash, display_name, status, email_verified, version, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, 'active', 1, 1, ?, ?)";
    let insert_role =
        "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
         VALUES (?, ?, NULL, ?, NULL)";
    let consume = "UPDATE bootstrap_tokens SET consumed_at = ?, consumed_by = ?
         WHERE id = ? AND consumed_at IS NULL";

    match tx {
        Either::Left(t) => {
            sqlx::query(insert_user)
                .bind(user_id)
                .bind(&input.username)
                .bind(&input.email)
                .bind(&input.password_hash)
                .bind(&input.display_name)
                .bind(now)
                .bind(now)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(insert_role)
                .bind(user_id)
                .bind(role_id)
                .bind(now)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
            let affected = sqlx::query(consume)
                .bind(now)
                .bind(user_id)
                .bind(token_row_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected();
            if affected != 1 {
                return Err("bootstrap token already consumed".to_string());
            }
        }
        Either::Right(t) => {
            sqlx::query(insert_user)
                .bind(user_id)
                .bind(&input.username)
                .bind(&input.email)
                .bind(&input.password_hash)
                .bind(&input.display_name)
                .bind(now)
                .bind(now)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
            sqlx::query(insert_role)
                .bind(user_id)
                .bind(role_id)
                .bind(now)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?;
            let affected = sqlx::query(consume)
                .bind(now)
                .bind(user_id)
                .bind(token_row_id)
                .execute(&mut **t)
                .await
                .map_err(|e| e.to_string())?
                .rows_affected();
            if affected != 1 {
                return Err("bootstrap token already consumed".to_string());
            }
        }
    }

    AuditEntry::user_action(user_id, "admin.bootstrap")
        .with_target("user", user_id)
        .with_effective_role("administrator")
        .with_reason("bootstrap first administrator")
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "username": input.username }))
        .record_in_tx(tx)
        .await
        .map_err(|e| e.to_string())?;
    Ok(())
}
