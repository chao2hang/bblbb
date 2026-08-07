//! M05-NOTIFY：站内通知与邮件投递。
//!
//! 模块骨架由主代理预注册（Wave M5a）；`model`（M05-SCHEMA）承载
//! notifications/notification_preferences 的数据模型与去重约束，
//! 后续 Wave（M05-NOTIFY）由对应 agent 补充通知投递/偏好/邮件实现。

pub mod model;
