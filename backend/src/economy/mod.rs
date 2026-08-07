//! M7：账本、等级/签到、商城与权益。
//!
//! 模块骨架由主代理预注册（Wave M5a）；`ledger`（M07-LEDGER）为账本内核，
//! 后续 Wave 由对应 agent 补充 `levels`/`shop` 子模块。

pub mod ledger;
