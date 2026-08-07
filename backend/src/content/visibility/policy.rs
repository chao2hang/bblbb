//! M04-VISIBILITY-01：访问策略封闭枚举助手与策略行解析。
//!
//! 单一事实来源是 [`crate::domain::posts::AccessPolicy`]
//! （`public/logged_in/after_reply/level/paid`）。本模块不重新发明枚举，只提供：
//!
//! - 封闭性判定（拒绝任意字符串 / named-user 可见性）；
//! - 策略行缺失回退（`posts.access_policy_id IS NULL` → public）；
//! - 遗留 `posts.visibility` 列桥接解析（0003 骨架行，值域与策略一致）；
//! - level 策略的 `min_level` 归一化（i64 → u32）。

use crate::content::model::ContentAccessPolicy;
use crate::domain::posts::AccessPolicy;

/// 全部受支持策略名（与 [`AccessPolicy::ALL`] 一一对应；文档/校验用）。
pub const ALL_POLICY_NAMES: &[&str] = &["public", "logged_in", "after_reply", "level", "paid"];

/// 判定策略名是否属于封闭枚举。`"private"/"followers"/"mentioned"` 等
/// named-user 可见性**一律拒绝**（枚举无用户定向成员）。
pub fn is_supported_policy_name(s: &str) -> bool {
    AccessPolicy::parse(s).is_some()
}

/// 策略行缺失/未设置（`posts.access_policy_id IS NULL`）→ 回退 public
/// （迁移 0037 语义：策略删除置空回退 public）。
pub fn effective_policy(policy: Option<&ContentAccessPolicy>) -> AccessPolicy {
    policy.map(|p| p.kind).unwrap_or(AccessPolicy::Public)
}

/// 遗留 `posts.visibility` 列 → 策略（0003 骨架行桥接；非法值回退 public——
/// 评估仍走 public，不会意外放行受限内容）。
pub fn legacy_visibility_policy(visibility: &str) -> AccessPolicy {
    AccessPolicy::parse(visibility).unwrap_or(AccessPolicy::Public)
}

/// level 策略的最低等级（u32 归一化；非 level 或未配置 → `None`）。
pub fn min_level_of(policy: &ContentAccessPolicy) -> Option<u32> {
    if policy.kind != AccessPolicy::Level {
        return None;
    }
    policy
        .min_level
        .map(|lv| lv.clamp(1, i64::from(u32::MAX)) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn closed_enum_accepts_exactly_five_values() {
        // 唯一事实来源：domain 枚举恰好 5 个成员，parse/as_str 双向闭环。
        assert_eq!(AccessPolicy::ALL.len(), 5, "封闭枚举必须恰好 5 值");
        for p in AccessPolicy::ALL {
            assert_eq!(AccessPolicy::parse(p.as_str()), Some(*p));
            assert!(
                ALL_POLICY_NAMES.contains(&p.as_str()),
                "ALL_POLICY_NAMES 遗漏 {}",
                p.as_str()
            );
        }
        for name in ALL_POLICY_NAMES {
            assert_eq!(AccessPolicy::parse(name).map(|p| p.as_str()), Some(*name));
        }
    }

    #[test]
    fn closed_enum_rejects_named_user_and_other_values() {
        // named-user 可见性 / 未来值 / 大小写 / 空白一律拒绝。
        for bad in [
            "private",
            "followers",
            "mentioned",
            "指定用户可见",
            "invite",
            "friends",
            "custom",
            "owner_only",
            "PUBLIC",
            "LoggedIn",
            " ",
            "",
        ] {
            assert_eq!(AccessPolicy::parse(bad), None, "{bad:?} 必须被拒绝");
            assert!(!is_supported_policy_name(bad), "{bad:?} 不是受支持策略");
        }
        // 枚举没有任何 user-targeting 成员：不存在承载用户标识的变体。
        assert!(!ALL_POLICY_NAMES.contains(&"private"));
    }

    #[test]
    fn effective_policy_falls_back_to_public() {
        assert_eq!(effective_policy(None), AccessPolicy::Public);
        let row = ContentAccessPolicy {
            id: "p".into(),
            kind: AccessPolicy::LoggedIn,
            min_level: None,
            currency_id: None,
            amount: None,
            reply_grant_persists: false,
            policy_version: 1,
            created_by: "u".into(),
            created_at: 0,
        };
        assert_eq!(effective_policy(Some(&row)), AccessPolicy::LoggedIn);
    }

    #[test]
    fn legacy_visibility_column_bridges() {
        for name in ALL_POLICY_NAMES {
            assert_eq!(
                legacy_visibility_policy(name),
                AccessPolicy::parse(name).unwrap()
            );
        }
        // 非法遗留值回退 public（不扩大权限）。
        assert_eq!(legacy_visibility_policy("bogus"), AccessPolicy::Public);
    }

    #[test]
    fn min_level_only_for_level_policy() {
        let base = ContentAccessPolicy {
            id: "p".into(),
            kind: AccessPolicy::Public,
            min_level: None,
            currency_id: None,
            amount: None,
            reply_grant_persists: false,
            policy_version: 1,
            created_by: "u".into(),
            created_at: 0,
        };
        let mut level = base.clone();
        level.kind = AccessPolicy::Level;
        level.min_level = Some(4);
        assert_eq!(min_level_of(&level), Some(4));
        level.min_level = Some(0); // 结构非法值按 1 收敛（fail-closed 下限）
        assert_eq!(min_level_of(&level), Some(1));
        let paid = ContentAccessPolicy {
            kind: AccessPolicy::Paid,
            ..base.clone()
        };
        assert_eq!(min_level_of(&paid), None);
    }
}
