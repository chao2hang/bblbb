//! M04-VISIBILITY：内容访问策略与防泄漏（P0）。
//!
//! 本模块是全部内容可读路径（list/detail/notifications/Feed/SEO/AI/attachments）
//! 必须复用的**唯一**访问决策引擎：
//!
//! - [`policy`]：封闭枚举助手与策略行解析（M04-VISIBILITY-01）——单一事实
//!   来源是 [`crate::domain::posts::AccessPolicy`]，不重新发明枚举；
//! - [`evaluate`]：统一 `evaluate(actor, content, context) -> AccessGrant`，
//!   grant 查询经 [`evaluate::GrantLookup`] 注入（DB 实现 + 单测 fake），
//!   查询失败一律 fail-closed（M04-VISIBILITY-02/06）；
//! - [`validate`]：`visibility_level ≤ 作者等级` 纯校验与稳定错误
//!   （M04-VISIBILITY-03）；
//! - [`projection`]：可复用投影过滤器——未解锁时正文/摘要/附件/搜索高亮/
//!   受限块等可逆编码字段**完全省略**（键缺失，不置 null；M04-VISIBILITY-07/09）；
//! - [`cache`]：persona 感知缓存头 `Cache-Control`/`Vary`/`ETag`
//!   （M04-VISIBILITY-08）——公共响应 `public, max-age=60` + `Vary: Cookie`
//!   + 由**完整投影体**派生的稳定 ETag；受限响应 `private, no-store`。
//!
//! ## 安全不变量
//!
//! 1. 评估只返回 grant（unlocked + reason + required_level + capabilities），
//!    **永不返回正文**；正文省略统一在 [`projection`] 收口；
//! 2. 所有 grant 查询失败按未解锁处理（fail-closed），绝不因 DB 抖动泄漏正文；
//! 3. 写路径（create/draft/publish/edit/scheduled）在服务端**重读**作者等级，
//!    拒绝 `visibility_level > author_level`（422 `visibility_level_exceeds_author`）；
//! 4. 匿名访问非 public 内容一律不解锁。

pub mod cache;
pub mod evaluate;
pub mod policy;
pub mod projection;
pub mod validate;

pub use cache::{cache_headers_for, etag_for_body, CacheHeaders};
pub use evaluate::{
    comment_grant_key, evaluate, post_grant_key, AccessContent, AccessGrant, Actor, DbGrantLookup,
    EvaluateContext, GrantLookup, GrantLookupError,
};
pub use policy::{
    effective_policy, is_supported_policy_name, legacy_visibility_policy, min_level_of,
    ALL_POLICY_NAMES,
};
pub use projection::{
    project_comment, project_post, AccessSummary, AttachmentRef, CommentFields, PostFields,
    ProjectionFilter,
};
pub use validate::{validate_visibility_level, VisibilityError};
