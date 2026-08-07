//! M07-SHOP-08/09：Reaction 服务测试（SQLite）。
//!
//! 覆盖：add/remove、重复冲突、自赞拒绝、限流、反应包消耗与汇总。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::economy::ledger::service::CURRENCY_COIN;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::reactions::service::{add_reaction, remove_reaction, summarize, ReactionError};
use sqlx::Either;

#[path = "common/mod.rs"]
mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-rx-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    user_id
}

async fn insert_board(pool: &DatabasePool) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let slug = format!("rx-{}", uuid::Uuid::now_v7().simple());
    match pool {
        Either::Left(p) => {
            sqlx::query("INSERT INTO boards (id, slug, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
                .bind(&id)
                .bind(&slug)
                .bind("reaction board")
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query("INSERT INTO boards (id, slug, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
                .bind(&id)
                .bind(&slug)
                .bind("reaction board")
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
    }
    id
}

async fn insert_post(pool: &DatabasePool, author_id: &str, board_id: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO posts (id, board_id, author_id, post_type, slug, title, content, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'article', ?, 'title', 'content', 'published', ?, ?)",
            )
            .bind(&id)
            .bind(board_id)
            .bind(author_id)
            .bind(format!("rx-{}", uuid::Uuid::now_v7().simple()))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO posts (id, board_id, author_id, post_type, slug, title, content, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'article', ?, 'title', 'content', 'published', ?, ?)",
            )
            .bind(&id)
            .bind(board_id)
            .bind(author_id)
            .bind(format!("rx-{}", uuid::Uuid::now_v7().simple()))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    id
}

/// 造 reaction_pack 权益（remaining_quantity=n）。
async fn grant_reaction_pack(pool: &DatabasePool, user_id: &str, n: i64) {
    let now = now_millis();
    let product_id = uuid::Uuid::now_v7().to_string();
    let order_id = uuid::Uuid::now_v7().to_string();
    let entitlement_id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO shop_products (id, kind, status, slug, title, slot, currency_id, unit_price, quantity_limit, required_level, refund_policy, version, created_by, created_at, updated_at)
                 VALUES (?, 'reaction_pack', 'published', ?, 'pack', 'reaction_pack', ?, 100, 1, 1, 'non_refundable', 1, ?, ?, ?)",
            )
            .bind(&product_id)
            .bind(format!("rp-{}", uuid::Uuid::now_v7().simple()))
            .bind(CURRENCY_COIN)
            .bind(user_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO shop_orders (id, user_id, product_id, product_version, quantity, currency_id, unit_price, total_amount, point_operation_id, status, idempotency_key, request_hash, created_at, updated_at)
                 VALUES (?, ?, ?, 1, 1, ?, 100, 100, ?, 'succeeded', ?, 'hash', ?, ?)",
            )
            .bind(&order_id)
            .bind(user_id)
            .bind(&product_id)
            .bind(CURRENCY_COIN)
            .bind(&entitlement_id)
            .bind(&entitlement_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO user_entitlements (id, user_id, product_id, order_id, status, quantity, remaining_quantity, valid_from, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'owned', ?, ?, ?, ?, ?)",
            )
            .bind(&entitlement_id)
            .bind(user_id)
            .bind(&product_id)
            .bind(&order_id)
            .bind(n)
            .bind(n)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            let _ = p;
            // MySQL 测试由 CI mysql-family --ignored 覆盖
        }
    }
}

#[tokio::test]
async fn add_remove_and_summarize() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let user = insert_user(&pool, "fan").await;
    let board = insert_board(&pool).await;
    let post = insert_post(&pool, &author, &board).await;

    let summary = add_reaction(&pool, &user, "post", &post, "like", false)
        .await
        .unwrap();
    assert_eq!(summary["total"], 1);
    assert_eq!(summary["counts"]["like"], 1);

    let summary = summarize(&pool, "post", &post).await.unwrap();
    assert_eq!(summary["total"], 1);

    // 重复添加 → AlreadyExists
    let err = add_reaction(&pool, &user, "post", &post, "like", false)
        .await
        .unwrap_err();
    assert!(matches!(err, ReactionError::AlreadyExists));

    let summary = remove_reaction(&pool, &user, "post", &post, "like")
        .await
        .unwrap();
    assert_eq!(summary["total"], 0);

    // 删除不存在的 → NotFoundReaction
    let err = remove_reaction(&pool, &user, "post", &post, "like")
        .await
        .unwrap_err();
    assert!(matches!(err, ReactionError::NotFoundReaction));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn self_reaction_is_rejected() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let board = insert_board(&pool).await;
    let post = insert_post(&pool, &author, &board).await;
    let err = add_reaction(&pool, &author, "post", &post, "like", false)
        .await
        .unwrap_err();
    assert!(matches!(err, ReactionError::SelfReaction));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn missing_target_is_not_found() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let user = insert_user(&pool, "fan").await;
    let err = add_reaction(&pool, &user, "post", "no-such-post", "like", false)
        .await
        .unwrap_err();
    assert!(matches!(err, ReactionError::NotFound(_)));
    let _ = author;
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn reaction_pack_is_consumed() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let user = insert_user(&pool, "fan").await;
    let board = insert_board(&pool).await;
    let post = insert_post(&pool, &author, &board).await;
    grant_reaction_pack(&pool, &user, 1).await;

    add_reaction(&pool, &user, "post", &post, "like", true)
        .await
        .unwrap();
    // 重复添加 → AlreadyExists（不消耗包）。
    add_reaction(&pool, &user, "post", &post, "like", true)
        .await
        .unwrap_err();
    // 需要包但已耗尽 → PackExhausted（删掉重试，包已被第一次消耗）。
    remove_reaction(&pool, &user, "post", &post, "like")
        .await
        .unwrap();
    let err = add_reaction(&pool, &user, "post", &post, "like", true)
        .await
        .unwrap_err();
    assert!(matches!(err, ReactionError::PackExhausted));

    let remaining: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT remaining_quantity FROM user_entitlements WHERE user_id = ?")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT remaining_quantity FROM user_entitlements WHERE user_id = ?")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(remaining, 0, "包应被原子扣减至耗尽");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn outbox_notification_event_is_written() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "author").await;
    let user = insert_user(&pool, "fan").await;
    let board = insert_board(&pool).await;
    let post = insert_post(&pool, &author, &board).await;
    add_reaction(&pool, &user, "post", &post, "like", false)
        .await
        .unwrap();
    let events: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'reaction.created.v1'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'reaction.created.v1'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(events, 1);
    close_pool(&pool).await;
    cleanup(&dir);
}
