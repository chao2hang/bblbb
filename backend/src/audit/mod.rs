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
use serde_json::{json, Value};
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
    pub fn user_action(user_id: &str, action: impl Into<String>) -> Self {
        Self {
            actor_id: Some(user_id.to_string()),
            effective_role: None,
            action: action.into(),
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

    // ── M01-AUDIT-06：高风险操作分类 helper ────────────────────────────────

    /// 管理员代操作（delegated/impersonation）：必须携带 effective role 与 reason。
    pub fn delegated_admin_action(
        operator: &str,
        effective_role: &str,
        action: &str,
        target_type: &str,
        target_id: &str,
        reason: &str,
    ) -> Self {
        Self::user_action(operator, action)
            .with_target(target_type, target_id)
            .with_effective_role(effective_role)
            .with_reason(reason)
    }

    /// 权限变更（角色/权限调整）：记录 subject、before/after 角色与策略版本。
    pub fn permission_change(
        actor: &str,
        subject_id: &str,
        role_before: &str,
        role_after: &str,
        reason: &str,
        policy_version: &str,
    ) -> Self {
        Self::user_action(actor, "admin.permission_change")
            .with_target("user", subject_id)
            .with_effective_role("administrator")
            .with_reason(reason)
            .with_policy_version(policy_version)
            .with_metadata(json!({
                "role": role_after,
                "before": json!({ "role": role_before }),
                "after": json!({ "role": role_after })
            }))
    }

    /// 配置变更：before/after 只记录白名单字段（M01-AUDIT-02 过滤）。
    pub fn config_change(
        actor: &str,
        config_key: &str,
        before: Option<&Value>,
        after: Option<&Value>,
        reason: &str,
        policy_version: &str,
    ) -> Self {
        Self::user_action(actor, "admin.config_change")
            .with_target("config", config_key)
            .with_effective_role("administrator")
            .with_reason(reason)
            .with_policy_version(policy_version)
            .with_metadata(json!({
                "config_key": config_key,
                "before": before.map(sanitize_for_audit).unwrap_or(Value::Null),
                "after": after.map(sanitize_for_audit).unwrap_or(Value::Null)
            }))
    }

    /// 账务变更（余额/扣款/退款等）：记录金额、币种与原因，不记录敏感凭据。
    pub fn accounting_change(
        actor: &str,
        target_type: &str,
        target_id: &str,
        amount: i64,
        currency: &str,
        reason: &str,
    ) -> Self {
        Self::user_action(actor, "ledger.change")
            .with_target(target_type, target_id)
            .with_effective_role("administrator")
            .with_reason(reason)
            .with_metadata(json!({ "amount": amount, "currency": currency }))
    }

    /// 内容审核/审查动作：必须携带 reason 与策略版本。
    pub fn moderation_action(
        actor: &str,
        target_type: &str,
        target_id: &str,
        action: &str,
        reason: &str,
        policy_version: &str,
    ) -> Self {
        Self::user_action(actor, action)
            .with_target(target_type, target_id)
            .with_effective_role("moderator")
            .with_reason(reason)
            .with_policy_version(policy_version)
    }

    /// Secret 变更：只记录 Secret 名称与动作，绝不接收/记录 Secret 值。
    pub fn secret_change(actor: &str, secret_name: &str, action: &str) -> Self {
        Self::user_action(actor, format!("secrets.{action}"))
            .with_target("secret", secret_name)
            .with_effective_role("administrator")
            .with_metadata(json!({ "secret_name": secret_name }))
    }

    /// 板块管理变更（创建/更新，M03-BOARDS-05）：必须携带 reason 与策略版本；
    /// before/after 只记录白名单字段（slug/name/description/parent_id/
    /// sort_order/visibility/posting_mode/is_active，M01-AUDIT-02 过滤）。
    pub fn board_change(
        actor: &str,
        action: &str,
        board_id: &str,
        before: Option<&Value>,
        after: &Value,
        reason: &str,
        policy_version: &str,
    ) -> Self {
        Self::user_action(actor, action)
            .with_target("board", board_id)
            .with_effective_role("administrator")
            .with_reason(reason)
            .with_policy_version(policy_version)
            .with_metadata(json!({
                "before": before.map(sanitize_for_audit).unwrap_or(Value::Null),
                "after": sanitize_for_audit(after),
            }))
    }

    /// 标签管理变更（创建/更新，M03-BOARDS-07）：必须携带 reason 与策略版本；
    /// before/after 只记录白名单字段（name/slug/description/color/group_id/
    /// is_active/usage_count 等，M01-AUDIT-02 过滤）。
    pub fn tag_change(
        actor: &str,
        action: &str,
        tag_id: &str,
        before: Option<&Value>,
        after: &Value,
        reason: &str,
        policy_version: &str,
    ) -> Self {
        Self::user_action(actor, action)
            .with_target("tag", tag_id)
            .with_effective_role("administrator")
            .with_reason(reason)
            .with_policy_version(policy_version)
            .with_metadata(json!({
                "before": before.map(sanitize_for_audit).unwrap_or(Value::Null),
                "after": sanitize_for_audit(after),
            }))
    }
    pub fn feature_flag_change(
        actor: &str,
        flag: &str,
        before: bool,
        after: bool,
        reason: &str,
        policy_version: &str,
    ) -> Self {
        Self::user_action(actor, "admin.feature_flag_change")
            .with_target("feature_flag", flag)
            .with_effective_role("administrator")
            .with_reason(reason)
            .with_policy_version(policy_version)
            .with_metadata(json!({
                "before": json!({ "enabled": before }),
                "after": json!({ "enabled": after })
            }))
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

    /// 在业务事务内写入审计（M01-AUDIT-08）：与业务变更同一事务提交，
    /// 回滚时审计同步消失——保证"高风险操作无审计无法提交"。
    ///
    /// 与 [`crate::outbox::enqueue_in_tx`] 共用事务类型；调用方必须先
    /// `begin`，在提交前调用。
    pub async fn record_in_tx<'e>(
        self,
        tx: &mut crate::outbox::OutboxTx<'e>,
    ) -> Result<(), sqlx::Error> {
        let id = uuid::Uuid::now_v7().to_string();
        let now = now_millis();
        let metadata_json = self
            .metadata
            .as_ref()
            .map(|v| serde_json::to_string(v).unwrap_or_default());

        match tx {
            Either::Left(t) => {
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
                .execute(&mut **t)
                .await?;
            }
            Either::Right(t) => {
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
                .execute(&mut **t)
                .await?;
            }
        }

        tracing::debug!(audit_id = %id, action = %self.action, "audit log recorded in transaction");
        Ok(())
    }
}

/// 审计 before/after 字段白名单（M01-AUDIT-02）。
///
/// 只允许记录这些字段；白名单之外的字段一律丢弃（含 `content`/`body`/
/// `hidden_*` 等隐藏正文、密码/Token/Secret/完整签名 URL 字段）。
pub const AUDIT_FIELD_ALLOWLIST: &[&str] = &[
    "title",
    "content_excerpt",
    "visibility",
    "status",
    "role",
    "permission",
    "policy_version",
    "reason",
    "sanction",
    "duration_days",
    "mute_until",
    "points",
    "balance",
    "currency",
    "level",
    "board_id",
    "board_name",
    "category",
    "tags",
    "quota_bytes",
    "storage_bytes",
    "max_upload_bytes",
    "download_count",
    "ip_prefix",
    "session_id",
    "expires_at",
    // 板块管理变更（M03-BOARDS-05）：非敏感展示/排序/状态字段
    "id",
    "slug",
    "name",
    "description",
    "parent_id",
    "sort_order",
    "posting_mode",
    "is_active",
    "created_at",
    "updated_at",
    // 标签管理变更（M03-BOARDS-07）：非敏感展示/分组字段
    "color",
    "group_id",
    "usage_count",
];

/// 对 before/after 对象做审计安全过滤（M01-AUDIT-02）。
///
/// - 字段级 allowlist：非白名单字段（含隐藏正文、密码、Token、Secret 字段）
///   被丢弃；
/// - 值级脱敏：白名单字段的字符串若包含密码/Secret/Bearer/完整签名 URL/
///   token 形态，替换为 `[REDACTED]`。
pub fn sanitize_for_audit(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                if AUDIT_FIELD_ALLOWLIST.contains(&key.as_str()) {
                    out.insert(key.clone(), sanitize_value(val));
                }
            }
            Value::Object(out)
        }
        _ => Value::Null,
    }
}

/// 递归脱敏允许保留的值。
fn sanitize_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, val) in map {
                if AUDIT_FIELD_ALLOWLIST.contains(&key.as_str()) {
                    out.insert(key.clone(), sanitize_value(val));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(sanitize_value).collect()),
        Value::String(text) => {
            if string_forbidden_for_audit(text) {
                Value::String("[REDACTED]".to_owned())
            } else {
                Value::String(text.clone())
            }
        }
        other => other.clone(),
    }
}

/// 字符串是否包含审计禁止的敏感内容：密码/Secret/Bearer、完整签名 URL、
/// token 形态长随机串。
fn string_forbidden_for_audit(text: &str) -> bool {
    let lower = text.to_lowercase();
    lower.contains("password=")
        || lower.contains("secret")
        || lower.contains("bearer ")
        || lower.contains("x-amz-signature")
        || lower.contains("x-signature")
        || lower.contains("signature=")
        || crate::jobs::payload::contains_token_shape(text)
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

/// 审计查询游标（M01-AUDIT-09）。
///
/// 深分页稳定排序键：`created_at DESC, id DESC`。编码为
/// `base64url("created_at:id")`，避免调用方猜测内部字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditCursor {
    pub created_at: i64,
    pub id: String,
}

impl AuditCursor {
    pub fn new(created_at: i64, id: impl Into<String>) -> Self {
        Self {
            created_at,
            id: id.into(),
        }
    }

    pub fn encode(&self) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(format!("{}:{}", self.created_at, self.id))
    }

    /// 解码游标；格式非法返回 [`AuditCursorError`]。
    pub fn decode(encoded: &str) -> Result<Self, AuditCursorError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| AuditCursorError::Malformed)?;
        let text = String::from_utf8(bytes).map_err(|_| AuditCursorError::Malformed)?;
        let (created_at, id) = text.split_once(':').ok_or(AuditCursorError::Malformed)?;
        let created_at = created_at
            .parse::<i64>()
            .map_err(|_| AuditCursorError::Malformed)?;
        if id.is_empty() {
            return Err(AuditCursorError::Malformed);
        }
        Ok(Self {
            created_at,
            id: id.to_owned(),
        })
    }
}

/// 游标解码错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuditCursorError {
    Malformed,
}

impl std::fmt::Display for AuditCursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed audit cursor")
    }
}

impl std::error::Error for AuditCursorError {}

/// 审计分页结果（M01-AUDIT-09）。
#[derive(Debug, Clone)]
pub struct AuditPage {
    pub items: Vec<AuditLogRow>,
    /// 下一页游标；`None` 表示已到末尾。
    pub next_cursor: Option<AuditCursor>,
}

/// 仅授权管理员使用的游标分页查询（M01-AUDIT-09）。
///
/// - 深分页使用 `(created_at, id)` 游标，不用 OFFSET（避免深分页偏移放大）；
/// - 每次最多 `limit` 条（钳制 1..=200）；`after` 为空从最新开始；
/// - 支持按 `actor_id` / `action` 过滤；
/// - 导出边界：本函数只暴露受控分页，不提供全量转储；调用方（管理员路由）
///   负责鉴权与审计本次查询。
pub async fn list_audit_logs_cursor(
    pool: &DatabasePool,
    limit: i64,
    after: Option<&AuditCursor>,
    actor_id: Option<&str>,
    action: Option<&str>,
) -> Result<AuditPage, sqlx::Error> {
    let limit = limit.clamp(1, 200);
    let fetch = limit + 1; // 多取一条判断是否有下一页

    let mut conditions: Vec<String> = Vec::new();
    if let Some(_actor) = actor_id {
        conditions.push("actor_id = ?".to_owned());
    }
    if let Some(_act) = action {
        conditions.push("action = ?".to_owned());
    }
    if let Some(_cursor) = after {
        conditions.push("(created_at < ? OR (created_at = ? AND id < ?))".to_owned());
    }
    let where_clause = if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    };
    let sql = format!(
        "SELECT id, actor_id, effective_role, action, target_type, target_id, reason, policy_version, metadata, request_id, ip_address, created_at
         FROM audit_logs{where_clause}
         ORDER BY created_at DESC, id DESC LIMIT ?"
    );

    let rows: Vec<AuditLogRow> = match pool {
        Either::Left(p) => {
            let mut q = sqlx::query_as::<_, AuditLogRow>(&sql);
            if let Some(actor) = actor_id {
                q = q.bind(actor);
            }
            if let Some(act) = action {
                q = q.bind(act);
            }
            if let Some(cursor) = after {
                q = q
                    .bind(cursor.created_at)
                    .bind(cursor.created_at)
                    .bind(&cursor.id);
            }
            q.bind(fetch).fetch_all(p).await?
        }
        Either::Right(p) => {
            let mut q = sqlx::query_as::<_, AuditLogRow>(&sql);
            if let Some(actor) = actor_id {
                q = q.bind(actor);
            }
            if let Some(act) = action {
                q = q.bind(act);
            }
            if let Some(cursor) = after {
                q = q
                    .bind(cursor.created_at)
                    .bind(cursor.created_at)
                    .bind(&cursor.id);
            }
            q.bind(fetch).fetch_all(p).await?
        }
    };

    let has_more = rows.len() > limit as usize;
    let items: Vec<AuditLogRow> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        items
            .last()
            .map(|row| AuditCursor::new(row.created_at, &row.id))
    } else {
        None
    };

    Ok(AuditPage { items, next_cursor })
}

#[derive(sqlx::FromRow, Debug, Clone)]
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

    // ── M01-AUDIT-02：before/after 字段 allowlist ──────────────────────────

    #[test]
    fn allowlist_keeps_only_approved_fields() {
        let before = json!({
            "status": "active",
            "title": "旧标题",
            "password_hash": "abcf00d…",
            "content": "完整隐藏正文，禁止记录",
            "hidden_reason": "内部敏感",
            "points": 100,
            "role": "member"
        });
        let cleaned = sanitize_for_audit(&before);
        let obj = cleaned.as_object().unwrap();
        assert_eq!(obj.len(), 4, "只保留白名单字段");
        assert_eq!(obj["status"], "active");
        assert_eq!(obj["title"], "旧标题");
        assert_eq!(obj["points"], 100);
        assert_eq!(obj["role"], "member");
        for forbidden in ["password_hash", "content", "hidden_reason"] {
            assert!(!obj.contains_key(forbidden), "字段 {forbidden} 必须被丢弃");
        }
    }

    #[test]
    fn token_shaped_value_in_allowlisted_field_is_redacted() {
        let token = crate::auth::token::generate_token();
        let before = json!({
            "title": format!("重置链接 https://x/{token}")
        });
        let cleaned = sanitize_for_audit(&before);
        let obj = cleaned.as_object().unwrap();
        let title = obj["title"].as_str().unwrap();
        assert!(!title.contains(&token), "token 必须被脱敏");
        assert_eq!(title, "[REDACTED]");
    }

    #[test]
    fn secret_and_signed_url_values_are_redacted() {
        for value in [
            "password=hunter2",
            "client_secret=abc",
            "Bearer eyJhbGciOiJIUzI1NiJ9",
            "https://cdn.example.com/f?sig=X-Amz-Signature=deadbeef",
            "https://cdn.example.com/f?X-Signature=abc",
        ] {
            let before = json!({ "content_excerpt": value });
            let cleaned = sanitize_for_audit(&before);
            assert_eq!(
                cleaned["content_excerpt"], "[REDACTED]",
                "敏感值必须脱敏: {value}"
            );
        }
    }

    #[test]
    fn nested_objects_are_filtered_recursively() {
        let before = json!({
            "tags": ["ai", "forum"],
            "board_id": "b-1",
            "nested": {
                "title": "ok",
                "reset_token": "should-not-survive"
            }
        });
        let cleaned = sanitize_for_audit(&before);
        let obj = cleaned.as_object().unwrap();
        assert_eq!(obj["tags"], json!(["ai", "forum"]), "数组白名单字段保留");
        assert!(
            !obj.contains_key("nested"),
            "非白名单字段 nested 必须被丢弃"
        );
    }

    #[test]
    fn non_object_input_is_null() {
        assert_eq!(sanitize_for_audit(&Value::String("x".into())), Value::Null);
        assert_eq!(sanitize_for_audit(&Value::Null), Value::Null);
    }

    // ── M01-AUDIT-06：分类 helper ──────────────────────────────────────────

    #[test]
    fn delegated_admin_action_carries_effective_role_and_reason() {
        let entry = AuditEntry::delegated_admin_action(
            "admin-1",
            "moderator",
            "admin.ban_user",
            "user",
            "u-9",
            "代操作：按举报人工复核执行",
        );
        assert_eq!(entry.actor_id.as_deref(), Some("admin-1"));
        assert_eq!(entry.effective_role.as_deref(), Some("moderator"));
        assert_eq!(entry.reason.as_deref(), Some("代操作：按举报人工复核执行"));
        assert_eq!(entry.target_id.as_deref(), Some("u-9"));
    }

    #[test]
    fn permission_change_records_before_after_roles() {
        let entry = AuditEntry::permission_change(
            "admin-1",
            "u-9",
            "member",
            "moderator",
            "晋升",
            "v1.0.0-rc.2",
        );
        assert_eq!(entry.action, "admin.permission_change");
        let meta = entry.metadata.unwrap();
        assert_eq!(meta["role"], "moderator");
        assert_eq!(meta["before"]["role"], "member");
    }

    #[test]
    fn secret_change_never_takes_a_value() {
        let entry = AuditEntry::secret_change("admin-1", "smtp_password", "rotate");
        assert_eq!(entry.action, "secrets.rotate");
        assert_eq!(entry.target_id.as_deref(), Some("smtp_password"));
        // metadata 只含名称，无任何值字段
        let meta = entry.metadata.unwrap();
        assert_eq!(meta["secret_name"], "smtp_password");
        assert!(meta.get("value").is_none());
    }

    #[test]
    fn feature_flag_change_records_before_after_state() {
        let entry = AuditEntry::feature_flag_change(
            "admin-1",
            "ai_summary",
            false,
            true,
            "灰度开启",
            "v1.0.0-rc.2",
        );
        assert_eq!(entry.target_id.as_deref(), Some("ai_summary"));
        let meta = entry.metadata.unwrap();
        assert_eq!(meta["before"]["enabled"], false);
        assert_eq!(meta["after"]["enabled"], true);
    }

    #[test]
    fn accounting_change_records_amount_and_currency() {
        let entry =
            AuditEntry::accounting_change("admin-1", "ledger", "l-88", -500, "B", "手动修正");
        let meta = entry.metadata.unwrap();
        assert_eq!(meta["amount"], -500);
        assert_eq!(meta["currency"], "B");
        assert_eq!(entry.reason.as_deref(), Some("手动修正"));
    }

    #[test]
    fn config_change_sanitizes_before_after_values() {
        let before = json!({ "max_upload_bytes": 10_485_760, "password": "hunter2" });
        let after = json!({ "max_upload_bytes": 20_971_520 });
        let entry = AuditEntry::config_change(
            "admin-1",
            "storage.max_upload_bytes",
            Some(&before),
            Some(&after),
            "提升配额",
            "v1.0.0-rc.2",
        );
        let meta = entry.metadata.unwrap();
        assert_eq!(meta["before"]["max_upload_bytes"], 10_485_760);
        assert!(
            meta["before"].get("password").is_none(),
            "配置变更不得记录密码"
        );
        assert_eq!(meta["after"]["max_upload_bytes"], 20_971_520);
    }

    // ── M01-AUDIT-09：游标分页 ─────────────────────────────────────────────

    #[test]
    fn audit_cursor_round_trips() {
        let cursor = AuditCursor::new(1_700_000_000_000, "audit-123");
        assert_eq!(AuditCursor::decode(&cursor.encode()), Ok(cursor));
        let cursor = AuditCursor::new(-5, "id-with-dashes");
        assert_eq!(AuditCursor::decode(&cursor.encode()), Ok(cursor));
    }

    #[test]
    fn audit_cursor_rejects_malformed_input() {
        assert_eq!(
            AuditCursor::decode("not!base64"),
            Err(AuditCursorError::Malformed)
        );
        assert_eq!(AuditCursor::decode(""), Err(AuditCursorError::Malformed));
        // base64 合法但内容无 "created_at:id" 结构
        assert_eq!(
            AuditCursor::decode("YWJj"), // "abc"
            Err(AuditCursorError::Malformed)
        );
        assert_eq!(
            AuditCursor::decode(&base64_url("1:")), // 空 id
            Err(AuditCursorError::Malformed)
        );
    }

    fn base64_url(text: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(text)
    }
}
