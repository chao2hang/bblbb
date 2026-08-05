//! M03-BOARDS-07：标签创建/更新——唯一性、版本冲突、权限与审计。
//!
//! - 创建：name/slug 唯一（`tags.name` 0003 / `tags_slug_uq` 0023 兜底）、
//!   group_id 存在性（软引用，服务层校验）、字段格式 → 单事务 INSERT +
//!   `admin.tag_create` 审计（M01-AUDIT-08 事务内）；
//! - 更新：If-Match 版本冲突（`tags.updated_at` 为乐观并发版本，迁移 0029；
//!   缺头 400、过期 409、UPDATE rows=0 兜底竞态）→ 单事务 UPDATE +
//!   `admin.tag_update` 审计（before/after 白名单字段）；
//! - reason 必填（矩阵：所有 `admin.*` 写操作记录 reason、actor、request_id）。

use serde_json::{json, Value};
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::error::AppError;
use crate::outbox::now_millis;

/// 标签名最大长度。
pub const TAG_NAME_MAX: usize = 64;
/// 标签 slug 最大长度。
pub const TAG_SLUG_MAX: usize = 64;
/// 标签说明最大长度。
pub const TAG_DESCRIPTION_MAX: usize = 500;
/// 标签颜色最大长度（如 `#RRGGBB`）。
pub const TAG_COLOR_MAX: usize = 16;

/// 创建标签输入（请求体解析；空串 = 未提供）。
pub struct TagCreateInput {
    pub name: String,
    pub slug: Option<String>,
    pub description: String,
    pub color: Option<String>,
    pub group_id: Option<String>,
}

/// 更新标签输入（部分字段；`Option<Option<T>>` 区分未提供 / 置空）。
pub struct TagUpdateInput {
    pub name: Option<String>,
    pub slug: Option<Option<String>>,
    pub description: Option<String>,
    pub color: Option<Option<String>>,
    pub group_id: Option<Option<String>>,
    pub is_active: Option<bool>,
}

/// 管理员标签行（含版本）。
#[derive(sqlx::FromRow)]
struct AdminTagRow {
    id: String,
    name: String,
    slug: Option<String>,
    description: String,
    color: Option<String>,
    group_id: Option<String>,
    usage_count: i64,
    is_active: i64,
    created_at: i64,
    updated_at: i64,
}

fn tag_projection(row: &AdminTagRow) -> Value {
    json!({
        "id": row.id,
        "name": row.name,
        "slug": row.slug,
        "description": row.description,
        "color": row.color,
        "group_id": row.group_id,
        "usage_count": row.usage_count,
        "is_active": row.is_active != 0,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

/// 写错误映射：唯一约束冲突（并发 name/slug 竞态，唯一索引兜底触发）→ 409；
/// 其余 → 500。
fn write_error(e: sqlx::Error, request_id: &str, detail: &str) -> AppError {
    if matches!(&e, sqlx::Error::Database(db) if db.is_unique_violation()) {
        AppError::conflict(detail, request_id)
    } else {
        AppError::internal(e.to_string(), request_id)
    }
}

fn validate_name(name: &str) -> Result<(), String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("name（标签名）不能为空".to_string());
    }
    let len = name.chars().count();
    if len > TAG_NAME_MAX {
        return Err(format!("name 长度不能超过 {TAG_NAME_MAX}（当前 {len}）"));
    }
    Ok(())
}

fn validate_slug(slug: &str) -> Result<(), String> {
    if slug.is_empty() {
        return Err("slug 不能为空".to_string());
    }
    let len = slug.chars().count();
    if len > TAG_SLUG_MAX {
        return Err(format!("slug 长度不能超过 {TAG_SLUG_MAX}（当前 {len}）"));
    }
    if !slug
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
    {
        return Err("slug 只能包含小写字母、数字和连字符（[a-z0-9-]+）".to_string());
    }
    Ok(())
}

fn validate_description(description: &str) -> Result<(), String> {
    let len = description.chars().count();
    if len > TAG_DESCRIPTION_MAX {
        return Err(format!(
            "description 长度不能超过 {TAG_DESCRIPTION_MAX}（当前 {len}）"
        ));
    }
    Ok(())
}

fn validate_color(color: &str) -> Result<(), String> {
    let len = color.chars().count();
    if len > TAG_COLOR_MAX {
        return Err(format!("color 长度不能超过 {TAG_COLOR_MAX}（当前 {len}）"));
    }
    Ok(())
}

async fn group_exists(pool: &DatabasePool, group_id: &str) -> Result<bool, String> {
    let count: i64 = match pool {
        Either::Left(db) => sqlx::query_scalar("SELECT COUNT(*) FROM tag_groups WHERE id = ?")
            .bind(group_id)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(db) => sqlx::query_scalar("SELECT COUNT(*) FROM tag_groups WHERE id = ?")
            .bind(group_id)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?,
    };
    Ok(count > 0)
}

async fn name_exists(pool: &DatabasePool, name: &str) -> Result<bool, String> {
    let count: i64 = match pool {
        Either::Left(db) => sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name = ?")
            .bind(name)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(db) => sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE name = ?")
            .bind(name)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?,
    };
    Ok(count > 0)
}

async fn slug_exists(pool: &DatabasePool, slug: &str) -> Result<bool, String> {
    let count: i64 = match pool {
        Either::Left(db) => sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE slug = ?")
            .bind(slug)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(db) => sqlx::query_scalar("SELECT COUNT(*) FROM tags WHERE slug = ?")
            .bind(slug)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?,
    };
    Ok(count > 0)
}

async fn load_tag(pool: &DatabasePool, tag_id: &str) -> Result<Option<AdminTagRow>, String> {
    match pool {
        Either::Left(db) => sqlx::query_as::<_, AdminTagRow>(
            "SELECT id, name, slug, description, color, group_id, usage_count, is_active, created_at, updated_at
             FROM tags WHERE id = ?",
        )
        .bind(tag_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string()),
        Either::Right(db) => sqlx::query_as::<_, AdminTagRow>(
            "SELECT id, name, slug, description, color, group_id, usage_count, is_active, created_at, updated_at
             FROM tags WHERE id = ?",
        )
        .bind(tag_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string()),
    }
}

/// 创建标签：校验（唯一性 + 组存在性 + 格式）→ 单事务 INSERT + 审计。
pub async fn create_tag(
    pool: &DatabasePool,
    actor_id: &str,
    input: TagCreateInput,
    reason: &str,
    request_id: &str,
) -> Result<Value, AppError> {
    validate_name(&input.name).map_err(|m| AppError::bad_request(m, request_id, None))?;
    validate_description(&input.description)
        .map_err(|m| AppError::bad_request(m, request_id, None))?;
    let slug = input
        .slug
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(s) = &slug {
        validate_slug(s).map_err(|m| AppError::bad_request(m, request_id, None))?;
    }
    let color = input
        .color
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(c) = &color {
        validate_color(c).map_err(|m| AppError::bad_request(m, request_id, None))?;
    }
    let group_id = input
        .group_id
        .as_deref()
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    if let Some(g) = &group_id {
        if !group_exists(pool, g)
            .await
            .map_err(|e| AppError::internal(e, request_id))?
        {
            return Err(AppError::bad_request(
                "group_id 必须引用存在的标签组",
                request_id,
                None,
            ));
        }
    }

    if name_exists(pool, &input.name)
        .await
        .map_err(|e| AppError::internal(e, request_id))?
    {
        return Err(AppError::conflict("tag name already exists", request_id));
    }
    if let Some(s) = &slug {
        if slug_exists(pool, s)
            .await
            .map_err(|e| AppError::internal(e, request_id))?
        {
            return Err(AppError::conflict("tag slug already exists", request_id));
        }
    }

    let tag_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let mut tx = match pool {
        Either::Left(p) => Either::Left(
            p.begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        ),
        Either::Right(p) => Either::Right(
            p.begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        ),
    };

    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO tags (id, name, slug, description, color, group_id, usage_count, is_active, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 0, 1, ?, ?)",
            )
            .bind(&tag_id)
            .bind(&input.name)
            .bind(&slug)
            .bind(&input.description)
            .bind(&color)
            .bind(&group_id)
            .bind(now)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| write_error(e, request_id, "tag name or slug already exists"))?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO tags (id, name, slug, description, color, group_id, usage_count, is_active, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 0, 1, ?, ?)",
            )
            .bind(&tag_id)
            .bind(&input.name)
            .bind(&slug)
            .bind(&input.description)
            .bind(&color)
            .bind(&group_id)
            .bind(now)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| write_error(e, request_id, "tag name or slug already exists"))?;
        }
    }

    let after = json!({
        "id": tag_id,
        "name": input.name,
        "slug": slug,
        "description": input.description,
        "color": color,
        "group_id": group_id,
        "usage_count": 0,
        "is_active": true,
        "created_at": now,
        "updated_at": now,
    });
    AuditEntry::tag_change(
        actor_id,
        "admin.tag_create",
        &tag_id,
        None,
        &after,
        reason,
        AUTHZ_POLICY_VERSION,
    )
    .record_in_tx(&mut tx)
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match tx {
        Either::Left(t) => t
            .commit()
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(t) => t
            .commit()
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    }

    Ok(json!({ "id": tag_id, "version": now }))
}

/// 更新标签：If-Match 版本冲突 → 校验 → 单事务 UPDATE + 审计。
pub async fn update_tag(
    pool: &DatabasePool,
    actor_id: &str,
    tag_id: &str,
    input: TagUpdateInput,
    if_match: i64,
    reason: &str,
    request_id: &str,
) -> Result<Value, AppError> {
    let current = load_tag(pool, tag_id)
        .await
        .map_err(|e| AppError::internal(e, request_id))?
        .ok_or_else(|| AppError::not_found("tag not found", request_id))?;
    if if_match != current.updated_at {
        return Err(AppError::version_conflict(
            "tag version conflict",
            request_id,
        ));
    }

    if let Some(name) = &input.name {
        validate_name(name).map_err(|m| AppError::bad_request(m, request_id, None))?;
        if name != &current.name
            && name_exists(pool, name)
                .await
                .map_err(|e| AppError::internal(e, request_id))?
        {
            return Err(AppError::conflict("tag name already exists", request_id));
        }
    }
    if let Some(inner) = &input.description {
        validate_description(inner).map_err(|m| AppError::bad_request(m, request_id, None))?;
    }
    if let Some(inner) = &input.color {
        if let Some(c) = inner.as_deref().filter(|s| !s.is_empty()) {
            validate_color(c).map_err(|m| AppError::bad_request(m, request_id, None))?;
        }
    }
    if let Some(inner) = &input.group_id {
        if let Some(g) = inner.as_deref().filter(|s| !s.is_empty()) {
            if !group_exists(pool, g)
                .await
                .map_err(|e| AppError::internal(e, request_id))?
            {
                return Err(AppError::bad_request(
                    "group_id 必须引用存在的标签组",
                    request_id,
                    None,
                ));
            }
        }
    }

    // 新 slug（区分未提供 / 置空）
    let new_slug: Option<String> = match &input.slug {
        None => current.slug.clone(),
        Some(inner) => inner
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    };
    if let Some(s) = &new_slug {
        validate_slug(s).map_err(|m| AppError::bad_request(m, request_id, None))?;
        if current.slug.as_deref() != Some(s.as_str())
            && slug_exists(pool, s)
                .await
                .map_err(|e| AppError::internal(e, request_id))?
        {
            return Err(AppError::conflict("tag slug already exists", request_id));
        }
    }

    // 新值（类型化，供 UPDATE 与审计 after 复用）
    let new_updated_at = now_millis();
    let new_name = input.name.clone().unwrap_or_else(|| current.name.clone());
    let new_description = input
        .description
        .clone()
        .unwrap_or_else(|| current.description.clone());
    let new_color: Option<String> = match &input.color {
        None => current.color.clone(),
        Some(inner) => inner
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    };
    let new_group_id: Option<String> = match &input.group_id {
        None => current.group_id.clone(),
        Some(inner) => inner
            .as_deref()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    };
    let new_is_active = input.is_active.unwrap_or(current.is_active != 0);

    let before = tag_projection(&current);
    let after = json!({
        "id": tag_id,
        "name": new_name.clone(),
        "slug": new_slug.clone(),
        "description": new_description.clone(),
        "color": new_color.clone(),
        "group_id": new_group_id.clone(),
        "usage_count": current.usage_count,
        "is_active": new_is_active,
        "created_at": current.created_at,
        "updated_at": new_updated_at,
    });

    let mut tx = match pool {
        Either::Left(p) => Either::Left(
            p.begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        ),
        Either::Right(p) => Either::Right(
            p.begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        ),
    };

    let affected: u64 = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE tags SET name = ?, slug = ?, description = ?, color = ?, group_id = ?,
                        is_active = ?, updated_at = ?
                 WHERE id = ? AND updated_at = ?",
        )
        .bind(&new_name)
        .bind(&new_slug)
        .bind(&new_description)
        .bind(&new_color)
        .bind(&new_group_id)
        .bind(new_is_active as i64)
        .bind(new_updated_at)
        .bind(tag_id)
        .bind(if_match)
        .execute(&mut **t)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE tags SET name = ?, slug = ?, description = ?, color = ?, group_id = ?,
                        is_active = ?, updated_at = ?
                 WHERE id = ? AND updated_at = ?",
        )
        .bind(&new_name)
        .bind(&new_slug)
        .bind(&new_description)
        .bind(&new_color)
        .bind(&new_group_id)
        .bind(new_is_active as i64)
        .bind(new_updated_at)
        .bind(tag_id)
        .bind(if_match)
        .execute(&mut **t)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
    };
    if affected == 0 {
        return Err(AppError::version_conflict(
            "tag version conflict",
            request_id,
        ));
    }

    AuditEntry::tag_change(
        actor_id,
        "admin.tag_update",
        tag_id,
        Some(&before),
        &after,
        reason,
        AUTHZ_POLICY_VERSION,
    )
    .record_in_tx(&mut tx)
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match tx {
        Either::Left(t) => t
            .commit()
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(t) => t
            .commit()
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    }

    Ok(json!({ "id": tag_id, "version": new_updated_at }))
}
