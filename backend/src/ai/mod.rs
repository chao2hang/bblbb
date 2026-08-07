//! AI Gateway 领域（M09-GATEWAY/TASKS/SUGGESTIONS）。
//!
//! - [`gateway`]：Provider 出站边界（HTTPS/host allowlist/端口/IP 阻断/重定向/
//!   超时/响应上限）、脱敏规则与 ProviderClient 抽象（真实 reqwest 实现 +
//!   测试 mock）；
//! - [`consent`]：逐次同意（ai_consents，(user,provider,purpose) 唯一）；
//! - [`tasks`]：异步任务（ai_tasks 状态机 queued/running/retry_wait/succeeded/
//!   cancelled/dead，幂等入队 + 取消 + 错误分类 + 至少一次消费去重）；
//! - [`suggestions`]：模型输出校验与建议（ai_suggestions，schema_version +
//!   base_revision 防旧覆盖新，采纳时重新鉴权/If-Match 幂等）。
//!
//! 领域层不依赖 axum；Gateway 网络调用经 [`gateway::ProviderClient`] 抽象，
//! 沙盒/测试用 mock，不发起真实外部请求。

pub mod consent;
pub mod gateway;
pub mod suggestions;
pub mod tasks;

pub use consent::{consent_for, grant_consent, has_active_consent, revoke_consent, ConsentError};
pub use gateway::{
    EgressPolicy, GatewayError, OutboundRequest, OutboundResponse, ProviderClient, RedactionMode,
    Redactor,
};
pub use suggestions::{
    accept_suggestion, create_suggestion, get_suggestion, parse_suggestion_payload,
    validate_suggestion, SuggestionError, SuggestionKind,
};
pub use tasks::{cancel_task, classify_error, enqueue_task, execute_task, task_state, TaskError};

/// 任务类型（与 ai_tasks.task_type / ai_suggestions.suggestion_type 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskKind {
    Formatting,
    Moderation,
    Seo,
    Tagging,
}

impl TaskKind {
    pub const ALL: [TaskKind; 4] = [
        TaskKind::Formatting,
        TaskKind::Moderation,
        TaskKind::Seo,
        TaskKind::Tagging,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            TaskKind::Formatting => "formatting",
            TaskKind::Moderation => "moderation",
            TaskKind::Seo => "seo",
            TaskKind::Tagging => "tagging",
        }
    }

    pub fn parse(value: &str) -> Option<TaskKind> {
        Self::ALL.iter().find(|v| v.as_str() == value).copied()
    }
}

/// 当前建议 schema 版本（模型输出结构校验用）。
pub const SUGGESTION_SCHEMA_VERSION: i64 = 1;
