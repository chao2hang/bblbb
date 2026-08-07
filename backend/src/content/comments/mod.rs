//! M04-COMMENTS：回复、引用与楼层服务（骨架由主代理预注册，本模块由
//! COMMENTS 域 agent 填充）。
//!
//! 内容校验规则在 `crate::domain::comments` 单一维护；楼层分配必须走
//! 服务层事务（M04-COMMENTS-03）。实现分布在：
//! - [`service`]：楼层分配、keyset 分页游标、作者限时编辑 + 不可变修订快照、
//!   软删除与评论投影（单一事实来源，路由层只做鉴权/校验/错误映射）。

pub mod service;
