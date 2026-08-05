//! M03-PROFILE-03/04：本人资料读取与更新服务。
//!
//! 存储映射：
//! - 昵称 `display_name` / 简介 `bio` / 签名 `signature` / 乐观并发版本
//!   `version`（0026）→ `users`；
//! - 时区 `timezone` / 主题 `theme_name` → `user_preferences`
//!   （行首访惰性创建，读取时缺失返回默认值）；
//! - 隐私 `email_visible_to` / `profile_visible_to` → `user_privacy`（同上）；
//! - 每次资料写操作在 `profile_revisions` 追加一条修订（SCHEMA-01 契约：
//!   资料写操作同事务写 revision）。
//!
//! PATCH 语义：只更新出现的字段（COALESCE），缺失字段保持原值；更新必须
//! 携带 `If-Match` 版本（OpenAPI updateMe 契约 required），版本过期 →
//! `409 version_conflict`。

use sqlx::Either;

use crate::auth::session::SessionUser;
use crate::db::DatabasePool;
use crate::outbox::now_millis;

/// 资料更新错误。
#[derive(Debug)]
pub enum ProfileUpdateError {
    /// 数据库错误。
    Database(String),
    /// `If-Match` 版本过期（乐观并发冲突）。
    VersionConflict,
}

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
    /// 乐观并发版本（users.version，0026；每次资料更新 +1）。
    pub version: i64,
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
            version: 1,
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
    /// 校验（M03-PROFILE-03/04）：长度、枚举、Unicode 控制字符、
    /// 富文本禁用（角括号）与危险链接 scheme。
    pub fn validate(&self) -> Result<(), String> {
        if let Some(v) = &self.display_name {
            let trimmed = v.trim();
            if trimmed.is_empty() {
                return Err("display_name 不能为空".to_string());
            }
            if trimmed.chars().count() > 80 {
                return Err("display_name 长度不能超过 80".to_string());
            }
            validate_plain_text(trimmed, "display_name", true)?;
        }
        if let Some(v) = &self.bio {
            if v.chars().count() > 2000 {
                return Err("bio 长度不能超过 2000".to_string());
            }
            validate_plain_text(v, "bio", false)?;
            validate_links(v, "bio")?;
        }
        if let Some(v) = &self.signature {
            if v.chars().count() > 500 {
                return Err("signature 长度不能超过 500".to_string());
            }
            validate_plain_text(v, "signature", false)?;
            validate_links(v, "signature")?;
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

/// 纯文本校验（M03-PROFILE-04）：禁止控制字符（保留 \n\t\r）与角括号
/// （富文本/HTML 禁用）。
fn validate_plain_text(value: &str, field: &str, single_line: bool) -> Result<(), String> {
    for c in value.chars() {
        if c.is_control() && c != '\n' && c != '\t' && c != '\r' {
            return Err(format!("{field} 包含非法控制字符"));
        }
        if single_line && (c == '\n' || c == '\r') {
            return Err(format!("{field} 不允许换行"));
        }
        if c == '<' || c == '>' {
            return Err(format!("{field} 不允许富文本/HTML 标记"));
        }
    }
    Ok(())
}

/// 链接校验（M03-PROFILE-04）：仅允许 http/https scheme，
/// 拒绝 javascript:/data:/vbscript:/file: 等危险 scheme。
fn validate_links(value: &str, field: &str) -> Result<(), String> {
    for word in value.split_whitespace() {
        if let Some(pos) = word.find("://") {
            let scheme = word[..pos].to_lowercase();
            if !matches!(scheme.as_str(), "http" | "https") {
                return Err(format!("{field} 只允许 http/https 链接"));
            }
        } else {
            let lower = word.to_lowercase();
            for bad in ["javascript:", "data:", "vbscript:", "file:"] {
                if lower.starts_with(bad) {
                    return Err(format!("{field} 包含禁止的链接 scheme: {bad}"));
                }
            }
        }
    }
    Ok(())
}

/// users 行投影：(display_name, bio, signature, version)。
type UserProfileRow = (Option<String>, Option<String>, Option<String>, i64);

/// 读取本人资料字段（行缺失时返回默认值，不建行——惰性创建发生在写）。
/// `display_name`/`version` 从 users 表读取（会话缓存的昵称在 PATCH 后会过期）。
pub async fn load_profile_fields(
    pool: &DatabasePool,
    user: &SessionUser,
) -> Result<ProfileFields, String> {
    let mut fields = ProfileFields::default();
    match pool {
        Either::Left(p) => {
            let row: Option<UserProfileRow> = sqlx::query_as(
                "SELECT display_name, bio, signature, version FROM users WHERE id = ?",
            )
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            if let Some((display_name, bio, signature, version)) = row {
                fields.display_name = display_name;
                fields.bio = bio;
                fields.signature = signature;
                fields.version = version;
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
            let row: Option<UserProfileRow> = sqlx::query_as(
                "SELECT display_name, bio, signature, version FROM users WHERE id = ?",
            )
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| e.to_string())?;
            if let Some((display_name, bio, signature, version)) = row {
                fields.display_name = display_name;
                fields.bio = bio;
                fields.signature = signature;
                fields.version = version;
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
/// `if_match` 为 If-Match 版本（users.version，0026）；版本过期 → VersionConflict。
pub async fn update_profile(
    pool: &DatabasePool,
    user_id: &str,
    update: ProfileUpdate,
    if_match: i64,
) -> Result<(), ProfileUpdateError> {
    let changed = update.changed_fields();
    if changed.is_empty() {
        return Ok(()); // 无变更：不写库、不写修订
    }

    let now = now_millis();
    let mut tx = match pool {
        Either::Left(p) => Either::Left(
            p.begin()
                .await
                .map_err(|e| ProfileUpdateError::Database(e.to_string()))?,
        ),
        Either::Right(p) => Either::Right(
            p.begin()
                .await
                .map_err(|e| ProfileUpdateError::Database(e.to_string()))?,
        ),
    };

    // 1. users：display_name/bio/signature（COALESCE 保持缺失字段原值）+
    //    版本乐观并发（version+1，WHERE version = if_match，过期 → 409）
    let affected = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE users
             SET display_name = COALESCE(?, display_name),
                 bio = COALESCE(?, bio),
                 signature = COALESCE(?, signature),
                 version = version + 1,
                 updated_at = ?
             WHERE id = ? AND version = ?",
        )
        .bind(&update.display_name)
        .bind(&update.bio)
        .bind(&update.signature)
        .bind(now)
        .bind(user_id)
        .bind(if_match)
        .execute(&mut **t)
        .await
        .map_err(|e| ProfileUpdateError::Database(e.to_string()))?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE users
             SET display_name = COALESCE(?, display_name),
                 bio = COALESCE(?, bio),
                 signature = COALESCE(?, signature),
                 version = version + 1,
                 updated_at = ?
             WHERE id = ? AND version = ?",
        )
        .bind(&update.display_name)
        .bind(&update.bio)
        .bind(&update.signature)
        .bind(now)
        .bind(user_id)
        .bind(if_match)
        .execute(&mut **t)
        .await
        .map_err(|e| ProfileUpdateError::Database(e.to_string()))?
        .rows_affected(),
    };
    if affected == 0 {
        return Err(ProfileUpdateError::VersionConflict);
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
        .map_err(|e| ProfileUpdateError::Database(e.to_string()))?,
        Either::Right(t) => sqlx::query_scalar(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM profile_revisions WHERE user_id = ?",
        )
        .bind(user_id)
        .fetch_one(&mut **t)
        .await
        .map_err(|e| ProfileUpdateError::Database(e.to_string()))?,
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
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
            .map_err(|e| ProfileUpdateError::Database(e.to_string()))?;
        }
    }

    match tx {
        Either::Left(t) => t
            .commit()
            .await
            .map_err(|e| ProfileUpdateError::Database(e.to_string())),
        Either::Right(t) => t
            .commit()
            .await
            .map_err(|e| ProfileUpdateError::Database(e.to_string())),
    }
}
