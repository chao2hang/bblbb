//! 索引写入可见性裁决（M03-SEARCH-STORE-05，P0；M08-INDEX-02 统一排除规则）。
//!
//! 索引写入只接受经过可见性裁决的安全文本，绝不存 `restricted_html`：
//!
//! 1. 可见性裁决——[`decide_post_indexability`] / [`decide_user_indexability`] /
//!    [`decide_board_indexability`] / [`decide_tag_indexability`]：
//!    draft/hidden/locked/deleted 帖子、非 public 可见性帖子、停用或非公开板块、
//!    停用/删除账号、停用/隐藏板块、停用标签一律排除（被排除即从索引移除，
//!    绝不把受限内容写入索引）；
//! 2. **统一排除规则（M08-INDEX-02）**——[`decide_public_post_indexability`]
//!    在 M03 基础上叠加 M04/M05/M08 的公开投影边界：审核中
//!    （`review_status='pending_review'`）、已删除（`deleted_at` 非空）、访问策略
//!    非 public（`content_access_policies.kind`，含 logged_in/after_reply/level/
//!    paid——即便遗留 `posts.visibility` 列仍为 public）、作者逐帖退出
//!    （`search_index_opt_out`）、管理员全站/板块关闭索引（deny 优先）一律排除；
//! 3. 安全文本——[`vet_index_text`]：索引正文/摘要必须是无 HTML 标记的清洗纯文本，
//!    拒绝 `restricted_html`/`restricted_markdown` 特征串与 `<标签>`，把受限部分
//!    挡在索引输入面之外（第二道防线；第一道是写路径只接收公开投影字段）。
//!
//! 受限部分（posts/comments 的 `restricted_markdown`/`restricted_html`，
//! SCHEMA.md §7）永远不进入本模块的任何输入面；本模块也绝不返回受限内容。

/// 帖子可索引性裁决。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexDecision {
    /// 可入索引（安全文本由写路径经 `vet_index_text` + `clean_index_text` 提供）。
    Indexable,
    /// 不可入索引（应触发从索引移除）。
    Excluded(ExclusionReason),
}

/// 排除原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionReason {
    /// post.status 不是 published（draft/pending_review/rejected/hidden/locked/deleted）。
    NotPublished,
    /// post.visibility 不是 public（logged_in/after_reply/level/paid 均不入公开索引）。
    NotPublic,
    /// 板块停用（board.is_active = 0）。
    BoardInactive,
    /// 板块非公开可见（members/restricted/hidden——内容对匿名不可见，不入公开索引）。
    BoardNotPublic,
    /// 作者账号停用/删除（users.status 非 active，或 deleted_at 非空）。
    AuthorUnavailable,
    /// 标签停用（tags.is_active = 0）。
    TagInactive,
    /// M08-INDEX-02：帖子已删除（posts.deleted_at 非空；软删除生命周期）。
    Deleted,
    /// M08-INDEX-02：审核中（posts.review_status = 'pending_review'）。
    UnderReview,
    /// M08-INDEX-02：有效访问策略非 public（content_access_policies.kind 为
    /// logged_in/after_reply/level/paid——公开索引只收 policy 为 public 的内容）。
    PolicyNotPublic,
    /// M08-INDEX-03：作者逐帖退出（posts.search_index_opt_out = 1）。
    AuthorOptedOut,
    /// M08-INDEX-03：管理员全站/板块策略关闭索引（deny，优先于作者 allow）。
    AdminIndexDisabled,
}

/// 帖子公开可索引性统一裁决输入（M08-INDEX-02/03）。
///
/// 覆盖 M03 基座 + M04 访问策略（`policy_kind`）+ M05 审核状态
/// （`review_status`）+ M08 逐帖退出/管理员策略：
#[allow(clippy::too_many_arguments)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PostPublicIndexInput<'a> {
    /// post.status（published 之外的 draft/hidden/deleted 等一律排除）。
    pub status: &'a str,
    /// 遗留 posts.visibility 列（public 之外排除）。
    pub visibility: &'a str,
    /// 有效访问策略 kind（content_access_policies.kind；`None` = 未设策略 =
    /// public）。非 public（logged_in/after_reply/level/paid）一律排除。
    pub policy_kind: Option<&'a str>,
    /// 板块启用。
    pub board_active: bool,
    /// 板块可见性（members/restricted/hidden 排除）。
    pub board_visibility: &'a str,
    /// 作者账号状态（active 之外排除）。
    pub author_status: &'a str,
    /// 作者删除时间（非空排除）。
    pub author_deleted_at: Option<i64>,
    /// 帖子删除时间（非空排除，M08-INDEX-02）。
    pub deleted_at: Option<i64>,
    /// 审核状态（Some("pending_review") 排除，M08-INDEX-02）。
    pub review_status: Option<&'a str>,
    /// 作者逐帖退出搜索索引（M08-INDEX-03）。
    pub search_index_opt_out: bool,
    /// 管理员全站/板块策略（'allow' 或 'deny'；deny 强制排除）。
    pub admin_search_index: &'a str,
}

/// 帖子公开可索引性统一裁决（M08-INDEX-02/03，P0）。
///
/// 检查顺序即排除优先级：状态 → 访问策略 → 板块 → 作者 → 删除 → 审核中 →
/// 作者逐帖退出 → 管理员策略。任一命中即 `Excluded`（触发从索引移除）；
/// 全部通过才 `Indexable`。
pub fn decide_public_post_indexability(input: &PostPublicIndexInput<'_>) -> IndexDecision {
    if input.status != "published" {
        return IndexDecision::Excluded(ExclusionReason::NotPublished);
    }
    if input.visibility != "public" {
        return IndexDecision::Excluded(ExclusionReason::NotPublic);
    }
    // 有效访问策略（M04 权威来源）非 public → 不入公开索引。
    if let Some(kind) = input.policy_kind {
        if kind != "public" {
            return IndexDecision::Excluded(ExclusionReason::PolicyNotPublic);
        }
    }
    if !input.board_active {
        return IndexDecision::Excluded(ExclusionReason::BoardInactive);
    }
    if input.board_visibility != "public" {
        return IndexDecision::Excluded(ExclusionReason::BoardNotPublic);
    }
    if input.author_status != "active" || input.author_deleted_at.is_some() {
        return IndexDecision::Excluded(ExclusionReason::AuthorUnavailable);
    }
    if input.deleted_at.is_some() {
        return IndexDecision::Excluded(ExclusionReason::Deleted);
    }
    if input.review_status == Some("pending_review") {
        return IndexDecision::Excluded(ExclusionReason::UnderReview);
    }
    if input.search_index_opt_out {
        return IndexDecision::Excluded(ExclusionReason::AuthorOptedOut);
    }
    if input.admin_search_index == "deny" {
        return IndexDecision::Excluded(ExclusionReason::AdminIndexDisabled);
    }
    IndexDecision::Indexable
}

/// 帖子可见性裁决（M03-SEARCH-STORE-05）：published + public + 板块公开可见
/// + 作者账号可用才可入索引。
#[allow(clippy::too_many_arguments)]
pub fn decide_post_indexability(
    status: &str,
    visibility: &str,
    board_active: bool,
    board_visibility: &str,
    author_status: &str,
    author_deleted_at: Option<i64>,
) -> IndexDecision {
    if status != "published" {
        return IndexDecision::Excluded(ExclusionReason::NotPublished);
    }
    if visibility != "public" {
        return IndexDecision::Excluded(ExclusionReason::NotPublic);
    }
    if !board_active {
        return IndexDecision::Excluded(ExclusionReason::BoardInactive);
    }
    if board_visibility != "public" {
        return IndexDecision::Excluded(ExclusionReason::BoardNotPublic);
    }
    if author_status != "active" || author_deleted_at.is_some() {
        return IndexDecision::Excluded(ExclusionReason::AuthorUnavailable);
    }
    IndexDecision::Indexable
}

/// 用户可索引性裁决：`active` 且未删除才可入索引（匿名化/封禁/注销中账号不出现在
/// 公开搜索的用户命中）。
pub fn decide_user_indexability(status: &str, deleted_at: Option<i64>) -> IndexDecision {
    if status != "active" || deleted_at.is_some() {
        return IndexDecision::Excluded(ExclusionReason::AuthorUnavailable);
    }
    IndexDecision::Indexable
}

/// 板块可索引性裁决：启用且 `public` 可见才可入索引（members/restricted/hidden
/// 板块不进入公开索引候选集）。
pub fn decide_board_indexability(is_active: bool, visibility: &str) -> IndexDecision {
    if !is_active {
        return IndexDecision::Excluded(ExclusionReason::BoardInactive);
    }
    if visibility != "public" {
        return IndexDecision::Excluded(ExclusionReason::BoardNotPublic);
    }
    IndexDecision::Indexable
}

/// 标签可索引性裁决：启用才可入索引。
pub fn decide_tag_indexability(is_active: bool) -> IndexDecision {
    if !is_active {
        return IndexDecision::Excluded(ExclusionReason::TagInactive);
    }
    IndexDecision::Indexable
}

/// 索引安全文本校验错误。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IndexTextError {
    /// 文本包含受限部分标记（`restricted_html`/`restricted_markdown`）。
    RestrictedMarker,
    /// 文本包含 HTML 标记（`<字母` 或 `</`）；索引只存清洗纯文本，不存渲染 HTML。
    HtmlMarker,
}

/// 校验索引安全文本（M03-SEARCH-STORE-05，P0）。
///
/// 拒绝 `restricted_html`/`restricted_markdown` 特征串与 HTML 标记；通过后
/// 返回原文本（调用方再经 `clean_index_text` 清洗并截断）。只应接收已通过
/// 可见性裁决的公开投影字段——本校验是第二道防线，防止受限部分进入索引输入面。
pub fn vet_index_text(raw: &str) -> Result<String, IndexTextError> {
    let lower = raw.to_ascii_lowercase();
    if lower.contains("restricted_html") || lower.contains("restricted_markdown") {
        return Err(IndexTextError::RestrictedMarker);
    }
    let bytes = raw.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        if b == b'<' {
            let next = bytes.get(i + 1).copied();
            if matches!(next, Some(c) if c.is_ascii_alphabetic() || c == b'/') {
                return Err(IndexTextError::HtmlMarker);
            }
        }
    }
    Ok(raw.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn post_indexability_gate() {
        let indexable =
            |status: &str, visibility: &str, board_active: bool, board_visibility: &str| {
                decide_post_indexability(
                    status,
                    visibility,
                    board_active,
                    board_visibility,
                    "active",
                    None,
                )
            };
        // 唯一可入索引组合：published + public + 板块启用且 public + 作者 active。
        assert_eq!(
            indexable("published", "public", true, "public"),
            IndexDecision::Indexable
        );
        // 状态排除。
        for status in [
            "draft",
            "pending_review",
            "rejected",
            "hidden",
            "locked",
            "deleted",
        ] {
            assert_eq!(
                indexable(status, "public", true, "public"),
                IndexDecision::Excluded(ExclusionReason::NotPublished),
                "{status} 不得入索引"
            );
        }
        // 可见性排除（公开索引只收 public）。
        for visibility in ["logged_in", "after_reply", "level", "paid"] {
            assert_eq!(
                indexable("published", visibility, true, "public"),
                IndexDecision::Excluded(ExclusionReason::NotPublic),
                "{visibility} 不得入公开索引"
            );
        }
        // 板块排除。
        assert_eq!(
            indexable("published", "public", false, "public"),
            IndexDecision::Excluded(ExclusionReason::BoardInactive)
        );
        for v in ["members", "restricted", "hidden"] {
            assert_eq!(
                indexable("published", "public", true, v),
                IndexDecision::Excluded(ExclusionReason::BoardNotPublic),
                "板块 {v} 内容不得入公开索引"
            );
        }
        // 作者排除。
        assert_eq!(
            decide_post_indexability("published", "public", true, "public", "banned", None),
            IndexDecision::Excluded(ExclusionReason::AuthorUnavailable)
        );
        assert_eq!(
            decide_post_indexability("published", "public", true, "public", "active", Some(1)),
            IndexDecision::Excluded(ExclusionReason::AuthorUnavailable)
        );
        assert_eq!(
            decide_post_indexability(
                "published",
                "public",
                true,
                "public",
                "pending_delete",
                None
            ),
            IndexDecision::Excluded(ExclusionReason::AuthorUnavailable)
        );
    }

    #[test]
    fn entity_indexability_gates() {
        assert_eq!(
            decide_user_indexability("active", None),
            IndexDecision::Indexable
        );
        assert_eq!(
            decide_user_indexability("banned", None),
            IndexDecision::Excluded(ExclusionReason::AuthorUnavailable)
        );
        assert_eq!(
            decide_user_indexability("active", Some(1)),
            IndexDecision::Excluded(ExclusionReason::AuthorUnavailable)
        );

        assert_eq!(
            decide_board_indexability(true, "public"),
            IndexDecision::Indexable
        );
        assert_eq!(
            decide_board_indexability(false, "public"),
            IndexDecision::Excluded(ExclusionReason::BoardInactive)
        );
        assert_eq!(
            decide_board_indexability(true, "hidden"),
            IndexDecision::Excluded(ExclusionReason::BoardNotPublic)
        );

        assert_eq!(decide_tag_indexability(true), IndexDecision::Indexable);
        assert_eq!(
            decide_tag_indexability(false),
            IndexDecision::Excluded(ExclusionReason::TagInactive)
        );
    }

    #[test]
    fn vet_rejects_restricted_markers_and_html() {
        assert_eq!(
            vet_index_text("前文 <div>restricted_html 泄露</div>"),
            Err(IndexTextError::RestrictedMarker)
        );
        assert_eq!(
            vet_index_text("restricted_markdown 部分"),
            Err(IndexTextError::RestrictedMarker)
        );
        assert_eq!(
            vet_index_text("这是公开正文"),
            Ok("这是公开正文".to_string())
        );
        assert_eq!(
            vet_index_text("<b>加粗</b> 正文"),
            Err(IndexTextError::HtmlMarker)
        );
        assert_eq!(
            vet_index_text("</div>结尾"),
            Err(IndexTextError::HtmlMarker)
        );
        assert_eq!(
            vet_index_text("<script>alert(1)</script>"),
            Err(IndexTextError::HtmlMarker)
        );
    }

    #[test]
    fn vet_allows_plain_text_with_angle_operators() {
        // 纯文本中的比较符/表情符号不是 HTML 标记。
        assert_eq!(
            vet_index_text("x < y 且 z > 0"),
            Ok("x < y 且 z > 0".to_string())
        );
        assert_eq!(vet_index_text("<3 爱心"), Ok("<3 爱心".to_string()));
        assert_eq!(
            vet_index_text("成本 ≤100 元"),
            Ok("成本 ≤100 元".to_string())
        );
        assert_eq!(
            vet_index_text("空行\n第二段"),
            Ok("空行\n第二段".to_string())
        );
    }

    // ── M08-INDEX-02/03：统一排除规则 ──────────────────────────────────────

    /// 基准可入索引输入：published + public 策略 + 板块公开启用 + 作者 active。
    fn base_input<'a>() -> PostPublicIndexInput<'a> {
        PostPublicIndexInput {
            status: "published",
            visibility: "public",
            policy_kind: Some("public"),
            board_active: true,
            board_visibility: "public",
            author_status: "active",
            author_deleted_at: None,
            deleted_at: None,
            review_status: None,
            search_index_opt_out: false,
            admin_search_index: "allow",
        }
    }

    #[test]
    fn unified_decision_accepts_only_full_public_path() {
        let decision = decide_public_post_indexability(&base_input());
        assert_eq!(decision, IndexDecision::Indexable);

        // 未设置访问策略（None = public 语义）同样可入索引。
        let mut no_policy = base_input();
        no_policy.policy_kind = None;
        assert_eq!(
            decide_public_post_indexability(&no_policy),
            IndexDecision::Indexable
        );
    }

    #[test]
    fn unified_decision_excludes_non_public_access_policy() {
        // 即使遗留 visibility 列为 public，M04 访问策略非 public 也排除。
        for kind in ["logged_in", "after_reply", "level", "paid"] {
            let mut input = base_input();
            input.policy_kind = Some(kind);
            assert_eq!(
                decide_public_post_indexability(&input),
                IndexDecision::Excluded(ExclusionReason::PolicyNotPublic),
                "访问策略 {kind} 不得入公开索引"
            );
        }
    }

    #[test]
    fn unified_decision_excludes_review_and_deleted() {
        let mut under_review = base_input();
        under_review.review_status = Some("pending_review");
        assert_eq!(
            decide_public_post_indexability(&under_review),
            IndexDecision::Excluded(ExclusionReason::UnderReview)
        );

        let mut deleted = base_input();
        deleted.deleted_at = Some(1_700_000_000_000);
        assert_eq!(
            decide_public_post_indexability(&deleted),
            IndexDecision::Excluded(ExclusionReason::Deleted)
        );
    }

    #[test]
    fn unified_decision_excludes_opt_out_and_admin_policy() {
        let mut author_out = base_input();
        author_out.search_index_opt_out = true;
        assert_eq!(
            decide_public_post_indexability(&author_out),
            IndexDecision::Excluded(ExclusionReason::AuthorOptedOut)
        );

        // 管理员 deny（全站或板块）优先于作者 allow：即使作者未退出也排除。
        let mut admin_deny = base_input();
        admin_deny.admin_search_index = "deny";
        assert_eq!(
            decide_public_post_indexability(&admin_deny),
            IndexDecision::Excluded(ExclusionReason::AdminIndexDisabled)
        );

        // 作者 allow 且管理员 allow → 可入索引。
        let mut allowed = base_input();
        allowed.search_index_opt_out = false;
        allowed.admin_search_index = "allow";
        assert_eq!(
            decide_public_post_indexability(&allowed),
            IndexDecision::Indexable
        );
    }

    #[test]
    fn unified_decision_keeps_m03_gates() {
        for status in ["draft", "hidden", "deleted", "locked"] {
            let mut input = base_input();
            input.status = status;
            assert_eq!(
                decide_public_post_indexability(&input),
                IndexDecision::Excluded(ExclusionReason::NotPublished)
            );
        }
        for v in ["logged_in", "after_reply", "level", "paid"] {
            let mut input = base_input();
            input.visibility = v;
            assert_eq!(
                decide_public_post_indexability(&input),
                IndexDecision::Excluded(ExclusionReason::NotPublic)
            );
        }
        for v in ["members", "restricted", "hidden"] {
            let mut input = base_input();
            input.board_visibility = v;
            assert_eq!(
                decide_public_post_indexability(&input),
                IndexDecision::Excluded(ExclusionReason::BoardNotPublic)
            );
        }
        let mut banned = base_input();
        banned.author_status = "banned";
        assert_eq!(
            decide_public_post_indexability(&banned),
            IndexDecision::Excluded(ExclusionReason::AuthorUnavailable)
        );
    }

    #[test]
    fn gate_and_vet_form_the_write_contract() {
        // 写路径契约：裁决 Indexable → vet 通过 → clean_index_text 清洗。
        let decision =
            decide_post_indexability("published", "public", true, "public", "active", None);
        assert_eq!(decision, IndexDecision::Indexable);

        let body = "公开正文，包含 <b>不应</b> 出现的 HTML";
        assert_eq!(vet_index_text(body), Err(IndexTextError::HtmlMarker));

        // 受限部分（restricted_html）即使混在公开正文中也必须被挡在输入面外。
        let leaky = "公开部分 <div class='restricted_html'>受限部分</div>";
        assert_eq!(vet_index_text(leaky), Err(IndexTextError::RestrictedMarker));
    }
}
