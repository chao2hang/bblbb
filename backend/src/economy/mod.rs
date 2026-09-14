//! M7：账本、签到、商城与权益。
//!
//! `ledger` 提供站内 B 币账本；`activity` 提供签到与活动奖励。用户等级
//! 不属于经济域，统一由 `crate::trust` 的 LinuxDo 式 TL0–TL4 信任等级提供。

pub mod activity;
pub mod ledger;
