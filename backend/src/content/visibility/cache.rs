//! M04-VISIBILITY-08：persona 感知缓存头。
//!
//! 只读内容响应的缓存头统一决策：
//!
//! - `public` 策略 → `Cache-Control: public, max-age=60` +
//!   `Vary: Cookie`（persona 随会话 Cookie 变化）+ 稳定 `ETag`——
//!   ETag 由**完整投影体**派生，因此两个 persona 的可见性不同时，
//!   ETag 必然不同，304 永不跨 persona 泄漏；
//! - 其余策略（logged_in/after_reply/level/paid）→
//!   `Cache-Control: private, no-store`，不跨 persona 缓存（无 ETag）。
//!
//! 决策键是**策略类型**而非 unlocked 标志：受限策略即使当前 actor 解锁
//! （作者/管理），响应仍按 persona 隔离。

use super::evaluate::AccessGrant;
use crate::domain::posts::AccessPolicy;

/// 缓存头三元组（路由层直接写入响应头）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheHeaders {
    pub cache_control: &'static str,
    pub vary: Option<&'static str>,
    pub etag: Option<String>,
}

/// 依据 grant 决策缓存头（规则见模块文档）。
pub fn cache_headers_for(grant: &AccessGrant, projected_body: &str) -> CacheHeaders {
    if grant.policy == AccessPolicy::Public {
        CacheHeaders {
            cache_control: "public, max-age=60",
            vary: Some("Cookie"),
            etag: Some(etag_for_body(projected_body)),
        }
    } else {
        CacheHeaders {
            cache_control: "private, no-store",
            vary: None,
            etag: None,
        }
    }
}

/// 稳定 ETag：sha256(投影体) 前 8 字节 hex，RFC 7232 引号强校验器格式。
/// 同一 persona + 同一内容 → 确定性一致；不同 persona（不同投影体）→ 不同。
pub fn etag_for_body(body: &str) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(body.as_bytes());
    let out = hasher.finalize();
    let hex: String = out[..8].iter().map(|b| format!("{b:02x}")).collect();
    format!("\"vis-{hex}\"")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::visibility::evaluate::{AccessGrant, CAP_UNLOCK_AFTER_REPLY};

    fn grant(policy: AccessPolicy, unlocked: bool) -> AccessGrant {
        AccessGrant {
            unlocked,
            policy,
            reason: policy.as_str(),
            required_level: None,
            capabilities: if unlocked {
                &[]
            } else {
                CAP_UNLOCK_AFTER_REPLY
            },
        }
    }

    #[test]
    fn public_responses_are_cacheable_with_vary_cookie() {
        let headers = cache_headers_for(&grant(AccessPolicy::Public, true), r#"{"id":"p1"}"#);
        assert_eq!(headers.cache_control, "public, max-age=60");
        assert_eq!(headers.vary, Some("Cookie"), "persona 随 Cookie 变化");
        assert!(headers.etag.is_some(), "public 必须派生稳定 ETag");
    }

    #[test]
    fn restricted_responses_are_private_no_store() {
        // 受限策略即使解锁（作者/管理）也按 persona 隔离。
        for (policy, unlocked) in [
            (AccessPolicy::LoggedIn, true),
            (AccessPolicy::AfterReply, true),
            (AccessPolicy::Level, true),
            (AccessPolicy::Paid, true),
            (AccessPolicy::AfterReply, false),
        ] {
            let headers = cache_headers_for(&grant(policy, unlocked), r#"{"id":"p1"}"#);
            assert_eq!(
                headers.cache_control, "private, no-store",
                "{policy:?} unlocked={unlocked} 必须 no-store"
            );
            assert_eq!(headers.vary, None, "{policy:?} 不得带 Vary");
            assert_eq!(
                headers.etag, None,
                "{policy:?} 不得发 ETag（跨 persona 泄漏面）"
            );
        }
    }

    /// M04-VISIBILITY-08：同一输入、不同 persona → 不同 ETag。
    #[test]
    fn different_personas_never_share_etag() {
        // 同一内容（同 id），但 persona 可见性不同 → 投影体不同 → ETag 不同。
        let anon = cache_headers_for(&grant(AccessPolicy::Public, true), r#"{"id":"p1"}"#);
        let unlocked_full = cache_headers_for(
            &grant(AccessPolicy::Public, true),
            r#"{"id":"p1","body_html":"<p>secret</p>"}"#,
        );
        assert_ne!(anon.etag, unlocked_full.etag, "不同投影体不得共享 ETag");
        assert!(anon.etag.unwrap().starts_with('"'));
    }

    #[test]
    fn etag_is_deterministic_for_same_persona_and_content() {
        let body = r#"{"id":"p1","access_summary":{"policy":"public","unlocked":true}}"#;
        let a = etag_for_body(body);
        let b = etag_for_body(body);
        assert_eq!(a, b, "同一 persona+内容 ETag 必须确定一致");
        let c = etag_for_body(&format!("{body}x"));
        assert_ne!(a, c, "内容变化 ETag 必须变化");
        // 格式稳定（可被 HeaderValue 接受）
        assert!(axum::http::HeaderValue::from_str(&a).is_ok());
    }
}
