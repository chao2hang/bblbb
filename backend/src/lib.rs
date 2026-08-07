//! BBLBB 后端 — Rust + axum
//!
//! 路由桩模块使用 `#[allow(unused_variables)]` 因为它们是待实现的占位处理器。

pub mod ai;
pub mod antibot;
pub mod app;
pub mod audit;
pub mod auth;
pub mod authz;
pub mod boards;
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
pub mod oidc;
pub mod outbox;
pub mod ratelimit;
pub mod reactions;
pub mod routes;
pub mod search;
pub mod shop;
pub mod storage;
pub mod tags;
pub mod users;
pub mod video;

pub use app::{
    build_router, build_router_full, build_router_with_flags, build_router_with_storage,
};
pub use config::AppConfig;
