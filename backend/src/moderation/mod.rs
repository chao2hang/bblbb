//! M05：风险审核、举报、处罚、申诉与通知数据模型与领域逻辑。
//!
//! 模块骨架由主代理预注册（Wave M5a），各域 agent 只填充自己负责的子模块
//! 文件，避免并行编辑本文件产生冲突。
//!
//! 子模块按 Wave 推进顺序逐个落地：
//! - `model`（M05-SCHEMA）：reports/moderation_cases/case_assignments/notes、
//!   moderation_actions/revisions、sanctions、appeals 的数据模型与迁移约束；
//! - `risk`/`cases`/`sanctions`/`appeals`：后续 Wave（M05-RISK/CASES/
//!   SANCTIONS/APPEALS）由各自 agent 添加 `pub mod` 声明并落地实现。

pub mod model;
