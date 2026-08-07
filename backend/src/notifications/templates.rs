//! M05-NOTIFY-01/02：通知模板键注册表与安全渲染。
//!
//! 模板键覆盖回复、引用、提及、审核、处罚、申诉、等级与安全通知。
//! 渲染只消费白名单标量参数（actor/kind/count 等），绝不复制隐藏正文或
//! 内部 note——`params` 中含 `body`/`content`/`note` 等键即拒绝。

/// 模板键 → 安全参数白名单。渲染只允许这些键出现。
const TEMPLATE_PARAM_WHITELIST: &[(&str, &[&str])] = &[
    ("reply.created", &["actor_name"]),
    ("quote.referenced", &["actor_name"]),
    ("mention.created", &["actor_name", "mention_count"]),
    ("moderation.action", &["action", "resource_type"]),
    ("sanction.applied", &["kind", "expires_hint"]),
    ("sanction.revoked", &["kind"]),
    ("appeal.changed", &["status"]),
    ("level.up", &["level"]),
    ("security.notice", &["kind"]),
];

/// 禁止出现在通知参数中的键（隐藏正文/内部 note 一律拒绝）。
pub const FORBIDDEN_NOTIFICATION_PARAMS: &[&str] = &[
    "body",
    "content",
    "markdown",
    "text",
    "note",
    "internal_note",
    "reason",
    "moderator_note",
    "decision_note",
    "post_body",
    "excerpt",
];

/// 通知模板键（M05-NOTIFY-01）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TemplateKey {
    ReplyCreated,
    QuoteReferenced,
    MentionCreated,
    ModerationAction,
    SanctionApplied,
    SanctionRevoked,
    AppealChanged,
    LevelUp,
    SecurityNotice,
}

impl TemplateKey {
    pub const ALL: [TemplateKey; 9] = [
        TemplateKey::ReplyCreated,
        TemplateKey::QuoteReferenced,
        TemplateKey::MentionCreated,
        TemplateKey::ModerationAction,
        TemplateKey::SanctionApplied,
        TemplateKey::SanctionRevoked,
        TemplateKey::AppealChanged,
        TemplateKey::LevelUp,
        TemplateKey::SecurityNotice,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            TemplateKey::ReplyCreated => "reply.created",
            TemplateKey::QuoteReferenced => "quote.referenced",
            TemplateKey::MentionCreated => "mention.created",
            TemplateKey::ModerationAction => "moderation.action",
            TemplateKey::SanctionApplied => "sanction.applied",
            TemplateKey::SanctionRevoked => "sanction.revoked",
            TemplateKey::AppealChanged => "appeal.changed",
            TemplateKey::LevelUp => "level.up",
            TemplateKey::SecurityNotice => "security.notice",
        }
    }

    pub fn parse(s: &str) -> Option<TemplateKey> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }

    /// 遗留通知类型（notifications.type CHECK）。
    pub fn legacy_type(self) -> &'static str {
        match self {
            TemplateKey::ReplyCreated
            | TemplateKey::QuoteReferenced
            | TemplateKey::MentionCreated => "reply",
            TemplateKey::ModerationAction
            | TemplateKey::SanctionApplied
            | TemplateKey::SanctionRevoked
            | TemplateKey::AppealChanged => "moderation",
            TemplateKey::LevelUp => "badge",
            TemplateKey::SecurityNotice => "system",
        }
    }
}

/// 模板键是否合法（注册表存在）。
pub fn is_known_template(key: &str) -> bool {
    TemplateKey::parse(key).is_some()
}

/// 渲染结果的标题与正文。
pub struct RenderedNotification {
    pub title: String,
    pub body: Option<String>,
}

/// 按模板键 + 白名单参数渲染站内通知（M05-NOTIFY-01/02）。
///
/// 仅使用白名单参数（标量）；非法参数键被忽略（比拒绝更稳——渲染永不因
/// 多传参数而失败，但敏感键在 [`validate_params`] 层被拒绝）。
pub fn render(
    key: TemplateKey,
    params: &serde_json::Map<String, serde_json::Value>,
) -> RenderedNotification {
    let p = |name: &str| -> Option<String> {
        params
            .get(name)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
    };
    let title = match key {
        TemplateKey::ReplyCreated => "有新回复".to_string(),
        TemplateKey::QuoteReferenced => "有新的引用".to_string(),
        TemplateKey::MentionCreated => "有人提及了你".to_string(),
        TemplateKey::ModerationAction => "内容审核结果".to_string(),
        TemplateKey::SanctionApplied => "收到处罚通知".to_string(),
        TemplateKey::SanctionRevoked => "处罚已撤销".to_string(),
        TemplateKey::AppealChanged => "申诉状态更新".to_string(),
        TemplateKey::LevelUp => "等级提升".to_string(),
        TemplateKey::SecurityNotice => "安全提醒".to_string(),
    };
    let body = match key {
        TemplateKey::ReplyCreated => p("actor_name").map(|name| format!("{name} 回复了你的内容")),
        TemplateKey::QuoteReferenced => {
            p("actor_name").map(|name| format!("{name} 引用了你的内容"))
        }
        TemplateKey::MentionCreated => {
            let count = p("mention_count").unwrap_or_else(|| "1".to_string());
            p("actor_name")
                .map(|name| format!("{name} 等 {count} 人提及了你"))
                .or(Some(format!("{count} 人提及了你")))
        }
        TemplateKey::ModerationAction => {
            let action = p("action").unwrap_or_else(|| "update".to_string());
            let resource = p("resource_type").unwrap_or_else(|| "内容".to_string());
            Some(format!("你的{resource}已{action}，详见站内详情"))
        }
        TemplateKey::SanctionApplied => {
            let kind = p("kind").unwrap_or_else(|| "处罚".to_string());
            let hint = p("expires_hint").unwrap_or_else(|| "永久".to_string());
            Some(format!("你收到一项{kind}，期限：{hint}；如有异议可申诉"))
        }
        TemplateKey::SanctionRevoked => {
            let kind = p("kind").unwrap_or_else(|| "处罚".to_string());
            Some(format!("你的{kind}已被撤销"))
        }
        TemplateKey::AppealChanged => {
            let status = p("status").unwrap_or_else(|| "更新".to_string());
            Some(format!("你的申诉状态变更为：{status}"))
        }
        TemplateKey::LevelUp => {
            let level = p("level").unwrap_or_else(|| "—".to_string());
            Some(format!("你已升到 Lv.{level}"))
        }
        TemplateKey::SecurityNotice => {
            let kind = p("kind").unwrap_or_else(|| "安全更新".to_string());
            Some(format!("账户安全提醒：{kind}，详情请查看站内安全中心"))
        }
    };
    RenderedNotification { title, body }
}

/// 校验通知参数（M05-NOTIFY-02）：拒绝携带隐藏正文/内部 note 键。
pub fn validate_params(params: &serde_json::Map<String, serde_json::Value>) -> Result<(), String> {
    for key in params.keys() {
        if FORBIDDEN_NOTIFICATION_PARAMS.contains(&key.as_str()) {
            return Err(format!(
                "notification params must not carry hidden content or internal note (`{key}`)"
            ));
        }
    }
    Ok(())
}

/// 校验给定模板键允许出现的参数（白名单之外的键只允许标量或忽略）。
pub fn allowed_params_for(key: &str) -> Option<&'static [&'static str]> {
    TEMPLATE_PARAM_WHITELIST
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, list)| *list)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(items: &[(&str, &str)]) -> serde_json::Map<String, serde_json::Value> {
        items
            .iter()
            .map(|(k, v)| (k.to_string(), serde_json::Value::String(v.to_string())))
            .collect()
    }

    #[test]
    fn template_keys_are_registered_and_parse() {
        for key in TemplateKey::ALL {
            assert!(is_known_template(key.as_str()));
            assert_eq!(TemplateKey::parse(key.as_str()), Some(key));
        }
        assert!(!is_known_template("unknown.template"));
        assert!(TemplateKey::parse("unknown.template").is_none());
    }

    #[test]
    fn render_uses_only_whitelist_params() {
        let params = map(&[("actor_name", "小明"), ("level", "5")]);
        let r = render(TemplateKey::ReplyCreated, &params);
        assert_eq!(r.title, "有新回复");
        assert!(r.body.unwrap().contains("小明"));

        let r = render(TemplateKey::LevelUp, &params);
        assert!(r.body.unwrap().contains("Lv.5"));
    }

    #[test]
    fn missing_params_use_safe_defaults() {
        let r = render(TemplateKey::SanctionApplied, &serde_json::Map::new());
        assert!(r.body.unwrap().contains("永久"));
        let r = render(TemplateKey::LevelUp, &serde_json::Map::new());
        assert!(r.body.unwrap().contains("Lv.—"));
    }

    #[test]
    fn forbidden_params_are_rejected() {
        for forbidden in FORBIDDEN_NOTIFICATION_PARAMS {
            let mut params = serde_json::Map::new();
            params.insert(
                (*forbidden).to_string(),
                serde_json::Value::String("隐藏正文".to_string()),
            );
            let err = validate_params(&params).unwrap_err();
            assert!(err.contains(forbidden), "must reject {forbidden}");
        }
    }
}
