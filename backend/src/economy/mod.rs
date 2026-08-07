//! M7：账本、等级/签到、商城与权益。
//!
//! 模块骨架由主代理预注册（Wave M5a）；`ledger`（M07-LEDGER）为账本内核，
//! `levels`（M07-LEVELS）与 `activity`（M07-LEVELS 活动/签到）为等级与经济域，
//! 由 Wave M6-M7 域 agent 填充。

pub mod activity;
pub mod ledger;
pub mod levels;
