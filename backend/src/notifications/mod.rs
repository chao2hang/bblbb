//! M05-NOTIFY：站内通知与邮件投递。
//!
//! 模块骨架由主代理预注册（Wave M5a）；`model`（M05-SCHEMA）承载
//! notifications/notification_preferences 的数据模型与去重约束；
//! `templates`（M05-NOTIFY-01/02）定义模板键与安全渲染；
//! `service`（M05-NOTIFY-03/04/05/06）实现站内通知创建/去重/列表/已读/
//! 偏好/权限复查。

pub mod model;
pub mod service;
pub mod templates;
