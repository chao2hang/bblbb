//! 用户域：显式投影 DTO、资料读取/更新与注销匿名化（M03-PROFILE）。
//!
//! 原则（M03-PROFILE-01）：
//! - 公开（`PublicProfile`）、本人（`Me`）、管理（`AdminUser`）三套投影
//!   各自是显式 DTO，只从显式字段构建；
//! - 禁止 `#[derive(Serialize)]` 数据库实体行直接序列化到响应；
//! - 字段 allowlist 细化与泄漏测试见 M03-PROFILE-02/09。

pub mod deletion;
pub mod dto;
pub mod profile;
