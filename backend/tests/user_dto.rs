//! M03-PROFILE-01：用户三套显式投影 DTO 契约——
//! - `PublicProfile` 序列化键集 == 公开 allowlist，绝不泄漏
//!   email/状态机内部状态/Session/IP/处罚/审计字段；
//! - `Me` 为本人投影，含本人可见字段；
//! - `AdminUser` 为管理投影，含内部状态与删除/注销时间，但绝不含凭据。
//!
//! 测试数据直接从 DTO 显式字段构建（不复用数据库实体序列化）。

use bblbb_backend::auth::session::SessionUser;
use bblbb_backend::users::dto::{AdminUser, Author, Me, PublicProfile, PUBLIC_PROFILE_ALLOWLIST};
use bblbb_backend::users::profile::ProfileFields;
use serde_json::Value;

fn sample_session_user() -> SessionUser {
    SessionUser {
        id: "00000000-0000-7000-8000-000000000001".to_string(),
        username: "alice".to_string(),
        email: "alice@example.com".to_string(),
        email_verified: true,
        status: "active".to_string(),
        display_name: Some("爱丽丝".to_string()),
        level: 3,
        roles: vec!["member".to_string()],
    }
}

fn sorted_keys(v: &Value) -> Vec<String> {
    let mut keys: Vec<String> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    keys
}

fn sample_public_profile() -> PublicProfile {
    PublicProfile {
        id: "00000000-0000-7000-8000-000000000001".to_string(),
        username: "alice".to_string(),
        display_name: Some("爱丽丝".to_string()),
        bio: Some("hello".to_string()),
        level: 3,
        avatar_attachment_id: Some("00000000-0000-7000-8000-000000000099".to_string()),
        cover_attachment_id: Some("00000000-0000-7000-8000-000000000098".to_string()),
        signature: Some("个性签名".to_string()),
        created_at: 1_700_000_000_000,
    }
}

/// 公开投影必须严格 allowlist：键集正好是公开字段，且不含任何私有字段。
#[test]
fn public_profile_is_strict_allowlist() {
    let profile = sample_public_profile();
    let v = serde_json::to_value(&profile).unwrap();
    let mut expected = vec![
        "id",
        "username",
        "display_name",
        "bio",
        "level",
        "avatar_attachment_id",
        "cover_attachment_id",
        "signature",
        "created_at",
    ];
    expected.sort();
    assert_eq!(
        sorted_keys(&v),
        expected,
        "公开投影必须严格 allowlist，不得多出或缺少字段"
    );

    for leaked in [
        "email",
        "email_verified",
        "status",
        "password_hash",
        "last_login_ip",
        "delete_requested_at",
        "deleted_at",
    ] {
        assert!(
            !v.as_object().unwrap().contains_key(leaked),
            "公开投影泄漏私有字段: {leaked}"
        );
    }
}

/// PublicProfile 序列化键集必须与 allowlist 常量精确一致；allowlist 本身
/// 不得包含任何敏感字段（M03-PROFILE-02）。
#[test]
fn public_profile_keys_match_allowlist_constant() {
    let profile = sample_public_profile();
    let v = serde_json::to_value(&profile).unwrap();
    let keys = sorted_keys(&v);
    let mut allowlist = PUBLIC_PROFILE_ALLOWLIST.to_vec();
    allowlist.sort();
    assert_eq!(
        keys, allowlist,
        "PublicProfile 键集必须与 PUBLIC_PROFILE_ALLOWLIST 常量一致"
    );

    for leaked in [
        "email",
        "email_normalized",
        "password_hash",
        "last_login_ip",
        "session",
        "sanction",
        "audit",
        "delete_requested_at",
        "deleted_at",
    ] {
        assert!(
            !PUBLIC_PROFILE_ALLOWLIST.contains(&leaked),
            "allowlist 不得包含敏感字段: {leaked}"
        );
    }
}

/// Cover/头像只返回稳定附件 UUID 引用，绝不包含远程/签名 URL（M03-PROFILE-05）。
#[test]
fn profile_media_refs_are_stable_uuids_not_urls() {
    let profile = sample_public_profile();
    let v = serde_json::to_value(&profile).unwrap();
    for field in ["avatar_attachment_id", "cover_attachment_id"] {
        let value = v[field].as_str().unwrap();
        assert!(
            uuid::Uuid::parse_str(value).is_ok(),
            "{field} 必须是 UUID: {value}"
        );
        assert!(
            !value.contains("://") && !value.contains("signed") && !value.contains("token"),
            "{field} 不得是远程/签名 URL: {value}"
        );
    }
}

/// 作者卡：profile_url 指向稳定公开主页端点 /users/{username}（M03-PROFILE-05）。
#[test]
fn author_card_uses_stable_profile_url() {
    let author = Author::from_public(&sample_public_profile());
    let v = serde_json::to_value(&author).unwrap();
    assert_eq!(v["username"], "alice");
    assert_eq!(v["display_name"], "爱丽丝");
    assert_eq!(v["level"], 3);
    assert_eq!(
        v["profile_url"], "/users/alice",
        "作者卡必须用稳定公开主页端点"
    );
    assert!(
        !v["profile_url"].as_str().unwrap().contains("://"),
        "profile_url 不得是远程 URL"
    );
}

/// Me 为本人的显式投影：含本人可见字段与 mfa_enabled。
#[test]
fn me_is_own_projection() {
    let profile = ProfileFields {
        bio: Some("bio".to_string()),
        display_name: Some("爱丽丝".to_string()),
        ..ProfileFields::default()
    };
    let me = Me::from_session(&sample_session_user(), true, &profile);
    let v = serde_json::to_value(&me).unwrap();
    for field in [
        "id",
        "username",
        "email",
        "email_verified",
        "status",
        "display_name",
        "bio",
        "timezone",
        "level",
        "roles",
        "mfa_enabled",
    ] {
        assert!(v.get(field).is_some(), "Me 投影缺少 {field}");
    }
    assert_eq!(v["mfa_enabled"], true);
    assert_eq!(v["roles"][0], "member");
    assert_eq!(v["email"], "alice@example.com");
}

/// AdminUser 为管理投影：含内部状态/时间，但绝不含凭据。
#[test]
fn admin_user_contains_admin_fields_without_credentials() {
    let admin = AdminUser {
        id: "00000000-0000-7000-8000-000000000001".to_string(),
        username: "alice".to_string(),
        email: "alice@example.com".to_string(),
        email_verified: true,
        status: "active".to_string(),
        display_name: Some("爱丽丝".to_string()),
        level: 3,
        roles: vec!["member".to_string()],
        created_at: 1_700_000_000_000,
        updated_at: 1_700_000_000_000,
        last_login_at: Some(1_700_000_000_000),
        delete_requested_at: None,
        deleted_at: None,
    };
    let v = serde_json::to_value(&admin).unwrap();
    for field in [
        "id",
        "username",
        "email",
        "email_verified",
        "status",
        "display_name",
        "level",
        "roles",
        "created_at",
        "updated_at",
        "last_login_at",
        "delete_requested_at",
        "deleted_at",
    ] {
        assert!(v.get(field).is_some(), "AdminUser 缺少 {field}");
    }
    for leaked in [
        "password_hash",
        "encrypted_secret",
        "recovery_code_hash",
        "csrf_secret_hash",
    ] {
        assert!(
            !v.as_object().unwrap().contains_key(leaked),
            "管理投影泄漏凭据字段: {leaked}"
        );
    }
}
