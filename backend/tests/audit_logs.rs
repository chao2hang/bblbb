//! M01-AUDIT-01：不可关闭的 audit_logs——actor、effective role、target、
//! action、reason、request_id 与 policy version。

use std::path::{Path, PathBuf};

use bblbb_backend::audit::{list_audit_logs, AuditEntry};
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
