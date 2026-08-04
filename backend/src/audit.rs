//! 审计日志模块 — 记录关键操作以便追溯
//!
//! 审计日志记录到 `audit_logs` 表，支持：
//! - 用户操作（登录、注册、发帖、评论等）
//! - 管理操作（封禁、删除、配置变更等）
//! - 系统事件（部署、迁移、密钥轮换等）

use chrono::Utc;
use serde_json::Value;
use sqlx::Either;

use crate::db::pool::DatabasePool;

/// 审计日志记录
pub struct AuditEntry {
    pub actor_id: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata: Option<Value>,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
}

impl AuditEntry {
    /// 创建用户操作审计记录
    pub fn user_action(user_id: &str, action: &str) -> Self {
        Self {
            actor_id: Some(user_id.to_string()),
            action: action.to_string(),
            target_type: None,
            target_id: None,
            metadata: None,
            request_id: None,
            ip_address: None,
        }
    }

    /// 设置操作目标
    pub fn with_target(mut self, target_type: &str, target_id: &str) -> Self {
        self.target_type = Some(target_type.to_string());
        self.target_id = Some(target_id.to_string());
        self
    }

    /// 设置元数据
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// 设置请求 ID
    pub fn with_request_id(mut self, request_id: &str) -> Self {
        self.request_id = Some(request_id.to_string());
        self
    }

    /// 设置 IP 地址
    pub fn with_ip(mut self, ip: &str) -> Self {
        self.ip_address = Some(ip.to_string());
        self
    }

    /// 写入数据库
    pub async fn record(self, pool: &DatabasePool) -> Result<(), sqlx::Error> {
        let id = uuid::Uuid::now_v7().to_string();
        let now = Utc::now().timestamp();
        let metadata_json = self
            .metadata
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        match pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO audit_logs (id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&self.actor_id)
                .bind(&self.action)
                .bind(&self.target_type)
                .bind(&self.target_id)
                .bind(&metadata_json)
                .bind(&self.request_id)
                .bind(&self.ip_address)
                .bind(now)
                .execute(p)
                .await?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT INTO audit_logs (id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&self.actor_id)
                .bind(&self.action)
                .bind(&self.target_type)
                .bind(&self.target_id)
                .bind(&metadata_json)
                .bind(&self.request_id)
                .bind(&self.ip_address)
                .bind(now)
                .execute(p)
                .await?;
            }
        }

        tracing::debug!(audit_id = %id, action = %self.action, "audit log recorded");
        Ok(())
    }
}

/// 查询审计日志（管理端用）
pub async fn list_audit_logs(
    pool: &DatabasePool,
    limit: i64,
    offset: i64,
    actor_id: Option<&str>,
    action: Option<&str>,
) -> Result<Vec<AuditLogRow>, sqlx::Error> {
    let limit = limit.clamp(1, 100);

    match pool {
        Either::Left(p) => {
            if let (Some(actor), Some(act)) = (actor_id, action) {
                sqlx::query_as::<_, AuditLogRow>(
                    "SELECT id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at
                     FROM audit_logs WHERE actor_id = ? AND action = ?
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(actor)
                .bind(act)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else if let Some(actor) = actor_id {
                sqlx::query_as::<_, AuditLogRow>(
                    "SELECT id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at
                     FROM audit_logs WHERE actor_id = ?
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(actor)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else if let Some(act) = action {
                sqlx::query_as::<_, AuditLogRow>(
                    "SELECT id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at
                     FROM audit_logs WHERE action = ?
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(act)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query_as::<_, AuditLogRow>(
                    "SELECT id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at
                     FROM audit_logs
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
        }
        Either::Right(p) => {
            if let (Some(actor), Some(act)) = (actor_id, action) {
                sqlx::query_as::<_, AuditLogRow>(
                    "SELECT id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at
                     FROM audit_logs WHERE actor_id = ? AND action = ?
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(actor)
                .bind(act)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else if let Some(actor) = actor_id {
                sqlx::query_as::<_, AuditLogRow>(
                    "SELECT id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at
                     FROM audit_logs WHERE actor_id = ?
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(actor)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else if let Some(act) = action {
                sqlx::query_as::<_, AuditLogRow>(
                    "SELECT id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at
                     FROM audit_logs WHERE action = ?
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(act)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query_as::<_, AuditLogRow>(
                    "SELECT id, actor_id, action, target_type, target_id, metadata, request_id, ip_address, created_at
                     FROM audit_logs
                     ORDER BY created_at DESC LIMIT ? OFFSET ?",
                )
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
        }
    }
}

#[derive(sqlx::FromRow)]
pub struct AuditLogRow {
    pub id: String,
    pub actor_id: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub metadata: Option<String>,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_entry_builder() {
        let entry = AuditEntry::user_action("user123", "login")
            .with_target("session", "session456")
            .with_request_id("req-789")
            .with_ip("127.0.0.1");

        assert_eq!(entry.actor_id.as_deref(), Some("user123"));
        assert_eq!(entry.action, "login");
        assert_eq!(entry.target_type.as_deref(), Some("session"));
        assert_eq!(entry.target_id.as_deref(), Some("session456"));
        assert_eq!(entry.request_id.as_deref(), Some("req-789"));
        assert_eq!(entry.ip_address.as_deref(), Some("127.0.0.1"));
    }
}
