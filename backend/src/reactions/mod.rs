//! M07-REACTIONS：互动 Reaction 模块。
//!
//! 主端点（POST/DELETE `/api/v1/posts/{id}/reactions`、`/api/v1/comments/{id}/reactions`）
//! 由主代理在 posts.rs/comments.rs 接线到 `service::add_reaction/remove_reaction`；
//! 本模块提供服务层与反应汇总查询。

pub mod service;

pub use service::{add_reaction, remove_reaction, summarize, ReactionError};
