//! M03-SEARCH-STORE-07：三数据库同一 Fixture 验证索引生命周期与基础查询契约。
//!
//! 同一套数据/操作在 SQLite FTS5、MySQL 8 FULLTEXT、MariaDB 10.11 FULLTEXT
//! 上断言一致：
//! 1. **查询**：帖子经 `handle_index_job` 入索引后，各引擎查询（FTS5 `MATCH`
//!    / `MATCH..AGAINST`）命中相同；内容更新后旧词不命中、新词命中；
//! 2. **删除**：源行删除后索引清理，查询不再命中（幂等）；
//! 3. **重建**：`rebuild_fts` 后行数与命中不变（幂等一致）；
//! 4. **旧 revision 不覆盖新**：stored.policy_revision 更大时重复执行被守卫
//!    拒绝，不覆盖更新的索引。
//!
//! - SQLite：本地始终运行（临时文件 + 迁移）；
//! - MySQL 8 / MariaDB 10.11：`BBLBB_TEST_MYSQL_URL` + `#[ignore]`
//!   （CI mysql-family 任务 `--ignored` 分别对两个数据库运行）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::worker::ClaimedJob;
use bblbb_backend::jobs::worker_loop::JobOutcome;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::search::{enqueue_index_job, handle_index_job, rebuild_fts};
use serde_json::json;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations";

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("{MIGRATIONS_ROOT}/{engine}"))
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-fix-{}", uuid::Uuid::now_v7()));
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

/// 引擎无关的索引命中数：SQLite FTS5 `MATCH` / MySQL·MariaDB `MATCH..AGAINST`。
async fn query_hits(pool: &DatabasePool, term: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH ?")
                .bind(term)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM search_documents
             WHERE MATCH (title, body) AGAINST (? IN NATURAL LANGUAGE MODE)",
        )
        .bind(term)
        .fetch_one(p)
        .await
        .unwrap(),
    }
}

/// 清空索引（Fixture 起始/收尾，保证 CI 共享库无残留干扰）。
async fn reset_index(pool: &DatabasePool) {
    exec(pool, "DELETE FROM search_documents", &[]).await;
}

/// 同一 Fixture 生命周期：查询/更新/删除/重建/旧 revision 不覆盖新。
async fn fixture_flow(pool: &DatabasePool) {
    reset_index(pool).await;
    let now = now_millis();

    let user_id = uuid::Uuid::now_v7().to_string();
    let board_id = uuid::Uuid::now_v7().to_string();
    let post_id = uuid::Uuid::now_v7().to_string();
    let board_slug = format!("board-{}", &board_id[..8]);

    exec(
        pool,
        "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
         VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
        &[&user_id, "author2", "a2@example.com", &now.to_string(), &now.to_string()],
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

    // 1) 查询：入索引后各引擎命中一致。
    let outcome = handle_index_job(pool, &claimed_job("post", &post_id)).await;
    assert!(matches!(outcome, JobOutcome::Succeeded), "{outcome:?}");
    assert_eq!(query_hits(pool, "sqlite").await, 1, "查询命中必须一致");
    assert_eq!(query_hits(pool, "fulltext").await, 1, "查询命中必须一致");
    assert_eq!(
        scalar(
            pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ?",
            &[&post_id]
        )
        .await,
        1
    );

    // 2) 更新：内容变更后旧词不命中、新词命中（各引擎一致）。
    exec(
        pool,
        "UPDATE posts SET title = 'MariaDB notes', content = 'mariadb fulltext indexing', updated_at = ? WHERE id = ?",
        &[&now.to_string(), &post_id],
    )
    .await;
    handle_index_job(pool, &claimed_job("post", &post_id)).await;
    assert_eq!(query_hits(pool, "sqlite").await, 0, "旧词必须不再命中");
    assert_eq!(query_hits(pool, "mariadb").await, 1, "新词必须命中");
    assert_eq!(query_hits(pool, "indexing").await, 1, "新词必须命中");

    // 3) 删除：源行删除后索引清理，查询不再命中（幂等重复执行）。
    exec(pool, "DELETE FROM posts WHERE id = ?", &[&post_id]).await;
    handle_index_job(pool, &claimed_job("post", &post_id)).await;
    handle_index_job(pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(
            pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ?",
            &[&post_id]
        )
        .await,
        0,
        "删除后索引必须清理"
    );
    assert_eq!(query_hits(pool, "mariadb").await, 0, "删除后必须不命中");

    // 4) 重建：重新入索引后 rebuild 幂等，行数与命中不变。
    exec(
        pool,
        "INSERT INTO posts (id, board_id, author_id, title, content, created_at, updated_at)
         VALUES (?, ?, ?, 'SQLite FTS5 索引', 'sqlite fulltext search index job', ?, ?)",
        &[
            &post_id,
            &board_id,
            &user_id,
            &now.to_string(),
            &now.to_string(),
        ],
    )
    .await;
    handle_index_job(pool, &claimed_job("post", &post_id)).await;
    let before = scalar(pool, "SELECT COUNT(*) FROM search_documents", &[]).await;
    rebuild_fts(pool).await.unwrap();
    rebuild_fts(pool).await.unwrap();
    assert_eq!(
        scalar(pool, "SELECT COUNT(*) FROM search_documents", &[]).await,
        before,
        "重建必须幂等不丢不重"
    );
    assert_eq!(query_hits(pool, "sqlite").await, 1, "重建后必须仍命中");

    // 5) 旧 revision 不覆盖新：stored.policy_revision 更大时重复执行被拒绝。
    let stored_before = scalar(
        pool,
        "SELECT policy_revision FROM search_documents WHERE doc_id = ?",
        &[&post_id],
    )
    .await;
    exec(
        pool,
        "UPDATE search_documents SET policy_revision = policy_revision + 100000 WHERE doc_id = ?",
        &[&post_id],
    )
    .await;
    let bumped = scalar(
        pool,
        "SELECT policy_revision FROM search_documents WHERE doc_id = ?",
        &[&post_id],
    )
    .await;
    assert!(bumped > stored_before);
    handle_index_job(pool, &claimed_job("post", &post_id)).await;
    assert_eq!(
        scalar(
            pool,
            "SELECT policy_revision FROM search_documents WHERE doc_id = ?",
            &[&post_id],
        )
        .await,
        bumped,
        "旧 revision 不得覆盖更新的索引"
    );

    // 入队合并（跨引擎一致）。
    enqueue_index_job(pool, "post", &post_id).await.unwrap();
    enqueue_index_job(pool, "post", &post_id).await.unwrap();
    let dedup = format!("search:index:post:{post_id}");
    assert_eq!(
        scalar(
            pool,
            "SELECT COUNT(*) FROM jobs WHERE kind = 'search.index' AND deduplication_key = ?",
            &[&dedup],
        )
        .await,
        1,
        "重复入队必须合并"
    );

    reset_index(pool).await;
}

// ─────────────────────────── 测试 ───────────────────────────

/// SQLite：本地始终运行。
#[tokio::test]
async fn sqlite_search_fixture() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    fixture_flow(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

/// MySQL 8：CI mysql-family 矩阵 --ignored 运行。
#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_search_fixture() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mysql")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    fixture_flow(&pool).await;
    close_pool(&pool).await;
}

/// MariaDB 10.11：CI mysql-family 矩阵 --ignored 运行。
#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mariadb_search_fixture() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mariadb")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    fixture_flow(&pool).await;
    close_pool(&pool).await;
}
