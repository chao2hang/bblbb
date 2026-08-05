//! M01-JOBS-11：进程在领取后、业务调用后、提交前后崩溃的恢复与去重结果。
//!
//! 崩溃矩阵（每个场景模拟一次进程崩溃后重启）：
//! 1. 领取后崩溃（job lease）：租约过期 → 其他 worker 安全重领 → 结果只产生一次。
//! 2. 业务调用后、提交前崩溃（outbox 消费者事务）：整事务回滚，去重标记与
//!    副作用一起消失 → 重投时重新执行，副作用恰好提交一次。
//! 3. 提交后崩溃/投递系统重投：去重标记持久 → 重复投递被跳过，不重复副作用。
//! 4. job 处理器副作用幂等：效果行唯一键约束，崩溃重跑不产生重复效果。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::worker;
use bblbb_backend::outbox;
use serde_json::json;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

/// 建库并应用全部真实迁移；创建副作用载体表；返回 (pool, sqlite 路径)。
async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-crash-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();

    match &pool {
        Either::Left(p) => {
            // 消费者副作用载体（无唯一约束，去重完全依赖 outbox_consumed）
            sqlx::query(
                "CREATE TABLE consumer_effects (
                     id INTEGER PRIMARY KEY AUTOINCREMENT,
                     event_id TEXT NOT NULL,
                     consumer TEXT NOT NULL
                 )",
            )
            .execute(p)
            .await
            .unwrap();
            // job 处理器副作用载体（唯一键 = 处理器幂等去重）
            sqlx::query(
                "CREATE TABLE job_effects (
                     job_key TEXT PRIMARY KEY,
                     created_at INTEGER NOT NULL
                 )",
            )
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

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

async fn insert_queued_job(pool: &DatabasePool, id: &str, dedup_key: Option<&str>) {
    let base = now_ms();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, 'default', 'mail', '{}', 1, 'queued', 0, 3, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(base - 10_000)
            .bind(dedup_key)
            .bind(base)
            .bind(base)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 模拟崩溃：任务保持 running 但 lease 已过期（worker 未释放）。
async fn expire_lease(pool: &DatabasePool, id: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE jobs SET locked_until = ? WHERE id = ? AND status = 'running'")
                .bind(now_ms() - 1_000)
                .bind(id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn job_status(pool: &DatabasePool, id: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM jobs WHERE id = ?")
            .bind(id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 场景 1：领取后崩溃（job lease）→ 租约过期 → 其他 worker 安全重领并完成，
/// 任务结果只产生一次。
#[tokio::test]
async fn crash_after_claim_recovers_via_lease_and_completes_once() {
    let (pool, dir) = pool_with_migrations().await;
    insert_queued_job(&pool, "j1", None).await;

    // worker-a 领取后崩溃（未完成、未释放）
    let claimed = worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    assert_eq!(job_status(&pool, "j1").await, "running");
    expire_lease(&pool, "j1").await;

    // 恢复：worker-b 重领（attempts=2）并完成
    let reclaimed = worker::claim_batch(&pool, "worker-b", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].id, "j1");
    assert_eq!(reclaimed[0].attempts, 2, "重领是一次新执行尝试");
    assert!(worker::complete_job(&pool, "worker-b", "j1").await.unwrap());

    assert_eq!(
        job_status(&pool, "j1").await,
        "succeeded",
        "崩溃恢复后最终成功"
    );
    // succeeded 是终态：不会再次被领取，结果不会重复产生
    let again = worker::claim_batch(&pool, "worker-c", "default", 10, 30_000)
        .await
        .unwrap();
    assert!(again.is_empty(), "终态任务不得被再次领取");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 场景 2：业务调用后、提交前崩溃（outbox 消费者）→ 整事务回滚 →
/// 重投时重新执行，副作用恰好提交一次。
#[tokio::test]
async fn crash_after_business_call_before_commit_reruns_exactly_once() {
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

    // 第一次处理：去重标记 + 副作用 + 标记 sent 后崩溃（回滚）
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(outbox::consume_in_tx(&mut tx, &event_id, "mailer")
        .await
        .unwrap());
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES (?, 'mailer')")
                .bind(&event_id)
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES (?, 'mailer')")
                .bind(&event_id)
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    outbox::mark_sent_in_tx(&mut tx, &event_id).await.unwrap();
    match tx {
        Either::Left(t) => t.rollback().await.unwrap(),
        Either::Right(t) => t.rollback().await.unwrap(),
    }

    // 崩溃后：事件仍 pending，去重标记与副作用一起消失
    let pending = outbox::fetch_pending(&pool, 10).await.unwrap();
    assert_eq!(pending.len(), 1, "回滚后事件保持 pending 可重投");

    // 重投：重新执行并提交 → 副作用恰好一次
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(outbox::consume_in_tx(&mut tx, &event_id, "mailer")
        .await
        .unwrap());
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES (?, 'mailer')")
                .bind(&event_id)
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES (?, 'mailer')")
                .bind(&event_id)
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    outbox::mark_sent_in_tx(&mut tx, &event_id).await.unwrap();
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    let effects: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM consumer_effects")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(effects, 1, "提交前崩溃重投后副作用必须恰好一次");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 场景 3：提交后崩溃 / 投递系统重投 → 去重标记持久 → 重复投递被跳过。
#[tokio::test]
async fn crash_after_commit_redelivery_is_skipped() {
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

    // 第一次处理完整提交（去重标记 + 副作用 + sent）
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(outbox::consume_in_tx(&mut tx, &event_id, "mailer")
        .await
        .unwrap());
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES (?, 'mailer')")
                .bind(&event_id)
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES (?, 'mailer')")
                .bind(&event_id)
                .execute(&mut **t)
                .await
                .unwrap();
        }
    }
    outbox::mark_sent_in_tx(&mut tx, &event_id).await.unwrap();
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    // 投递系统重投：事件行被重置回 pending（消息系统视角的重复投递）
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE outbox_events SET status = 'pending', processed_at = NULL WHERE id = ?",
            )
            .bind(&event_id)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    // 消费者再次取回：去重标记仍在 → consume 返回 false，不产生副作用
    let pending = outbox::fetch_pending(&pool, 10).await.unwrap();
    assert_eq!(pending.len(), 1, "重投事件可再次取回");
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(
        !outbox::consume_in_tx(&mut tx, &event_id, "mailer")
            .await
            .unwrap(),
        "已消费事件的重投必须被去重"
    );
    match tx {
        Either::Left(t) => t.rollback().await.unwrap(),
        Either::Right(t) => t.rollback().await.unwrap(),
    }

    let effects: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM consumer_effects")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(effects, 1, "重投不得重复产生副作用");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 场景 4：job 处理器副作用幂等——效果行唯一键约束，崩溃重跑不重复效果。
#[tokio::test]
async fn job_effect_idempotency_prevents_duplicate_on_crash_rerun() {
    let (pool, dir) = pool_with_migrations().await;
    insert_queued_job(&pool, "j1", Some("effect-post-p1")).await;

    // 第一次执行：效果提交成功（写入唯一键行），随后在 complete 前崩溃
    let claimed = worker::claim_batch(&pool, "worker-a", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO job_effects (job_key, created_at) VALUES ('effect-post-p1', ?)",
            )
            .bind(now_ms())
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    expire_lease(&pool, "j1").await; // 崩溃：未 complete

    // 重领后处理器幂等重跑：唯一键冲突视为已生效，不重复写效果，再 complete
    let reclaimed = worker::claim_batch(&pool, "worker-b", "default", 10, 30_000)
        .await
        .unwrap();
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].attempts, 2);
    match &pool {
        Either::Left(p) => {
            let err = sqlx::query(
                "INSERT INTO job_effects (job_key, created_at) VALUES ('effect-post-p1', ?)",
            )
            .bind(now_ms())
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                err.to_string().to_lowercase().contains("unique"),
                "重跑必须命中唯一键（幂等处理器据此跳过）"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    assert!(worker::complete_job(&pool, "worker-b", "j1").await.unwrap());
    assert_eq!(job_status(&pool, "j1").await, "succeeded");

    let effects: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM job_effects")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(effects, 1, "崩溃重跑不得重复产生副作用");

    close_pool(&pool).await;
    cleanup(&dir);
}
