//! BBLBB 后端 — Rust + axum
//!
//! 路由桩模块使用 `#[allow(unused_variables)]` 因为它们是待实现的占位处理器。

pub mod app;
pub mod audit;
pub mod auth;
pub mod authz;
pub mod config;
pub mod db;
pub mod domain;
pub mod error;
pub mod events;
pub mod idempotency;
pub mod jobs;
pub mod middleware;
pub mod outbox;
pub mod ratelimit;
pub mod routes;
pub mod users;

pub use app::{build_router, build_router_with_flags};
pub use config::AppConfig;
