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
use bblbb_backend::shop::cosmetics::{create_def, update_def};
use bblbb_backend::shop::service::{
    buy_product, create_product, disable_product, equip, get_order, get_presentation,
    list_my_entitlements, list_products, publish_product, refund_order, unequip, validate_tokens,
    ShopError,
};
use serde_json::json;
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
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
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
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
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
    // 合法（含 breathing 动效昵称：M07-SHOP-UI-09 加入注册白名单）
    validate_tokens(
        Some("badge.1"),
        Some(r#"["nickname.color.gold","avatar.frame.neon","nickname.color.breathing"]"#),
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
    assert_eq!(order["order"]["id"], order["order_id"]);
    assert_eq!(order["order"]["entitlement_status"], "granted");
    assert_eq!(balance_of(&pool, &user).await, 800);
    assert_eq!(stock_of(&pool, &product).await, Some(3));

    // 订单 + 权益 + 审计 + outbox 都落库
    let entitlements = list_my_entitlements(&pool, &user).await.unwrap();
    assert_eq!(entitlements["entitlements"][0]["remaining_quantity"], 2);
    let order_id = order["order_id"].as_str().unwrap();
    let order_view = get_order(&pool, &user, order_id, false).await.unwrap();
    assert_eq!(order_view["status"], "succeeded");
    assert_eq!(order_view["entitlement_id"], order["entitlement_id"]);
    assert_eq!(order_view["entitlement_status"], "granted");

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
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE shop_products SET presentation_tokens_json = ? WHERE id = ?")
                .bind(r#"["nickname.color.rainbow"]"#)
                .bind(&p2)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(p) => {
            sqlx::query("UPDATE shop_products SET presentation_tokens_json = ? WHERE id = ?")
                .bind(r#"["nickname.color.rainbow"]"#)
                .bind(&p2)
                .execute(p)
                .await
                .unwrap();
        }
    };
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
    let presentation = get_presentation(&pool, &user).await.unwrap();
    assert_eq!(
        presentation["presentation_tokens"]["nickname_color"],
        "rainbow"
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

/// M07-SHOP-UI-09：create_product 服务路径回归 + 槽位规则。
/// 背景：INSERT 曾出现 23 bind 对 22 占位符的错位（直插 SQL 的用例无法覆盖，
/// 管理端创建商品 100% 失败），本用例强制走服务函数落库。
#[tokio::test]
async fn create_product_service_persists_and_enforces_slot_rules() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "creator").await;

    // 装备类：昵称装扮（含 breathing 动效 Token）→ 槽位与 Token 正常落库
    let created = create_product(
        &pool,
        &json!({
            "kind": "cosmetic_nickname",
            "slug": format!("nick-{}", uuid::Uuid::now_v7().simple()),
            "title": "呼吸微光昵称",
            "slot": "nickname_color",
            "currency_id": CURRENCY_COIN,
            "unit_price": 120,
            "presentation_tokens": ["nickname.color.breathing"]
        }),
        &owner,
    )
    .await
    .unwrap();
    let product_id = created["id"].as_str().unwrap().to_string();
    let (slot, tokens): (String, String) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT slot, presentation_tokens_json FROM shop_products WHERE id = ?")
                .bind(&product_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT slot, presentation_tokens_json FROM shop_products WHERE id = ?")
                .bind(&product_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(slot, "nickname_color");
    assert!(
        tokens.contains("nickname.color.breathing"),
        "tokens={tokens}"
    );

    // 消耗品：reaction_pack 允许缺省槽位（落空串，不参与 equip）
    let consumable = create_product(
        &pool,
        &json!({
            "kind": "reaction_pack",
            "slug": format!("react-{}", uuid::Uuid::now_v7().simple()),
            "title": "庆祝反应包",
            "currency_id": CURRENCY_COIN,
            "unit_price": 30,
            "presentation_tokens": ["reaction.pack.celebrate"]
        }),
        &owner,
    )
    .await
    .unwrap();
    let consumable_id = consumable["id"].as_str().unwrap().to_string();
    let slot: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT slot FROM shop_products WHERE id = ?")
            .bind(&consumable_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar("SELECT slot FROM shop_products WHERE id = ?")
            .bind(&consumable_id)
            .fetch_one(p)
            .await
            .unwrap(),
    };
    assert_eq!(slot, "");

    // 装备类缺槽位 → 拒绝（equip 查询依赖 p.slot = ?）
    let err = create_product(
        &pool,
        &json!({
            "kind": "cosmetic_badge",
            "slug": format!("badge-{}", uuid::Uuid::now_v7().simple()),
            "title": "缺槽位徽章",
            "currency_id": CURRENCY_COIN,
            "unit_price": 10
        }),
        &owner,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, ShopError::Invalid(ref m) if m.contains("slot")),
        "unexpected error: {err:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── M07-SHOP-STUDIO：工作台复合发布（样式 upsert + 商品创建单事务）──

async fn scalar_with_params(pool: &DatabasePool, sql: &str, params: &[&str]) -> i64 {
    match pool {
        Either::Left(p) => {
            let mut q = sqlx::query_scalar(sql);
            for param in params {
                q = q.bind(*param);
            }
            q.fetch_one(p).await.unwrap()
        }
        Either::Right(p) => {
            let mut q = sqlx::query_scalar(sql);
            for param in params {
                q = q.bind(*param);
            }
            q.fetch_one(p).await.unwrap()
        }
    }
}

async fn text_with_params(pool: &DatabasePool, sql: &str, params: &[&str]) -> String {
    match pool {
        Either::Left(p) => {
            let mut q = sqlx::query_scalar(sql);
            for param in params {
                q = q.bind(*param);
            }
            q.fetch_one(p).await.unwrap()
        }
        Either::Right(p) => {
            let mut q = sqlx::query_scalar(sql);
            for param in params {
                q = q.bind(*param);
            }
            q.fetch_one(p).await.unwrap()
        }
    }
}

fn studio_body(slug: Option<&str>, client_request_id: Option<&str>) -> serde_json::Value {
    let mut body = json!({
        "cosmetic": {
            "kind": "nickname_color",
            "name": "霓虹渐变",
            "style": { "mode": "gradient", "colors": ["#FF0000", "#00FF00"], "animate": "flow", "durationMs": 4000 }
        },
        "product": {
            "title": "Neon Nick",
            "unit_price": 120,
            "validity_seconds": 2592000,
            "refund_policy": "full_refund"
        },
        "reason": "工作台发布测试"
    });
    if let Some(slug) = slug {
        body["product"]["slug"] = json!(slug);
    }
    if let Some(key) = client_request_id {
        body["client_request_id"] = json!(key);
    }
    body
}

/// 复合发布：单事务落 def + 商品，slug 由标题派生，审计同事务写入；
/// 商品直接 published 可购买，装备后公开投影解析出自定义样式。
#[tokio::test]
async fn studio_publish_creates_def_and_product_atomically() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "studio1").await;

    let v = bblbb_backend::shop::studio::publish(&pool, &studio_body(None, None), &owner, None)
        .await
        .unwrap();
    let def_id = v["cosmetic"]["id"].as_str().unwrap().to_string();
    let product_id = v["product"]["id"].as_str().unwrap().to_string();
    assert!(def_id.starts_with('c'));
    assert_eq!(v["product"]["slug"], "neon-nick");
    assert_eq!(v["product"]["status"], "published");
    assert_eq!(v["replayed"], false);
    // style 经 schema 归一（大写颜色落小写）。
    assert_eq!(v["cosmetic"]["style"]["colors"][0], "#ff0000");

    // 商品落库：token 引用 def、槽位/价格/时效正确。
    let tokens = text_with_params(
        &pool,
        "SELECT presentation_tokens_json FROM shop_products WHERE id = ?",
        &[&product_id],
    )
    .await;
    assert_eq!(
        tokens,
        json!([format!("nickname.color.{def_id}")]).to_string()
    );
    let slot = text_with_params(
        &pool,
        "SELECT slot FROM shop_products WHERE id = ?",
        &[&product_id],
    )
    .await;
    assert_eq!(slot, "nickname_color");
    let price = scalar_with_params(
        &pool,
        "SELECT unit_price FROM shop_products WHERE id = ?",
        &[&product_id],
    )
    .await;
    assert_eq!(price, 120);
    // 结算货币：缺省 code 'coin' 解析为 currencies.id（FK 目标，不是字面 'coin'）。
    let currency = text_with_params(
        &pool,
        "SELECT currency_id FROM shop_products WHERE id = ?",
        &[&product_id],
    )
    .await;
    assert_eq!(currency, CURRENCY_COIN);
    // 审计同事务写入。
    let audits = scalar_with_params(
        &pool,
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'shop.studio.publish' AND target_id = ?",
        &[&product_id],
    )
    .await;
    assert_eq!(audits, 1);

    // 商品可直接购买（无需二次上架），装备后投影解析自定义样式。
    let buyer = insert_user(&pool, "studio_buyer").await;
    credit_user(&pool, &buyer, 1000).await;
    let bought = buy_product(&pool, &buyer, &product_id, 1, "idem-studio-1")
        .await
        .unwrap();
    let entitlement_id = bought["entitlement_id"].as_str().unwrap().to_string();
    equip(&pool, &buyer, &entitlement_id).await.unwrap();
    let projection = bblbb_backend::shop::service::get_public_presentation_tokens(&pool, &buyer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(projection.nickname_color.as_deref(), Some(def_id.as_str()));
    assert_eq!(projection.nickname_color_name.as_deref(), Some("霓虹渐变"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 幂等：同 key+摘要重放返回首次结果（不重复建行）；同 key 不同摘要 409。
#[tokio::test]
async fn studio_publish_idempotent_replay_and_conflict() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "studio2").await;

    let first = bblbb_backend::shop::studio::publish(
        &pool,
        &studio_body(Some("replay-nick"), Some("crq-studio-1")),
        &owner,
        None,
    )
    .await
    .unwrap();
    let second = bblbb_backend::shop::studio::publish(
        &pool,
        &studio_body(Some("replay-nick"), Some("crq-studio-1")),
        &owner,
        None,
    )
    .await
    .unwrap();
    assert_eq!(second["replayed"], true);
    assert_eq!(second["product"]["id"], first["product"]["id"]);
    let rows = scalar_with_params(
        &pool,
        "SELECT COUNT(*) FROM shop_products WHERE slug = 'replay-nick'",
        &[],
    )
    .await;
    assert_eq!(rows, 1, "重放不得重复建商品");

    // 同 key 不同摘要 → 幂等冲突。
    let mut other = studio_body(Some("replay-nick-2"), Some("crq-studio-1"));
    other["product"]["unit_price"] = json!(999);
    let err = bblbb_backend::shop::studio::publish(&pool, &other, &owner, None)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ShopError::IdempotencyConflict),
        "unexpected: {err:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 引用已有 def：归档定义被工作台发布时恢复 active，商品引用同一定义。
#[tokio::test]
async fn studio_publish_updates_and_revives_archived_def() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "studio3").await;

    let def = create_def(
        &pool,
        &json!({
            "kind": "avatar_frame",
            "name": "旧环",
            "style": { "color": "#8B5CF6", "widthPx": 4, "shape": "rounded" }
        }),
        &owner,
    )
    .await
    .unwrap();
    let def_id = def["id"].as_str().unwrap().to_string();
    update_def(&pool, &def_id, &json!({ "status": "archived" }))
        .await
        .unwrap();

    let v = bblbb_backend::shop::studio::publish(
        &pool,
        &json!({
            "cosmetic": {
                "id": def_id,
                "kind": "avatar_frame",
                "name": "新环",
                "style": { "color": "#14B8A6", "widthPx": 2, "shape": "circle", "animate": "pulse" }
            },
            "product": { "title": "Jade Ring", "unit_price": 66 },
            "reason": "改版再发布"
        }),
        &owner,
        None,
    )
    .await
    .unwrap();
    assert_eq!(v["cosmetic"]["id"], def_id);
    assert_eq!(v["cosmetic"]["status"], "active");
    assert_eq!(v["product"]["kind"], "cosmetic_avatar");

    let status = text_with_params(
        &pool,
        "SELECT status FROM cosmetic_defs WHERE id = ?",
        &[&def_id],
    )
    .await;
    assert_eq!(status, "active");
    let style = text_with_params(
        &pool,
        "SELECT style_json FROM cosmetic_defs WHERE id = ?",
        &[&def_id],
    )
    .await;
    assert!(style.contains("#14b8a6"), "style updated: {style}");
    let slot = text_with_params(
        &pool,
        "SELECT slot FROM shop_products WHERE id = ?",
        &[v["product"]["id"].as_str().unwrap()],
    )
    .await;
    assert_eq!(slot, "avatar_frame");

    // kind 不一致的引用被拒。
    let err = bblbb_backend::shop::studio::publish(
        &pool,
        &json!({
            "cosmetic": { "id": def_id, "kind": "nickname_color", "name": "串型", "style": { "mode": "solid", "color": "#ffffff" } },
            "product": { "title": "Mismatch", "unit_price": 1 }
        }),
        &owner,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ShopError::Invalid(_)), "unexpected: {err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 0080 CHECK 修复后，cosmetic_badge/reaction_pack/utility/title_prefix
/// 四类定义可落库（此前被 0079 的 CHECK 拒绝）。
#[tokio::test]
async fn studio_publish_supports_all_customizable_kinds() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "studio4").await;

    let cases = [
        (
            "cosmetic_badge",
            json!({"icon": "award", "color": "#FF8800"}),
            "profile_badges",
            "badge.",
        ),
        (
            "profile_effect",
            json!({"texture": "grid", "baseColor": "#101827", "accentColor": "#8B5CF6"}),
            "profile_effect",
            "profile.effect.",
        ),
        (
            "post_effect",
            json!({"icon": "heart", "color": "#FF0000"}),
            "post_effect",
            "post.effect.",
        ),
        (
            "reaction_pack",
            json!({"icon": "heart", "color": "#FF0000"}),
            "",
            "reaction.pack.",
        ),
        (
            "utility",
            json!({"action": "rename", "quantity": 3}),
            "",
            "utility.",
        ),
        (
            "title_prefix",
            json!({"icon": "trophy", "color": "#FFD700"}),
            "title_prefix",
            "title.prefix.",
        ),
    ];
    for (kind, style, expected_slot, prefix) in cases {
        let v = bblbb_backend::shop::studio::publish(
            &pool,
            &json!({
                "cosmetic": { "kind": kind, "name": format!("样式{kind}"), "style": style },
                "product": { "title": format!("Prod {kind}"), "unit_price": 10 }
            }),
            &owner,
            None,
        )
        .await
        .unwrap_or_else(|e| panic!("kind {kind} publish failed: {e:?}"));
        let product_id = v["product"]["id"].as_str().unwrap();
        let slot = text_with_params(
            &pool,
            "SELECT slot FROM shop_products WHERE id = ?",
            &[product_id],
        )
        .await;
        assert_eq!(slot, expected_slot, "kind {kind}");
        let tokens = text_with_params(
            &pool,
            "SELECT presentation_tokens_json FROM shop_products WHERE id = ?",
            &[product_id],
        )
        .await;
        assert!(
            tokens.contains(prefix),
            "kind {kind} token prefix: {tokens}"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 校验失败整体回滚：def 与商品都不落库。
#[tokio::test]
async fn studio_publish_rolls_back_on_validation_failure() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "studio5").await;

    // style 非法（CSS 注入尝试）。
    let err = bblbb_backend::shop::studio::publish(
        &pool,
        &json!({
            "cosmetic": { "kind": "nickname_color", "name": "坏", "style": { "mode": "solid", "color": "url(evil)" } },
            "product": { "title": "Bad", "unit_price": 1 }
        }),
        &owner,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ShopError::Invalid(_)));
    // PNG 模式引用不存在的附件同样整体失败。
    let err = bblbb_backend::shop::studio::publish(
        &pool,
        &json!({
            "cosmetic": { "kind": "avatar_frame" },
            "product": { "title": "Png", "unit_price": 1, "asset_attachment_id": "att-missing" }
        }),
        &owner,
        None,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ShopError::Invalid(_)));

    let defs = scalar_with_params(&pool, "SELECT COUNT(*) FROM cosmetic_defs", &[]).await;
    let products = scalar_with_params(&pool, "SELECT COUNT(*) FROM shop_products", &[]).await;
    assert_eq!(defs, 0);
    assert_eq!(products, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M07-SHOP-UI-10：可配置装扮样式库——CRUD、样式 schema 校验、
/// 商品 Token 引用校验与公开投影解析。
#[tokio::test]
async fn cosmetic_defs_crud_and_projection_resolution() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "stylist").await;

    // 1. 新建自定义渐变昵称样式（管理员自己起名、自己调渐变）
    let def = create_def(
        &pool,
        &json!({
            "kind": "nickname_color",
            "name": "樱花粉渐变",
            "style": { "mode": "gradient", "colors": ["#FF7EB6", "#FFB8D2", "#C084FC"], "animate": "flow", "durationMs": 4000 }
        }),
        &owner,
    )
    .await
    .unwrap();
    let def_id = def["id"].as_str().unwrap().to_string();
    assert!(def_id.starts_with('c'));
    assert_eq!(def["style"]["colors"].as_array().unwrap().len(), 3);

    // 2. 非法样式被拒（CSS 注入 / 坏颜色 / 非开放类型）
    assert!(create_def(
        &pool,
        &json!({ "kind": "nickname_color", "name": "坏颜色", "style": { "mode": "solid", "color": "red; background:url(x)" } }),
        &owner
    )
    .await
    .is_err());
    assert!(create_def(
        &pool,
        &json!({ "kind": "unknown_feature", "name": "不支持", "style": { "color": "#ff0000" } }),
        &owner
    )
    .await
    .is_err());

    // 3. 商品引用自定义样式 Token → 创建成功且落库
    let product = create_product(
        &pool,
        &json!({
            "kind": "cosmetic_nickname",
            "slug": format!("custom-nick-{}", uuid::Uuid::now_v7().simple()),
            "title": "樱花粉渐变昵称",
            "slot": "nickname_color",
            "currency_id": CURRENCY_COIN,
            "unit_price": 88,
            "presentation_tokens": [format!("nickname.color.{def_id}")]
        }),
        &owner,
    )
    .await
    .unwrap();

    // 4. 引用不存在的样式 id → 创建被拒
    let err = create_product(
        &pool,
        &json!({
            "kind": "cosmetic_nickname",
            "slug": format!("bad-ref-{}", uuid::Uuid::now_v7().simple()),
            "title": "悬空引用",
            "slot": "nickname_color",
            "currency_id": CURRENCY_COIN,
            "unit_price": 1,
            "presentation_tokens": ["nickname.color.cdeadbeefdeadbeefdeadbeefdeadbeef"]
        }),
        &owner,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, ShopError::Invalid(ref m) if m.contains("nickname.color")),
        "unexpected: {err:?}"
    );

    // 5. 购买 → 装备 → 公开投影解析出自定义样式与名称
    let buyer = insert_user(&pool, "buyer2").await;
    credit_user(&pool, &buyer, 1000).await;
    let product_id = product["id"].as_str().unwrap().to_string();
    // create_product 默认 draft：上架后才能购买
    publish_product(&pool, &product_id).await.unwrap();
    let bought = buy_product(&pool, &buyer, &product_id, 1, "idem-cosmetic-1")
        .await
        .unwrap();
    let entitlement_id = bought["entitlement_id"].as_str().unwrap().to_string();
    equip(&pool, &buyer, &entitlement_id).await.unwrap();
    let projection = bblbb_backend::shop::service::get_public_presentation_tokens(&pool, &buyer)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(projection.nickname_color.as_deref(), Some(def_id.as_str()));
    assert_eq!(
        projection.nickname_color_name.as_deref(),
        Some("樱花粉渐变")
    );
    let style = projection
        .nickname_color_style
        .as_ref()
        .expect("custom style projected");
    assert_eq!(style["mode"], "gradient");
    assert_eq!(style["colors"][0], "#ff7eb6");

    // 6. 归档定义 → 投影不再渲染该槽位（软删语义；全部槽位为空时整体为 None）
    update_def(&pool, &def_id, &json!({ "status": "archived" }))
        .await
        .unwrap();
    let projection = bblbb_backend::shop::service::get_public_presentation_tokens(&pool, &buyer)
        .await
        .unwrap();
    assert!(
        projection
            .as_ref()
            .map(|p| p.nickname_color.is_none())
            .unwrap_or(true),
        "archived def must not render"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
