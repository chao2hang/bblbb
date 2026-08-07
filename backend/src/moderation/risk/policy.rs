//! 风险输入最小集合与版本化策略（M05-RISK-01）。
//!
//! 契约：
//! - [`RiskInput`] 是评估所需的**最小特征集**：只含规则/Provider 需要的数据，
//!   绝不包含举报人、内部 note、隐藏正文、规则细节或 Prompt——无论是传给
//!   Provider（AI 审核接口）还是作者状态投影（M05-RISK-06）都只暴露安全
//!   [`ReasonCategory`]，不含具体命中内容。
//! - [`Thresholds`] 来自版本化策略 `risk_policies.thresholds_json`
//!   （M05-RISK-08 管理员更新）；未配置时使用 [`DEFAULT_THRESHOLDS`]
//!   （version 0 = 内置默认，`risk_policies` 首行 version 1 起）。
//! - [`RiskVerdict`] 只有两种结果：`allow` 直接发布，或 `pending_review`
//!   进入人工队列（原子写 review_status，见 service/发布路径）。

use serde::{Deserialize, Serialize};

use super::service::RiskError;

/// 内置默认策略 ID（`risk_policies.id`，唯一逻辑策略行）。
pub const DEFAULT_RISK_POLICY_ID: &str = "risk-policy-default";

/// 内置默认策略版本（无任何 `risk_policies` 行时的兜底版本）。
pub const BUILTIN_POLICY_VERSION: i64 = 0;

/// 风险输入最小集合。
#[derive(Debug, Clone)]
pub struct RiskInput {
    pub author_id: String,
    /// 账号创建时间（Unix 毫秒）；缺失视为老用户（新用户规则豁免）。
    pub author_created_at: Option<i64>,
    /// 作者当前等级。
    pub author_level: i64,
    pub board_id: String,
    pub title: String,
    pub body_markdown: String,
    /// 评估时刻（Unix 毫秒）。
    pub now: i64,
}

/// 安全原因类别：作者状态投影只输出类别本身（M05-RISK-06），
/// 不含举报人、内部 note、命中规则细节（如具体敏感词）或 Prompt。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCategory {
    /// 疑似垃圾/广告。
    SpamLike,
    /// 链接占比过高。
    LinkHeavy,
    /// 与近期内容重复。
    Duplicate,
    /// 命中敏感词规则。
    Sensitive,
    /// 发布频率过高。
    Frequency,
    /// 新用户集中发帖。
    NewUser,
}

impl ReasonCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::SpamLike => "spam_like",
            Self::LinkHeavy => "link_heavy",
            Self::Duplicate => "duplicate",
            Self::Sensitive => "sensitive",
            Self::Frequency => "frequency",
            Self::NewUser => "new_user",
        }
    }
}

/// 规则阈值（版本化策略的可序列化载荷）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Thresholds {
    /// 新用户（账号存在时长 < `new_user_grace_secs`）累计发帖达到该值 → 高风险。
    pub new_user_max_posts: u32,
    /// 新用户判定窗口（秒）。
    pub new_user_grace_secs: i64,
    /// 正文中链接数量超过该值 → 高风险。
    pub max_links: u32,
    /// 敏感词列表（仅规则内部匹配用；作者投影只给 `Sensitive` 类别）。
    pub sensitive_words: Vec<String>,
    /// 频率窗口（秒）内发帖达到该值 → 高风险。
    pub max_frequency_posts: u32,
    pub frequency_window_secs: i64,
    /// 重复内容判定窗口（秒）内存在同指纹正文（其他作者）→ 高风险。
    pub duplicate_window_secs: i64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            new_user_max_posts: 3,
            new_user_grace_secs: 86_400 * 7, // 7 天
            max_links: 3,
            sensitive_words: vec![],
            max_frequency_posts: 10,
            frequency_window_secs: 3_600,      // 1 小时
            duplicate_window_secs: 7 * 86_400, // 7 天
        }
    }
}

/// 内置默认阈值（对应 version 0；生产环境由管理员版本化覆盖）。
pub const DEFAULT_THRESHOLDS: Thresholds = Thresholds {
    new_user_max_posts: 3,
    new_user_grace_secs: 86_400 * 7,
    max_links: 3,
    sensitive_words: Vec::new(),
    max_frequency_posts: 10,
    frequency_window_secs: 3_600,
    duplicate_window_secs: 7 * 86_400,
};

/// 加载后的版本化策略。
#[derive(Debug, Clone)]
pub struct RiskPolicy {
    pub version: i64,
    pub thresholds: Thresholds,
}

impl RiskPolicy {
    /// 内置默认（version 0）。
    pub fn builtin() -> Self {
        Self {
            version: BUILTIN_POLICY_VERSION,
            thresholds: Thresholds {
                ..DEFAULT_THRESHOLDS
            },
        }
    }

    /// 从 `risk_policies.thresholds_json` 解析。
    pub fn parse(version: i64, json: &str) -> Result<Self, RiskError> {
        let thresholds: Thresholds = serde_json::from_str(json)
            .map_err(|e| RiskError::InvalidPolicy(format!("thresholds_json: {e}")))?;
        Ok(Self {
            version,
            thresholds,
        })
    }

    /// 序列化阈值载荷（写入 `thresholds_json`）。
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.thresholds).unwrap_or_else(|_| "{}".to_string())
    }
}

/// 评估结果：要么放行，要么进入人工队列（不直接执行任何动作）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RiskVerdict {
    Allow {
        policy_version: i64,
    },
    PendingReview {
        reason: ReasonCategory,
        policy_version: i64,
    },
}

impl RiskVerdict {
    pub fn policy_version(&self) -> i64 {
        match self {
            Self::Allow { policy_version } => *policy_version,
            Self::PendingReview { policy_version, .. } => *policy_version,
        }
    }

    pub fn is_pending_review(&self) -> bool {
        matches!(self, Self::PendingReview { .. })
    }

    pub fn reason_category(&self) -> Option<ReasonCategory> {
        match self {
            Self::PendingReview { reason, .. } => Some(*reason),
            Self::Allow { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_thresholds_are_sane() {
        let t = Thresholds::default();
        assert_eq!(t.new_user_max_posts, 3);
        assert!(t.new_user_grace_secs > 0);
        assert_eq!(t.max_links, 3);
        assert!(t.sensitive_words.is_empty());
        assert!(t.max_frequency_posts > 0);
        assert!(t.duplicate_window_secs > 0);
    }

    #[test]
    fn policy_roundtrip_json() {
        let p = RiskPolicy {
            version: 3,
            thresholds: Thresholds {
                max_links: 5,
                sensitive_words: vec!["x".into()],
                ..Thresholds::default()
            },
        };
        let parsed = RiskPolicy::parse(3, &p.to_json()).unwrap();
        assert_eq!(parsed.version, 3);
        assert_eq!(parsed.thresholds.max_links, 5);
        assert_eq!(parsed.thresholds.sensitive_words, vec!["x"]);
    }

    #[test]
    fn verdict_helpers() {
        let allow = RiskVerdict::Allow { policy_version: 2 };
        assert!(!allow.is_pending_review());
        assert_eq!(allow.policy_version(), 2);
        assert_eq!(allow.reason_category(), None);
        let pr = RiskVerdict::PendingReview {
            reason: ReasonCategory::LinkHeavy,
            policy_version: 2,
        };
        assert!(pr.is_pending_review());
        assert_eq!(pr.reason_category(), Some(ReasonCategory::LinkHeavy));
        assert_eq!(ReasonCategory::LinkHeavy.as_str(), "link_heavy");
    }
}
