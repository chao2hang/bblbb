//! 信任等级域（LinuxDo 式 TL0–TL4，移植自 linux.do / Discourse）。
//!
//! 层次：
//! - [`rules`]：纯逻辑——阈值解析、达标判定、逐项进度报告（含单元测试）；
//! - [`store`]：SQL 仓储——行为统计写入（幂等）、聚合、等级缓存与只追加
//!   事件（SQLite / MySQL 双方言）；
//! - [`service`]：编排——评估引擎（晋升/TL3 降级 + 2 周宽限）、进度视图、
//!   手动授予、best-effort 事件钩子。
//!
//! 数据模型见 `migrations/{sqlite,mysql,mariadb}/0070_trust_levels.sql`，
//! 端点与权限映射见 `docs/TRUST-LEVELS.md`（documented non-contract 端点，
//! 权限复用 `user.read_own` / `user.edit_own` / `level.manage`）。
//!
//! 信任等级是全站唯一用户等级：反映阅读、访问、点赞与处罚记录，
//! 并作为内容门槛、商城资格和配额档位的运行时等级来源；不由 B 币余额计算。

pub mod rules;
pub mod service;
pub mod store;

pub use service::{
    add_read_time, evaluate_user, manual_set, on_comments_read, on_reaction_changed,
    on_sanction_changed, on_session_active, on_topic_viewed, progress, Evaluation, NextLevel,
    TrustError, TrustProgress, WindowInfo, TL3_GRACE_MS,
};
