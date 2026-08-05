//! M01-JOBS-02：业务事务内写 Outbox，回滚时事件同步消失。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox;
use serde_json::json;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

/// 建库并应用全部真实迁移；返回 (pool, sqlite 文件路径)。
async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-outbox-{}", uuid::Uuid::now_v7()));
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

async fn outbox_count(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events")
            .fetch_one(p)
            .await
            .unwrap(),
    }
}

/// 回滚：业务变更与 Outbox 事件必须同步消失。
#[tokio::test]
async fn outbox_event_disappears_on_rollback() {
    let (pool, dir) = pool_with_migrations().await;

    // 建一张业务表作为业务变更载体
    match &pool {
        Either::Left(p) => {
            sqlx::query("CREATE TABLE business_changes (id TEXT PRIMARY KEY, note TEXT NOT NULL)")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    // 业务写入 + Outbox 事件在同一事务
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO business_changes (id, note) VALUES ('b1', 'hello')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO business_changes (id, note) VALUES ('b1', 'hello')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    outbox::enqueue_in_tx(&mut tx, "test.registered.v1", json!({ "user_id": "u1" }))
        .await
        .unwrap();

    // 回滚 → 业务行与事件都消失
    match tx {
        Either::Left(t) => t.rollback().await.unwrap(),
        Either::Right(t) => t.rollback().await.unwrap(),
    }
    assert_eq!(outbox_count(&pool).await, 0, "回滚后 Outbox 事件必须消失");

    let business: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM business_changes")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(business, 0, "回滚后业务变更也必须消失");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 提交：业务变更与 Outbox 事件都持久化，事件为 pending、payload_version=1。
#[tokio::test]
async fn outbox_event_persists_on_commit() {
    let (pool, dir) = pool_with_migrations().await;

    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    let event_id = outbox::enqueue_in_tx(&mut tx, "test.reply.v1", json!({ "post_id": "p1" }))
        .await
        .unwrap();
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    assert_eq!(outbox_count(&pool).await, 1, "提交后事件必须持久化");

    match &pool {
        Either::Left(p) => {
            let row: (String, String, i64, i64) = sqlx::query_as(
                "SELECT status, payload, payload_version, attempts FROM outbox_events WHERE id = ?",
            )
            .bind(&event_id)
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(row.0, "pending");
            assert_eq!(row.1, r#"{"post_id":"p1"}"#);
            assert_eq!(row.2, 1, "payload_version 必须为 1");
            assert_eq!(row.3, 0);
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}
