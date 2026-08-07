//! M05-CASES：举报、案件与内容动作。
//!
//! 构建在 M05-SCHEMA 的数据模型（`super::model`：reports/moderation_cases/
//! case_assignments/moderation_actions 等）之上：
//! - `service`：举报创建（原因枚举/详情限长/窗口去重/统一安全响应）、撤回、
//!   案件开单/状态迁移/派单（板块范围 + 利益冲突 + 审计 + Outbox）、
//!   内容动作（hide/restore/delete + revision/审计）。

pub mod service;
