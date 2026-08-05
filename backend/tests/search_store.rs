//! M03-SEARCH-STORE-02：SQLite FTS5 迁移、触发器同步与重建命令。
//!
//! 断言 0030 迁移在 SQLite 上建立：
//! 1. `search_documents`（常规元数据表，rowid 供 external content 映射）；
//! 2. `search_fts`（FTS5 external content 虚拟表，content='search_documents'）；
//! 3. 三个同步触发器——search_documents INSERT/UPDATE/DELETE 自动维护
//!    search_fts（Job 只写元数据表，不直接写 FTS 表）；
//! 4. 重建命令（`INSERT INTO search_fts(search_fts) VALUES('rebuild')`）幂等，
//!    重建后与 content 表一致。
//!
//! 只跑 SQLite（本地）；MySQL/MariaDB FULLTEXT 等价由 0031/0032 迁移与
//! `schema_fixture` 三库 Fixture（`BBLBB_TEST_MYSQL_URL` + `--ignored`）覆盖。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::search::rebuild_fts;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations";

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("{MIGRATIONS_ROOT}/{engine}"))
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-search-{}", uuid::Uuid::now_v7()));
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

async fn count_fts(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM search_fts")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM search_fts")
            .fetch_one(p)
            .await
            .unwrap(),
    }
}

async fn match_hits(pool: &DatabasePool, term: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH ?")
                .bind(term)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH ?")
                .bind(term)
                .fetch_one(p)
                .await
                .unwrap()
        }
    }
}

/// 0030 迁移建立 search_documents + search_fts（external content）。
#[tokio::test]
async fn migration_creates_document_and_fts_tables() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    match &pool {
        Either::Left(p) => {
            let docs: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='search_documents'",
            )
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(docs, 1, "search_documents 表必须存在");

            // search_fts 是 FTS5 虚拟表（type='table'，sql 含 'USING fts5'）。
            let fts: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='search_fts'",
            )
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(fts, 1, "search_fts 虚拟表必须存在");
            let fts_sql: String = sqlx::query_scalar(
                "SELECT sql FROM sqlite_master WHERE type='table' AND name='search_fts'",
            )
            .fetch_one(p)
            .await
            .unwrap();
            assert!(
                fts_sql.contains("USING fts5"),
                "search_fts 必须是 FTS5: {fts_sql}"
            );
            assert!(
                fts_sql.contains("content='search_documents'"),
                "search_fts 必须是 external content: {fts_sql}"
            );

            // 三个同步触发器。
            let triggers: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='trigger' AND name LIKE 'search_fts_%'",
            )
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(triggers, 3, "必须存在 INSERT/UPDATE/DELETE 三个同步触发器");
        }
        Either::Right(_) => panic!("本测试只跑 SQLite"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 插入/更新/删除 search_documents 行后 search_fts 自动同步（触发器策略）。
#[tokio::test]
async fn fts_triggers_sync_insert_update_delete() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();

    // 插入一篇 post：FTS 自动收录（title/body 全文；其他字段不进 FTS）。
    exec(
        &pool,
        "INSERT INTO search_documents
             (doc_id, entity_type, title, body, excerpt, slug, tags_json, source_revision, policy_revision, indexed_at)
         VALUES (?, 'post', ?, ?, ?, ?, '[]', ?, ?, ?)",
        &[
            "01911fd5-f000-7000-8000-0000000000a1",
            "SQLite FTS5 测试",
            "sqlite fulltext search index storage",
            "sqlite fulltext search index storage",
            "my-post-1",
            &now.to_string(),
            &now.to_string(),
            &now.to_string(),
        ],
    )
    .await;
    assert_eq!(count_fts(&pool).await, 1, "插入后 FTS 必须同步 1 行");
    assert_eq!(match_hits(&pool, "sqlite").await, 1, "title/body 命中");
    assert_eq!(match_hits(&pool, "fulltext").await, 1, "body 命中");

    // 更新 title：FTS 内容随之更新（旧 token 不再命中）。
    exec(
        &pool,
        "UPDATE search_documents SET title = ?, body = ? WHERE doc_id = ?",
        &[
            "MariaDB fulltext notes",
            "mariadb fulltext index notes",
            "01911fd5-f000-7000-8000-0000000000a1",
        ],
    )
    .await;
    assert_eq!(count_fts(&pool).await, 1, "更新不改变行数");
    assert_eq!(
        match_hits(&pool, "sqlite").await,
        0,
        "旧正文 token 必须不再命中"
    );
    assert_eq!(match_hits(&pool, "mariadb").await, 1, "新标题必须命中");
    assert_eq!(match_hits(&pool, "fulltext").await, 1, "新正文必须命中");

    // 删除：FTS 同步删除。
    exec(
        &pool,
        "DELETE FROM search_documents WHERE doc_id = ?",
        &["01911fd5-f000-7000-8000-0000000000a1"],
    )
    .await;
    assert_eq!(count_fts(&pool).await, 0, "删除后 FTS 必须清空");
    assert_eq!(match_hits(&pool, "mariadb").await, 0, "删除后必须不可命中");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 重建命令：`INSERT INTO search_fts(search_fts) VALUES('rebuild')` 幂等，
/// 重建后与 search_documents 一致（不丢数据、不重复）。
#[tokio::test]
async fn rebuild_command_is_idempotent_and_consistent() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();

    let rows = [
        ("01911fd5-f000-7000-8000-0000000000b1", "rust concurrency"),
        (
            "01911fd5-f000-7000-8000-0000000000b2",
            "sqlite fts5 external content",
        ),
        (
            "01911fd5-f000-7000-8000-0000000000b3",
            "postgresql? no, three databases",
        ),
    ];
    for (id, body) in rows {
        exec(
            &pool,
            "INSERT INTO search_documents
                 (doc_id, entity_type, title, body, excerpt, slug, tags_json, source_revision, policy_revision, indexed_at)
             VALUES (?, 'post', ?, ?, ?, ?, '[]', ?, ?, ?)",
            &[id, "title", body, body, id, &now.to_string(), &now.to_string(), &now.to_string()],
        )
        .await;
    }
    assert_eq!(count_fts(&pool).await, 3);

    // 重建命令幂等：两次重建后行数与命中都不变。
    rebuild_fts(&pool).await.unwrap();
    rebuild_fts(&pool).await.unwrap();
    assert_eq!(count_fts(&pool).await, 3, "重建不得丢失/重复行");
    assert_eq!(match_hits(&pool, "concurrency").await, 1);
    assert_eq!(match_hits(&pool, "fts5").await, 1);
    assert_eq!(match_hits(&pool, "databases").await, 1);

    // 重建后继续走触发器同步。
    exec(
        &pool,
        "DELETE FROM search_documents WHERE doc_id = ?",
        &["01911fd5-f000-7000-8000-0000000000b2"],
    )
    .await;
    assert_eq!(count_fts(&pool).await, 2);
    assert_eq!(match_hits(&pool, "fts5").await, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}
