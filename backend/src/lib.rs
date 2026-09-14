//! BBLBB 后端 — Rust + axum
//!
//! 路由桩模块使用 `#[allow(unused_variables)]` 因为它们是待实现的占位处理器。

// clippy 1.98 的 `result_large_err`（Err 变体 ≥160B）在全仓触发 351 处：
// AppError 是 axum 统一应用错误枚举，含 sqlx 错误等大值变体。逐点
// `Box::new(AppError)` 需改动全部路由返回签名；rust-toolchain.toml 为
// channel="stable"（浮动）——本地 1.97 不触发、CI 1.98 触发。crate 级
// allow 使两种工具链一致（1.97 下该 allow 为无害冗余）。
#![allow(clippy::result_large_err)]

pub mod achievements;
pub mod ai;
pub mod antibot;
pub mod app;
pub mod audit;
pub mod auth;
pub mod authz;
pub mod boards;
pub mod bootstrap;
pub mod config;
pub mod content;
pub mod db;
pub mod domain;
pub mod download;
pub mod economy;
pub mod email;
pub mod error;
pub mod events;
pub mod feeds;
pub mod idempotency;
pub mod jobs;
pub mod marketplace;
pub mod middleware;
pub mod moderation;
pub mod notifications;
pub mod observability;
pub mod oidc;
pub mod outbox;
pub mod plugins;
pub mod ratelimit;
pub mod reactions;
pub mod routes;
pub mod search;
pub mod shop;
pub mod storage;
pub mod tags;
pub mod theme;
pub mod trust;
pub mod users;
pub mod video;

pub use app::{
    build_router, build_router_full, build_router_with_flags, build_router_with_storage,
};
pub use config::AppConfig;
