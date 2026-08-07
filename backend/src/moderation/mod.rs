//! M05：风险审核、举报、处罚、申诉与通知数据模型与领域逻辑。
//!
//! 模块骨架由主代理预注册（Wave M5a），各域 agent 只填充自己负责的子模块
//! 文件，避免并行编辑本文件产生冲突。
//!
//! 子模块按 Wave 推进顺序逐个落地：
//! - `model`（M05-SCHEMA）：reports/moderation_cases/case_assignments/notes、
//!   moderation_actions/revisions、sanctions、appeals 的数据模型与迁移约束；
//! - `risk`（M05-RISK）：发布前后风险评估（版本化策略、规则、AI 建议接口、
//!   pending_review 门禁、指标），由主代理实现（需集成共享发布路径）；
//! - `cases`（M05-CASES）：举报创建/撤回/去重、案件状态机与派单、利益冲突、
//!   内容动作（hide/restore/delete 等）与审计/Outbox；
//! - `sanctions`（M05-SANCTIONS）：处罚创建/撤销/实时计算、ban 撤销 Session、
//!   越权防护与安全状态投影；
//! - `appeals`：后续 Wave 由对应 agent 添加 `pub mod` 声明并落地实现。

pub mod cases;
pub mod model;
pub mod risk;
pub mod sanctions;
