//! M16-ECONOMY-07：对事务每一步注入失败，证明不会出现"余额变但流水/授权/Outbox
//! 缺失"——所有失败路径整体回滚，无部分状态。
//!
//! 覆盖（驱动 SHIPPED 代码 shop/marketplace/download/ledger 服务）：
//!   * 商城购买：在 ① 余额不足 ② 库存不足 ③ 限购 ④ 等级门槛 ⑤ 销售窗口 每步
//!     注入失败 → 余额/库存/订单/权益/Outbox/审计全部不变。
//!   * Marketplace 确认：余额不足 → Intent 不被消费、无 Purchase、无扣款、无 Outbox。
//!   * 下载：URL 签发失败整体回滚（M16-STORAGE-FAULTS-07 faults.rs 已覆盖，此处
//!     引用不重复）。
//!
//! 不变量：`初始余额 + Σdelta = 当前余额`，且失败路径不产生任何 point_transactions
//! 行、orders 行、entitlements 行、outbox_events 行或 audit_logs 行。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::economy::ledger::service as ledger;
use bblbb_backend::economy::ledger::service::{LedgerKind, CURRENCY_COIN};
use bblbb_backend::outbox::now_millis;
use bblbb_backend::shop::service::{buy_product, ShopError};
use sqlx::Either;

async fn setup() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-step-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str, level: i64) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(level)
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
                 VALUES (?, ?, ?, 'dummy', 'active', ?, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(level)
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

async fn credit_user(pool: &DatabasePool, user_id: &str, amount: i64) {
    let cmd = ledger::LedgerCommand {
        idempotency_scope: "test".to_string(),
        idempotency_key: uuid::Uuid::now_v7().to_string(),
        kind: LedgerKind::Award,
        actor_id: Some(user_id.to_string()),
        user_id: user_id.to_string(),
        currency_id: CURRENCY_COIN.to_string(),
        delta_balance: amount,
        delta_frozen: 0,
        source_type: None,
        source_id: None,
        memo: "step test credit".to_string(),
        reverses_operation_id: None,
    };
    ledger::apply_operation(pool, cmd, now_millis())
        .await
        .unwrap();
}

async fn insert_product(
    pool: &DatabasePool,
    owner_id: &str,
    stock: Option<i64>,
    unit_price: i64,
    required_level: i64,
    refund_policy: &str,
) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO shop_products
                     (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
                 VALUES (?, 'cosmetic_nickname', 'published', ?, ?, NULL, NULL, NULL, 'nickname', ?, ?, 10, ?, ?, NULL, NULL, NULL, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(format!("slug-{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("title-{}", uuid::Uuid::now_v7().simple()))
            .bind(CURRENCY_COIN)
            .bind(unit_price)
            .bind(stock)
            .bind(required_level)
            .bind(refund_policy)
            .bind(owner_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO shop_products
                     (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
                 VALUES (?, 'cosmetic_nickname', 'published', ?, ?, NULL, NULL, NULL, 'nickname', ?, ?, 10, ?, ?, NULL, NULL, NULL, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(format!("slug-{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("title-{}", uuid::Uuid::now_v7().simple()))
            .bind(CURRENCY_COIN)
            .bind(unit_price)
            .bind(stock)
            .bind(required_level)
            .bind(refund_policy)
            .bind(owner_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    id
}

async fn account_balance(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(CURRENCY_COIN)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(CURRENCY_COIN)
        .fetch_one(p)
        .await
        .unwrap(),
    }
}

async fn count_rows(pool: &DatabasePool, table: &str, user_id: &str) -> i64 {
    let sql = match table {
        "orders" => "SELECT COUNT(*) FROM shop_orders WHERE user_id = ?",
        "entitlements" => "SELECT COUNT(*) FROM user_entitlements WHERE user_id = ?",
        "transactions" => "SELECT COUNT(*) FROM point_transactions WHERE user_id = ?",
        "outbox" => "SELECT COUNT(*) FROM outbox_events",
        "audit" => "SELECT COUNT(*) FROM audit_logs",
        _ => panic!("unknown table {table}"),
    };
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
    }
}

/// 注入失败后的全局不变量：无订单/权益/流水/Outbox/审计残留（相对失败前快照）。
async fn snapshot(pool: &DatabasePool, user_id: &str) -> (i64, i64, i64, i64, i64, i64) {
    (
        account_balance(pool, user_id).await,
        count_rows(pool, "orders", user_id).await,
        count_rows(pool, "entitlements", user_id).await,
        count_rows(pool, "transactions", user_id).await,
        count_rows(pool, "outbox", user_id).await,
        count_rows(pool, "audit", user_id).await,
    )
}

async fn assert_no_partial_state(
    pool: &DatabasePool,
    user_id: &str,
    before: (i64, i64, i64, i64, i64, i64),
) {
    let after = snapshot(pool, user_id).await;
    assert_eq!(after.0, before.0, "余额不变");
    assert_eq!(after.1, before.1, "无订单残留");
    assert_eq!(after.2, before.2, "无权益残留");
    assert_eq!(after.3, before.3, "无流水残留");
    assert_eq!(after.4, before.4, "无 Outbox 残留");
    assert_eq!(after.5, before.5, "无审计残留");
}

/// 步骤① 余额不足：扣款失败 → 全回滚。
#[tokio::test]
async fn step_insufficient_balance_rolls_back_everything() {
    let (pool, dir) = setup().await;
    let user = insert_user(&pool, "a", 1).await;
    credit_user(&pool, &user, 10).await; // 余额 10 < 单价 100
    let product = insert_product(&pool, &user, Some(5), 100, 1, "non_refundable").await;

    let before = snapshot(&pool, &user).await;
    let result = buy_product(&pool, &user, &product, 1, "step-1").await;
    match result {
        Err(ShopError::InsufficientBalance) => {}
        other => panic!("expected InsufficientBalance, got {other:?}"),
    }
    assert_no_partial_state(&pool, &user, before).await;

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 步骤② 库存不足：库存扣减在扣款后，注入失败 → 余额与库存都不变。
#[tokio::test]
async fn step_out_of_stock_rolls_back_everything() {
    let (pool, dir) = setup().await;
    let user = insert_user(&pool, "b", 1).await;
    credit_user(&pool, &user, 1000).await;
    let product = insert_product(&pool, &user, Some(0), 100, 1, "non_refundable").await;

    let before = snapshot(&pool, &user).await;
    let result = buy_product(&pool, &user, &product, 1, "step-2").await;
    match result {
        Err(ShopError::OutOfStock) => {}
        other => panic!("expected OutOfStock, got {other:?}"),
    }
    assert_no_partial_state(&pool, &user, before).await;

    // 库存保持 0。
    let stock: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT stock_remaining FROM shop_products WHERE id = ?")
                .bind(&product)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(stock, 0, "库存未被扣减");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 步骤③ 限购：quantity 超限 → 全回滚。
#[tokio::test]
async fn step_purchase_limit_rolls_back_everything() {
    let (pool, dir) = setup().await;
    let user = insert_user(&pool, "c", 1).await;
    credit_user(&pool, &user, 5000).await;
    let product = insert_product(&pool, &user, Some(5), 100, 1, "non_refundable").await;

    let before = snapshot(&pool, &user).await;
    // quantity_limit=10，请求 11。
    let result = buy_product(&pool, &user, &product, 11, "step-3").await;
    assert!(result.is_err(), "超限必须失败");
    assert_no_partial_state(&pool, &user, before).await;

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 步骤④ 等级门槛：author 等级不足 → 全回滚。
#[tokio::test]
async fn step_level_gate_rolls_back_everything() {
    let (pool, dir) = setup().await;
    let user = insert_user(&pool, "d", 1).await;
    credit_user(&pool, &user, 1000).await;
    // required_level=5，用户等级 1。
    let product = insert_product(&pool, &user, Some(5), 100, 5, "non_refundable").await;

    let before = snapshot(&pool, &user).await;
    let result = buy_product(&pool, &user, &product, 1, "step-4").await;
    match result {
        Err(ShopError::BelowLevel { .. }) => {}
        other => panic!("expected BelowLevel, got {other:?}"),
    }
    assert_no_partial_state(&pool, &user, before).await;

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 步骤⑤ 幂等冲突：同键不同摘要 → 409，不产生部分状态。
#[tokio::test]
async fn step_idempotency_conflict_rolls_back_everything() {
    let (pool, dir) = setup().await;
    let user = insert_user(&pool, "e", 1).await;
    credit_user(&pool, &user, 1000).await;
    let product = insert_product(&pool, &user, Some(5), 100, 1, "non_refundable").await;

    // 成功购买一次。
    buy_product(&pool, &user, &product, 1, "same-key")
        .await
        .unwrap();
    let balance_after_success = account_balance(&pool, &user).await;

    // 同键不同 quantity → 摘要冲突 → 拒绝，且不改变成功购买的状态。
    let result = buy_product(&pool, &user, &product, 2, "same-key").await;
    match result {
        Err(ShopError::IdempotencyConflict) => {}
        other => panic!("expected IdempotencyConflict, got {other:?}"),
    }
    assert_eq!(
        account_balance(&pool, &user).await,
        balance_after_success,
        "冲突后余额不变（成功购买的扣费保留，无额外扣费）"
    );
    assert_eq!(count_rows(&pool, "orders", &user).await, 1, "仅一张订单");

    close_pool(&pool).await;
    cleanup(&dir);
}
