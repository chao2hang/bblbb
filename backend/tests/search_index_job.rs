//! M03-SEARCH-STORE-06：索引幂等 Job——创建/更新/隐藏/删除/恢复/退出索引。
//!
//! 驱动真实入口（`handle_index_job` / `enqueue_index_job`）：
//! 1. 创建：published+public+板块公开+作者 active → 入索引，FTS MATCH 命中；
//! 2. 隐藏：status→hidden → 从索引移除（FTS 同步）；恢复 → 重新入索引；
//! 3. 删除：源行删除 → 索引清理（幂等）；
//! 4. 退出索引：作者 banned → 移除；
//! 5. 旧 revision 不覆盖新：stored.policy_revision 更大时重复执行被拒绝；
//! 6. 入队幂等：同一实体重复入队经 deduplication_key 合并；
//! 7. 无效 payload → 永久死信。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::retry::RetryClass;
use bblbb_backend::jobs::worker::ClaimedJob;
use bblbb_backend::jobs::worker_loop::JobOutcome;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::search::{enqueue_index_job, handle_index_job};
use serde_json::json;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations";

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("{MIGRATIONS_ROOT}/{engine}"))
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-idxjob-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("sqlite")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

async fn exec(pool: &DatabasePool, sql: &str, args: &[&str]) {
    match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(sql);
            for a in args {
                q = q.bind(a);
            }
            q.execute(p).await.unwrap();
        }
        Either::Right(p) => {
            let mut q = sqlx::query(sql);
            for a in args {
                q = q.bind(a);
            }
            q.execute(p).await.unwrap();
        }
    }
}

async fn scalar(pool: &DatabasePool, sql: &str, args: &[&str]) -> i64 {
    match pool {
        Either::Left(p) => {
            let mut q = sqlx::query_scalar::<_, i64>(sql);
            for a in args {
                q = q.bind(a);
            }
            q.fetch_one(p).await.unwrap()
        }
        Either::Right(p) => {
            let mut q = sqlx::query_scalar::<_, i64>(sql);
            for a in args {
                q = q.bind(a);
            }
            q.fetch_one(p).await.unwrap()
        }
    }
}

fn claimed_job(entity_type: &str, entity_id: &str) -> ClaimedJob {
    ClaimedJob {
        id: format!("job-{entity_type}-{entity_id}"),
        queue: "default".to_string(),
        kind: "search.index".to_string(),
        payload: json!({ "entity_type": entity_type, "entity_id": entity_id }),
        payload_version: 1,
        attempts: 1,
        max_attempts: 5,
        locked_until: now_millis() + 30_000,
    }
}

/// 种子：active 用户 + public 活跃板块 + published/public 帖子（带一个启用标签）。
async fn seed_post_fixture(pool: &DatabasePool) -> (String, String, String, String) {
    let now = now_millis();
    let user_id = uuid::Uuid::now_v7().to_string();
    let board_id = uuid::Uuid::now_v7().to_string();
    let post_id = uuid::Uuid::now_v7().to_string();
    let tag_id = uuid::Uuid::now_v7().to_string();
    // slug 必须全局唯一（boards_slug_uq），避免与种子板块冲突。
    let board_slug = format!("board-{}", &board_id[..8]);

    exec(
        pool,
        "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
         VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
        &[&user_id, "author1", "a1@example.com", &now.to_string(), &now.to_string()],
    )
    .await;
    exec(
        pool,
        "INSERT INTO boards (id, slug, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        &[
            &board_id,
            &board_slug,
            "技术讨论",
            &now.to_string(),
            &now.to_string(),
        ],
    )
    .await;
    exec(
        pool,
        "INSERT INTO posts (id, board_id, author_id, title, content, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
        &[
            &post_id,
            &board_id,
            &user_id,
            "SQLite FTS5 索引",
            "sqlite fulltext search index job",
            &now.to_string(),
            &now.to_string(),
        ],
    )
    .await;
    exec(
        pool,
        "INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)",
        &[&tag_id, "rust", &now.to_string()],
    )
    .await;
    exec(
        pool,
        "INSERT INTO post_tags (post_id, tag_id) VALUES (?, ?)",
        &[&post_id, &tag_id],
    )
    .await;
    (user_id, board_id, post_id, tag_id)
}

// ─────────────────────────── 测试 ───────────────────────────

/// 创建：published+public+板块公开+作者 active 的帖子入索引，FTS MATCH 命中。
#[tokio::test]
async fn create_indexes_published_public_post() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (_, _, post_id, _) = seed_post_fixture(&pool).await;

    let outcome = handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert!(matches!(outcome, JobOutcome::Succeeded), "{outcome:?}");

    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ?",
            &[&post_id]
        )
        .await,
        1,
        "帖子必须入索引"
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH ?",
            &["sqlite"]
        )
        .await,
        1,
        "FTS 必须命中 sqlite"
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH ?",
            &["fulltext"]
        )
        .await,
        1,
        "FTS 必须命中 fulltext"
    );
    // 标签入索引（tags_json）。
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ? AND tags_json LIKE ?",
            &[&post_id, "%rust%"]
        )
        .await,
        1,
        "启用标签必须进入 tags_json"
    );

    // 幂等：重复执行结果一致。
    let outcome2 = handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert!(matches!(outcome2, JobOutcome::Succeeded), "{outcome2:?}");
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ?",
            &[&post_id]
        )
        .await,
        1
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 隐藏→移除、恢复→重新入索引、删除源行→清理，全程幂等。
#[tokio::test]
async fn hide_restore_delete_are_idempotent() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (_, _, post_id, _) = seed_post_fixture(&pool).await;
    let now = now_millis();

    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        1
    );

    // 隐藏：status → hidden → 从索引移除（FTS 同步）。
    exec(
        &pool,
        "UPDATE posts SET status = 'hidden', updated_at = ? WHERE id = ?",
        &[&now.to_string(), &post_id],
    )
    .await;
    let outcome = handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert!(matches!(outcome, JobOutcome::Succeeded), "{outcome:?}");
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        0
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH ?",
            &["sqlite"]
        )
        .await,
        0
    );

    // 隐藏后重复执行：幂等（无可删）。
    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        0
    );

    // 恢复：status → published → 重新入索引。
    exec(
        &pool,
        "UPDATE posts SET status = 'published', updated_at = ? WHERE id = ?",
        &[&now.to_string(), &post_id],
    )
    .await;
    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        1
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH ?",
            &["sqlite"]
        )
        .await,
        1
    );

    // 删除源行：索引清理，且对已删除源重复执行幂等。
    exec(&pool, "DELETE FROM posts WHERE id = ?", &[&post_id]).await;
    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        0
    );
    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        0
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 退出索引：作者 banned → 从索引移除。
#[tokio::test]
async fn exit_index_when_author_banned() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _, post_id, _) = seed_post_fixture(&pool).await;
    let now = now_millis();

    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        1
    );

    exec(
        &pool,
        "UPDATE users SET status = 'banned', updated_at = ? WHERE id = ?",
        &[&now.to_string(), &user_id],
    )
    .await;
    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        0,
        "banned 作者内容退出索引"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 旧 revision 不覆盖新：stored.policy_revision 更大时重复执行被守卫拒绝。
#[tokio::test]
async fn old_revision_does_not_overwrite_new() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (_, _, post_id, _) = seed_post_fixture(&pool).await;

    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    let stored_before = scalar(
        &pool,
        "SELECT policy_revision FROM search_documents WHERE doc_id = ?",
        &[&post_id],
    )
    .await;

    // 模拟更早并发写者已经写入了更新的策略修订。
    exec(
        &pool,
        "UPDATE search_documents SET policy_revision = policy_revision + 100000 WHERE doc_id = ?",
        &[&post_id],
    )
    .await;
    let bumped = scalar(
        &pool,
        "SELECT policy_revision FROM search_documents WHERE doc_id = ?",
        &[&post_id],
    )
    .await;
    assert!(bumped > stored_before);

    // 陈旧写者（candidate = 源状态旧修订）重复执行：被拒绝，不覆盖新。
    handle_index_job(&pool, &claimed_job("post", &post_id)).await;
    let after = scalar(
        &pool,
        "SELECT policy_revision FROM search_documents WHERE doc_id = ?",
        &[&post_id],
    )
    .await;
    assert_eq!(after, bumped, "旧 revision 不得覆盖更新的索引");
    assert_eq!(
        scalar(&pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        1
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 入队幂等：同一实体重复入队经 deduplication_key 合并。
#[tokio::test]
async fn enqueue_coalesces_duplicate_jobs() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (_, _, post_id, _) = seed_post_fixture(&pool).await;

    enqueue_index_job(&pool, "post", &post_id).await.unwrap();
    enqueue_index_job(&pool, "post", &post_id).await.unwrap();
    enqueue_index_job(&pool, "post", &post_id).await.unwrap();

    let dedup = format!("search:index:post:{post_id}");
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index' AND deduplication_key = ?",
            &[&dedup]
        )
        .await,
        1,
        "重复入队必须合并为 1 个待处理 Job"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 无效 payload / 未知实体类型 → 永久死信。
#[tokio::test]
async fn invalid_payload_is_permanent() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    let bad = ClaimedJob {
        id: "job-bad".to_string(),
        queue: "default".to_string(),
        kind: "search.index".to_string(),
        payload: json!({ "entity_id": "x" }),
        payload_version: 1,
        attempts: 1,
        max_attempts: 5,
        locked_until: now_millis() + 30_000,
    };
    match handle_index_job(&pool, &bad).await {
        JobOutcome::Failed {
            class: RetryClass::Permanent,
            ..
        } => {}
        other => panic!("无效 payload 必须永久死信，实际 {other:?}"),
    }

    let unknown = claimed_job("draft", "x");
    match handle_index_job(&pool, &unknown).await {
        JobOutcome::Failed {
            class: RetryClass::Permanent,
            ..
        } => {}
        other => panic!("未知实体类型必须永久死信，实际 {other:?}"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 板块/标签/用户实体同样走裁决入索引（启用/公开 → 入；停用/删除 → 出）。
#[tokio::test]
async fn board_tag_user_indexing_gates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, board_id, _, tag_id) = seed_post_fixture(&pool).await;
    let now = now_millis();

    // board：启用 + public → 入索引。
    handle_index_job(&pool, &claimed_job("board", &board_id)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ? AND entity_type = 'board'",
            &[&board_id]
        )
        .await,
        1
    );
    // 板块停用 → 出索引。
    exec(
        &pool,
        "UPDATE boards SET is_active = 0, updated_at = ? WHERE id = ?",
        &[&now.to_string(), &board_id],
    )
    .await;
    handle_index_job(&pool, &claimed_job("board", &board_id)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ?",
            &[&board_id]
        )
        .await,
        0
    );

    // tag：启用 → 入索引。
    handle_index_job(&pool, &claimed_job("tag", &tag_id)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ? AND entity_type = 'tag'",
            &[&tag_id]
        )
        .await,
        1
    );
    exec(
        &pool,
        "UPDATE tags SET is_active = 0, updated_at = ? WHERE id = ?",
        &[&now.to_string(), &tag_id],
    )
    .await;
    handle_index_job(&pool, &claimed_job("tag", &tag_id)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ?",
            &[&tag_id]
        )
        .await,
        0
    );

    // user：active → 入索引；pending_delete → 出索引。
    handle_index_job(&pool, &claimed_job("user", &user_id)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ? AND entity_type = 'user'",
            &[&user_id]
        )
        .await,
        1
    );
    exec(
        &pool,
        "UPDATE users SET status = 'pending_delete', updated_at = ? WHERE id = ?",
        &[&now.to_string(), &user_id],
    )
    .await;
    handle_index_job(&pool, &claimed_job("user", &user_id)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ?",
            &[&user_id]
        )
        .await,
        0
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
