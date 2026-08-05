//! M01-AUDIT-08：审计与业务事务原子性、Outbox request ID 贯通、敏感数据脱敏。

use std::path::{Path, PathBuf};

use bblbb_backend::audit::{list_audit_logs, sanitize_for_audit, AuditEntry};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox;
use bblbb_backend::outbox::OutboxTx;
use serde_json::json;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-atomic-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();

    // 业务载体表（模拟高风险操作的目标变更）
    match &pool {
        Either::Left(p) => {
            sqlx::query("CREATE TABLE business_rows (id TEXT PRIMARY KEY, note TEXT NOT NULL)")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
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

async fn business_count(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM business_rows")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn audit_count(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn begin_tx(pool: &DatabasePool) -> OutboxTx<'_> {
    match pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    }
}

/// 审计与业务变更同事务提交：两者一起持久化。
#[tokio::test]
async fn audit_and_business_commit_together() {
    let (pool, dir) = pool_with_migrations().await;

    let mut tx = begin_tx(&pool).await;
    // 业务变更（高风险操作的目标）
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO business_rows (id, note) VALUES ('b1', 'ban user u-9')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO business_rows (id, note) VALUES ('b1', 'ban user u-9')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    // 审计同事务写入
    AuditEntry::delegated_admin_action(
        "admin-1",
        "moderator",
        "admin.ban_user",
        "user",
        "u-9",
        "违规",
    )
    .with_request_id("req-atomic")
    .record_in_tx(&mut tx)
    .await
    .unwrap();
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    assert_eq!(business_count(&pool).await, 1, "业务变更提交");
    assert_eq!(audit_count(&pool).await, 1, "审计同事务提交");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 审计与业务变更同事务回滚：两者一起消失（高风险操作无审计不得提交）。
#[tokio::test]
async fn audit_and_business_rollback_together() {
    let (pool, dir) = pool_with_migrations().await;

    let mut tx = begin_tx(&pool).await;
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO business_rows (id, note) VALUES ('b1', 'ban')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO business_rows (id, note) VALUES ('b1', 'ban')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    AuditEntry::delegated_admin_action(
        "admin-1",
        "moderator",
        "admin.ban_user",
        "user",
        "u-9",
        "违规",
    )
    .record_in_tx(&mut tx)
    .await
    .unwrap();
    match tx {
        Either::Left(t) => t.rollback().await.unwrap(),
        Either::Right(t) => t.rollback().await.unwrap(),
    }

    assert_eq!(business_count(&pool).await, 0, "回滚后业务变更消失");
    assert_eq!(audit_count(&pool).await, 0, "回滚后审计同步消失");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Outbox request ID 贯通：同一事务写入的业务变更、审计与 Outbox 事件，
/// 提交后 audit_logs.request_id 与事件一致，消费方仍可依 event_id 幂等处理。
#[tokio::test]
async fn outbox_request_id_flows_through_transaction() {
    let (pool, dir) = pool_with_migrations().await;

    let mut tx = begin_tx(&pool).await;
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO business_rows (id, note) VALUES ('b1', 'post published')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO business_rows (id, note) VALUES ('b1', 'post published')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    // 审计携带 request_id
    AuditEntry::user_action("u-1", "post.publish")
        .with_target("post", "p-1")
        .with_request_id("req-flow-1")
        .with_ip("203.0.113.5")
        .record_in_tx(&mut tx)
        .await
        .unwrap();
    // Outbox 事件同事务
    let event_id = outbox::enqueue_in_tx(&mut tx, "post.published.v1", json!({ "post_id": "p-1" }))
        .await
        .unwrap();
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    // 审计行 request_id 贯通
    let rows = list_audit_logs(&pool, 10, 0, None, None).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].request_id.as_deref(), Some("req-flow-1"));

    // Outbox 事件可被消费方取回（fetch_pending 返回，含 event_id 幂等键）
    let pending = outbox::fetch_pending(&pool, 10).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, event_id);
    assert_eq!(pending[0].event_type, "post.published.v1");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 敏感数据脱敏：审计 metadata 经 allowlist 过滤后再写库，原子路径也不泄漏。
#[tokio::test]
async fn atomic_audit_metadata_is_redacted() {
    let (pool, dir) = pool_with_migrations().await;

    let before = json!({
        "status": "active",
        "password_hash": "abcf00d",
        "title": "旧标题"
    });
    let after = json!({ "status": "banned", "title": "新标题" });
    let metadata = json!({
        "before": sanitize_for_audit(&before),
        "after": sanitize_for_audit(&after)
    });

    let mut tx = begin_tx(&pool).await;
    AuditEntry::delegated_admin_action(
        "admin-1",
        "moderator",
        "admin.ban_user",
        "user",
        "u-9",
        "违规",
    )
    .with_metadata(metadata)
    .record_in_tx(&mut tx)
    .await
    .unwrap();
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    let rows = list_audit_logs(&pool, 10, 0, None, None).await.unwrap();
    let stored = rows[0].metadata.as_deref().unwrap_or("").to_owned();
    assert!(!stored.contains("abcf00d"), "密码不得进入原子审计路径");
    assert!(!stored.contains("password_hash"));
    assert!(stored.contains("旧标题"), "白名单字段保留");

    close_pool(&pool).await;
    cleanup(&dir);
}
