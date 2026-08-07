//! M12-REFUND：退款 reversal、结算、Webhook、对账与恒等式测试（SQLite）。
//!
//! 覆盖：reversal-only（不修改原 Purchase/流水）、并发累计上限、Client
//! 作用域、管理员退款、pending→available 结算与退款补偿恒等式、Webhook
//! HMAC/时间窗/重放/dead-letter、对账差异分类、`Σ(delta)=0` 恒等式。

mod support;

use std::pin::Pin;

use bblbb_backend::marketplace::balance;
use bblbb_backend::marketplace::checkout;
use bblbb_backend::marketplace::clients;
use bblbb_backend::marketplace::reconcile;
use bblbb_backend::marketplace::refunds::{self, RefundInput};
use bblbb_backend::marketplace::webhooks::{
    self, WebhookClient, WebhookResponse, WebhookSendError,
};
use bblbb_backend::marketplace::MarketplaceError;
use hmac::Mac;
use support::*;

struct Ctx {
    buyer: String,
    client_id: String,
    client_internal: String,
    offer_id: String,
}

async fn setup_ctx(
    amount: i64,
    fee_bps: i64,
) -> (bblbb_backend::db::DatabasePool, std::path::PathBuf, Ctx) {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let buyer = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let (oauth, _secret) = create_oauth_confidential(&pool, "rf").await;
    let _client = create_marketplace_client(
        &pool,
        &oauth.client_id,
        &owner,
        fee_bps,
        Some("https://merchant.example/hook"),
    )
    .await;
    let principal =
        service_auth(&pool, &oauth.client_id, &_secret, "marketplace.offer.write").await;
    let offer = create_active_offer(&pool, &principal, "rf-1", amount, None).await;
    credit_user(&pool, &buyer, 10_000).await;
    let ctx = Ctx {
        buyer,
        client_id: oauth.client_id.clone(),
        client_internal: _client.id.clone(),
        offer_id: offer.id.clone(),
    };
    (pool, dir, ctx)
}

/// 造一笔 Purchase，返回 (purchase_id, amount, merchant_order_id)。
async fn make_purchase(pool: &bblbb_backend::db::DatabasePool, ctx: &Ctx) -> (String, i64, String) {
    let client = clients::fetch_client_by_internal_id(pool, &ctx.client_internal)
        .await
        .unwrap()
        .unwrap();
    let offer = bblbb_backend::marketplace::offers::get_offer(pool, &ctx.offer_id)
        .await
        .unwrap()
        .unwrap();
    let purchase = buy_flow(pool, &ctx.buyer, &client, &offer, 1).await;
    let id = purchase["id"].as_str().unwrap().to_string();
    (
        id,
        purchase["amount"].as_i64().unwrap(),
        purchase["merchant_order_id"].as_str().unwrap().to_string(),
    )
}

// ─────────────────────────── M12-REFUND-01/02 ───────────────────────────

#[tokio::test]
async fn refund_is_reversal_only_and_respects_cumulative_cap() {
    let (pool, dir, ctx) = setup_ctx(100, 0).await;
    let (purchase_id, _amount, _) = make_purchase(&pool, &ctx).await;
    let balance_before = balance_of(&pool, &ctx.buyer).await;

    let principal = clients::fetch_client_by_client_id(&pool, &ctx.client_id)
        .await
        .unwrap()
        .unwrap();
    let input = RefundInput {
        amount: 40,
        reason_code: "customer_request".into(),
        merchant_refund_id: "ref-1".into(),
    };
    let refund = refunds::create_refund_inner(
        &pool,
        &principal.client_id,
        &principal.owner_user_id,
        "client",
        Some(&principal),
        &purchase_id,
        &input,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(refund["status"], serde_json::json!("processed"));
    assert_eq!(balance_of(&pool, &ctx.buyer).await, balance_before + 40);

    // 原 Purchase 与流水未被修改（reversal-only）。
    let purchase = checkout::get_purchase(&pool, &purchase_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(purchase.refunded_amount, 40);
    assert_eq!(purchase.status, "partially_refunded");

    // 并发累计上限：再多退 70 超限（40+70 > 100）→ RefundExceedsPurchase。
    let mut handles = Vec::new();
    for i in 0..4 {
        let pool = pool.clone();
        let purchase_id = purchase_id.clone();
        let principal = principal.clone();
        handles.push(tokio::spawn(async move {
            let input = RefundInput {
                amount: 70,
                reason_code: "customer_request".into(),
                merchant_refund_id: format!("ref-conc-{i}"),
            };
            refunds::create_refund_inner(
                &pool,
                &principal.client_id,
                &principal.owner_user_id,
                "client",
                Some(&principal),
                &purchase_id,
                &input,
                &format!("idem-c-{i}-{}", uuid::Uuid::now_v7().simple()),
            )
            .await
        }));
    }
    let mut processed = 0;
    let mut exceeded = 0;
    for h in handles {
        match h.await.unwrap() {
            Ok(v) => {
                assert_eq!(v["status"], serde_json::json!("processed"));
                processed += 1;
            }
            Err(MarketplaceError::RefundExceedsPurchase) => exceeded += 1,
            Err(e) => panic!("unexpected: {e:?}"),
        }
    }
    // 剩余可退 60：70 超限 → 至多 0 笔 70 退款成功（每笔 70>60）。
    assert_eq!(processed, 0);
    assert!(exceeded >= 1);
    // 精确退剩余 60 成功 → 累计 100 = 原金额，状态 refunded。
    let input = RefundInput {
        amount: 60,
        reason_code: "customer_request".into(),
        merchant_refund_id: "ref-final".into(),
    };
    let refund = refunds::create_refund_inner(
        &pool,
        &principal.client_id,
        &principal.owner_user_id,
        "client",
        Some(&principal),
        &purchase_id,
        &input,
        &format!("idem-f-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(refund["status"], serde_json::json!("processed"));
    let purchase = checkout::get_purchase(&pool, &purchase_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(purchase.refunded_amount, 100);
    assert_eq!(purchase.status, "refunded");
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn client_cannot_refund_another_clients_purchase() {
    let (pool, dir, ctx) = setup_ctx(100, 0).await;
    let (purchase_id, _, _) = make_purchase(&pool, &ctx).await;
    let (oauth_b, secret_b) = create_oauth_confidential(&pool, "other").await;
    let owner_b = insert_user(&pool, "owner_b").await;
    let client_b = create_marketplace_client(&pool, &oauth_b.client_id, &owner_b, 0, None).await;
    let _ = service_auth(&pool, &oauth_b.client_id, &secret_b, "marketplace.refund").await;
    let input = RefundInput {
        amount: 10,
        reason_code: "customer_request".into(),
        merchant_refund_id: "ref-x".into(),
    };
    let err = refunds::create_refund_inner(
        &pool,
        &client_b.client_id,
        &client_b.owner_user_id,
        "client",
        Some(&client_b),
        &purchase_id,
        &input,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::Forbidden(_)));
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn admin_refund_works_without_client_scope() {
    let (pool, dir, ctx) = setup_ctx(100, 0).await;
    let (purchase_id, _, _) = make_purchase(&pool, &ctx).await;
    let balance_before = balance_of(&pool, &ctx.buyer).await;
    let admin = insert_user(&pool, "admin").await;
    let input = RefundInput {
        amount: 100,
        reason_code: "admin_override".into(),
        merchant_refund_id: "ref-admin".into(),
    };
    let refund = refunds::create_refund(
        &pool,
        &admin,
        "admin",
        None,
        &purchase_id,
        &input,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(refund["status"], serde_json::json!("processed"));
    assert_eq!(balance_of(&pool, &ctx.buyer).await, balance_before + 100);
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-REFUND-04 ───────────────────────────

#[tokio::test]
async fn settlement_and_refund_preserve_double_sided_identity() {
    let (pool, dir, ctx) = setup_ctx(1000, 100).await;
    // fee = 1000*100bps/10000 = 10；merchant_net = 990。
    let (purchase_id, amount, _) = make_purchase(&pool, &ctx).await;
    let account = balance::get_account(&pool, &ctx.client_internal, CURRENCY_COIN)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(account.pending_balance, 990);
    let ledger_merchant_before = ledger_balance(&pool, &ctx.client_internal).await;

    // 结算：pending → available（总额不变）。
    balance::settle_pending(&pool, &ctx.client_internal, 990, now_millis())
        .await
        .unwrap();
    let account = balance::get_account(&pool, &ctx.client_internal, CURRENCY_COIN)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(account.pending_balance, 0);
    assert_eq!(account.available_balance, 990);
    assert_eq!(account.total(), 990);

    // 退款（全部）：买方 +amount；商户 available -990；平台费 -10（按比例）。
    let principal = clients::fetch_client_by_client_id(&pool, &ctx.client_id)
        .await
        .unwrap()
        .unwrap();
    let input = RefundInput {
        amount,
        reason_code: "customer_request".into(),
        merchant_refund_id: "ref-settle".into(),
    };
    let refund = refunds::create_refund_inner(
        &pool,
        &principal.client_id,
        &principal.owner_user_id,
        "client",
        Some(&principal),
        &purchase_id,
        &input,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(refund["status"], serde_json::json!("processed"));

    // 买方余额恢复；商户余额归零。
    assert_eq!(balance_of(&pool, &ctx.buyer).await, 10_000);
    let account = balance::get_account(&pool, &ctx.client_internal, CURRENCY_COIN)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(account.total(), 0);
    let ledger_merchant_after = ledger_balance(&pool, &ctx.client_internal).await;
    assert_eq!(ledger_merchant_after, 0);
    // 恒等式：merchant ledger delta = -990。
    assert_eq!(ledger_merchant_before - ledger_merchant_after, 990);
    cleanup(&dir);
    close_pool(&pool).await;
}

async fn ledger_balance(pool: &bblbb_backend::db::DatabasePool, client_internal: &str) -> i64 {
    let user = bblbb_backend::marketplace::merchant_ledger_user(client_internal);
    bblbb_backend::economy::ledger::service::get_account(pool, &user, CURRENCY_COIN)
        .await
        .map(|a| a.balance + a.frozen_balance)
        .unwrap_or(0)
}

#[tokio::test]
async fn insufficient_merchant_balance_goes_requested_and_freezes_new_sales() {
    let (pool, dir, ctx) = setup_ctx(100, 0).await;
    let (purchase_id, amount, _) = make_purchase(&pool, &ctx).await;
    // 把商户可用余额清零（直接改运营表模拟异常场景，不产生账本差异——测试仅验证冻结逻辑）。
    match &pool {
        sqlx::Either::Left(p) => {
            sqlx::query("UPDATE marketplace_merchant_accounts SET available_balance = 0, pending_balance = 0 WHERE client_id = ?")
                .bind(&ctx.client_internal)
                .execute(p)
                .await
                .unwrap();
        }
        sqlx::Either::Right(_) => panic!("SQLite only"),
    }
    let principal = clients::fetch_client_by_client_id(&pool, &ctx.client_id)
        .await
        .unwrap()
        .unwrap();
    let input = RefundInput {
        amount,
        reason_code: "customer_request".into(),
        merchant_refund_id: "ref-insufficient".into(),
    };
    let refund = refunds::create_refund_inner(
        &pool,
        &principal.client_id,
        &principal.owner_user_id,
        "client",
        Some(&principal),
        &purchase_id,
        &input,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(refund["status"], serde_json::json!("requested"));
    // Client 新销售被冻结。
    let client = clients::fetch_client_by_internal_id(&pool, &ctx.client_internal)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(client.status, "disabled");
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-REFUND-05/06 ───────────────────────────

/// 测试用 Webhook HTTP 客户端（可配置响应）。
#[allow(clippy::type_complexity)]
struct MockWebhook {
    status: u16,
    calls: std::sync::atomic::AtomicUsize,
    last: std::sync::Mutex<Option<(Vec<(String, String)>, Vec<u8>)>>,
}

impl MockWebhook {
    fn new(status: u16) -> Self {
        Self {
            status,
            calls: std::sync::atomic::AtomicUsize::new(0),
            last: std::sync::Mutex::new(None),
        }
    }
}

impl WebhookClient for MockWebhook {
    fn post(
        &self,
        url: &str,
        headers: Vec<(String, String)>,
        body: Vec<u8>,
    ) -> Pin<
        Box<
            dyn std::future::Future<Output = Result<WebhookResponse, WebhookSendError>> + Send + '_,
        >,
    > {
        let status = self.status;
        self.calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let _ = url;
        *self.last.lock().unwrap() = Some((headers, body));
        Box::pin(async move { Ok(WebhookResponse { status }) })
    }
}

#[tokio::test]
async fn webhook_hmac_time_window_replay_and_delivery_records() {
    let (pool, dir, ctx) = setup_ctx(100, 0).await;
    let (_purchase_id, _amount, _) = make_purchase(&pool, &ctx).await;
    let client = clients::fetch_client_by_internal_id(&pool, &ctx.client_internal)
        .await
        .unwrap()
        .unwrap();
    let secret = webhook_secret_of(&pool, &client);

    // 1) 接收方校验向量：正确签名 + 时间窗内 → OK。
    let body = b"{\"event_id\":\"evt-1\",\"event_type\":\"marketplace.purchase_succeeded.v1\"}";
    let ts = now_millis();
    let mut mac = hmac::Hmac::<sha2::Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(&webhooks::signature_input(ts, "evt-1", body));
    let sig = hex::encode(mac.finalize().into_bytes());
    webhooks::verify_webhook_request(&secret, ts, "evt-1", body, &sig, now_millis()).unwrap();
    // 2) 错误签名拒绝。
    assert!(webhooks::verify_webhook_request(
        &secret,
        ts,
        "evt-1",
        body,
        &"deadbeef".repeat(8),
        now_millis()
    )
    .is_err());
    // 3) 时间窗超限拒绝。
    let old_ts = ts - 10 * 60 * 1000;
    assert!(
        webhooks::verify_webhook_request(&secret, old_ts, "evt-1", body, &sig, now_millis())
            .is_err()
    );
    // 4) event_id 重放保护摘要。
    assert_eq!(
        webhooks::event_id_hash("evt-1"),
        webhooks::event_id_hash("evt-1")
    );
    assert_ne!(
        webhooks::event_id_hash("evt-1"),
        webhooks::event_id_hash("evt-2")
    );

    // 5) 投递记录 + HMAC 头 + 2xx → sent。购买事务已为 Outbox 事件登记一条
    //    投递记录（post-commit；真实路径）。
    let deliveries = webhooks::list_deliveries(&pool, Some(&ctx.client_internal), None, None, 10)
        .await
        .unwrap();
    assert_eq!(deliveries.len(), 1);
    assert_eq!(
        deliveries[0].event_type,
        bblbb_backend::events::types::MARKETPLACE_PURCHASE_SUCCEEDED
    );
    let mock = MockWebhook::new(200);
    let ok = webhooks::deliver_one(
        &pool,
        &client,
        &deliveries[0],
        WEBHOOK_MASTER_KEY,
        &mock,
        now_millis(),
    )
    .await
    .unwrap();
    assert!(ok);
    let sent = webhooks::list_deliveries(&pool, Some(&ctx.client_internal), Some("sent"), None, 10)
        .await
        .unwrap();
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].attempts, 1);
    assert_eq!(sent[0].last_status_code, Some(200));
    // payload 最小化：不含 user_id/email/balance。
    let payload_str = serde_json::to_string(&sent[0].payload).unwrap();
    assert!(!payload_str.contains("user_id"));
    assert!(!payload_str.contains("email"));
    assert!(!payload_str.contains("balance"));
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn webhook_non_2xx_backs_off_and_dead_letters() {
    let (pool, dir, ctx) = setup_ctx(100, 0).await;
    let client = clients::fetch_client_by_internal_id(&pool, &ctx.client_internal)
        .await
        .unwrap()
        .unwrap();
    let payload = webhooks::minimal_payload(
        "evt-fail-1",
        bblbb_backend::events::types::MARKETPLACE_PURCHASE_SUCCEEDED,
        &serde_json::json!({
            "client_id": ctx.client_id,
            "purchase_id": "p-fail-1",
            "status": "succeeded",
            "amount": 1,
            "currency_id": CURRENCY_COIN,
            "merchant_order_id": "x",
        }),
    );
    webhooks::register_delivery(
        &pool,
        &ctx.client_internal,
        "evt-fail-1",
        "marketplace.purchase_succeeded.v1",
        &payload,
        now_millis(),
    )
    .await
    .unwrap();
    // 首次 500 → 退避重试（pending + next_retry_at 未来）。
    let mock = MockWebhook::new(500);
    let deliveries =
        webhooks::list_deliveries(&pool, Some(&ctx.client_internal), Some("pending"), None, 10)
            .await
            .unwrap();
    let ok = webhooks::deliver_one(
        &pool,
        &client,
        &deliveries[0],
        WEBHOOK_MASTER_KEY,
        &mock,
        now_millis(),
    )
    .await
    .unwrap();
    assert!(!ok);
    let after =
        webhooks::list_deliveries(&pool, Some(&ctx.client_internal), Some("pending"), None, 10)
            .await
            .unwrap();
    assert_eq!(after[0].attempts, 1);
    assert!(after[0].next_retry_at > now_millis());

    // 连续 500 到 max_attempts → dead_letter（保留记录可手动重放）。
    let mut current = after[0].clone();
    let mock = MockWebhook::new(500);
    for _ in 1..bblbb_backend::marketplace::WEBHOOK_MAX_ATTEMPTS {
        let ok = webhooks::deliver_one(
            &pool,
            &client,
            &current,
            WEBHOOK_MASTER_KEY,
            &mock,
            now_millis(),
        )
        .await
        .unwrap();
        assert!(!ok);
        let rows = webhooks::list_deliveries(&pool, Some(&ctx.client_internal), None, None, 10)
            .await
            .unwrap();
        current = rows[0].clone();
    }
    assert_eq!(current.status, "dead_letter");
    assert_eq!(
        current.attempts,
        bblbb_backend::marketplace::WEBHOOK_MAX_ATTEMPTS
    );
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-REFUND-07/08 ───────────────────────────

#[tokio::test]
async fn reconciliation_classifies_diffs_and_identity_holds() {
    let (pool, dir, ctx) = setup_ctx(100, 0).await;
    let (purchase_id, _, _) = make_purchase(&pool, &ctx).await;
    let after_cursor = now_millis() - 60_000;

    // 正常窗口：consistent。
    let report =
        reconcile::run_reconciliation(&pool, &ctx.client_internal, after_cursor, now_millis())
            .await
            .unwrap();
    assert_eq!(report["status"], serde_json::json!("consistent"));
    assert_eq!(report["purchases_count"], serde_json::json!(1));
    assert_eq!(report["amount_sum"], serde_json::json!(100));
    assert_eq!(report["window_identity_sum"], serde_json::json!(0));

    // 破坏恒等式：删除商户 operation 流水 → diff_found(missing_ledger_op)。
    match &pool {
        sqlx::Either::Left(p) => {
            let merchant_op: String =
                sqlx::query_scalar("SELECT merchant_operation_id FROM purchases WHERE id = ?")
                    .bind(&purchase_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            sqlx::query("DELETE FROM point_transactions WHERE operation_id = ?")
                .bind(&merchant_op)
                .execute(p)
                .await
                .unwrap();
        }
        sqlx::Either::Right(_) => panic!("SQLite only"),
    }
    let report =
        reconcile::run_reconciliation(&pool, &ctx.client_internal, after_cursor, now_millis())
            .await
            .unwrap();
    assert_eq!(report["status"], serde_json::json!("diff_found"));
    let diffs = report["diffs"].as_array().unwrap();
    assert!(
        diffs
            .iter()
            .any(|d| d["class"] == serde_json::json!("missing_ledger_op")),
        "expected missing_ledger_op diff"
    );
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn refund_identity_sum_is_zero_and_immutable_history_preserved() {
    let (pool, dir, ctx) = setup_ctx(200, 50).await;
    // fee = 200*50/10000 = 1；merchant_net = 199。
    let (purchase_id, _amount, _) = make_purchase(&pool, &ctx).await;
    let principal = clients::fetch_client_by_client_id(&pool, &ctx.client_id)
        .await
        .unwrap()
        .unwrap();

    // 半额退款 100：fee_refund = 1 * 100/200 = 0（整数舍入），merchant 扣 100。
    let input = RefundInput {
        amount: 100,
        reason_code: "partial".into(),
        merchant_refund_id: "ref-half".into(),
    };
    refunds::create_refund_inner(
        &pool,
        &principal.client_id,
        &principal.owner_user_id,
        "client",
        Some(&principal),
        &purchase_id,
        &input,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();

    // 恒等式：原 Purchase 买方 op、商户 op、平台费 op + 退款 reversal 全部
    // 加总 = 0（不修改历史流水）。
    let op_rows: Vec<(String, i64, i64)> = match &pool {
        sqlx::Either::Left(p) => sqlx::query_as::<_, (String, i64, i64)>(
            "SELECT po.id, pt.delta_balance, pt.delta_frozen
                 FROM point_operations po JOIN point_transactions pt ON pt.operation_id = po.id
                 WHERE po.idempotency_scope = ? OR po.idempotency_scope = ? ORDER BY po.created_at",
        )
        .bind(format!("marketplace.purchase.{purchase_id}"))
        .bind(format!("marketplace.refund.{purchase_id}"))
        .fetch_all(p)
        .await
        .unwrap(),
        sqlx::Either::Right(_) => panic!("SQLite only"),
    };
    let delta_sum: i64 = op_rows.iter().map(|(_, b, f)| b + f).sum();
    assert_eq!(delta_sum, 0, "Σ(delta_balance + delta_frozen) must be 0");
    assert!(op_rows.len() >= 5); // buyer + merchant + fee + buyer_rev + merchant_rev

    // 原 Purchase 行未被删除/改金额。
    let purchase = checkout::get_purchase(&pool, &purchase_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(purchase.amount, 200);
    assert_eq!(purchase.refunded_amount, 100);
    cleanup(&dir);
    close_pool(&pool).await;
}
