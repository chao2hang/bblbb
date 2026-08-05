//! M02-IDENTITY-02：规范化用户名/邮箱的跨库唯一约束 + 大小写/Unicode Fixture。
//!
//! 规范化列排序规则大小写敏感（BINARY/utf8mb4_bin），唯一性由"应用层先
//! 规范化再入库"保证：`User` / `USER` / `Ｕｓｅｒ` 都规范化为 `user`，
//! 二次插入命中唯一约束。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::{normalize_email, normalize_username};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-norm-{}", uuid::Uuid::now_v7()));
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

/// 直接以给定（可能未规范化的）值插入用户；返回 Result 便于断言唯一约束。
async fn insert_user_raw(
    pool: &DatabasePool,
    id: &str,
    username_normalized: &str,
    email_normalized: &str,
) -> Result<(), sqlx::Error> {
    let now = now_ms();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'phc-test', 'pending', ?, ?)",
            )
            .bind(id)
            .bind(username_normalized)
            .bind(email_normalized)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 大小写 Fixture：`User` 与 `USER` 规范化后必须唯一冲突。
#[tokio::test]
async fn username_case_variants_collide_after_normalization() {
    let (pool, dir) = pool_with_migrations().await;

    // 第一次：写入规范化值
    insert_user_raw(
        &pool,
        "u1",
        &normalize_username("User"),
        &normalize_email("user1@example.com"),
    )
    .await
    .unwrap();

    // 第二次：大写变体规范化后同一值 → 唯一约束拒绝
    assert!(
        insert_user_raw(
            &pool,
            "u2",
            &normalize_username("USER"),
            &normalize_email("user2@example.com")
        )
        .await
        .is_err(),
        "USER 与 User 规范化后必须唯一冲突"
    );

    // 控制组：未规范化直接写入不冲突（证明大小写敏感排序规则 + 应用层规范化的必要性）
    insert_user_raw(&pool, "u3", "UPPER", &normalize_email("user3@example.com"))
        .await
        .unwrap();
    insert_user_raw(&pool, "u4", "upper", &normalize_email("user4@example.com"))
        .await
        .unwrap();

    close_pool(&pool).await;
    cleanup(&dir);
}

/// Unicode Fixture：全角、连字、带音字母变体规范化后与 ASCII 变体冲突。
#[tokio::test]
async fn unicode_variants_collide_after_normalization() {
    let (pool, dir) = pool_with_migrations().await;

    // 全角用户名 Ｕｓｅｒ → user
    insert_user_raw(
        &pool,
        "u1",
        &normalize_username("Ｕｓｅｒ"),
        &normalize_email("user1@example.com"),
    )
    .await
    .unwrap();
    assert!(
        insert_user_raw(
            &pool,
            "u2",
            &normalize_username("user"),
            &normalize_email("user2@example.com")
        )
        .await
        .is_err(),
        "全角 Ｕｓｅｒ 与 user 规范化后必须唯一冲突"
    );

    // 带音字母 ÅNGSTRÖM → ångström；同规范化值冲突
    insert_user_raw(
        &pool,
        "u3",
        &normalize_username("ÅNGSTRÖM"),
        &normalize_email("user3@example.com"),
    )
    .await
    .unwrap();
    assert!(
        insert_user_raw(
            &pool,
            "u4",
            &normalize_username("ångström"),
            &normalize_email("user4@example.com"),
        )
        .await
        .is_err(),
        "ÅNGSTRÖM 与 ångström 规范化后必须唯一冲突"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 邮箱 Fixture：大小写/全角变体规范化后与标准形式唯一冲突。
#[tokio::test]
async fn email_variants_collide_after_normalization() {
    let (pool, dir) = pool_with_migrations().await;

    insert_user_raw(
        &pool,
        "u1",
        &normalize_username("user1"),
        &normalize_email("User@Example.COM"),
    )
    .await
    .unwrap();
    assert!(
        insert_user_raw(
            &pool,
            "u2",
            &normalize_username("user2"),
            &normalize_email("USER@example.com"),
        )
        .await
        .is_err(),
        "邮箱大小写变体规范化后必须唯一冲突"
    );
    assert!(
        insert_user_raw(
            &pool,
            "u3",
            &normalize_username("user3"),
            &normalize_email("ｕｓｅｒ＠ｅｘａｍｐｌｅ．ｃｏｍ"),
        )
        .await
        .is_err(),
        "邮箱全角变体规范化后必须唯一冲突"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
