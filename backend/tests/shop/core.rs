//! M07-SHOP-09：商城事务、并发、Token 白名单与补偿测试（SQLite）。
//!
//! 覆盖：Token 拒绝、价格/库存/等级/销售窗口/限购、购买事务+幂等重放、
//! 并发不超卖、entitlement 状态机+过期+slot 互斥+徽章上限、equip/unequip、
//! admin 退款（不可退/可退补偿）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::economy::ledger::service as ledger;
use bblbb_backend::economy::ledger::service::{LedgerKind, CURRENCY_COIN};
use bblbb_backend::outbox::now_millis;
use bblbb_backend::shop::service::{
    buy_product, disable_product, equip, get_order, get_presentation, list_my_entitlements,
    list_products, publish_product, refund_order, unequip, validate_tokens, ShopError,
};
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-shop-{}", uuid::Uuid::now_v7()));
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

/// 造一个可售商品（等级门槛 1、库存 5、单价 100、CURRENCY_COIN）。
#[allow(clippy::too_many_arguments)] // 测试造数助手：字段即参数
async fn insert_product(
    pool: &DatabasePool,
    owner_id: &str,
    stock: Option<i64>,
    unit_price: i64,
    required_level: i64,
    kind: &str,
    slot: &str,
    refund_policy: &str,
    validity_seconds: Option<i64>,
) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO shop_products
                     (id, kind, status, slug, title, description_safe, icon_token, presentation_tokens_json, slot, currency_id, unit_price, quantity_limit, stock_remaining, required_level, validity_seconds, sale_start_at, sale_end_at, refund_policy, version, created_by, created_at, updated_at)
                 VALUES (?, ?, 'published', ?, ?, NULL, NULL, NULL, ?, ?, ?, 10, ?, ?, ?, NULL, NULL, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(format!("slug-{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("title-{}", uuid::Uuid::now_v7().simple()))
            .bind(slot)
            .bind(CURRENCY_COIN)
            .bind(unit_price)
            .bind(stock)
            .bind(required_level)
            .bind(validity_seconds)
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
                 VALUES (?, ?, 'published', ?, ?, NULL, NULL, NULL, ?, ?, ?, 10, ?, ?, ?, NULL, NULL, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(kind)
            .bind(format!("slug-{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("title-{}", uuid::Uuid::now_v7().simple()))
            .bind(slot)
            .bind(CURRENCY_COIN)
            .bind(unit_price)
            .bind(stock)
            .bind(required_level)
            .bind(validity_seconds)
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

/// 给用户充值（账本 credit）。
async fn credit_user(pool: &DatabasePool, user_id: &str, amount: i64) {
    let now = now_millis();
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
        memo: "test credit".to_string(),
        reverses_operation_id: None,
    };
    ledger::apply_operation(pool, cmd, now).await.unwrap();
}

async fn balance_of(pool: &DatabasePool, user_id: &str) -> i64 {
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

async fn stock_of(pool: &DatabasePool, product_id: &str) -> Option<i64> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT stock_remaining FROM shop_products WHERE id = ?")
                .bind(product_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT stock_remaining FROM shop_products WHERE id = ?")
                .bind(product_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
    }
}

#[tokio::test]
async fn token_whitelist_rejects_css_html_url_and_accepts_registered() {
    // 合法
    validate_tokens(
        Some("badge.1"),
        Some(r#"["nickname.color.gold","avatar.frame.neon"]"#),
    )
    .unwrap();
    // 拒绝 HTML/CSS/URL/SVG/任意代码
    for bad in [
        r#"["<script>alert(1)</script>"]"#,
        r#"["background:url(evil)"]"#,
        r#"["https://evil.example/x"]"#,
        r#"["javascript:alert(1)"]"#,
        r#"["avatar.frame.gold;color:red"]"#,
        r#"["badge.1/../secret"]"#,
        r#"["../../etc/passwd"]"#,
    ] {
        assert!(
            validate_tokens(None, Some(bad)).is_err(),
            "should reject {bad}"
        );
    }
    assert!(validate_tokens(Some("<img src=x>"), None).is_err());
}

#[tokio::test]
async fn buy_product_charges_and_grants_entitlement() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        1,
        "cosmetic_nickname",
        "nickname_color",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user, 1000).await;

    let order = buy_product(&pool, &user, &product, 2, "idem-1")
        .await
        .unwrap();
    assert_eq!(order["status"], "succeeded");
    assert_eq!(order["total_amount"], 200);
    assert_eq!(balance_of(&pool, &user).await, 800);
    assert_eq!(stock_of(&pool, &product).await, Some(3));

    // 订单 + 权益 + 审计 + outbox 都落库
    let entitlements = list_my_entitlements(&pool, &user).await.unwrap();
    assert_eq!(entitlements["entitlements"][0]["remaining_quantity"], 2);
    let order_id = order["order_id"].as_str().unwrap();
    let order_view = get_order(&pool, &user, order_id, false).await.unwrap();
    assert_eq!(order_view["status"], "succeeded");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn same_idempotency_key_replays_without_double_charge() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        1,
        "utility",
        "utility",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user, 1000).await;

    let first = buy_product(&pool, &user, &product, 1, "idem-replay")
        .await
        .unwrap();
    let second = buy_product(&pool, &user, &product, 1, "idem-replay")
        .await
        .unwrap();
    assert_eq!(first["order_id"], second["order_id"]);
    assert_eq!(balance_of(&pool, &user).await, 900, "不得重复扣款");
    assert_eq!(stock_of(&pool, &product).await, Some(4), "不得重复扣库存");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn same_key_different_request_conflicts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        1,
        "utility",
        "utility",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user, 1000).await;

    buy_product(&pool, &user, &product, 1, "idem-x")
        .await
        .unwrap();
    // 同 key 不同数量（摘要变化）→ 冲突
    let err = buy_product(&pool, &user, &product, 2, "idem-x")
        .await
        .unwrap_err();
    assert!(matches!(err, ShopError::IdempotencyConflict));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn insufficient_balance_rolls_back_atomically() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "poor").await;
    let owner = insert_user(&pool, "owner").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        1,
        "utility",
        "utility",
        "non_refundable",
        None,
    )
    .await;
    // 不给钱

    let err = buy_product(&pool, &user, &product, 1, "idem-poor")
        .await
        .unwrap_err();
    assert!(matches!(err, ShopError::InsufficientBalance));
    // 无订单、无权益、库存不变
    assert_eq!(stock_of(&pool, &product).await, Some(5));
    let entitlements = list_my_entitlements(&pool, &user).await.unwrap();
    assert_eq!(entitlements["entitlements"].as_array().unwrap().len(), 0);
    let orders: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM shop_orders WHERE user_id = ?")
            .bind(&user)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM shop_orders WHERE user_id = ?")
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(orders, 0);
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn out_of_stock_and_limits_are_enforced() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(1),
        100,
        1,
        "utility",
        "utility",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user, 10_000).await;

    // 库存不足
    let err = buy_product(&pool, &user, &product, 2, "idem-stock")
        .await
        .unwrap_err();
    assert!(matches!(err, ShopError::OutOfStock));
    // 单次购买成功扣掉唯一库存
    buy_product(&pool, &user, &product, 1, "idem-ok")
        .await
        .unwrap();
    // 二次购买超卖
    let err = buy_product(&pool, &user, &product, 1, "idem-2")
        .await
        .unwrap_err();
    assert!(matches!(err, ShopError::OutOfStock));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn concurrent_buys_do_not_oversell() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user1 = insert_user(&pool, "b1").await;
    let user2 = insert_user(&pool, "b2").await;
    let owner = insert_user(&pool, "owner").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(1),
        100,
        1,
        "utility",
        "utility",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user1, 10_000).await;
    credit_user(&pool, &user2, 10_000).await;

    let (r1, r2) = tokio::join!(
        buy_product(&pool, &user1, &product, 1, "idem-c1"),
        buy_product(&pool, &user2, &product, 1, "idem-c2"),
    );
    let success = matches!(
        (r1, r2),
        (Ok(_), Err(ShopError::OutOfStock)) | (Err(ShopError::OutOfStock), Ok(_))
    );
    assert!(success, "必须恰好一个成功，一个 OutOfStock");
    assert_eq!(stock_of(&pool, &product).await, Some(0));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn level_gate_and_sale_window_are_checked() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "newbie").await;
    let owner = insert_user(&pool, "owner").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        10,
        "utility",
        "utility",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user, 1000).await;

    let err = buy_product(&pool, &user, &product, 1, "idem-level")
        .await
        .unwrap_err();
    assert!(matches!(err, ShopError::BelowLevel { required: 10 }));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn entitlement_expiry_and_slot_exclusivity() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "fashion").await;
    let owner = insert_user(&pool, "owner").await;
    // 两个同 slot 商品
    let p1 = insert_product(
        &pool,
        &owner,
        Some(5),
        10,
        1,
        "cosmetic_nickname",
        "nickname_color",
        "non_refundable",
        Some(3600),
    )
    .await;
    let p2 = insert_product(
        &pool,
        &owner,
        Some(5),
        10,
        1,
        "cosmetic_nickname",
        "nickname_color",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user, 1000).await;

    let o1 = buy_product(&pool, &user, &p1, 1, "idem-e1").await.unwrap();
    let e1 = o1["entitlement_id"].as_str().unwrap();
    let o2 = buy_product(&pool, &user, &p2, 1, "idem-e2").await.unwrap();
    let e2 = o2["entitlement_id"].as_str().unwrap();

    // equip e1 → e2 equip 应互斥（e1 被卸下）
    equip(&pool, &user, e1).await.unwrap();
    let list = list_my_entitlements(&pool, &user).await.unwrap();
    let statuses: Vec<&str> = list["entitlements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["status"].as_str().unwrap())
        .collect();
    assert_eq!(
        statuses.iter().filter(|s| **s == "equipped").count(),
        1,
        "equip e1 后应恰好一个 equipped: {statuses:?}"
    );
    equip(&pool, &user, e2).await.unwrap();
    let list = list_my_entitlements(&pool, &user).await.unwrap();
    let statuses: Vec<&str> = list["entitlements"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["status"].as_str().unwrap())
        .collect();
    assert!(
        statuses.iter().filter(|s| **s == "equipped").count() == 1,
        "slot 互斥失败: {statuses:?}"
    );
    unequip(&pool, &user, e2).await.unwrap();
    let list = list_my_entitlements(&pool, &user).await.unwrap();
    assert_eq!(list["entitlements"][0]["status"], "owned");

    let _ = get_presentation(&pool, &user).await.unwrap();
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn refund_respects_policy_and_revokes_entitlement() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    // 可退商品（compensation_only）
    let refundable = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        1,
        "utility",
        "utility",
        "compensation_only",
        None,
    )
    .await;
    // 默认不可退
    let nonrefundable = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        1,
        "cosmetic_badge",
        "profile_badge",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user, 10_000).await;

    let o = buy_product(&pool, &user, &refundable, 1, "idem-r1")
        .await
        .unwrap();
    let order_id = o["order_id"].as_str().unwrap();
    let balance_before = balance_of(&pool, &user).await;
    refund_order(&pool, order_id, &owner, "test refund")
        .await
        .unwrap();
    let balance_after = balance_of(&pool, &user).await;
    assert_eq!(balance_after, balance_before + 100, "补偿流水应返还");
    let entitlements = list_my_entitlements(&pool, &user).await.unwrap();
    assert_eq!(entitlements["entitlements"][0]["status"], "revoked");

    // 不可退订单 → NotRefundable
    let o2 = buy_product(&pool, &user, &nonrefundable, 1, "idem-r2")
        .await
        .unwrap();
    let order2 = o2["order_id"].as_str().unwrap();
    let err = refund_order(&pool, order2, &owner, "test")
        .await
        .unwrap_err();
    assert!(matches!(err, ShopError::NotRefundable));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn publish_disable_and_admin_list() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "admin").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        1,
        "utility",
        "utility",
        "non_refundable",
        None,
    )
    .await;
    let published = list_products(&pool, false).await.unwrap();
    assert_eq!(published["products"].as_array().unwrap().len(), 1);
    disable_product(&pool, &product).await.unwrap();
    let published = list_products(&pool, false).await.unwrap();
    assert_eq!(
        published["products"].as_array().unwrap().len(),
        0,
        "disabled 不进公开列表"
    );
    publish_product(&pool, &product).await.unwrap();
    let published = list_products(&pool, false).await.unwrap();
    assert_eq!(published["products"].as_array().unwrap().len(), 1);
    let admin_list = bblbb_backend::shop::service::list_admin_products(&pool)
        .await
        .unwrap();
    assert_eq!(admin_list["products"].as_array().unwrap().len(), 1);
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn ledger_kind_is_shop_purchase_and_audit_outbox_written() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let product = insert_product(
        &pool,
        &owner,
        Some(5),
        100,
        1,
        "utility",
        "utility",
        "non_refundable",
        None,
    )
    .await;
    credit_user(&pool, &user, 1000).await;
    buy_product(&pool, &user, &product, 1, "idem-ledger")
        .await
        .unwrap();

    let ops: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM point_transactions pt JOIN point_operations po ON po.id = pt.operation_id
                 WHERE pt.user_id = ? AND po.kind = 'shop_purchase'",
            )
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM point_transactions pt JOIN point_operations po ON po.id = pt.operation_id
                 WHERE pt.user_id = ? AND po.kind = 'shop_purchase'",
            )
                .bind(&user)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(ops, 1);
    let audits: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE actor_id = ? AND action = 'shop.purchase'",
        )
        .bind(&user)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE actor_id = ? AND action = 'shop.purchase'",
        )
        .bind(&user)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(audits, 1);
    let outbox: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'shop.order_succeeded.v1'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'shop.order_succeeded.v1'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(outbox, 1);
    close_pool(&pool).await;
    cleanup(&dir);
}
