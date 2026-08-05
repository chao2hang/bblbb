//! 板块领域/服务层（M03-BOARDS）。
//!
//! - [`hierarchy`]：板块层级读取——最大深度限制与循环父级检测
//!   （M03-BOARDS-01；`boards.parent_id` 软自引用层级的完整性由服务层裁决，
//!   SCHEMA.md §6）。

pub mod hierarchy;

pub use hierarchy::{
    build_hierarchy, load_hierarchy, validate_parent, BoardHierarchy, BoardRef, HierarchyError,
    MAX_BOARD_DEPTH,
};
