//! 路由模块边界（M00-BACKEND-01）
//!
//! 各领域模块在独立文件中挂载 `/api/v1/**` 路由；admin 集中管理后台端点。
//!
//! M00-BACKEND-03 评估（部分达成，建议 M1 后重构）：
//! 处理器目前直接在路由层使用 `sqlx` 查询与 `Either` 分支（SQLite/MySQL），
//! 尚未抽离独立的 domain/service 层，也没有依赖注入的存储/邮件/AI 接口。
//! 作为 M0 骨架可接受；建议在 M1（DB 仓储契约）落地时同步引入
//! `domain`/`service` 层与仓储 trait，使业务代码不再依赖 axum/sqlx 实现细节。

pub mod health;
pub mod openapi;
pub mod ready;

pub mod admin;
pub mod ai;
pub mod auth;
pub mod boards;
pub mod comments;
pub mod drafts;
pub mod economy;
pub mod feeds;
pub mod marketplace;
pub mod mfa;
pub mod moderation;
pub mod oidc;
pub mod posts;
pub mod search;
pub mod storage;
pub mod themes;
pub mod users;
pub mod video;
