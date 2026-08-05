//! 领域层 — 纯业务逻辑与类型。
//!
//! 约束（由 `M00-BACKEND-03` 验收，`make check-domain` 静态扫描强制）：
//!
//! - 本模块及其子模块**不得**依赖 axum、sqlx、SMTP/S3 SDK 或读取进程环境变量；
//! - 路由层（axum）负责 HTTP 适配与数据库访问，领域层只承载业务规则与校验；
//! - 领域类型一经构造即合法（parse-then-use 模式），路由层无需重复校验。

pub mod comments;
pub mod posts;
