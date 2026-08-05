//! M03-PROFILE-03：本人资料读取与更新服务。
//!
//! 存储映射：
//! - 昵称 `display_name` / 简介 `bio` / 签名 `signature` → `users`；
//! - 时区 `timezone` / 主题 `theme_name` → `user_preferences`
//!   （行首访惰性创建，读取时缺失返回默认值）；
//! - 隐私 `email_visible_to` / `profile_visible_to` → `user_privacy`（同上）；
//! - 每次资料写操作在 `profile_revisions` 追加一条修订（SCHEMA-01 契约：
//!   资料写操作同事务写 revision）。
//!
//! PATCH 语义：只更新出现的字段（COALESCE），缺失字段保持原值。

use sqlx::Either;

use crate::auth::session::SessionUser;
use crate::db::DatabasePool;
use crate::outbox::now_millis;

/// 本人资料读取投影（users + user_preferences + user_privacy）。
#[derive(Debug, Clone)]
pub struct ProfileFields {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub signature: Option<String>,
    pub timezone: String,
    pub theme_name: Option<String>,
    pub email_visible_to: String,
    pub profile_visible_to: String,
}

impl Default for ProfileFields {
    fn default() -> Self {
        Self {
            display_name: None,
            bio: None,
            signature: None,
            timezone: "UTC".to_string(),
            theme_name: None,
            email_visible_to: "nobody".to_string(),
            profile_visible_to: "everyone".to_string(),
        }
    }
}

/// 资料更新请求（全部可空 = 只更新出现字段）。
#[derive(Debug, Clone, Default)]
pub struct ProfileUpdate {
    pub display_name: Option<String>,
    pub bio: Option<String>,
    pub signature: Option<String>,
    pub timezone: Option<String>,
    pub theme_name: Option<String>,
    pub email_visible_to: Option<String>,
    pub profile_visible_to: Option<String>,
}

impl ProfileUpdate {
    /// 基础校验：长度与枚举（Unicode/富文本禁用等细化见 M03-PROFILE-04）。
    pub fn validate(&self) -> Result<(), String> {
        if let Some(v) = &self.display_name {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                return Err("display_name 不能为空".to_string());
            }
            if trimmed.chars().count() > 80 {
                return Err("display_name 长度不能超过 80".to_string());
            }
        }
        if let Some(v) = &self.bio {
            if v.chars().count() > 2000 {
                return Err("bio 长度不能超过 2000".to_string());
            }
        }
        if let Some(v) = &self.signature {
            if v.chars().count() > 500 {
                return Err("signature 长度不能超过 500".to_string());
            }
        }
        if let Some(v) = &self.timezone {
            if v.trim().is_empty() {
                return Err("timezone 不能为空".to_string());
            }
            if v.chars().count() > 64 {
                return Err("timezone 长度不能超过 64".to_string());
            }
        }
        if let Some(v) = &self.theme_name {
            if !matches!(v.as_str(), "default" | "dark" | "light") {
                return Err("theme 必须是 default/dark/light 之一".to_string());
            }
        }
        for (name, v) in [
            ("email_visible_to", &self.email_visible_to),
            ("profile_visible_to", &self.profile_visible_to),
        ] {
            if let Some(v) = v {
                if !matches!(v.as_str(), "everyone" | "registered" | "nobody") {
                    return Err(format!("{name} 必须是 everyone/registered/nobody 之一"));
                }
            }
        }
        Ok(())
    }

    /// 被修改的字段列表（用于 revision 摘要；全空 = 无变更不写修订）。
    fn changed_fields(&self) -> Vec<&'static str> {
        let mut changed = Vec::new();
        if self.display_name.is_some() {
            changed.push("display_name");
        }
        if self.bio.is_some() {
            changed.push("bio");
        }
        if self.signature.is_some() {
            changed.push("signature");
        }
        if self.timezone.is_some() {
            changed.push("timezone");
        }
        if self.theme_name.is_some() {
            changed.push("theme_name");
        }
        if self.email_visible_to.is_some() {
            changed.push("email_visible_to");
        }
        if self.profile_visible_to.is_some() {
            changed.push("profile_visible_to");
        }
        changed
    }
}

/// 读取本人资料字段（行缺失时返回默认值，不建行——惰性创建发生在写）。
/// `display_name` 从 users 表读取（会话缓存的昵称在 PATCH 后会过期）。
pub async fn load_profile_fields(
    pool: &DatabasePool,
    user: &SessionUser,
) -> Result<ProfileFields, String> {
    let mut fields = ProfileFields::default();
    match pool {
        Either::Left(p) => {
            let row: Option<(Option<String>, Option<String>, Option<String>)> =
                sqlx::query_as("SELECT display_name, bio, signature FROM users WHERE id = ?")
                    .bind(&user.id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            if let Some((display_name, bio, signature)) = row {
                fields.display_name = display_name;
                fields.bio = bio;
                fields.signature = signature;
            }
            let pref: Option<(String, Option<String>)> = sqlx::query_as(
                "SELECT timezone, theme_name FROM user_preferences WHERE user_id = ?",
            )
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            if let Some((timezone, theme_name)) = pref {
                fields.timezone = timezone;
                fields.theme_name = theme_name;
            }
            let privacy: Option<(String, String)> = sqlx::query_as(
                "SELECT email_visible_to, profile_visible_to FROM user_privacy WHERE user_id = ?",
            )
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            if let Some((email_visible_to, profile_visible_to)) = privacy {
                fields.email_visible_to = email_visible_to;
                fields.profile_visible_to = profile_visible_to;
            }
        }
        Either::Right(p) => {
            let row: Option<(Option<String>, Option<String>, Option<String>)> =
                sqlx::query_as("SELECT display_name, bio, signature FROM users WHERE id = ?")
                    .bind(&user.id)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| e.to_string())?;
            if let Some((display_name, bio, signature)) = row {
                fields.display_name = display_name;
                fields.bio = bio;
                fields.signature = signature;
            }
            let pref: Option<(String, Option<String>)> = sqlx::query_as(
                "SELECT timezone, theme_name FROM user_preferences WHERE user_id = ?",
            )
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            if let Some((timezone, theme_name)) = pref {
                fields.timezone = timezone;
                fields.theme_name = theme_name;
            }
            let privacy: Option<(String, String)> = sqlx::query_as(
                "SELECT email_visible_to, profile_visible_to FROM user_privacy WHERE user_id = ?",
            )
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            if let Some((email_visible_to, profile_visible_to)) = privacy {
                fields.email_visible_to = email_visible_to;
                fields.profile_visible_to = profile_visible_to;
            }
        }
    }
    Ok(fields)
}

/// 单事务更新资料：users + user_preferences + user_privacy + profile_revisions。
pub async fn update_profile(
    pool: &DatabasePool,
    user_id: &str,
    update: ProfileUpdate,
) -> Result<(), String> {
    let changed = update.changed_fields();
    if changed.is_empty() {
        return Ok(()); // 无变更：不写库、不写修订
    }

    let now = now_millis();
    let mut tx = match pool {
        Either::Left(p) => Either::Left(p.begin().await.map_err(|e| e.to_string())?),
        Either::Right(p) => Either::Right(p.begin().await.map_err(|e| e.to_string())?),
    };

    // 1. users：display_name/bio/signature（COALESCE 保持缺失字段原值）
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "UPDATE users
                 SET display_name = COALESCE(?, display_name),
                     bio = COALESCE(?, bio),
                     signature = COALESCE(?, signature),
                     updated_at = ?
                 WHERE id = ?",
            )
            .bind(&update.display_name)
            .bind(&update.bio)
            .bind(&update.signature)
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(t) => {
            sqlx::query(
                "UPDATE users
                 SET display_name = COALESCE(?, display_name),
                     bio = COALESCE(?, bio),
                     signature = COALESCE(?, signature),
                     updated_at = ?
                 WHERE id = ?",
            )
            .bind(&update.display_name)
            .bind(&update.bio)
            .bind(&update.signature)
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    // 2. user_preferences 惰性创建 + 更新
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_preferences (user_id, timezone, locale, updated_at)
                 VALUES (?, 'UTC', 'zh-CN', ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE user_preferences
                 SET timezone = COALESCE(?, timezone),
                     theme_name = COALESCE(?, theme_name),
                     updated_at = ?
                 WHERE user_id = ?",
            )
            .bind(&update.timezone)
            .bind(&update.theme_name)
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT IGNORE INTO user_preferences (user_id, timezone, locale, updated_at)
                 VALUES (?, 'UTC', 'zh-CN', ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE user_preferences
                 SET timezone = COALESCE(?, timezone),
                     theme_name = COALESCE(?, theme_name),
                     updated_at = ?
                 WHERE user_id = ?",
            )
            .bind(&update.timezone)
            .bind(&update.theme_name)
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    // 3. user_privacy 惰性创建 + 更新
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT OR IGNORE INTO user_privacy (user_id, email_visible_to, profile_visible_to, updated_at)
                 VALUES (?, 'nobody', 'everyone', ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE user_privacy
                 SET email_visible_to = COALESCE(?, email_visible_to),
                     profile_visible_to = COALESCE(?, profile_visible_to),
                     updated_at = ?
                 WHERE user_id = ?",
            )
            .bind(&update.email_visible_to)
            .bind(&update.profile_visible_to)
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT IGNORE INTO user_privacy (user_id, email_visible_to, profile_visible_to, updated_at)
                 VALUES (?, 'nobody', 'everyone', ?)",
            )
            .bind(user_id)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE user_privacy
                 SET email_visible_to = COALESCE(?, email_visible_to),
                     profile_visible_to = COALESCE(?, profile_visible_to),
                     updated_at = ?
                 WHERE user_id = ?",
            )
            .bind(&update.email_visible_to)
            .bind(&update.profile_visible_to)
            .bind(now)
            .bind(user_id)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    // 4. profile_revisions：下一修订号 + 变更摘要（SCHEMA-01 契约）
    let next_revision: i64 = match &mut tx {
        Either::Left(t) => sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM profile_revisions WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_one(&mut **t)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(t) => sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM profile_revisions WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_one(&mut **t)
        .await
        .map_err(|e| e.to_string())?,
    };
    let changes_json = format!(
        "{{\"fields\":[{}]}}",
        changed
            .iter()
            .map(|f| format!("\"{f}\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    let revision_id = uuid::Uuid::now_v7().to_string();
    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO profile_revisions (id, user_id, revision, changes_json, actor_user_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&revision_id)
            .bind(user_id)
            .bind(next_revision)
            .bind(&changes_json)
            .bind(user_id)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO profile_revisions (id, user_id, revision, changes_json, actor_user_id, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(&revision_id)
            .bind(user_id)
            .bind(next_revision)
            .bind(&changes_json)
            .bind(user_id)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| e.to_string())?;
        }
    }

    match tx {
        Either::Left(t) => t.commit().await.map_err(|e| e.to_string()),
        Either::Right(t) => t.commit().await.map_err(|e| e.to_string()),
    }
}
