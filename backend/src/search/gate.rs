//! 索引写入可见性裁决（M03-SEARCH-STORE-05，P0）。
//!
//! 索引写入只接受经过可见性裁决的安全文本，绝不存 `restricted_html`：
//!
//! 1. 可见性裁决——[`decide_post_indexability`] / [`decide_user_indexability`] /
//!    [`decide_board_indexability`] / [`decide_tag_indexability`]：
//!    draft/hidden/locked/deleted 帖子、非 public 可见性帖子、停用或非公开板块、
//!    停用/删除账号、停用/隐藏板块、停用标签一律排除（被排除即从索引移除，
//!    绝不把受限内容写入索引）；
//! 2. 安全文本——[`vet_index_text`]：索引正文/摘要必须是无 HTML 标记的清洗纯文本，
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
