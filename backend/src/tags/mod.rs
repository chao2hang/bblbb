//! 标签领域/服务层（M03-BOARDS-06）。
//!
//! - [`load_tag_groups`]：标签组读取（slug 全局唯一、sort_order 排序）；
//! - [`load_active_tags`] / [`load_all_tags`]：标签读取（slug、展示名、
//!   组、颜色、禁用状态 `is_active`；`is_active=0` 移出公开投影）；
//! - [`admin`]：标签创建/更新——唯一性、版本冲突、权限与审计
//!   （M03-BOARDS-07）。
//!
//! `usage_count`（0003 骨架）是可重建缓存，不是真实来源（SCHEMA.md §6）。

pub mod admin;

pub use admin::{create_tag, update_tag, TagCreateInput, TagUpdateInput};

use sqlx::Either;

use crate::db::DatabasePool;

/// 标签组投影。
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TagGroup {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub sort_order: i64,
    pub created_at: i64,
}

/// 标签投影（含禁用状态；`is_active=0` = 禁用）。
#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct Tag {
    pub id: String,
    pub name: String,
    pub slug: Option<String>,
    pub description: String,
    pub color: Option<String>,
    pub group_id: Option<String>,
    pub usage_count: i64,
    pub is_active: i64,
    pub created_at: i64,
}

impl Tag {
    pub fn enabled(&self) -> bool {
        self.is_active != 0
    }
}

/// 标签组读取（sort_order, created_at 稳定排序）。
pub async fn load_tag_groups(pool: &DatabasePool) -> Result<Vec<TagGroup>, String> {
    match pool {
        Either::Left(db) => sqlx::query_as::<_, TagGroup>(
            "SELECT id, name, slug, sort_order, created_at FROM tag_groups
                 ORDER BY sort_order ASC, created_at ASC, id ASC",
        )
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string()),
        Either::Right(db) => sqlx::query_as::<_, TagGroup>(
            "SELECT id, name, slug, sort_order, created_at FROM tag_groups
                 ORDER BY sort_order ASC, created_at ASC, id ASC",
        )
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string()),
    }
}

/// 启用标签读取（`is_active = 1`；公开投影，`usage_count` 降序）。
pub async fn load_active_tags(pool: &DatabasePool) -> Result<Vec<Tag>, String> {
    match pool {
        Either::Left(db) => {
            sqlx::query_as::<_, Tag>(
                "SELECT id, name, slug, description, color, group_id, usage_count, is_active, created_at
                 FROM tags WHERE is_active = 1
                 ORDER BY usage_count DESC, created_at ASC, id ASC",
            )
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())
        }
        Either::Right(db) => {
            sqlx::query_as::<_, Tag>(
                "SELECT id, name, slug, description, color, group_id, usage_count, is_active, created_at
                 FROM tags WHERE is_active = 1
                 ORDER BY usage_count DESC, created_at ASC, id ASC",
            )
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())
        }
    }
}

/// 全部标签读取（含禁用；管理端投影，`usage_count` 降序）。
pub async fn load_all_tags(pool: &DatabasePool) -> Result<Vec<Tag>, String> {
    match pool {
        Either::Left(db) => {
            sqlx::query_as::<_, Tag>(
                "SELECT id, name, slug, description, color, group_id, usage_count, is_active, created_at
                 FROM tags ORDER BY usage_count DESC, created_at ASC, id ASC",
            )
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())
        }
        Either::Right(db) => {
            sqlx::query_as::<_, Tag>(
                "SELECT id, name, slug, description, color, group_id, usage_count, is_active, created_at
                 FROM tags ORDER BY usage_count DESC, created_at ASC, id ASC",
            )
            .fetch_all(db)
            .await
            .map_err(|e| e.to_string())
        }
    }
}
