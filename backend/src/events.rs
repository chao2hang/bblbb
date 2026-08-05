//! 领域事件注册表（M01-AUDIT-07）。
//!
//! 事件名遵循 `<domain>.<action>.v<major>`；`payload_version` 默认 1。
//! 本注册表与 `docs/EVENT-CATALOG.md` 事件目录保持一致，由
//! `scripts/check-event-catalog.rb` 机械比对（缺失/漂移/版本不一致即失败）。
//!
//! Outbox 与业务事实同事务写入（M01-JOBS-02）；消费者按 `event_id`
//! 幂等去重（M01-JOBS-06）。

/// 事件类型常量（与 EVENT-CATALOG.md 一一对应）。
pub mod types {
    pub const USER_REGISTERED: &str = "user.registered.v1";
    pub const USER_STATUS_CHANGED: &str = "user.status_changed.v1";
    pub const POST_PUBLISHED: &str = "post.published.v1";
    pub const POST_VISIBILITY_CHANGED: &str = "post.visibility_changed.v1";
    pub const COMMENT_CREATED: &str = "comment.created.v1";
    pub const MODERATION_CASE_CHANGED: &str = "moderation.case_changed.v1";
    pub const SANCTION_CHANGED: &str = "sanction.changed.v1";
    pub const ATTACHMENT_READY: &str = "attachment.ready.v1";
    pub const DOWNLOAD_AUTHORIZATION_CREATED: &str = "download.authorization_created.v1";
    pub const POINTS_OPERATION_COMPLETED: &str = "points.operation_completed.v1";
    pub const MARKETPLACE_PURCHASE_SUCCEEDED: &str = "marketplace.purchase_succeeded.v1";
    pub const MARKETPLACE_REFUND_SUCCEEDED: &str = "marketplace.refund_succeeded.v1";
    pub const MARKETPLACE_SETTLEMENT_DUE: &str = "marketplace.settlement_due.v1";
    pub const AI_TASK_COMPLETED: &str = "ai.task_completed.v1";
    pub const VIDEO_EMBED_CHANGED: &str = "video.embed_changed.v1";
    pub const SHOP_ORDER_SUCCEEDED: &str = "shop.order_succeeded.v1";
    pub const SHOP_ENTITLEMENT_CHANGED: &str = "shop.entitlement_changed.v1";
    pub const ACTIVITY_CLAIMED: &str = "activity.claimed.v1";
    pub const REACTION_CREATED: &str = "reaction.created.v1";
    pub const REACTION_REMOVED: &str = "reaction.removed.v1";
    pub const CONFIG_POLICY_CHANGED: &str = "config.policy_changed.v1";
}

/// 全部注册事件类型（有序，供检查/枚举使用）。
pub fn all_event_types() -> &'static [&'static str] {
    &[
        types::USER_REGISTERED,
        types::USER_STATUS_CHANGED,
        types::POST_PUBLISHED,
        types::POST_VISIBILITY_CHANGED,
        types::COMMENT_CREATED,
        types::MODERATION_CASE_CHANGED,
        types::SANCTION_CHANGED,
        types::ATTACHMENT_READY,
        types::DOWNLOAD_AUTHORIZATION_CREATED,
        types::POINTS_OPERATION_COMPLETED,
        types::MARKETPLACE_PURCHASE_SUCCEEDED,
        types::MARKETPLACE_REFUND_SUCCEEDED,
        types::MARKETPLACE_SETTLEMENT_DUE,
        types::AI_TASK_COMPLETED,
        types::VIDEO_EMBED_CHANGED,
        types::SHOP_ORDER_SUCCEEDED,
        types::SHOP_ENTITLEMENT_CHANGED,
        types::ACTIVITY_CLAIMED,
        types::REACTION_CREATED,
        types::REACTION_REMOVED,
        types::CONFIG_POLICY_CHANGED,
    ]
}

/// 事件 payload 版本（当前目录全部为 v1；升级时在此递增并同步目录）。
pub fn payload_version(event_type: &str) -> i64 {
    // event_type 形如 `<domain>.<action>.v<major>`，major 即 payload_version
    match event_type.rsplit_once(".v") {
        Some((_, major)) => major.parse::<i64>().unwrap_or(1),
        None => 1,
    }
}

/// 校验事件类型格式合法且已注册。
pub fn is_known_event(event_type: &str) -> bool {
    all_event_types().contains(&event_type)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_registered_events_follow_naming_and_version() {
        for event in all_event_types() {
            let parts: Vec<&str> = event.split('.').collect();
            assert!(
                parts.len() >= 3,
                "{event} 必须为 <domain>.<action>.v<major>"
            );
            assert!(event.ends_with(".v1"), "{event} 当前目录版本必须为 v1");
            assert_eq!(payload_version(event), 1);
        }
    }

    #[test]
    fn registry_has_no_duplicates() {
        let mut events = all_event_types().to_vec();
        let total = events.len();
        events.sort();
        events.dedup();
        assert_eq!(events.len(), total, "事件注册表不得有重复");
        assert_eq!(total, 21, "与 EVENT-CATALOG.md 的 21 个事件一致");
    }

    #[test]
    fn is_known_event_matches() {
        assert!(is_known_event("post.published.v1"));
        assert!(!is_known_event("post.published.v2"));
        assert!(!is_known_event("unknown.event.v1"));
    }
}
