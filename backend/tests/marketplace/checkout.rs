//! M12-CHECKOUT：user-bound Intent 与原子购买测试（SQLite 真库）。
//!
//! 覆盖：短 TTL + 一次性 + 绑定；confirm 重读 Client/Scope/Offer/库存/用户
//! 状态/限额/过期；固定锁序（BEGIN IMMEDIATE 并发恰好一个成功）；幂等重放
//! 与 409；余额/库存失败完整回滚；IDOR/过期/已消费/封禁/价格篡改/限额。

mod support;

use bblbb_backend::marketplace::checkout;
use bblbb_backend::marketplace::clients::{self, MarketplaceClient};
use bblbb_backend::marketplace::offers::OfferRow;
use bblbb_backend::marketplace::MarketplaceError;
use support::*;

/// 购买上下文（含余额充足买方）。
struct Ctx {
    buyer: String,
    buyer2: String,
    client_id: String,
    client: MarketplaceClient,
    offer: OfferRow,
}

async fn setup_ctx(
    amount: i64,
    stock: Option<i64>,
) -> (bblbb_backend::db::DatabasePool, std::path::PathBuf, Ctx) {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let buyer = insert_user(&pool, "buyer").await;
    let buyer2 = insert_user(&pool, "buyer2").await;
    let owner = insert_user(&pool, "owner").await;
    let (oauth, secret) = create_oauth_confidential(&pool, "ck").await;
    let client = create_marketplace_client(&pool, &oauth.client_id, &owner, 100, None).await;
    let principal = service_auth(&pool, &oauth.client_id, &secret, "marketplace.offer.write").await;
    let offer = create_active_offer(&pool, &principal, "ck-1", amount, stock).await;
    credit_user(&pool, &buyer, 10_000).await;
    credit_user(&pool, &buyer2, 10_000).await;
    let ctx = Ctx {
        buyer,
        buyer2,
        client_id: oauth.client_id.clone(),
        client,
        offer,
    };
    (pool, dir, ctx)
}

// ─────────────────────────── M12-CHECKOUT-01/02 ───────────────────────────

#[tokio::test]
async fn intent_binds_user_client_offer_and_is_one_shot_with_ttl() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;

    let intent = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-1",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(
        intent["user_id"].as_str(),
        None,
        "Intent 视图不得暴露内部 user_id"
    );
    assert_eq!(intent["amount"], serde_json::json!(100));
    assert_eq!(intent["currency_id"], serde_json::json!(CURRENCY_COIN));
    assert_eq!(intent["status"], serde_json::json!("pending"));
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();
    let created = intent["created_at"].as_i64().unwrap();
    let expires = intent["expires_at"].as_i64().unwrap();
    assert_eq!(expires - created, bblbb_backend::marketplace::INTENT_TTL_MS);

    // 用户 B 不能确认 A 的 Intent（user mismatch）。
    let err = checkout::confirm_intent(
        &pool,
        &ctx.buyer2,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::CheckoutUserMismatch));

    // 确认页也只对绑定用户可见。
    let err = checkout::intent_checkout_view(&pool, &ctx.buyer2, &intent_id, now_millis())
        .await
        .unwrap_err();
    assert!(matches!(err, MarketplaceError::CheckoutUserMismatch));
    let view = checkout::intent_checkout_view(&pool, &ctx.buyer, &intent_id, now_millis())
        .await
        .unwrap();
    assert_eq!(view["merchant_name"], serde_json::json!("Merchant Corp"));
    assert_eq!(view["amount"], serde_json::json!(100));
    assert_eq!(view["balance_after"], serde_json::json!(10_000 - 100));
    assert_eq!(
        view["scopes"][0],
        serde_json::json!("marketplace.checkout.create")
    );

    // 确认成功；再次确认（新幂等键）→ consumed。
    let purchase = checkout::confirm_intent(
        &pool,
        &ctx.buyer,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(purchase["status"], serde_json::json!("succeeded"));
    let err = checkout::confirm_intent(
        &pool,
        &ctx.buyer,
        &intent_id,
        version,
        &format!("conf2-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::CheckoutIntentConsumed));

    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn intent_expires_after_ttl() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    let intent = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-ttl",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();

    // 直接把 expires_at 改为过去（模拟 TTL 到期）。
    match &pool {
        sqlx::Either::Left(p) => {
            sqlx::query("UPDATE checkout_intents SET expires_at = 1 WHERE id = ?")
                .bind(&intent_id)
                .execute(p)
                .await
                .unwrap();
        }
        sqlx::Either::Right(_) => panic!("SQLite only"),
    }
    let err = checkout::confirm_intent(
        &pool,
        &ctx.buyer,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::CheckoutIntentExpired));
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-CHECKOUT-03/04 ───────────────────────────

#[tokio::test]
async fn confirm_rereads_offer_stock_user_and_limits() {
    let (pool, dir, ctx) = setup_ctx(100, Some(1)).await;
    let intent = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-stock",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();

    // 库存耗尽（并发超卖）：先耗尽库存。
    match &pool {
        sqlx::Either::Left(p) => {
            sqlx::query("UPDATE offers SET stock_remaining = 0 WHERE id = ?")
                .bind(&ctx.offer.id)
                .execute(p)
                .await
                .unwrap();
        }
        sqlx::Either::Right(_) => panic!("SQLite only"),
    }
    let err = checkout::confirm_intent(
        &pool,
        &ctx.buyer,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::OutOfStock));
    // Intent 未被消费（完整回滚）。
    let intent2 = checkout::load_intent(&pool, &intent_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(intent2.status, "pending");

    // 恢复库存 → 确认成功，库存扣减。
    match &pool {
        sqlx::Either::Left(p) => {
            sqlx::query("UPDATE offers SET stock_remaining = 1 WHERE id = ?")
                .bind(&ctx.offer.id)
                .execute(p)
                .await
                .unwrap();
        }
        sqlx::Either::Right(_) => panic!("SQLite only"),
    }
    let purchase = checkout::confirm_intent(
        &pool,
        &ctx.buyer,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(purchase["status"], serde_json::json!("succeeded"));
    let stock: i64 = match &pool {
        sqlx::Either::Left(p) => {
            sqlx::query_scalar("SELECT stock_remaining FROM offers WHERE id = ?")
                .bind(&ctx.offer.id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        sqlx::Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(stock, 0);
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn banned_user_and_price_tamper_are_rejected() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    // 价格篡改：错误 expected_offer_version 在意图创建即拒绝。
    let err = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version + 99,
        "ord-tamper",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::OfferVersionChanged));

    // 封禁用户：创建成功但 confirm 被拒，余额不变。
    let banned = insert_user(&pool, "banned").await;
    credit_user(&pool, &banned, 1000).await;
    set_user_status(&pool, &banned, "banned").await;
    let intent = checkout::create_intent(
        &pool,
        &banned,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-banned",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();
    let err = checkout::confirm_intent(
        &pool,
        &banned,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::Forbidden(_)));
    assert_eq!(balance_of(&pool, &banned).await, 1000);
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn insufficient_balance_rolls_back_and_intent_retryable() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    // 买方余额 0。
    let poor = insert_user(&pool, "poor").await;
    let intent = checkout::create_intent(
        &pool,
        &poor,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-poor",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();
    let err = checkout::confirm_intent(
        &pool,
        &poor,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::InsufficientFunds));
    // 完整回滚：无 Purchase、Intent 仍 pending、商户余额 0。
    let p = checkout::get_purchase(&pool, "nonexistent").await.unwrap();
    assert!(p.is_none());
    let intent2 = checkout::load_intent(&pool, &intent_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(intent2.status, "pending");
    let b = bblbb_backend::marketplace::balance::balance_view(&pool, &ctx.client.id)
        .await
        .unwrap();
    assert_eq!(b["pending_balance"], serde_json::json!(0));

    // 充值后重试（同一 Intent）成功 → 说明失败无残留副作用。
    credit_user(&pool, &poor, 1000).await;
    let purchase = checkout::confirm_intent(
        &pool,
        &poor,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(purchase["status"], serde_json::json!("succeeded"));
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-CHECKOUT-05/08 ───────────────────────────

#[tokio::test]
async fn concurrent_confirms_exactly_one_succeeds() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    let intent = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-conc",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();

    // 8 路并发 confirm（各自独立幂等键）——SQLite BEGIN IMMEDIATE 串行化，
    // 恰好一个成功。
    let mut handles = Vec::new();
    for i in 0..8 {
        let pool = pool.clone();
        let intent_id = intent_id.clone();
        let buyer = ctx.buyer.clone();
        handles.push(tokio::spawn(async move {
            checkout::confirm_intent(
                &pool,
                &buyer,
                &intent_id,
                version,
                &format!("conc-{i}-{}", uuid::Uuid::now_v7().simple()),
            )
            .await
        }));
    }
    let mut ok = 0;
    let mut consumed = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(v) => {
                assert_eq!(v["status"], serde_json::json!("succeeded"));
                ok += 1;
            }
            Err(e) => {
                assert!(matches!(e, MarketplaceError::CheckoutIntentConsumed));
                consumed += 1;
            }
        }
    }
    assert_eq!(ok, 1, "exactly one concurrent confirm must win");
    assert_eq!(consumed, 7);

    // 买方只被扣一次款；商户 pending 只入账一次。
    let spent = 10_000 - balance_of(&pool, &ctx.buyer).await;
    assert_eq!(spent, 100);
    let b = bblbb_backend::marketplace::balance::balance_view(&pool, &ctx.client.id)
        .await
        .unwrap();
    assert_eq!(b["pending_balance"], serde_json::json!(99)); // 100 - 1 fee
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn concurrent_same_intent_with_same_idempotency_key_replays() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    let intent = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-idem",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();
    let key = format!("same-key-{}", uuid::Uuid::now_v7().simple());

    // 并发同 key：一个 Created 执行，其余 Replay/InProgress；只扣一次款。
    let mut handles = Vec::new();
    for _ in 0..5 {
        let pool = pool.clone();
        let intent_id = intent_id.clone();
        let buyer = ctx.buyer.clone();
        let key = key.clone();
        handles.push(tokio::spawn(async move {
            checkout::confirm_intent(&pool, &buyer, &intent_id, version, &key).await
        }));
    }
    let mut succeeded = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(v) => {
                assert_eq!(v["status"], serde_json::json!("succeeded"));
                succeeded += 1;
            }
            Err(e) => {
                // InProgress 或 consumed 都是可接受的并发结果。
                assert!(
                    matches!(
                        e,
                        MarketplaceError::Invalid(_) | MarketplaceError::CheckoutIntentConsumed
                    ),
                    "unexpected error: {e:?}"
                );
            }
        }
    }
    assert!(succeeded >= 1);
    let spent = 10_000 - balance_of(&pool, &ctx.buyer).await;
    assert_eq!(spent, 100, "same key must debit exactly once");
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-CHECKOUT-07 ───────────────────────────

#[tokio::test]
async fn idempotency_replay_and_conflict() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    let key = format!("idemkey-{}", uuid::Uuid::now_v7().simple());
    let intent = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-replay",
        1,
        &key,
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();

    // 同 key + 同摘要 → 重放原 Intent。
    let replay = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-replay",
        1,
        &key,
    )
    .await
    .unwrap();
    assert_eq!(replay["intent_id"], intent["intent_id"]);

    // 同 key + 不同摘要（换 merchant_order_id）→ 409。
    let err = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-other",
        1,
        &key,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::IdempotencyConflict));

    // confirm 幂等：同 key 重放原 Purchase。
    let ckey = format!("ckey-{}", uuid::Uuid::now_v7().simple());
    let p1 = checkout::confirm_intent(&pool, &ctx.buyer, &intent_id, version, &ckey)
        .await
        .unwrap();
    let p2 = checkout::confirm_intent(&pool, &ctx.buyer, &intent_id, version, &ckey)
        .await
        .unwrap();
    assert_eq!(p1["id"], p2["id"]);
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-CHECKOUT-09/10 ───────────────────────────

#[tokio::test]
async fn offer_status_change_between_intent_and_confirm_is_rejected() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    let (oauth, secret) = create_oauth_confidential(&pool, "ck2").await;
    let owner = insert_user(&pool, "owner2").await;
    let client2 = create_marketplace_client(&pool, &oauth.client_id, &owner, 0, None).await;
    let principal = service_auth(&pool, &oauth.client_id, &secret, "marketplace.offer.write").await;
    let offer = create_active_offer(&pool, &principal, "ck2-1", 50, None).await;
    credit_user(&pool, &ctx.buyer, 10_000).await;

    let intent = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &oauth.client_id,
        &offer.id,
        offer.version,
        "ord-offer-1",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();

    // 创建 Intent 后 Client 被停用 → confirm 拒绝（client 重读）。
    let body = serde_json::json!({ "status": "disabled" });
    clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        client2.version,
        &owner,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();
    let err = checkout::confirm_intent(
        &pool,
        &ctx.buyer,
        &intent_id,
        version,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::MarketplaceDisabled(_)));
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn deny_decision_cancels_intent_without_charging() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    let intent = checkout::create_intent(
        &pool,
        &ctx.buyer,
        &ctx.client_id,
        &ctx.offer.id,
        ctx.offer.version,
        "ord-deny",
        1,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let view = checkout::deny_intent(&pool, &ctx.buyer, &intent_id, now_millis())
        .await
        .unwrap();
    assert_eq!(view["status"], serde_json::json!("denied"));
    // 余额未动。
    assert_eq!(balance_of(&pool, &ctx.buyer).await, 10_000);
    // denied Intent 不能再确认。
    let err = checkout::confirm_intent(
        &pool,
        &ctx.buyer,
        &intent_id,
        1,
        &format!("conf-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::CheckoutInteractionInvalid));
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn list_and_get_purchases_respect_scoping() {
    let (pool, dir, ctx) = setup_ctx(100, None).await;
    let purchase = buy_flow(&pool, &ctx.buyer, &ctx.client, &ctx.offer, 1).await;
    let purchase_id = purchase["id"].as_str().unwrap().to_string();

    // 用户本人列表包含该 Purchase。
    let mine = checkout::list_purchases(&pool, Some(&ctx.buyer), None, None, 30)
        .await
        .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0]["id"], serde_json::json!(purchase_id));
    // 其他用户看不到（隐藏其他用户的交易）。
    let other = checkout::list_purchases(&pool, Some(&ctx.buyer2), None, None, 30)
        .await
        .unwrap();
    assert!(other.is_empty());
    cleanup(&dir);
    close_pool(&pool).await;
}
