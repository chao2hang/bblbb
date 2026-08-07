//! M05-SCHEMA：notifications 数据模型与约束校验（纯数据/约束层，无路由）。
//!
//! 覆盖迁移 0045 对应的行结构、枚举与规则：
//! - 通知类别枚举（activity/moderation/system/security/digest）与
//!   `notifications.category` CHECK 一致；
//! - 投递去重键构造（delivery_dedup_key，表级
//!   `UNIQUE(user_id, delivery_dedup_key)` 兜底，NULL 不去重）；
//! - 「安全通知不可被普通偏好全关」：security 类别至少保留一个投递渠道
//!   （`notification_preferences` 表 CHECK 的应用层镜像）。

/// 通知类别（notifications.category / notification_preferences.category，0045）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationCategory {
    Activity,
    Moderation,
    System,
    Security,
    Digest,
}

impl NotificationCategory {
    /// 全部合法取值（与 0045 CHECK 一致）。
    pub const ALL: [NotificationCategory; 5] = [
        NotificationCategory::Activity,
        NotificationCategory::Moderation,
        NotificationCategory::System,
        NotificationCategory::Security,
        NotificationCategory::Digest,
    ];

    /// 数据库字面值。
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationCategory::Activity => "activity",
            NotificationCategory::Moderation => "moderation",
            NotificationCategory::System => "system",
            NotificationCategory::Security => "security",
            NotificationCategory::Digest => "digest",
        }
    }

    /// 从数据库字面值解析；非法值返回 `None`。
    pub fn parse(s: &str) -> Option<NotificationCategory> {
        Self::ALL.into_iter().find(|v| v.as_str() == s)
    }

    /// 安全通知类别：不可被普通偏好全关（M05-NOTIFY 强制）。
    pub fn is_security(self) -> bool {
        matches!(self, NotificationCategory::Security)
    }
}

/// 通知投递渠道（notification_preferences 的 channel 开关，0045）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NotificationChannel {
    Email,
    InApp,
    Push,
}

impl NotificationChannel {
    /// 全部渠道（对应 email_enabled/in_app_enabled/push_enabled 列）。
    pub const ALL: [NotificationChannel; 3] = [
        NotificationChannel::Email,
        NotificationChannel::InApp,
        NotificationChannel::Push,
    ];

    /// 数据库字面值（用于日志/调试）。
    pub fn as_str(self) -> &'static str {
        match self {
            NotificationChannel::Email => "email",
            NotificationChannel::InApp => "in_app",
            NotificationChannel::Push => "push",
        }
    }
}

/// 站内通知（notifications 表，0004 建表 + 0045 扩展列）。
///
/// `delivery_dedup_key` 非空时参与 `UNIQUE(user_id, delivery_dedup_key)`
/// 去重；`NULL` 不去重（三库一致）。`category` 与遗留 `type` 枚举正交；
/// 安全通知仍由 `security_kind` 标记具体安全事件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub id: String,
    pub user_id: String,
    /// 遗留通知类型（`system`/`reply`/`mention`/`reaction`/`moderation`/
    /// `badge`/`digest`，0004 CHECK）。
    pub r#type: String,
    pub title: String,
    pub body: Option<String>,
    pub link: Option<String>,
    pub is_read: bool,
    pub created_at: i64,
    pub read_at: Option<i64>,
    pub security_kind: Option<String>,
    pub template_key: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    pub delivery_dedup_key: Option<String>,
    pub category: NotificationCategory,
}

impl Notification {
    /// 投递去重键：同一 (user_id, resource_type, resource_id) 在去重窗口内
    /// 只保留一条通知。与 `report_dedup_key` 同手法：折叠为单列参与 UNIQUE。
    pub fn build_delivery_dedup_key(
        user_id: &str,
        resource_type: &str,
        resource_id: &str,
    ) -> String {
        format!("{user_id}|{resource_type}|{resource_id}")
    }
}

/// 通知偏好行（notification_preferences 表，0045）。每用户每类别一条
/// （PRIMARY KEY(user_id, category)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotificationPreference {
    pub user_id: String,
    pub category: NotificationCategory,
    pub email_enabled: bool,
    pub in_app_enabled: bool,
    pub push_enabled: bool,
    pub updated_at: i64,
}

impl NotificationPreference {
    /// 「安全通知不可被普通偏好全关」校验（0045 CHECK 的应用层镜像）：
    /// security 类别必须至少保留一个投递渠道；其余类别允许全关。
    pub fn validate(
        category: NotificationCategory,
        email_enabled: bool,
        in_app_enabled: bool,
        push_enabled: bool,
    ) -> Result<(), String> {
        if category.is_security() && !email_enabled && !in_app_enabled && !push_enabled {
            return Err("安全通知不可被普通偏好全关：至少保留一个投递渠道".to_string());
        }
        Ok(())
    }

    /// 本行是否所有渠道都已关闭（security 类别会被 0045 CHECK 拒绝）。
    pub fn is_category_fully_disabled(&self) -> bool {
        !self.email_enabled && !self.in_app_enabled && !self.push_enabled
    }
}
