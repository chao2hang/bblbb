//! M01-AUDIT-03：幂等记录数据模型——scope/key/request hash/status/
//! response reference/expiry 的 schema 契约。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::idempotency::{
    begin_or_replay, complete, mark_failed, request_hash, validate_request_hash, IdempotencyKey,
    IdempotencyOutcome, IdempotencyStatus,
};
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-idem-{}", uuid::Uuid::now_v7()));
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

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// 插入一条幂等记录；返回 Result 便于断言唯一/CHECK 约束。
async fn insert_record(
    pool: &DatabasePool,
    id: &str,
    scope: &str,
    key: &str,
    status: &str,
    hash: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO idempotency_records (id, scope, key, request_hash, status, expires_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(id)
            .bind(scope)
            .bind(key)
            .bind(hash)
            .bind(status)
            .bind(now + 86_400_000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 幂等记录 schema 契约：字段、status CHECK、scope+key 唯一、expiry 索引。
#[tokio::test]
async fn idempotency_schema_contract() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    let hash = request_hash(br#"{"post_id":"p1"}"#);
    assert!(validate_request_hash(&hash).is_ok());

    match &pool {
        Either::Left(p) => {
            // 列齐全
            let columns: Vec<String> = sqlx::query_scalar(
                "SELECT name FROM pragma_table_info('idempotency_records') ORDER BY cid",
            )
            .fetch_all(p)
            .await
            .unwrap();
            for expected in [
                "id",
                "scope",
                "key",
                "request_hash",
                "status",
                "response_reference",
                "expires_at",
                "created_at",
                "updated_at",
            ] {
                assert!(
                    columns.contains(&expected.to_string()),
                    "idempotency_records 缺少列 {expected}"
                );
            }

            // 合法状态（in_progress/completed/failed）通过；非法 status 被 CHECK 拒绝
            assert!(
                insert_record(&pool, "r-in", "pay", "order-0", "in_progress", &hash, now)
                    .await
                    .is_ok()
            );
            assert!(
                insert_record(&pool, "r-c", "pay", "order-c", "completed", &hash, now)
                    .await
                    .is_ok()
            );
            assert!(
                insert_record(&pool, "r-f", "pay", "order-f", "failed", &hash, now)
                    .await
                    .is_ok()
            );
            assert!(
                insert_record(&pool, "r-bad", "pay", "order-bad", "bad_status", &hash, now)
                    .await
                    .is_err(),
                "非法 status 必须被 CHECK 拒绝"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// scope+key 唯一约束：重复键被拒绝（并发首请求去重的兜底）。
#[tokio::test]
async fn scope_and_key_are_unique() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    let hash = request_hash(b"payload");

    insert_record(&pool, "r1", "pay", "order-1", "completed", &hash, now)
        .await
        .unwrap();
    assert!(
        insert_record(&pool, "r2", "pay", "order-1", "completed", &hash, now)
            .await
            .is_err(),
        "相同 scope+key 必须被唯一约束拒绝"
    );
    insert_record(&pool, "r3", "pay", "order-2", "completed", &hash, now)
        .await
        .unwrap();
    insert_record(&pool, "r4", "download", "order-1", "completed", &hash, now)
        .await
        .unwrap(); // 不同 scope 不冲突

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 模型与数据库行往返：IdempotencyRecord 字段映射一致。
#[tokio::test]
async fn record_model_maps_to_database_row() {
    let (pool, dir) = pool_with_migrations().await;
    let now = now_ms();
    let key = IdempotencyKey::new("download", "dl-42").unwrap();
    let hash = request_hash(b"attachment-42");

    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO idempotency_records (id, scope, key, request_hash, status, response_reference, expires_at, created_at, updated_at)
                 VALUES ('rec-1', ?, ?, ?, 'in_progress', ?, ?, ?, ?)",
            )
            .bind(&key.scope)
            .bind(&key.key)
            .bind(&hash)
            .bind(Some("job-9"))
            .bind(now + 3_600_000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let row = match &pool {
        Either::Left(p) => {
            sqlx::query_as::<_, IdempotencyRow>(
                "SELECT id, scope, key, request_hash, status, response_reference, expires_at, created_at, updated_at
                 FROM idempotency_records WHERE id = 'rec-1'",
            )
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(row.scope, "download");
    assert_eq!(row.key, "dl-42");
    assert_eq!(
        row.status,
        IdempotencyStatus::InProgress.as_str(),
        "status 映射为 in_progress"
    );
    assert_eq!(row.response_reference.as_deref(), Some("job-9"));
    assert_eq!(row.expires_at, now + 3_600_000);
    assert_eq!(row.request_hash, hash, "request hash 原样存储");
    assert_eq!(row.created_at, now, "created_at 为毫秒时间戳");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M01-AUDIT-04：相同 key+摘要 → 返回原结果；相同 key+不同摘要 → 稳定 409。
#[tokio::test]
async fn replay_returns_original_result_and_conflict_is_stable() {
    let (pool, dir) = pool_with_migrations().await;
    let key = IdempotencyKey::new("pay", "order-777").unwrap();
    let hash_a = request_hash(br#"{"amount":100}"#);
    let hash_b = request_hash(br#"{"amount":200}"#);

    // 首次请求 → Created
    let first = begin_or_replay(&pool, &key, &hash_a, 86_400_000)
        .await
        .unwrap();
    let IdempotencyOutcome::Created { record_id } = first else {
        panic!("首次请求应 Created，得到 {first:?}");
    };

    // 进行中且同摘要 → InProgress
    let in_progress = begin_or_replay(&pool, &key, &hash_a, 86_400_000)
        .await
        .unwrap();
    assert_eq!(
        in_progress,
        IdempotencyOutcome::InProgress,
        "进行中不重复执行"
    );

    // 完成：保存响应引用（原结果）
    assert!(complete(&pool, &record_id, "resp-777").await.unwrap());

    // 相同 key+摘要 → Replay 原结果
    let replay = begin_or_replay(&pool, &key, &hash_a, 86_400_000)
        .await
        .unwrap();
    assert_eq!(
        replay,
        IdempotencyOutcome::Replay {
            response_reference: Some("resp-777".to_owned())
        },
        "相同 key+摘要必须返回原结果"
    );

    // 相同 key+不同摘要 → 稳定 Conflict（多次调用一致）
    for _ in 0..3 {
        let conflict = begin_or_replay(&pool, &key, &hash_b, 86_400_000)
            .await
            .unwrap();
        assert_eq!(
            conflict,
            IdempotencyOutcome::Conflict,
            "相同 key+不同摘要必须稳定返回 409"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M01-AUDIT-04/05：失败记录可重试；过期记录可重新开始。
#[tokio::test]
async fn failed_records_are_retryable_and_expired_records_restart() {
    let (pool, dir) = pool_with_migrations().await;
    let key = IdempotencyKey::new("download", "dl-999").unwrap();
    let hash = request_hash(b"attachment");

    // 首次 → 失败
    let first = begin_or_replay(&pool, &key, &hash, 86_400_000)
        .await
        .unwrap();
    let IdempotencyOutcome::Created { record_id } = first else {
        panic!("expected created")
    };
    assert!(mark_failed(&pool, &record_id).await.unwrap());

    // 同 key+摘要且 failed → Failed 变体（可重试）
    let retry = begin_or_replay(&pool, &key, &hash, 86_400_000)
        .await
        .unwrap();
    assert!(matches!(
        retry,
        IdempotencyOutcome::Failed {
            response_reference: None
        }
    ));

    // 记录过期后 → 删除并重新开始（Created）
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE idempotency_records SET expires_at = ? WHERE id = ?")
                .bind(now_ms() - 1_000)
                .bind(&record_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let restarted = begin_or_replay(&pool, &key, &hash, 86_400_000)
        .await
        .unwrap();
    assert!(
        matches!(restarted, IdempotencyOutcome::Created { .. }),
        "过期记录应删除并重新开始，得到 {restarted:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[derive(sqlx::FromRow)]
#[allow(dead_code)] // id/updated_at 由 FromRow 必需，此处不逐个断言
struct IdempotencyRow {
    id: String,
    scope: String,
    key: String,
    request_hash: String,
    status: String,
    response_reference: Option<String>,
    expires_at: i64,
    created_at: i64,
    updated_at: i64,
}
