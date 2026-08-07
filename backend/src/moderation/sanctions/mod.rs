//! M05-SANCTIONS：处罚、实时生效与撤销。
//!
//! 构建在 M05-SCHEMA 的 `sanctions`/`sanction_reversals` 迁移与模型之上；
//! 实现见 `service`（创建/撤销/实时计算/越权防护/安全投影）。

pub mod service;
