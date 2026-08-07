//! M04-VISIBILITY-02/06：统一访问评估。
//!
//! `evaluate(actor, content, context) -> AccessGrant` 是全部内容可读路径
//! （list/detail/notifications/Feed/SEO/AI/attachments）必须复用的**唯一**
//! 访问决策函数。约定：
//!
//! - 返回 grant（解锁与否 + reason + required_level + capabilities），
//!   **绝不返回正文**——正文省略是 [`super::projection`] 的职责；
//! - grant 查询经 [`GrantLookup`] trait 注入（DB 实现 [`DbGrantLookup`]，
//!   单测用 fake），评估逻辑与数据库解耦；
//! - 一切 grant 查询失败按“未解锁”处理（fail-closed），绝不因 DB 抖动泄漏正文。
//!
//! ## 规则（M04-VISIBILITY-02）
//!
//! - `public` → 始终解锁（reason `"public"`）；
//! - `logged_in` → 有 actor 即解锁（reason `"logged_in"`）；
//! - `level` → `actor.level >= policy.min_level`（reason `"level"`；
//!   `required_level` 在 [`super::projection::AccessSummary`] 中暴露）；
//! - `after_reply` → 作者本人 / 管理 override / 有效 reply grant
//!   （`grant_target_key = post:{post_id}`，`source_kind='reply'`，
//!   `revoked_at IS NULL`；reason `"after_reply"`）；
//! - `paid` → 有效 purchase grant（`grant_target_key = post:{post_id}`，
//!   `source_kind='purchase'`，`revoked_at IS NULL`；reason `"paid"`。
//!   扣款/grant 创建是 M7，本里程碑只读 grant）；
//! - 匿名（actor=None）+ 非 public → 一律不解锁。

use std::future::Future;
use std::pin::Pin;

use sqlx::Either;

use crate::content::model::ContentAccessPolicy;
use crate::db::DatabasePool;
use crate::domain::posts::AccessPolicy;

/// 手动 async-trait：`BoxFuture` 使 [`GrantLookup`] 对象安全（无 async-trait
/// 依赖也能以 `&dyn GrantLookup` 注入 fake/DB 实现）。
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// 当前请求方身份（None = 匿名）。只含纯身份字段；管理 override 是
/// context 的临时授权（实时聚合传入），不混入 Actor。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Actor<'a> {
    pub id: &'a str,
    pub level: u32,
    pub username: &'a str,
}

/// 待评估内容（policy + visibility_level + author_level）。
///
/// - `grant_target_key`：归一化 grant 目标键（`post:{post_id}` /
///   `comment:{comment_id}`），after_reply/paid 查询的锚点；None = 无 grant
///   维度（纯 public/logged_in/level 内容）；
/// - `author_id`：内容作者（after_reply“作者自见”规则）；
/// - `min_level`：level 策略所需等级（u32 归一化）；
/// - `visibility_level` / `author_level`：写入路径已保证
///   `visibility_level ≤ author_level`；评估规则当前以 policy/min_level/grant
///   为准，二者为调用方审计/上报携带。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessContent<'a> {
    pub grant_target_key: Option<&'a str>,
    pub author_id: Option<&'a str>,
    pub policy: AccessPolicy,
    pub min_level: Option<u32>,
    pub visibility_level: u32,
    pub author_level: u32,
}

impl<'a> AccessContent<'a> {
    /// 从策略行构造评估输入（缺省 visibility_level=1；target/author 由调用方
    /// 按资源补全）。
    pub fn from_policy(policy: &ContentAccessPolicy) -> Self {
        Self {
            grant_target_key: None,
            author_id: None,
            policy: policy.kind,
            min_level: super::policy::min_level_of(policy),
            visibility_level: 1,
            author_level: 1,
        }
    }
}

/// 评估结果（reason 即解锁依据；不含任何内容数据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccessGrant {
    pub unlocked: bool,
    pub policy: AccessPolicy,
    pub reason: &'static str,
    /// level 策略的最低门槛（OpenAPI `access_summary.required_level`）。
    pub required_level: Option<u32>,
    /// UI 解锁提示（保持最小且安全，不含内容/用户数据）。
    pub capabilities: &'static [&'static str],
}

/// UI 解锁提示集合（安全：仅是枚举名，不含正文/用户数据）。
pub const CAP_NONE: &[&str] = &[];
pub const CAP_LOGIN: &[&str] = &["login"];
pub const CAP_UNLOCK_AFTER_REPLY: &[&str] = &["unlock_after_reply"];
pub const CAP_PURCHASE: &[&str] = &["purchase"];
pub const CAP_REQUEST_ACCESS: &[&str] = &["request_access"];

/// grant 查询失败（DB 层）；evaluate 一律按未解锁处理（fail-closed）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantLookupError(pub String);

/// grant 查询抽象：判定用户对某目标键是否存在 `source_kind` 的有效 grant
/// （`revoked_at IS NULL`）。evaluate 只依赖本 trait，不直接触碰数据库。
/// `Send + Sync` 使 `&dyn GrantLookup` 能跨 await 安全持有（axum handler
/// future 必须 `Send`；DB/内存实现均满足）。
pub trait GrantLookup: Send + Sync {
    fn has_valid_grant<'a>(
        &'a self,
        user_id: &'a str,
        grant_target_key: &'a str,
        source_kind: &'a str,
    ) -> BoxFuture<'a, Result<bool, GrantLookupError>>;
}

/// 评估上下文（grants 为 trait 对象，无 Debug 约束）。
#[derive(Clone, Copy)]
pub struct EvaluateContext<'g> {
    pub grants: &'g dyn GrantLookup,
    /// 服务端当前时间（Unix 毫秒；grant 有效性语义 M7 扩展，当前规则只按
    /// `revoked_at IS NULL` 判定）。
    pub now: i64,
    /// 管理/审核 override（after_reply 专用；由调用方依据权限实时聚合
    /// （`post.moderate`）传入，**禁止客户端自证**）。
    pub moderator_override: bool,
}

/// 统一访问评估（规则见模块文档）。
pub async fn evaluate(
    actor: Option<&Actor<'_>>,
    content: &AccessContent<'_>,
    ctx: &EvaluateContext<'_>,
) -> AccessGrant {
    use AccessPolicy::*;
    let policy = content.policy;
    match policy {
        Public => AccessGrant {
            unlocked: true,
            policy,
            reason: "public",
            required_level: None,
            capabilities: CAP_NONE,
        },
        LoggedIn => {
            let unlocked = actor.is_some();
            AccessGrant {
                unlocked,
                policy,
                reason: "logged_in",
                required_level: None,
                capabilities: if unlocked { CAP_NONE } else { CAP_LOGIN },
            }
        }
        Level => {
            let unlocked =
                actor.is_some_and(|a| content.min_level.is_some_and(|need| a.level >= need));
            AccessGrant {
                unlocked,
                policy,
                reason: "level",
                required_level: content.min_level,
                capabilities: if unlocked {
                    CAP_NONE
                } else {
                    CAP_REQUEST_ACCESS
                },
            }
        }
        AfterReply => {
            let is_author = actor.is_some_and(|a| content.author_id == Some(a.id));
            let has_grant = grant_for(actor, content, ctx, "reply").await;
            let unlocked = is_author || ctx.moderator_override || has_grant;
            AccessGrant {
                unlocked,
                policy,
                reason: "after_reply",
                required_level: None,
                capabilities: if unlocked {
                    CAP_NONE
                } else {
                    CAP_UNLOCK_AFTER_REPLY
                },
            }
        }
        Paid => {
            let has_grant = grant_for(actor, content, ctx, "purchase").await;
            AccessGrant {
                unlocked: has_grant,
                policy,
                reason: "paid",
                required_level: None,
                capabilities: if has_grant { CAP_NONE } else { CAP_PURCHASE },
            }
        }
    }
}

/// 查询有效 grant；任何查询失败 → false（fail-closed）。
async fn grant_for(
    actor: Option<&Actor<'_>>,
    content: &AccessContent<'_>,
    ctx: &EvaluateContext<'_>,
    source_kind: &str,
) -> bool {
    let (Some(a), Some(key)) = (actor, content.grant_target_key) else {
        return false;
    };
    ctx.grants
        .has_valid_grant(a.id, key, source_kind)
        .await
        .unwrap_or(false)
}

/// 归一化 grant 目标键（migration 0040 约定：`post:{post_id}`）。
pub fn post_grant_key(post_id: &str) -> String {
    format!("post:{post_id}")
}

/// 归一化 grant 目标键（migration 0040 约定：`comment:{comment_id}`）。
pub fn comment_grant_key(comment_id: &str) -> String {
    format!("comment:{comment_id}")
}

/// DB 版 grant 查询（sqlx Either 分支；SQLite 与 MySQL/MariaDB 同构）。
#[derive(Debug, Clone, Copy)]
pub struct DbGrantLookup<'p> {
    pub pool: &'p DatabasePool,
}

impl GrantLookup for DbGrantLookup<'_> {
    fn has_valid_grant<'a>(
        &'a self,
        user_id: &'a str,
        grant_target_key: &'a str,
        source_kind: &'a str,
    ) -> BoxFuture<'a, Result<bool, GrantLookupError>> {
        Box::pin(async move {
            let sql = "SELECT 1 FROM content_access_grants
                       WHERE user_id = ? AND grant_target_key = ? AND source_kind = ?
                         AND revoked_at IS NULL
                       LIMIT 1";
            let found: Option<i64> = match self.pool {
                Either::Left(p) => sqlx::query_scalar(sql)
                    .bind(user_id)
                    .bind(grant_target_key)
                    .bind(source_kind)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| GrantLookupError(e.to_string()))?,
                Either::Right(p) => sqlx::query_scalar(sql)
                    .bind(user_id)
                    .bind(grant_target_key)
                    .bind(source_kind)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| GrantLookupError(e.to_string()))?,
            };
            Ok(found.is_some())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 单测 fake：内存 grant 表 + 可选失败注入。
    struct FakeGrants {
        grants: Vec<(String, String, String)>,
        fail: bool,
    }

    impl FakeGrants {
        fn new() -> Self {
            Self {
                grants: Vec::new(),
                fail: false,
            }
        }
        fn grant(mut self, user: &str, key: &str, kind: &str) -> Self {
            self.grants
                .push((user.to_string(), key.to_string(), kind.to_string()));
            self
        }
        fn fail(mut self) -> Self {
            self.fail = true;
            self
        }
    }

    impl GrantLookup for FakeGrants {
        fn has_valid_grant<'a>(
            &'a self,
            user_id: &'a str,
            grant_target_key: &'a str,
            source_kind: &'a str,
        ) -> BoxFuture<'a, Result<bool, GrantLookupError>> {
            Box::pin(async move {
                if self.fail {
                    return Err(GrantLookupError("boom".to_string()));
                }
                Ok(self
                    .grants
                    .iter()
                    .any(|(u, k, s)| u == user_id && k == grant_target_key && s == source_kind))
            })
        }
    }

    fn ctx<'a>(grants: &'a dyn GrantLookup, moderator_override: bool) -> EvaluateContext<'a> {
        EvaluateContext {
            grants,
            now: 1_000,
            moderator_override,
        }
    }

    fn content(policy: AccessPolicy, min_level: Option<u32>) -> AccessContent<'static> {
        AccessContent {
            grant_target_key: None,
            author_id: None,
            policy,
            min_level,
            visibility_level: 1,
            author_level: 5,
        }
    }

    fn author<'a>(id: &'a str, level: u32) -> Actor<'a> {
        Actor {
            id,
            level,
            username: "alice",
        }
    }

    #[tokio::test]
    async fn public_always_unlocked_even_anonymous() {
        let fake = FakeGrants::new();
        let c = content(AccessPolicy::Public, None);
        let g = evaluate(None, &c, &ctx(&fake, false)).await;
        assert!(g.unlocked);
        assert_eq!(g.reason, "public");
        assert_eq!(g.required_level, None);
        assert_eq!(g.capabilities, CAP_NONE);
    }

    #[tokio::test]
    async fn logged_in_requires_actor() {
        let fake = FakeGrants::new();
        let c = content(AccessPolicy::LoggedIn, None);
        let g = evaluate(None, &c, &ctx(&fake, false)).await;
        assert!(!g.unlocked, "匿名不得解锁 logged_in");
        assert_eq!(g.reason, "logged_in");
        assert_eq!(g.capabilities, CAP_LOGIN);

        let a = author("u1", 1);
        let g = evaluate(Some(&a), &c, &ctx(&fake, false)).await;
        assert!(g.unlocked);
        assert_eq!(g.capabilities, CAP_NONE);
    }

    #[tokio::test]
    async fn level_requires_min_level_and_exposes_required_level() {
        let fake = FakeGrants::new();
        let c = content(AccessPolicy::Level, Some(4));

        let low = author("u1", 3);
        let g = evaluate(Some(&low), &c, &ctx(&fake, false)).await;
        assert!(!g.unlocked, "等级 3 < 门槛 4 必须锁定");
        assert_eq!(g.reason, "level");
        assert_eq!(g.required_level, Some(4));
        assert_eq!(g.capabilities, CAP_REQUEST_ACCESS);

        let high = author("u1", 4);
        let g = evaluate(Some(&high), &c, &ctx(&fake, false)).await;
        assert!(g.unlocked);
        assert_eq!(g.required_level, Some(4));

        // min_level 缺失（非法策略行）→ fail-closed 不解锁
        let broken = content(AccessPolicy::Level, None);
        let g = evaluate(Some(&high), &broken, &ctx(&fake, false)).await;
        assert!(!g.unlocked, "min_level 缺失必须不解锁");
        assert_eq!(g.required_level, None);
    }

    #[tokio::test]
    async fn after_reply_grant_author_and_moderator_override() {
        let key = post_grant_key("p1");
        let fake = FakeGrants::new().grant("buyer", &key, "reply");
        let c = AccessContent {
            grant_target_key: Some(&key),
            author_id: Some("author1"),
            policy: AccessPolicy::AfterReply,
            min_level: None,
            visibility_level: 1,
            author_level: 5,
        };

        // 无 grant 的非作者 → 锁定
        let stranger = author("s1", 5);
        let g = evaluate(Some(&stranger), &c, &ctx(&fake, false)).await;
        assert!(!g.unlocked);
        assert_eq!(g.reason, "after_reply");
        assert_eq!(g.capabilities, CAP_UNLOCK_AFTER_REPLY);

        // 有 reply grant 的买家 → 解锁
        let buyer = author("buyer", 1);
        let g = evaluate(Some(&buyer), &c, &ctx(&fake, false)).await;
        assert!(g.unlocked, "有效 reply grant 必须解锁");
        assert_eq!(g.reason, "after_reply");

        // 作者自见（无 grant）
        let owner = author("author1", 2);
        let g = evaluate(Some(&owner), &c, &ctx(&fake, false)).await;
        assert!(g.unlocked, "作者始终能看自己内容");

        // 管理 override（moderator_override=true）→ 解锁
        let g = evaluate(Some(&stranger), &c, &ctx(&fake, true)).await;
        assert!(g.unlocked, "管理 override 必须解锁");

        // 匿名 → 锁定
        let g = evaluate(None, &c, &ctx(&fake, false)).await;
        assert!(!g.unlocked, "匿名不得解锁 after_reply");
    }

    #[tokio::test]
    async fn paid_requires_purchase_grant() {
        let key = post_grant_key("p1");
        let fake = FakeGrants::new().grant("buyer", &key, "purchase");
        let c = AccessContent {
            grant_target_key: Some(&key),
            author_id: Some("author1"),
            policy: AccessPolicy::Paid,
            min_level: None,
            visibility_level: 1,
            author_level: 5,
        };

        // 无 purchase grant → 锁定（含作者本人：本里程碑 paid 只认 grant）
        let owner = author("author1", 5);
        let g = evaluate(Some(&owner), &c, &ctx(&fake, false)).await;
        assert!(!g.unlocked);
        assert_eq!(g.reason, "paid");
        assert_eq!(g.capabilities, CAP_PURCHASE);

        // 有 purchase grant → 解锁
        let buyer = author("buyer", 1);
        let g = evaluate(Some(&buyer), &c, &ctx(&fake, false)).await;
        assert!(g.unlocked);
        assert_eq!(g.reason, "paid");
        assert_eq!(g.capabilities, CAP_NONE);

        // 只有 reply grant（无 purchase）→ 不解锁
        let only_reply = FakeGrants::new().grant("buyer", &key, "reply");
        let g = evaluate(Some(&buyer), &c, &ctx(&only_reply, false)).await;
        assert!(!g.unlocked, "reply grant 不能解锁 paid 内容");

        // 匿名 → 锁定
        let g = evaluate(None, &c, &ctx(&fake, false)).await;
        assert!(!g.unlocked);
    }

    #[tokio::test]
    async fn grant_lookup_failure_is_fail_closed() {
        let key = post_grant_key("p1");
        let fake = FakeGrants::new().grant("buyer", &key, "purchase").fail();
        let c = AccessContent {
            grant_target_key: Some(&key),
            author_id: None,
            policy: AccessPolicy::Paid,
            min_level: None,
            visibility_level: 1,
            author_level: 5,
        };
        let buyer = author("buyer", 1);
        let g = evaluate(Some(&buyer), &c, &ctx(&fake, false)).await;
        assert!(!g.unlocked, "grant 查询失败必须不解锁（fail-closed）");
        assert_eq!(g.reason, "paid");
    }

    #[tokio::test]
    async fn anonymous_non_public_never_unlocked() {
        let fake = FakeGrants::new();
        for policy in [
            AccessPolicy::LoggedIn,
            AccessPolicy::Level,
            AccessPolicy::AfterReply,
            AccessPolicy::Paid,
        ] {
            let c = content(policy, Some(1));
            let g = evaluate(None, &c, &ctx(&fake, false)).await;
            assert!(!g.unlocked, "{policy:?} 匿名必须锁定");
        }
    }

    #[test]
    fn grant_key_normalization_is_stable() {
        assert_eq!(
            post_grant_key("01911fd5-f000-7561-a2a5-3dd6434157f0"),
            "post:01911fd5-f000-7561-a2a5-3dd6434157f0"
        );
        assert_eq!(comment_grant_key("c1"), "comment:c1");
    }
}
