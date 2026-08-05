//! M01-AUDIT-01/02：不可关闭的 audit_logs——actor、effective role、target、
//! action、reason、request_id、policy version；before/after 字段 allowlist
//! 禁止密码、Token、Secret、隐藏正文和完整签名 URL。

use std::path::{Path, PathBuf};

use bblbb_backend::audit::{list_audit_logs, sanitize_for_audit, AuditEntry};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use serde_json::json;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-audit-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

/// 完整字段（actor/effective role/target/action/reason/request_id/policy version）
/// 写入后原样读回。
#[tokio::test]
async fn audit_entry_round_trips_all_m01_audit_01_fields() {
    let (pool, dir) = pool_with_migrations().await;

    AuditEntry::user_action("moderator-1", "admin.ban_user")
        .with_target("user", "user-456")
        .with_effective_role("moderator")
        .with_reason("repeated spam after three warnings")
        .with_policy_version("v1.0.0-rc.2")
        .with_request_id("req-abc123")
        .with_ip("203.0.113.7")
        .with_metadata(json!({ "sanction": "ban_7d" }))
        .record(&pool)
        .await
        .unwrap();

    let rows = list_audit_logs(&pool, 10, 0, None, None).await.unwrap();
    assert_eq!(rows.len(), 1, "审计记录必须持久化");

    let row = &rows[0];
    assert_eq!(row.actor_id.as_deref(), Some("moderator-1"));
    assert_eq!(row.effective_role.as_deref(), Some("moderator"));
    assert_eq!(row.action, "admin.ban_user");
    assert_eq!(row.target_type.as_deref(), Some("user"));
    assert_eq!(row.target_id.as_deref(), Some("user-456"));
    assert_eq!(
        row.reason.as_deref(),
        Some("repeated spam after three warnings")
    );
    assert_eq!(row.policy_version.as_deref(), Some("v1.0.0-rc.2"));
    assert_eq!(row.request_id.as_deref(), Some("req-abc123"));
    assert_eq!(row.ip_address.as_deref(), Some("203.0.113.7"));
    assert!(
        row.metadata.as_deref().unwrap_or("").contains("ban_7d"),
        "metadata 必须原样存储"
    );

    // 时间戳为 Unix 毫秒（SCHEMA §2.2）
    assert!(
        row.created_at >= 1_700_000_000_000,
        "created_at 必须是毫秒时间戳，得到 {}",
        row.created_at
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// before/after 经 allowlist 过滤后写入：数据库中的 metadata 不含密码/Token/
/// Secret/隐藏正文/完整签名 URL。
#[tokio::test]
async fn audit_metadata_never_contains_forbidden_sensitive_data() {
    let (pool, dir) = pool_with_migrations().await;
    let token = bblbb_backend::auth::token::generate_token();

    let before = json!({
        "status": "active",
        "password_hash": "abcf00d",
        "content": "完整隐藏正文",
        "title": "旧标题"
    });
    let after = json!({
        "status": "banned",
        "reset_token": token,
        "title": format!("封禁通知 {}", token),
        "content_excerpt": "https://cdn.example.com/f?sig=X-Amz-Signature=deadbeef"
    });

    let metadata = json!({
        "before": sanitize_for_audit(&before),
        "after": sanitize_for_audit(&after)
    });

    AuditEntry::user_action("moderator-1", "admin.ban_user")
        .with_target("user", "user-456")
        .with_effective_role("moderator")
        .with_reason("spam")
        .with_policy_version("v1.0.0-rc.2")
        .with_request_id("req-xyz")
        .with_metadata(metadata)
        .record(&pool)
        .await
        .unwrap();

    let rows = list_audit_logs(&pool, 10, 0, None, None).await.unwrap();
    let stored = rows[0].metadata.as_deref().unwrap_or("").to_owned();

    assert!(!stored.contains(&token), "数据库不得出现原始 token");
    assert!(!stored.contains("password_hash"), "不得出现密码字段");
    assert!(!stored.contains("abcf00d"), "不得出现密码值");
    assert!(!stored.contains("完整隐藏正文"), "不得出现隐藏正文");
    assert!(
        !stored.contains("X-Amz-Signature=deadbeef"),
        "不得出现完整签名 URL"
    );
    assert!(stored.contains("旧标题"), "白名单字段应保留");
    assert!(stored.contains("[REDACTED]"), "敏感值应被脱敏标记");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M01-AUDIT-06：管理员代操作/权限变更/配置/账务/审核/Secret/Feature Flag
/// 分类 helper 都能写入并读回。
#[tokio::test]
async fn audit_helpers_record_all_categories() {
    let (pool, dir) = pool_with_migrations().await;

    // 管理员代操作
    AuditEntry::delegated_admin_action(
        "admin-1",
        "moderator",
        "admin.ban_user",
        "user",
        "u-9",
        "代操作",
    )
    .record(&pool)
    .await
    .unwrap();
    // 权限变更
    AuditEntry::permission_change(
        "admin-1",
        "u-9",
        "member",
        "moderator",
        "晋升",
        "v1.0.0-rc.2",
    )
    .record(&pool)
    .await
    .unwrap();
    // 配置变更
    AuditEntry::config_change(
        "admin-1",
        "storage.max_upload_bytes",
        Some(&json!({ "max_upload_bytes": 10_485_760, "password": "x" })),
        Some(&json!({ "max_upload_bytes": 20_971_520 })),
        "提升配额",
        "v1.0.0-rc.2",
    )
    .record(&pool)
    .await
    .unwrap();
    // 账务变更
    AuditEntry::accounting_change("admin-1", "ledger", "l-88", -500, "B", "手动修正")
        .record(&pool)
        .await
        .unwrap();
    // 内容审核
    AuditEntry::moderation_action(
        "mod-1",
        "post",
        "p-42",
        "moderation.hide",
        "违规",
        "v1.0.0-rc.2",
    )
    .record(&pool)
    .await
    .unwrap();
    // Secret 变更
    AuditEntry::secret_change("admin-1", "smtp_password", "rotate")
        .record(&pool)
        .await
        .unwrap();
    // Feature Flag 变更
    AuditEntry::feature_flag_change(
        "admin-1",
        "ai_summary",
        false,
        true,
        "灰度开启",
        "v1.0.0-rc.2",
    )
    .record(&pool)
    .await
    .unwrap();

    let rows = list_audit_logs(&pool, 100, 0, None, None).await.unwrap();
    assert_eq!(rows.len(), 7, "7 类审计 helper 各写一条");

    // 分类 helper 都能读回（action 命中），且 Secret 记录不含任何值
    let actions: Vec<String> = rows.iter().map(|r| r.action.clone()).collect();
    for expected in [
        "admin.ban_user",
        "admin.permission_change",
        "admin.config_change",
        "ledger.change",
        "moderation.hide",
        "secrets.rotate",
        "admin.feature_flag_change",
    ] {
        assert!(
            actions.contains(&expected.to_string()),
            "缺少分类审计 {expected}: {actions:?}"
        );
    }
    let secret_row = rows.iter().find(|r| r.action == "secrets.rotate").unwrap();
    let secret_meta = secret_row.metadata.as_deref().unwrap_or("");
    assert!(
        secret_meta.contains("smtp_password") && !secret_meta.contains("value"),
        "Secret 审计只含名称不含值: {secret_meta}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 不可关闭：audit_logs 无 status/disabled/enabled 列（只追加、无关闭开关），
/// 且包含 M01-AUDIT-01 要求的全部列。
#[tokio::test]
async fn audit_table_is_append_only_without_disable_columns() {
    let (pool, dir) = pool_with_migrations().await;

    match &pool {
        Either::Left(p) => {
            let columns: Vec<String> =
                sqlx::query_scalar("SELECT name FROM pragma_table_info('audit_logs') ORDER BY cid")
                    .fetch_all(p)
                    .await
                    .unwrap();
            for expected in [
                "id",
                "actor_id",
                "effective_role",
                "action",
                "target_type",
                "target_id",
                "reason",
                "policy_version",
                "metadata",
                "request_id",
                "ip_address",
                "created_at",
            ] {
                assert!(
                    columns.contains(&expected.to_string()),
                    "audit_logs 缺少列 {expected}"
                );
            }
            // 不存在任何关闭/状态开关列 → 审计不可被禁用
            for forbidden in ["status", "disabled", "enabled", "archived"] {
                assert!(
                    !columns.contains(&forbidden.to_string()),
                    "audit_logs 不得包含可关闭审计的列 {forbidden}"
                );
            }
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}
