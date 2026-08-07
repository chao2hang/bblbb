//! AI 审核建议接口与禁用 Null Adapter（M05-RISK-04/05）。
//!
//! 契约：
//! - Provider 只能接收 [`RiskInput`]（最小特征集，M05-RISK-01），返回
//!   [`AiSuggestion`]——**结果只能是建议**（`NoAction` 或 `Flag(category)`）。
//!   [`AiSuggestion`] 没有"执行动作"的变体，从类型上杜绝 AI 直接执行封禁、
//!   删除、放行、权限变更或账务动作（M05-RISK-05）。
//! - 建议最终只能把内容路由到人工队列（`pending_review`）或放行；是否采纳由
//!   service 编排（超时/失败/迟到时按规则结果兜底，不阻塞发布流程）。
//! - [`NullAiModerationProvider`]：AI 功能关闭时的适配器，恒返回 `NoAction`。

use async_trait::async_trait;

use super::policy::{ReasonCategory, RiskInput};

/// AI 审核建议（只读建议，无副作用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AiSuggestion {
    /// 未发现需要人工复核的内容。
    NoAction,
    /// 建议进入人工队列（分类为安全 reason category）。
    Flag(ReasonCategory),
}

/// AI 审核 Provider 接口。
#[async_trait]
pub trait AiModerationProvider: Send + Sync {
    /// 返回对输入内容的审核建议。
    ///
    /// 调用方必须为本次调用设置截止时间（`tokio::time::timeout`）：
    /// 超时/失败/迟到时该建议被忽略，不阻塞发布流程（M05-RISK-07）。
    async fn suggest(&self, input: &RiskInput, now: i64) -> AiSuggestion;
}

/// 禁用时的 Null Adapter：恒返回 `NoAction`（M05-RISK-04）。
#[derive(Debug, Clone, Copy, Default)]
pub struct NullAiModerationProvider;

#[async_trait]
impl AiModerationProvider for NullAiModerationProvider {
    async fn suggest(&self, _input: &RiskInput, _now: i64) -> AiSuggestion {
        AiSuggestion::NoAction
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn null_provider_is_always_no_action() {
        let input = RiskInput {
            author_id: "a".into(),
            author_created_at: None,
            author_level: 1,
            board_id: "b".into(),
            title: String::new(),
            body_markdown: String::new(),
            now: 1_000,
        };
        let p = NullAiModerationProvider;
        assert_eq!(p.suggest(&input, 1_000).await, AiSuggestion::NoAction);
    }
}
