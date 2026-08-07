//! M04-VISIBILITY：投影过滤 + persona 感知缓存头（纯逻辑，无 DB）。
//!
//! 覆盖：
//! - M04-VISIBILITY-07：未解锁 DTO 敏感键完全缺失（get→None，非 null）；
//!   access_summary 恒存在；
//! - M04-VISIBILITY-08：public → `public, max-age=60`+`Vary: Cookie`+稳定 ETag；
//!   受限 → `private, no-store` 无 ETag；不同 persona 不共享 ETag；
//! - M04-VISIBILITY-09：ProjectionFilter 批量混合策略——每个 item 按自己的
//!   grant 投影；excerpt 只在解锁时出现；隐藏正文 canary 永不泄漏。

use bblbb_backend::content::visibility::cache::{cache_headers_for, etag_for_body};
use bblbb_backend::content::visibility::evaluate::{AccessGrant, CAP_UNLOCK_AFTER_REPLY};
use bblbb_backend::content::visibility::projection::{
    project_comment, project_post, AccessSummary, AttachmentRef, CommentFields, PostFields,
    ProjectionFilter,
};
use bblbb_backend::domain::posts::AccessPolicy;
use serde_json::json;

const CANARY: &str = "CANARY-SECRET-BODY-MARKER";

fn grant(policy: AccessPolicy, unlocked: bool) -> AccessGrant {
    AccessGrant {
        unlocked,
        policy,
        reason: policy.as_str(),
        required_level: match policy {
            AccessPolicy::Level => Some(4),
            _ => None,
        },
        capabilities: if unlocked {
            &[]
        } else {
            CAP_UNLOCK_AFTER_REPLY
        },
    }
}

fn post_fields(body: &str) -> PostFields {
    PostFields {
        id: "p1".into(),
        title: "标题".into(),
        author_id: "u1".into(),
        author_username: Some("alice".into()),
        author_display_name: Some("爱丽丝".into()),
        author_level: 5,
        post_type: "discussion".into(),
        status: "published".into(),
        board_id: "b1".into(),
        slug: Some("p1-slug".into()),
        reply_count: 2,
        view_count: 10,
        created_at: 1,
        updated_at: 2,
        pinned_at: None,
        scheduled_at: None,
        published_at: Some(1),
        last_reply_at: None,
        closed_at: None,
        body_html: Some(format!("<p>{body}</p>")),
        excerpt: Some("excerpt".into()),
        attachments: vec![AttachmentRef {
            attachment_id: "a1".into(),
            kind: "cover".into(),
            position: 1,
        }],
        search_highlight: Some("<mark>hl</mark>".into()),
        restricted_html: Some("<div>restricted</div>".into()),
    }
}

fn comment_fields() -> CommentFields {
    CommentFields {
        id: "c1".into(),
        post_id: "p1".into(),
        author_id: "u2".into(),
        author_username: Some("bob".into()),
        floor: 1,
        created_at: 3,
        updated_at: 4,
        content_html: Some("<p>comment</p>".into()),
        content_markdown: Some("comment".into()),
        search_highlight: Some("comment".into()),
    }
}

// ───────────────────────── M04-VISIBILITY-07：DTO 省略 ─────────────────────

#[test]
fn locked_post_omits_body_excerpt_attachments_highlight() {
    let value = project_post(
        post_fields(CANARY),
        grant(AccessPolicy::AfterReply, false),
        5,
    );
    for key in [
        "body_html",
        "excerpt",
        "attachments",
        "search_highlight",
        "restricted_html",
    ] {
        assert_eq!(
            value.get(key),
            None,
            "未解锁时 {key} 键必须完全缺失（get→None，非 null）"
        );
    }
    // 公开元数据保留 + access_summary 恒存在
    assert_eq!(value["id"], "p1");
    assert_eq!(value["author"]["username"], "alice");
    assert_eq!(value["access_summary"]["policy"], "after_reply");
    assert_eq!(value["access_summary"]["unlocked"], false);
    assert_eq!(value["capabilities"], json!(["unlock_after_reply"]));
    // 序列化输出不含正文痕迹
    let s = value.to_string();
    assert!(!s.contains(CANARY), "正文 canary 泄漏: {s}");
    assert!(!s.contains("excerpt"), "摘要泄漏: {s}");
    assert!(!s.contains("restricted"), "受限块泄漏: {s}");
}

#[test]
fn unlocked_post_includes_sensitive_keys() {
    let value = project_post(post_fields("hello"), grant(AccessPolicy::Public, true), 5);
    for key in [
        "body_html",
        "excerpt",
        "attachments",
        "search_highlight",
        "restricted_html",
    ] {
        assert!(value.get(key).is_some(), "解锁时 {key} 必须存在");
    }
    assert_eq!(value["body_html"], "<p>hello</p>");
    assert_eq!(value["access_summary"]["unlocked"], true);
}

#[test]
fn locked_comment_omits_content_keys() {
    let value = project_comment(comment_fields(), grant(AccessPolicy::AfterReply, false));
    for key in ["content_html", "content_markdown", "search_highlight"] {
        assert_eq!(value.get(key), None, "未解锁评论 {key} 必须缺失");
    }
    assert_eq!(value["floor"], 1);
    assert_eq!(value["access_summary"]["unlocked"], false);
    assert!(!value.to_string().contains("comment"), "评论正文泄漏");
}

#[test]
fn unlocked_comment_includes_content() {
    let value = project_comment(comment_fields(), grant(AccessPolicy::Public, true));
    assert_eq!(value["content_html"], "<p>comment</p>");
    assert_eq!(value["content_markdown"], "comment");
}

#[test]
fn access_summary_always_present() {
    let locked = project_post(post_fields(CANARY), grant(AccessPolicy::Level, false), 5);
    assert_eq!(locked["access_summary"]["policy"], "level");
    assert_eq!(locked["access_summary"]["required_level"], 4);
    assert_eq!(locked["access_summary"]["unlocked"], false);
    let summary = AccessSummary::from_grant(&grant(AccessPolicy::Level, false));
    assert_eq!(summary.policy, "level");
    assert_eq!(summary.required_level, Some(4));
    assert!(!summary.unlocked);
}

// ───────────────────── M04-VISIBILITY-09：可复用投影过滤器 ─────────────────

#[test]
fn batch_mixed_policies_each_item_projected_by_own_grant() {
    let locked = ProjectionFilter::new(grant(AccessPolicy::AfterReply, false));
    let unlocked = ProjectionFilter::new(grant(AccessPolicy::Public, true));

    let items = [
        locked.project_post(post_fields(CANARY), 5), // 锁定帖（canary 正文）
        unlocked.project_post(post_fields("ok"), 5), // 解锁帖
        locked.project_comment(comment_fields()),    // 锁定评论
    ];

    // item 0：锁定 → 无正文、无摘要，canary 不可见
    let s0 = items[0].to_string();
    assert!(!s0.contains(CANARY), "锁定帖 canary 泄漏: {s0}");
    assert!(items[0].get("body_html").is_none());
    assert!(items[0].get("excerpt").is_none(), "锁定帖不得暴露 excerpt");
    assert_eq!(items[0]["access_summary"]["unlocked"], false);

    // item 1：解锁 → 正文/摘要可见
    assert!(items[1].get("body_html").is_some());
    assert!(items[1].get("excerpt").is_some());
    assert_eq!(items[1]["access_summary"]["unlocked"], true);

    // item 2：锁定评论 → 正文不可见
    assert!(items[2].get("content_html").is_none());
    assert_eq!(items[2]["access_summary"]["unlocked"], false);
}

#[test]
fn batch_canary_never_leaks_anywhere() {
    let locked = ProjectionFilter::new(grant(AccessPolicy::Paid, false));
    let outputs = vec![
        locked.project_post(post_fields(CANARY), 5),
        locked.project_comment(comment_fields()),
        serde_json::to_value(locked.access_summary()).unwrap(),
    ];
    for out in outputs {
        assert!(
            !format!("{out:?}").contains(CANARY),
            "锁定投影泄漏 canary: {out:?}"
        );
    }
}

#[test]
fn excerpt_present_only_when_unlocked() {
    let locked = project_post(post_fields("x"), grant(AccessPolicy::AfterReply, false), 5);
    assert_eq!(locked.get("excerpt"), None);
    let unlocked = project_post(post_fields("x"), grant(AccessPolicy::Public, true), 5);
    assert_eq!(unlocked["excerpt"], "excerpt");
}

// ───────────────────────── M04-VISIBILITY-08：缓存头 ───────────────────────

#[test]
fn public_gets_public_cache_vary_cookie_etag() {
    let body = r#"{"id":"p1","access_summary":{"policy":"public","unlocked":true}}"#;
    let h = cache_headers_for(&grant(AccessPolicy::Public, true), body);
    assert_eq!(h.cache_control, "public, max-age=60");
    assert_eq!(h.vary, Some("Cookie"));
    assert!(h.etag.is_some());
    // ETag 可放入 HTTP 头
    assert!(axum::http::HeaderValue::from_str(h.etag.as_deref().unwrap()).is_ok());
}

#[test]
fn restricted_policies_get_private_no_store() {
    for policy in [
        AccessPolicy::LoggedIn,
        AccessPolicy::AfterReply,
        AccessPolicy::Level,
        AccessPolicy::Paid,
    ] {
        // 即使解锁（作者/管理）也按 persona 隔离
        for unlocked in [false, true] {
            let h = cache_headers_for(&grant(policy, unlocked), r#"{"id":"p1"}"#);
            assert_eq!(
                h.cache_control, "private, no-store",
                "{policy:?} {unlocked}"
            );
            assert_eq!(h.vary, None);
            assert_eq!(h.etag, None, "{policy:?} 不得发 ETag");
        }
    }
}

#[test]
fn different_personas_never_share_etag() {
    // 同一内容 id，但 persona 可见性不同 → 投影体不同 → ETag 不同
    let anon_body = r#"{"id":"p1","access_summary":{"policy":"public","unlocked":true}}"#;
    let full_body = r#"{"id":"p1","body_html":"<p>secret</p>","access_summary":{"policy":"public","unlocked":true}}"#;
    let h1 = cache_headers_for(&grant(AccessPolicy::Public, true), anon_body);
    let h2 = cache_headers_for(&grant(AccessPolicy::Public, true), full_body);
    assert_ne!(h1.etag, h2.etag, "不同 persona 不得共享 ETag");
}

#[test]
fn etag_is_deterministic_for_same_persona_and_content() {
    let body = r#"{"id":"p1","access_summary":{"policy":"public","unlocked":true}}"#;
    assert_eq!(etag_for_body(body), etag_for_body(body));
    assert_ne!(etag_for_body(body), etag_for_body(&format!("{body} ")));
}
