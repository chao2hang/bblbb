//! 审计日志模块（M01-AUDIT-01）— 记录关键操作以便追溯。
//!
//! 契约：
//! - **不可关闭**：审计写入是强制路径，没有任何配置开关可以禁用；本模块
//!   不提供删除/修改审计记录的 API，`audit_logs` 表只追加。
//! - 每条记录包含 actor、effective role（执行时生效的角色）、target、
//!   action、reason、request_id 与 policy version（本次权限判定的策略版本）。
//! - 时间戳为 Unix 毫秒（SCHEMA §2.2）。
//! - `metadata`/before/after 必须经过字段 allowlist 过滤（M01-AUDIT-02），
//!   禁止密码、Token、Secret、隐藏正文和完整签名 URL。
//!
//! 高风险操作必须先写审计再提交业务事务（事务内调用 `record_in_tx` 语义见
//! M01-AUDIT-08）；本模块的 `record` 是独立写入，供非事务路径使用。

use chrono::Utc;
use serde_json::Value;
use sqlx::Either;

use crate::db::pool::DatabasePool;

/// 当前 Unix 毫秒（跨库时间约定 SCHEMA §2.2）。
fn now_millis() -> i64 {
    Utc::now().timestamp_millis()
}

/// 审计日志记录。
pub struct AuditEntry {
    pub actor_id: Option<String>,
    /// 生效角色（执行权限判定后实际使用的角色）。
    pub effective_role: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    /// 操作原因（管理员代操作、处罚、申诉等）。
    pub reason: Option<String>,
    /// 权限策略版本（本次判定依据的 policy version）。
    pub policy_version: Option<String>,
    pub metadata: Option<Value>,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
}

impl AuditEntry {
    /// 创建用户操作审计记录。
    pub fn user_action(user_id: &str, action: &str) -> Self {
        Self {
            actor_id: Some(user_id.to_string()),
            effective_role: None,
            action: action.to_string(),
            target_type: None,
            target_id: None,
            reason: None,
            policy_version: None,
            metadata: None,
            request_id: None,
            ip_address: None,
        }
    }

    /// 创建系统/管理员操作审计记录（无 actor 或 actor 单独指定）。
    pub fn system_action(action: &str) -> Self {
        Self::user_action("__system__", action)
    }

    /// 设置操作目标。
    pub fn with_target(mut self, target_type: &str, target_id: &str) -> Self {
        self.target_type = Some(target_type.to_string());
        self.target_id = Some(target_id.to_string());
        self
    }

    /// 设置生效角色（M01-AUDIT-01）。
    pub fn with_effective_role(mut self, role: &str) -> Self {
        self.effective_role = Some(role.to_string());
        self
    }

    /// 设置操作原因（M01-AUDIT-01）。
    pub fn with_reason(mut self, reason: &str) -> Self {
        self.reason = Some(reason.to_string());
        self
    }

    /// 设置权限策略版本（M01-AUDIT-01）。
    pub fn with_policy_version(mut self, version: &str) -> Self {
        self.policy_version = Some(version.to_string());
        self
    }

    /// 设置元数据（必须已通过字段 allowlist 过滤，M01-AUDIT-02）。
    pub fn with_metadata(mut self, metadata: Value) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// 设置请求 ID。
    pub fn with_request_id(mut self, request_id: &str) -> Self {
        self.request_id = Some(request_id.to_string());
        self
    }

    /// 设置 IP 地址。
    pub fn with_ip(mut self, ip: &str) -> Self {
        self.ip_address = Some(ip.to_string());
        self
    }

    /// 写入数据库（不可关闭的强制路径）。
    pub async fn record(self, pool: &DatabasePool) -> Result<(), sqlx::Error> {
        let id = uuid::Uuid::now_v7().to_string();
        let now = now_millis();
        let metadata_json = self
            .metadata
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        match pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO audit_logs
                         (id, actor_id, effective_role, action, target_type, target_id, reason, policy_version, metadata, request_id, ip_address, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&self.actor_id)
                .bind(&self.effective_role)
                .bind(&self.action)
                .bind(&self.target_type)
                .bind(&self.target_id)
                .bind(&self.reason)
                .bind(&self.policy_version)
                .bind(&metadata_json)
                .bind(&self.request_id)
                .bind(&self.ip_address)
                .bind(now)
                .execute(p)
                .await?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT INTO audit_logs
                         (id, actor_id, effective_role, action, target_type, target_id, reason, policy_version, metadata, request_id, ip_address, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&id)
                .bind(&self.actor_id)
                .bind(&self.effective_role)
                .bind(&self.action)
                .bind(&self.target_type)
                .bind(&self.target_id)
                .bind(&self.reason)
                .bind(&self.policy_version)
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

/// 查询审计日志（管理端用）。
pub async fn list_audit_logs(
    pool: &DatabasePool,
    limit: i64,
    offset: i64,
    actor_id: Option<&str>,
    action: Option<&str>,
) -> Result<Vec<AuditLogRow>, sqlx::Error> {
    let limit = limit.clamp(1, 100);

    const SELECT_COLUMNS: &str = "SELECT id, actor_id, effective_role, action, target_type, target_id, reason, policy_version, metadata, request_id, ip_address, created_at
        FROM audit_logs";

    match pool {
        Either::Left(p) => {
            if let (Some(actor), Some(act)) = (actor_id, action) {
                sqlx::query_as::<_, AuditLogRow>(&format!("{SELECT_COLUMNS} WHERE actor_id = ? AND action = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"))
                    .bind(actor).bind(act).bind(limit).bind(offset).fetch_all(p).await
            } else if let Some(actor) = actor_id {
                sqlx::query_as::<_, AuditLogRow>(&format!(
                    "{SELECT_COLUMNS} WHERE actor_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
                ))
                .bind(actor)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else if let Some(act) = action {
                sqlx::query_as::<_, AuditLogRow>(&format!(
                    "{SELECT_COLUMNS} WHERE action = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
                ))
                .bind(act)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query_as::<_, AuditLogRow>(&format!(
                    "{SELECT_COLUMNS} ORDER BY created_at DESC LIMIT ? OFFSET ?"
                ))
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            }
        }
        Either::Right(p) => {
            if let (Some(actor), Some(act)) = (actor_id, action) {
                sqlx::query_as::<_, AuditLogRow>(&format!("{SELECT_COLUMNS} WHERE actor_id = ? AND action = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"))
                    .bind(actor).bind(act).bind(limit).bind(offset).fetch_all(p).await
            } else if let Some(actor) = actor_id {
                sqlx::query_as::<_, AuditLogRow>(&format!(
                    "{SELECT_COLUMNS} WHERE actor_id = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
                ))
                .bind(actor)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else if let Some(act) = action {
                sqlx::query_as::<_, AuditLogRow>(&format!(
                    "{SELECT_COLUMNS} WHERE action = ? ORDER BY created_at DESC LIMIT ? OFFSET ?"
                ))
                .bind(act)
                .bind(limit)
                .bind(offset)
                .fetch_all(p)
                .await
            } else {
                sqlx::query_as::<_, AuditLogRow>(&format!(
                    "{SELECT_COLUMNS} ORDER BY created_at DESC LIMIT ? OFFSET ?"
                ))
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
    pub effective_role: Option<String>,
    pub action: String,
    pub target_type: Option<String>,
    pub target_id: Option<String>,
    pub reason: Option<String>,
    pub policy_version: Option<String>,
    pub metadata: Option<String>,
    pub request_id: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audit_entry_builder_carries_all_m01_audit_01_fields() {
        let entry = AuditEntry::user_action("user123", "admin.ban_user")
            .with_target("user", "user456")
            .with_effective_role("moderator")
            .with_reason("repeated violations")
            .with_policy_version("v1.0.0-rc.2")
            .with_request_id("req-789")
            .with_ip("127.0.0.1");

        assert_eq!(entry.actor_id.as_deref(), Some("user123"));
        assert_eq!(entry.action, "admin.ban_user");
        assert_eq!(entry.target_type.as_deref(), Some("user"));
        assert_eq!(entry.target_id.as_deref(), Some("user456"));
        assert_eq!(entry.effective_role.as_deref(), Some("moderator"));
        assert_eq!(entry.reason.as_deref(), Some("repeated violations"));
        assert_eq!(entry.policy_version.as_deref(), Some("v1.0.0-rc.2"));
        assert_eq!(entry.request_id.as_deref(), Some("req-789"));
        assert_eq!(entry.ip_address.as_deref(), Some("127.0.0.1"));
    }
}
