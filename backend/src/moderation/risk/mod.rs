//! M05-RISK：发布前后风险评估。
//!
//! 模块职责（对照 todo/M03-M05-community.md 的 M05-RISK 工作包）：
//! - `policy`：风险输入最小集合（不向 Provider 暴露内部/隐藏数据）、版本化
//!   策略与默认阈值、安全 reason category 与 RiskVerdict（M05-RISK-01/06）；
//! - `rules`：新用户前 N 帖、链接数、重复内容、敏感词、频率规则（M05-RISK-02）；
//! - `provider`：AI moderation suggestion 接口 + 禁用 Null Adapter，结果只能
//!   是建议（M05-RISK-04）；AI 不能直接执行封禁/删除/放行/权限/账务动作
//!   （M05-RISK-05，接口层面无此类动作）；
//! - `service`：评估编排（规则超时/AI 关闭/失败/迟到降级、旧 policy 不复用）、
//!   管理员版本化更新（reason+审计+并发版本控制）、指标记录（不记录正文）。

pub mod policy;
pub mod provider;
pub mod rules;
pub mod service;
