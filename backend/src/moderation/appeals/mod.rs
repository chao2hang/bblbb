//! M05-APPEALS：申诉与独立复核。
//!
//! 构建在 M05-SCHEMA 的 `appeals`/`appeal_decisions`（0044）之上：
//!
//! - [`service::create_appeal`]（M05-APPEALS-01/02）：可申诉对象（非撤销处罚、
//!   本人）、7 天窗口、每处罚至多一条、文字长度 1..5000、禁止附件引用；
//! - [`service::assign_reviewer`]（M05-APPEALS-03）：排除申诉人本人、原处罚
//!   执行者（原处理者）、超范围（板块 scope 不符）与无有效 assignment 人员；
//! - [`service::decide_appeal`]（M05-APPEALS-04/06）：uphold/partial/reject
//!   决定只追加 `appeal_decisions`；uphold 以撤销记录修正（不删历史）联动
//!   `sanction_reversals`；optimistic concurrency 以 `updated_at` 版本守卫。
//! - 投影（M05-APPEALS-05）：申诉人侧不含内部 note/复核人，审核员侧含之。

pub mod service;
