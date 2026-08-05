//! M01-JOBS-06：消费者以 event_id / job idempotency key 去重，
//! 至少一次投递不得产生重复业务副作用。

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
    let dir = std::env::temp_dir().join(format!("bblbb-consumer-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();

    // 业务副作用载体表（模拟消费者写出的业务行）
    match &pool {
        Either::Left(p) => {
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

async fn effect_count(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM consumer_effects")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn event_status(pool: &DatabasePool, id: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM outbox_events WHERE id = ?")
            .bind(id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 完整消费者循环：取回 pending 事件，逐个在事务内去重 + 副作用 + 标记 sent。
/// 返回本次实际执行副作用的次数。
async fn drain_pending(pool: &DatabasePool, consumer: &str) -> i64 {
    let mut effects = 0;
    let pending = outbox::fetch_pending(pool, 50).await.unwrap();
    for event in pending {
        let mut tx = match pool {
            Either::Left(p) => Either::Left(p.begin().await.unwrap()),
            Either::Right(p) => Either::Right(p.begin().await.unwrap()),
        };
        if outbox::consume_in_tx(&mut tx, &event.id, consumer)
            .await
            .unwrap()
        {
            // 业务副作用：与去重标记同一事务
            match &mut tx {
                Either::Left(t) => {
                    sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES (?, ?)")
                        .bind(&event.id)
                        .bind(consumer)
                        .execute(&mut **t)
                        .await
                        .unwrap();
                }
                Either::Right(t) => {
                    sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES (?, ?)")
                        .bind(&event.id)
                        .bind(consumer)
                        .execute(&mut **t)
                        .await
                        .unwrap();
                }
            }
            effects += 1;
        }
        outbox::mark_sent_in_tx(&mut tx, &event.id).await.unwrap();
        match tx {
            Either::Left(t) => t.commit().await.unwrap(),
            Either::Right(t) => t.commit().await.unwrap(),
        }
    }
    effects
}

/// 同一消费者对同一事件只领取一次；不同消费者各自独立去重。
#[tokio::test]
async fn consume_in_tx_wins_once_per_consumer() {
    let (pool, dir) = pool_with_migrations().await;

    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    let event_id = outbox::enqueue_in_tx(&mut tx, "test.registered.v1", json!({ "user_id": "u1" }))
        .await
        .unwrap();
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    // 同一消费者第一次 → true
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(outbox::consume_in_tx(&mut tx, &event_id, "mailer")
        .await
        .unwrap());
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    // 同一消费者第二次（重复投递）→ false
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(
        !outbox::consume_in_tx(&mut tx, &event_id, "mailer")
            .await
            .unwrap(),
        "同一消费者重复投递必须被去重"
    );
    match tx {
        Either::Left(t) => t.rollback().await.unwrap(),
        Either::Right(t) => t.rollback().await.unwrap(),
    }

    // 不同消费者 → true（各自独立去重）
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(outbox::consume_in_tx(&mut tx, &event_id, "search-index")
        .await
        .unwrap());
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 两个消费者竞争同一事件：只有一个能执行副作用，事件只被标记一次 sent。
#[tokio::test]
async fn racing_consumers_do_not_duplicate_side_effect() {
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

    // 消费者 A 与 B 都取回了同一 pending 事件（至少一次投递）
    let pending = outbox::fetch_pending(&pool, 50).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, event_id);

    // A：领取成功 → 副作用 + 标记 sent → 提交
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(outbox::consume_in_tx(&mut tx, &event_id, "mailer")
        .await
        .unwrap());
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES ('a', 'mailer')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES ('a', 'mailer')")
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

    // B：去重失败 → 跳过副作用
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(
        !outbox::consume_in_tx(&mut tx, &event_id, "mailer")
            .await
            .unwrap(),
        "重复投递必须被去重，不得再执行副作用"
    );
    match tx {
        Either::Left(t) => t.rollback().await.unwrap(),
        Either::Right(t) => t.rollback().await.unwrap(),
    }

    assert_eq!(effect_count(&pool).await, 1, "副作用只能出现一次");
    assert_eq!(event_status(&pool, &event_id).await, "sent");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 消费者崩溃：事务回滚后，去重标记与副作用一起消失；重投时重新执行并
/// 恰好提交一次副作用。
#[tokio::test]
async fn crash_rollback_replays_side_effect_exactly_once() {
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

    // 第一次处理：写入副作用后“崩溃”（回滚）→ 事件仍 pending，无任何残留
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(outbox::consume_in_tx(&mut tx, &event_id, "mailer")
        .await
        .unwrap());
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES ('x', 'mailer')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES ('x', 'mailer')")
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
    assert_eq!(effect_count(&pool).await, 0, "回滚后副作用不得残留");
    assert_eq!(
        event_status(&pool, &event_id).await,
        "pending",
        "回滚后事件仍可重投"
    );

    // 重投：去重标记随上次回滚消失 → 可再次领取，副作用只提交一次
    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    assert!(
        outbox::consume_in_tx(&mut tx, &event_id, "mailer")
            .await
            .unwrap(),
        "崩溃后重投应可重新领取"
    );
    match &mut tx {
        Either::Left(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES ('x', 'mailer')")
                .execute(&mut **t)
                .await
                .unwrap();
        }
        Either::Right(t) => {
            sqlx::query("INSERT INTO consumer_effects (event_id, consumer) VALUES ('x', 'mailer')")
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
    assert_eq!(effect_count(&pool).await, 1, "副作用恰好提交一次");
    assert_eq!(event_status(&pool, &event_id).await, "sent");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 完整消费者循环：事件全部投递一次；再次循环无残留事件、无重复副作用。
#[tokio::test]
async fn consumer_loop_delivers_each_event_once() {
    let (pool, dir) = pool_with_migrations().await;

    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(p) => Either::Right(p.begin().await.unwrap()),
    };
    outbox::enqueue_in_tx(&mut tx, "test.a.v1", json!({ "n": 1 }))
        .await
        .unwrap();
    outbox::enqueue_in_tx(&mut tx, "test.b.v1", json!({ "n": 2 }))
        .await
        .unwrap();
    match tx {
        Either::Left(t) => t.commit().await.unwrap(),
        Either::Right(t) => t.commit().await.unwrap(),
    }

    // 第一轮：两个事件各执行一次副作用
    let effects = drain_pending(&pool, "mailer").await;
    assert_eq!(effects, 2);
    assert_eq!(effect_count(&pool).await, 2);

    // 第二轮：没有 pending 事件，不会重复
    let effects = drain_pending(&pool, "mailer").await;
    assert_eq!(effects, 0, "已投递事件不得再次产生副作用");
    assert_eq!(effect_count(&pool).await, 2);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Job idempotency key 去重：相同 deduplication_key 只能创建一个 job，
/// 因此该业务副作用只入队一次（至少一次创建不产生重复副作用）。
#[tokio::test]
async fn job_deduplication_key_prevents_duplicate_creation() {
    let (pool, dir) = pool_with_migrations().await;
    let now = chrono::Utc::now().timestamp_millis();

    insert_index_job(&pool, "job-1", now).await.unwrap();
    assert!(
        insert_index_job(&pool, "job-2", now).await.is_err(),
        "相同 deduplication_key 必须被唯一约束拒绝（业务副作用只入队一次）"
    );

    // 只存在一个索引 job
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM jobs WHERE deduplication_key = 'index-post-p1'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(count, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

async fn insert_index_job(pool: &DatabasePool, id: &str, now: i64) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO jobs (id, queue, kind, payload, payload_version, status, attempts, max_attempts, available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, 'search', 'index_post', '{\"post_id\":\"p1\"}', 1, 'queued', 0, 5, ?, 'index-post-p1', ?, ?)",
            )
            .bind(id)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}
