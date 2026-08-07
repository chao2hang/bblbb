//! M05-NOTIFY：站内通知服务（创建/去重/列表/已读/偏好/权限复查）。
//!
//! - [`create_notification`]（M05-NOTIFY-02/05）：只存资源 ID 与安全模板参数，
//!   不复制隐藏正文或内部 note；以 (收件人, 模板, 资源) 建立去重键，
//!   `INSERT OR IGNORE`/`INSERT IGNORE` 幂等——重放 Outbox 不重复通知。
//! - [`list_notifications`]/[`mark_read`]/[`mark_all_read`]/[`unread_count`]
//!   （M05-NOTIFY-03）：游标分页、单条/批量已读与未读计数。
//! - [`get_preferences`]/[`set_preference`]（M05-NOTIFY-04）：类别偏好；
//!   security 类别不可被普通偏好全关（模型 CHECK 镜像）。
//! - [`project_list`]（M05-NOTIFY-06）：读取时重新检查目标权限，资源
//!   隐藏/删除后只显示安全失效状态，不泄漏标题/正文。

use std::collections::HashMap;

use serde_json::{json, Value};
use sqlx::Either;

use crate::db::DatabasePool;
use crate::moderation::model::SanctionKind;
use crate::notifications::model::{Notification, NotificationCategory, NotificationPreference};
use crate::notifications::templates::{is_known_template, render, validate_params, TemplateKey};
use crate::outbox::now_millis;

/// 通知服务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotifyError {
    Db(String),
    Invalid(String),
}

impl From<sqlx::Error> for NotifyError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for NotifyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "notifications db error: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid notification: {msg}"),
        }
    }
}

impl std::error::Error for NotifyError {}

/// 通知创建结果。
#[derive(Debug, Clone)]
pub struct CreateNotificationResult {
    pub notification: Notification,
    /// false = 命中去重键未插入（重放幂等）。
    pub inserted: bool,
}

/// 通知创建输入（M05-NOTIFY-02）。
#[derive(Debug, Clone)]
pub struct CreateNotificationInput {
    pub user_id: String,
    pub category: NotificationCategory,
    pub template_key: TemplateKey,
    /// 遗留通知类型；None 时由模板键推导。
    pub r#type: Option<String>,
    pub resource_type: Option<String>,
    pub resource_id: Option<String>,
    /// 安全模板参数（白名单标量；禁止隐藏正文/内部 note）。
    pub params: serde_json::Map<String, Value>,
}

/// 创建通知（M05-NOTIFY-02/05）。
///
/// - 模板键必须注册；参数必须通过 [`validate_params`]（无 body/content/note 等）；
/// - 去重键 = `{user_id}|{template_key}|{resource_type}|{resource_id}`，
///   配合 `UNIQUE(user_id, delivery_dedup_key)` 幂等插入。
pub async fn create_notification(
    pool: &DatabasePool,
    input: CreateNotificationInput,
    now: i64,
) -> Result<CreateNotificationResult, NotifyError> {
    if !is_known_template(input.template_key.as_str()) {
        return Err(NotifyError::Invalid(
            "unknown notification template key".to_string(),
        ));
    }
    validate_params(&input.params).map_err(NotifyError::Invalid)?;

    let rendered = render(input.template_key, &input.params);
    let resource_type = input.resource_type.as_deref().unwrap_or("");
    let resource_id = input.resource_id.as_deref().unwrap_or("");
    let dedup_key = if resource_type.is_empty() && resource_id.is_empty() {
        None
    } else {
        Some(format!(
            "{}|{}|{}|{}",
            input.user_id,
            input.template_key.as_str(),
            resource_type,
            resource_id
        ))
    };

    let notification = Notification {
        id: uuid::Uuid::now_v7().to_string(),
        user_id: input.user_id,
        r#type: input
            .r#type
            .unwrap_or_else(|| input.template_key.legacy_type().to_string()),
        title: rendered.title,
        body: rendered.body,
        link: if resource_type.is_empty() {
            None
        } else {
            Some(format!("/{}s/{}", resource_type, resource_id))
        },
        is_read: false,
        created_at: now,
        read_at: None,
        security_kind: if input.category.is_security() {
            Some(input.template_key.as_str().to_string())
        } else {
            None
        },
        template_key: Some(input.template_key.as_str().to_string()),
        resource_type: input.resource_type,
        resource_id: input.resource_id,
        delivery_dedup_key: dedup_key,
        category: input.category,
    };

    let inserted = match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO notifications
                     (id, user_id, type, title, body, link, is_read, created_at, read_at,
                      security_kind, template_key, resource_type, resource_id, delivery_dedup_key, category)
                 VALUES (?, ?, ?, ?, ?, ?, 0, ?, NULL, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&notification.id)
            .bind(&notification.user_id)
            .bind(&notification.r#type)
            .bind(&notification.title)
            .bind(&notification.body)
            .bind(&notification.link)
            .bind(now)
            .bind(&notification.security_kind)
            .bind(&notification.template_key)
            .bind(&notification.resource_type)
            .bind(&notification.resource_id)
            .bind(&notification.delivery_dedup_key)
            .bind(notification.category.as_str())
            .execute(p)
            .await?
            .rows_affected()
                == 1
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO notifications
                     (id, user_id, type, title, body, link, is_read, created_at, read_at,
                      security_kind, template_key, resource_type, resource_id, delivery_dedup_key, category)
                 VALUES (?, ?, ?, ?, ?, ?, 0, ?, NULL, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&notification.id)
            .bind(&notification.user_id)
            .bind(&notification.r#type)
            .bind(&notification.title)
            .bind(&notification.body)
            .bind(&notification.link)
            .bind(now)
            .bind(&notification.security_kind)
            .bind(&notification.template_key)
            .bind(&notification.resource_type)
            .bind(&notification.resource_id)
            .bind(&notification.delivery_dedup_key)
            .bind(notification.category.as_str())
            .execute(p)
            .await?
            .rows_affected()
                == 1
        }
    };

    Ok(CreateNotificationResult {
        notification,
        inserted,
    })
}

/// 站内通知游标列表（M05-NOTIFY-03）。
///
/// 游标为通知 id（UUIDv7，字典序即时间序）；返回 `limit` 条更新的通知，
/// `has_more` 表示还有更早的。
pub async fn list_notifications(
    pool: &DatabasePool,
    user_id: &str,
    limit: i64,
    unread_only: bool,
    cursor: Option<&str>,
) -> Result<(Vec<Notification>, bool), NotifyError> {
    let limit = limit.clamp(1, 50);
    let fetch = limit + 1;
    let rows: Vec<NotificationRow> = match pool {
        Either::Left(p) => {
            let base = if unread_only {
                "SELECT id, user_id, type, title, body, link, is_read, created_at, read_at,
                        security_kind, template_key, resource_type, resource_id, delivery_dedup_key, category
                 FROM notifications WHERE user_id = ? AND is_read = 0"
            } else {
                "SELECT id, user_id, type, title, body, link, is_read, created_at, read_at,
                        security_kind, template_key, resource_type, resource_id, delivery_dedup_key, category
                 FROM notifications WHERE user_id = ?"
            };
            let mut sql = base.to_string();
            if cursor.is_some() {
                sql.push_str(" AND id < ?");
            }
            sql.push_str(" ORDER BY id DESC LIMIT ?");
            let mut q = sqlx::query_as::<_, NotificationRow>(&sql).bind(user_id);
            if let Some(c) = cursor {
                q = q.bind(c);
            }
            q.bind(fetch).fetch_all(p).await?
        }
        Either::Right(p) => {
            let base = if unread_only {
                "SELECT id, user_id, type, title, body, link, is_read, created_at, read_at,
                        security_kind, template_key, resource_type, resource_id, delivery_dedup_key, category
                 FROM notifications WHERE user_id = ? AND is_read = 0"
            } else {
                "SELECT id, user_id, type, title, body, link, is_read, created_at, read_at,
                        security_kind, template_key, resource_type, resource_id, delivery_dedup_key, category
                 FROM notifications WHERE user_id = ?"
            };
            let mut sql = base.to_string();
            if cursor.is_some() {
                sql.push_str(" AND id < ?");
            }
            sql.push_str(" ORDER BY id DESC LIMIT ?");
            let mut q = sqlx::query_as::<_, NotificationRow>(&sql).bind(user_id);
            if let Some(c) = cursor {
                q = q.bind(c);
            }
            q.bind(fetch).fetch_all(p).await?
        }
    };
    let has_more = rows.len() as i64 > limit;
    let items = rows
        .into_iter()
        .take(limit as usize)
        .map(NotificationRow::into_model)
        .collect();
    Ok((items, has_more))
}

/// 单条已读（M05-NOTIFY-03）：仅本人通知；返回是否命中。
pub async fn mark_read(
    pool: &DatabasePool,
    user_id: &str,
    notification_id: &str,
    now: i64,
) -> Result<bool, NotifyError> {
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE notifications SET is_read = 1, read_at = ? WHERE id = ? AND user_id = ? AND is_read = 0",
        )
        .bind(now)
        .bind(notification_id)
        .bind(user_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE notifications SET is_read = 1, read_at = ? WHERE id = ? AND user_id = ? AND is_read = 0",
        )
        .bind(now)
        .bind(notification_id)
        .bind(user_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(affected > 0)
}

/// 批量已读（M05-NOTIFY-03）：返回更新的条数。
pub async fn mark_all_read(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<i64, NotifyError> {
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE notifications SET is_read = 1, read_at = ? WHERE user_id = ? AND is_read = 0",
        )
        .bind(now)
        .bind(user_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE notifications SET is_read = 1, read_at = ? WHERE user_id = ? AND is_read = 0",
        )
        .bind(now)
        .bind(user_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    Ok(affected as i64)
}

/// 未读计数（M05-NOTIFY-03）。
pub async fn unread_count(pool: &DatabasePool, user_id: &str) -> Result<i64, NotifyError> {
    let count: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND is_read = 0",
            )
            .bind(user_id)
            .fetch_one(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND is_read = 0",
            )
            .bind(user_id)
            .fetch_one(p)
            .await?
        }
    };
    Ok(count)
}

/// 读取偏好（M05-NOTIFY-04）：缺行类别按全渠道开启默认。
pub async fn get_preferences(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Vec<NotificationPreference>, NotifyError> {
    let rows: Vec<PrefRow> =
        match pool {
            Either::Left(p) => sqlx::query_as::<_, PrefRow>(
                "SELECT user_id, category, email_enabled, in_app_enabled, push_enabled, updated_at
             FROM notification_preferences WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_all(p)
            .await?,
            Either::Right(p) => sqlx::query_as::<_, PrefRow>(
                "SELECT user_id, category, email_enabled, in_app_enabled, push_enabled, updated_at
             FROM notification_preferences WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_all(p)
            .await?,
        };
    let mut found = HashMap::new();
    for row in rows {
        let category =
            NotificationCategory::parse(&row.category).unwrap_or(NotificationCategory::System);
        found.insert(category, row.into_model());
    }
    let mut prefs = Vec::new();
    for category in NotificationCategory::ALL {
        prefs.push(found.remove(&category).unwrap_or(NotificationPreference {
            user_id: user_id.to_string(),
            category,
            email_enabled: true,
            in_app_enabled: true,
            push_enabled: true,
            updated_at: 0,
        }));
    }
    Ok(prefs)
}

/// 更新类别偏好（M05-NOTIFY-04）。
///
/// security 类别不可被普通偏好全关（`NotificationPreference::validate` +
/// 0045 CHECK 双保险）。
pub async fn set_preference(
    pool: &DatabasePool,
    user_id: &str,
    category: NotificationCategory,
    email_enabled: bool,
    in_app_enabled: bool,
    push_enabled: bool,
    now: i64,
) -> Result<(), NotifyError> {
    NotificationPreference::validate(category, email_enabled, in_app_enabled, push_enabled)
        .map_err(NotifyError::Invalid)?;
    let (e, i, p) = (
        i64::from(email_enabled),
        i64::from(in_app_enabled),
        i64::from(push_enabled),
    );
    match pool {
        Either::Left(pool) => {
            sqlx::query(
                "INSERT INTO notification_preferences (user_id, category, email_enabled, in_app_enabled, push_enabled, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON CONFLICT(user_id, category) DO UPDATE SET
                    email_enabled = excluded.email_enabled,
                    in_app_enabled = excluded.in_app_enabled,
                    push_enabled = excluded.push_enabled,
                    updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(category.as_str())
            .bind(e)
            .bind(i)
            .bind(p)
            .bind(now)
            .execute(pool)
            .await?;
        }
        Either::Right(pool) => {
            sqlx::query(
                "INSERT INTO notification_preferences (user_id, category, email_enabled, in_app_enabled, push_enabled, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE
                    email_enabled = VALUES(email_enabled),
                    in_app_enabled = VALUES(in_app_enabled),
                    push_enabled = VALUES(push_enabled),
                    updated_at = VALUES(updated_at)",
            )
            .bind(user_id)
            .bind(category.as_str())
            .bind(e)
            .bind(i)
            .bind(p)
            .bind(now)
            .execute(pool)
            .await?;
        }
    }
    Ok(())
}

/// 读取时权限复查（M05-NOTIFY-06）：资源隐藏/删除后只显示安全失效状态。
///
/// 目前复查 `post` 类型（隐藏/删除）；其余类型（本人拥有的处罚/申诉等）
/// 不额外隐藏。
pub async fn project_list(
    pool: &DatabasePool,
    items: Vec<Notification>,
) -> Result<Vec<Value>, NotifyError> {
    let post_ids: Vec<String> = items
        .iter()
        .filter(|n| n.resource_type.as_deref() == Some("post"))
        .filter_map(|n| n.resource_id.clone())
        .collect();
    let mut unavailable: HashMap<String, bool> = HashMap::new();
    if !post_ids.is_empty() {
        let rows: Vec<(String, String)> = match pool {
            Either::Left(p) => {
                // 动态占位符（SQLite 也支持 ?）
                let placeholders = vec!["?"; post_ids.len()].join(",");
                let sql = format!("SELECT id, status FROM posts WHERE id IN ({placeholders})");
                let mut q = sqlx::query_as::<_, (String, String)>(&sql);
                for id in &post_ids {
                    q = q.bind(id);
                }
                q.fetch_all(p).await?
            }
            Either::Right(p) => {
                let placeholders = vec!["?"; post_ids.len()].join(",");
                let sql = format!("SELECT id, status FROM posts WHERE id IN ({placeholders})");
                let mut q = sqlx::query_as::<_, (String, String)>(&sql);
                for id in &post_ids {
                    q = q.bind(id);
                }
                q.fetch_all(p).await?
            }
        };
        for (id, status) in rows {
            let blocked = status == "hidden" || status == "deleted";
            unavailable.insert(id, blocked);
        }
    }

    Ok(items
        .into_iter()
        .map(|n| {
            let is_post = n.resource_type.as_deref() == Some("post");
            let hidden = is_post
                && n.resource_id
                    .as_ref()
                    .map(|id| unavailable.get(id).copied().unwrap_or(false))
                    .unwrap_or(false);
            if hidden {
                // 安全失效状态：不泄漏原标题/正文/链接。
                json!({
                    "id": n.id,
                    "category": n.category.as_str(),
                    "template_key": n.template_key,
                    "is_read": n.is_read,
                    "created_at": n.created_at,
                    "read_at": n.read_at,
                    "title": "内容不可用",
                    "body": "相关内容已被隐藏或删除",
                    "link": null,
                    "unavailable": true,
                })
            } else {
                json!({
                    "id": n.id,
                    "category": n.category.as_str(),
                    "template_key": n.template_key,
                    "type": n.r#type,
                    "title": n.title,
                    "body": n.body,
                    "link": n.link,
                    "is_read": n.is_read,
                    "created_at": n.created_at,
                    "read_at": n.read_at,
                    "unavailable": false,
                })
            }
        })
        .collect())
}

/// 供路由使用的助手：当前 Unix 毫秒。
pub fn now() -> i64 {
    now_millis()
}

/// 处罚 kind 中文标签（安全模板参数；不引用内部依据）。
pub fn kind_label(kind: SanctionKind) -> &'static str {
    match kind {
        SanctionKind::Warning => "警告",
        SanctionKind::RateLimit => "限流",
        SanctionKind::Mute => "禁言",
        SanctionKind::BoardMute => "板块禁言",
        SanctionKind::Ban => "封禁",
    }
}

/// 通知行 → 模型。
#[derive(sqlx::FromRow)]
struct NotificationRow {
    id: String,
    user_id: String,
    r#type: String,
    title: String,
    body: Option<String>,
    link: Option<String>,
    is_read: i64,
    created_at: i64,
    read_at: Option<i64>,
    security_kind: Option<String>,
    template_key: Option<String>,
    resource_type: Option<String>,
    resource_id: Option<String>,
    delivery_dedup_key: Option<String>,
    category: String,
}

impl NotificationRow {
    fn into_model(self) -> Notification {
        Notification {
            id: self.id,
            user_id: self.user_id,
            r#type: self.r#type,
            title: self.title,
            body: self.body,
            link: self.link,
            is_read: self.is_read != 0,
            created_at: self.created_at,
            read_at: self.read_at,
            security_kind: self.security_kind,
            template_key: self.template_key,
            resource_type: self.resource_type,
            resource_id: self.resource_id,
            delivery_dedup_key: self.delivery_dedup_key,
            category: NotificationCategory::parse(&self.category)
                .unwrap_or(NotificationCategory::System),
        }
    }
}

/// 偏好行 → 模型。
#[derive(sqlx::FromRow)]
struct PrefRow {
    user_id: String,
    category: String,
    email_enabled: i64,
    in_app_enabled: i64,
    push_enabled: i64,
    updated_at: i64,
}

impl PrefRow {
    fn into_model(self) -> NotificationPreference {
        NotificationPreference {
            user_id: self.user_id,
            category: NotificationCategory::parse(&self.category)
                .unwrap_or(NotificationCategory::System),
            email_enabled: self.email_enabled != 0,
            in_app_enabled: self.in_app_enabled != 0,
            push_enabled: self.push_enabled != 0,
            updated_at: self.updated_at,
        }
    }
}
