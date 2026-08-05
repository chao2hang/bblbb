//! 板块领域/服务层（M03-BOARDS）。
//!
//! - [`hierarchy`]：板块层级读取——最大深度限制与循环父级检测
//!   （M03-BOARDS-01；`boards.parent_id` 软自引用层级的完整性由服务层裁决，
//!   SCHEMA.md §6）；
//! - [`validation`]：板块 slug/标题/说明/排序/状态/发帖规则校验
//!   （M03-BOARDS-02）；
//! - [`slug_exists`]：slug 唯一性检查（`boards_slug_uq` 唯一索引兜底）。

pub mod hierarchy;
pub mod validation;

pub use hierarchy::{
    build_hierarchy, load_hierarchy, validate_parent, BoardHierarchy, BoardRef, HierarchyError,
    MAX_BOARD_DEPTH,
};
pub use validation::{
    validate_board_fields, validate_board_update, validation_to_error, BoardValidationError,
    DESCRIPTION_MAX, NAME_MAX, POSTING_MODES, SLUG_MAX, SLUG_MIN, SORT_ORDER_MAX, SORT_ORDER_MIN,
};

use sqlx::Either;

use crate::db::DatabasePool;

/// slug 是否已被占用（create/update 前置友好冲突检查；唯一索引兜底）。
pub async fn slug_exists(pool: &DatabasePool, slug: &str) -> Result<bool, String> {
    let count: i64 = match pool {
        Either::Left(db) => sqlx::query_scalar("SELECT COUNT(*) FROM boards WHERE slug = ?")
            .bind(slug)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(db) => sqlx::query_scalar("SELECT COUNT(*) FROM boards WHERE slug = ?")
            .bind(slug)
            .fetch_one(db)
            .await
            .map_err(|e| e.to_string())?,
    };
    Ok(count > 0)
}
