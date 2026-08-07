//! M12-CLIENTS：Client / Scope / Offer 管理安全测试（SQLite 真库）。
//!
//! 覆盖：Public Client 拒绝、secret hash-only、URL/SSRF 校验、逐 scope
//! 审批与限额、紧急停用、Offer 服务端登记（结账时不可改价/改收款方）、
//! merchant balance 不提现、普通 OIDC scope 不能扣款、禁用/撤销/旧版本/
//! 超限/URL 变体。

mod support;

use axum::response::IntoResponse;
use bblbb_backend::marketplace::clients;
use bblbb_backend::marketplace::offers;
use bblbb_backend::marketplace::webhooks;
use bblbb_backend::marketplace::{marketplace_error_to_app, MarketplaceError};
use serde_json::json;
use support::*;

// ─────────────────────────── M12-CLIENTS-01 ───────────────────────────

#[tokio::test]
async fn public_oauth_client_is_rejected_and_secret_is_hash_only() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "owner").await;
    let admin = insert_user(&pool, "admin").await;

    // Public Client 不能注册 Marketplace。
    let public_input = bblbb_backend::oidc::clients::ClientCreateInput {
        name: "public app".into(),
        client_type: "public".into(),
        redirect_uris: vec!["https://app.example/cb".into()],
        post_logout_uris: vec![],
        scopes: vec!["openid".into()],
    };
    let (public_oauth, _) =
        bblbb_backend::oidc::clients::create_client(&pool, &public_input, &admin, now_millis())
            .await
            .unwrap();
    let body = json!({
        "name": "Public App",
        "owner_user_id": owner,
        "terms_url": "https://app.example/terms",
        "privacy_url": "https://app.example/privacy",
        "webhook_url": "https://app.example/hook",
        "redirect_uris": ["https://app.example/cb"],
        "fee_bps": 0,
        "status": "active",
    });
    let err = clients::upsert_client(
        &pool,
        &public_oauth.client_id,
        &body,
        1,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::InvalidClient(_)));
    assert_eq!(err.code(), "marketplace_invalid_client");

    // Confidential Client 注册成功；webhook secret 只存密文（无明文列）。
    let (oauth, secret) = create_oauth_confidential(&pool, "conf").await;
    let body = json!({
        "name": "Conf App",
        "owner_user_id": owner,
        "terms_url": "https://app.example/terms",
        "privacy_url": "https://app.example/privacy",
        "webhook_url": "https://app.example/hook",
        "redirect_uris": ["https://app.example/cb"],
        "fee_bps": 0,
        "status": "active",
    });
    let client = clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        1,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();
    let (rotated, plaintext) = clients::rotate_webhook_secret(
        &pool,
        &client.client_id,
        &admin,
        "test",
        WEBHOOK_MASTER_KEY,
        now_millis(),
    )
    .await
    .unwrap();
    // 存储值 ≠ 明文（密文 hex）；能解密回明文。
    assert_ne!(rotated.webhook_secret_hash.as_deref().unwrap(), plaintext);
    assert_eq!(
        webhooks::decrypt_webhook_secret(
            WEBHOOK_MASTER_KEY,
            rotated.webhook_secret_hash.as_deref().unwrap()
        )
        .unwrap(),
        plaintext
    );
    // secret 本身不在任何视图 JSON 中（除轮换响应一次性明文）。
    let view = clients::client_view_json(&rotated);
    assert!(view.get("webhook_secret_hash").is_none());
    assert!(view.get("webhook_secret").is_none());
    let _ = secret;
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-CLIENTS-02 ───────────────────────────

#[tokio::test]
async fn url_validation_blocks_non_https_and_ssrf_targets() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "owner").await;
    let admin = insert_user(&pool, "admin").await;
    let (oauth, _) = create_oauth_confidential(&pool, "url").await;

    // 非 HTTPS 拒绝。
    assert!(matches!(
        clients::validate_webhook_url("http://merchant.example/hook"),
        Err(MarketplaceError::InvalidUrl(_))
    ));
    // 私网/回环/链路本地拒绝。
    for bad in [
        "https://127.0.0.1/hook",
        "https://10.0.0.5/hook",
        "https://192.168.1.1/hook",
        "https://169.254.169.254/latest/meta-data",
        "https://[::1]/hook",
        "https://100.64.0.1/hook",
    ] {
        assert!(
            matches!(
                clients::validate_webhook_url(bad),
                Err(MarketplaceError::UrlBlocked(_)) | Err(MarketplaceError::InvalidUrl(_))
            ),
            "must reject {bad}"
        );
    }
    // userinfo / fragment 拒绝。
    assert!(clients::validate_webhook_url("https://user:pass@merchant.example/hook").is_err());
    assert!(clients::validate_webhook_url("https://merchant.example/hook#frag").is_err());

    // 注册时 webhook/terms/privacy URL 同样校验。
    let body = json!({
        "name": "Bad App",
        "owner_user_id": owner,
        "terms_url": "https://merchant.example/terms",
        "privacy_url": "https://merchant.example/privacy",
        "webhook_url": "http://127.0.0.1/hook",
        "redirect_uris": ["https://merchant.example/cb"],
        "fee_bps": 0,
        "status": "active",
    });
    let err = clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        1,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        MarketplaceError::UrlBlocked(_) | MarketplaceError::InvalidUrl(_)
    ));

    // redirect URI 必须 HTTPS。
    assert!(clients::validate_redirect_uris(&["https://merchant.example/cb".to_string()]).is_ok());
    assert!(clients::validate_redirect_uris(&["http://merchant.example/cb".to_string()]).is_err());
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-CLIENTS-03/04 ───────────────────────────

#[tokio::test]
async fn scope_approval_workflow_and_ordinary_oidc_scopes_cannot_debit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "owner").await;
    let admin = insert_user(&pool, "admin").await;
    let (oauth, secret) = create_oauth_confidential(&pool, "scope").await;

    // 初始注册（pending）：服务认证失败。
    let body = json!({
        "name": "Scope App",
        "owner_user_id": owner,
        "terms_url": "https://merchant.example/terms",
        "privacy_url": "https://merchant.example/privacy",
        "webhook_url": "https://merchant.example/hook",
        "redirect_uris": ["https://merchant.example/cb"],
        "fee_bps": 0,
        "status": "pending",
    });
    let client = clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        1,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(client.status, "pending");

    // pending Client 服务认证被拒。
    let err =
        clients::service_authenticate(&pool, &oauth.client_id, &secret, "marketplace.offer.write")
            .await
            .unwrap_err();
    assert!(matches!(err, MarketplaceError::MarketplaceDisabled(_)));

    // 逐 scope 审批：只批 offer.write，未批 checkout scopes。
    let scopes = json!([
        {"scope": "marketplace.offer.write", "status": "approved", "limits": {}},
    ]);
    let body = json!({
        "status": "active",
        "scopes": scopes,
    });
    clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        1,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();
    let principal =
        clients::service_authenticate(&pool, &oauth.client_id, &secret, "marketplace.offer.write")
            .await
            .unwrap();
    assert!(principal.client.is_active());

    // 未批准的 scope 拒绝。
    let err = clients::service_authenticate(&pool, &oauth.client_id, &secret, "marketplace.refund")
        .await
        .unwrap_err();
    assert!(matches!(err, MarketplaceError::MarketplaceDisabled(_)));

    // 普通 OIDC scope 不进入 marketplace 白名单。
    assert!(!clients::is_valid_scope("openid"));
    assert!(!clients::is_valid_scope("profile"));
    assert!(!clients::is_valid_scope("marketplace.purchase2"));
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-CLIENTS-05/06 ───────────────────────────

#[tokio::test]
async fn checkout_derives_amount_and_recipient_from_server_snapshot() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let buyer = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let (oauth, secret) = create_oauth_confidential(&pool, "snapshot").await;
    let client = create_marketplace_client(
        &pool,
        &oauth.client_id,
        &owner,
        100,
        Some("https://merchant.example/hook"),
    )
    .await;
    let principal = service_auth(&pool, &oauth.client_id, &secret, "marketplace.offer.write").await;
    let offer = create_active_offer(&pool, &principal, "snap-1", 200, Some(5)).await;
    credit_user(&pool, &buyer, 1000).await;

    // 意图创建：请求体带 amount/currency/merchant → 被忽略（服务端派生）。
    let intent = bblbb_backend::marketplace::checkout::create_intent(
        &pool,
        &buyer,
        &oauth.client_id,
        &offer.id,
        offer.version,
        "ord-snap-1",
        1,
        &format!("idem-snap-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(intent["amount"], json!(200));
    assert_eq!(intent["currency_id"], json!(CURRENCY_COIN));
    assert_eq!(intent["client_id"], json!(client.id));
    assert_ne!(intent["amount"], json!(1)); // 不可能是请求方伪造的价格

    // confirm 使用快照金额；fee = 200 * 100bps / 10000 = 2；merchant_net = 198。
    let purchase = bblbb_backend::marketplace::checkout::confirm_intent(
        &pool,
        &buyer,
        intent["intent_id"].as_str().unwrap(),
        intent["version"].as_i64().unwrap(),
        &format!("conf-snap-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    assert_eq!(purchase["amount"], json!(200));
    assert_eq!(purchase["fee_amount"], json!(2));
    assert_eq!(purchase["merchant_net"], json!(198));

    // merchant 运营余额：pending=198（不可提现，只站内可追踪）。
    let balance = bblbb_backend::marketplace::balance::balance_view(&pool, &client.id)
        .await
        .unwrap();
    assert_eq!(balance["pending_balance"], json!(198));
    assert_eq!(balance["available_balance"], json!(0));
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn merchant_balance_cannot_cash_out_or_transfer() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let buyer = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let victim = insert_user(&pool, "victim").await;
    let (oauth, secret) = create_oauth_confidential(&pool, "cashout").await;
    let client = create_marketplace_client(&pool, &oauth.client_id, &owner, 0, None).await;
    let principal = service_auth(&pool, &oauth.client_id, &secret, "marketplace.offer.write").await;
    let offer = create_active_offer(&pool, &principal, "cash-1", 100, None).await;
    credit_user(&pool, &buyer, 1000).await;
    buy_flow(&pool, &buyer, &client, &offer, 1).await;

    // 账本禁止提现/兑换词（validate_command 兜底）。
    let cash_cmd = bblbb_backend::economy::ledger::service::LedgerCommand {
        idempotency_scope: "mp.cash".into(),
        idempotency_key: uuid::Uuid::now_v7().to_string(),
        kind: bblbb_backend::economy::ledger::service::LedgerKind::Award,
        actor_id: None,
        user_id: bblbb_backend::marketplace::merchant_ledger_user(&client.id),
        currency_id: CURRENCY_COIN.to_string(),
        delta_balance: -100,
        delta_frozen: 0,
        source_type: Some("withdraw".into()),
        source_id: None,
        memo: "withdraw to external bank".to_string(),
        reverses_operation_id: None,
    };
    let err =
        bblbb_backend::economy::ledger::service::apply_operation(&pool, cash_cmd, now_millis())
            .await
            .unwrap_err();
    assert!(matches!(
        err,
        bblbb_backend::economy::ledger::service::LedgerError::Invalid(_)
    ));

    // merchant 余额不能转给普通用户（LedgerKind::Transfer 被拒绝）。
    let transfer_cmd = bblbb_backend::economy::ledger::service::LedgerCommand {
        idempotency_scope: "mp.transfer".into(),
        idempotency_key: uuid::Uuid::now_v7().to_string(),
        kind: bblbb_backend::economy::ledger::service::LedgerKind::Transfer,
        actor_id: None,
        user_id: victim.to_string(),
        currency_id: CURRENCY_COIN.to_string(),
        delta_balance: 100,
        delta_frozen: 0,
        source_type: Some("marketplace_merchant".into()),
        source_id: Some(client.id.clone()),
        memo: "merchant payout".to_string(),
        reverses_operation_id: None,
    };
    let err =
        bblbb_backend::economy::ledger::service::apply_operation(&pool, transfer_cmd, now_millis())
            .await
            .unwrap_err();
    assert!(matches!(
        err,
        bblbb_backend::economy::ledger::service::LedgerError::Invalid(_)
    ));
    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M12-CLIENTS-07/08 ───────────────────────────

#[tokio::test]
async fn emergency_disable_blocks_new_sales_but_history_stays_queryable() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let buyer = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let admin = insert_user(&pool, "admin").await;
    let (oauth, secret) = create_oauth_confidential(&pool, "emg").await;
    let client = create_marketplace_client(&pool, &oauth.client_id, &owner, 0, None).await;
    let principal = service_auth(&pool, &oauth.client_id, &secret, "marketplace.offer.write").await;
    let offer = create_active_offer(&pool, &principal, "emg-1", 100, None).await;
    credit_user(&pool, &buyer, 1000).await;
    let purchase = buy_flow(&pool, &buyer, &client, &offer, 1).await;
    let purchase_id = purchase["id"].as_str().unwrap().to_string();

    // 紧急停用（If-Match）。
    let disabled = clients::emergency_disable(
        &pool,
        &oauth.client_id,
        "suspected abuse",
        &admin,
        "admin",
        client.version,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(disabled.status, "emergency_disabled");

    // 新 Intent 被拒。
    let err = bblbb_backend::marketplace::checkout::create_intent(
        &pool,
        &buyer,
        &oauth.client_id,
        &offer.id,
        offer.version,
        "ord-emg-2",
        1,
        &format!("idem-emg-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::MarketplaceDisabled(_)));

    // 服务操作（offer.write）也被拒。
    let err =
        clients::service_authenticate(&pool, &oauth.client_id, &secret, "marketplace.offer.write")
            .await
            .unwrap_err();
    assert!(matches!(err, MarketplaceError::MarketplaceDisabled(_)));

    // 历史 Purchase 仍可查询。
    let history = bblbb_backend::marketplace::checkout::get_purchase(&pool, &purchase_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(history.id, purchase_id);
    assert_eq!(history.amount, 100);
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn disabled_client_and_revoked_scope_and_old_offer_and_over_limit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let buyer = insert_user(&pool, "buyer").await;
    let owner = insert_user(&pool, "owner").await;
    let admin = insert_user(&pool, "admin").await;
    let (oauth, secret) = create_oauth_confidential(&pool, "disable").await;
    let client = create_marketplace_client(&pool, &oauth.client_id, &owner, 0, None).await;
    let principal = service_auth(&pool, &oauth.client_id, &secret, "marketplace.offer.write").await;
    let offer = create_active_offer(&pool, &principal, "d-1", 100, None).await;
    credit_user(&pool, &buyer, 5000).await;

    // Client 禁用 → 新 Intent 拒绝。
    let body = json!({ "status": "disabled" });
    clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        client.version,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();
    let err = bblbb_backend::marketplace::checkout::create_intent(
        &pool,
        &buyer,
        &oauth.client_id,
        &offer.id,
        offer.version,
        "ord-disable-1",
        1,
        &format!("idem-d-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::MarketplaceDisabled(_)));

    // 重新激活（版本续期）。
    let body = json!({ "status": "active" });
    let client = clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        2,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();

    // Scope 撤销：checkout scopes disabled → 新 Intent 拒绝。
    let body = json!({
        "scopes": [
            {"scope": "marketplace.checkout.create", "status": "disabled", "limits": {}},
            {"scope": "marketplace.purchase", "status": "disabled", "limits": {}},
        ]
    });
    clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        client.version,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();
    // Scope 撤销后 Client 版本 +1（供后续恢复使用）。
    let client = clients::fetch_client_by_internal_id(&pool, &client.id)
        .await
        .unwrap()
        .unwrap();
    let err = bblbb_backend::marketplace::checkout::create_intent(
        &pool,
        &buyer,
        &oauth.client_id,
        &offer.id,
        offer.version,
        "ord-scope-1",
        1,
        &format!("idem-s-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::MarketplaceDisabled(_)));

    // 恢复 scopes。
    let body = json!({
        "scopes": [
            {"scope": "marketplace.checkout.create", "status": "approved", "limits": {"max_amount_per_transaction": 100000}},
            {"scope": "marketplace.purchase", "status": "approved", "limits": {}},
        ]
    });
    let client = clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        client.version,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();

    // 旧版本 Offer 不能创建新 Intent（先 PATCH 出新版本）。
    let bump = json!({
        "external_offer_id": offer.external_offer_id,
        "title": offer.title,
        "description": offer.description_safe,
        "currency_id": offer.currency_id,
        "unit_amount": 250,
        "quantity_min": 1,
        "quantity_max": 1,
        "stock_policy": offer.stock_policy,
        "stock_remaining": offer.stock_remaining,
        "status": "active",
    });
    let new_offer = offers::update_offer(
        &pool,
        &principal,
        &offer.id,
        offer.version,
        &bump,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(new_offer.version, offer.version + 1);
    let err = bblbb_backend::marketplace::checkout::create_intent(
        &pool,
        &buyer,
        &oauth.client_id,
        &offer.id,
        offer.version, // 旧版本
        "ord-old-1",
        1,
        &format!("idem-o-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::OfferVersionChanged));

    // 超限：单笔限额 100000，报价 250 正常；改为超限限额后拒绝。
    let body = json!({
        "scopes": [
            {"scope": "marketplace.checkout.create", "status": "approved", "limits": {"max_amount_per_transaction": 100}},
            {"scope": "marketplace.purchase", "status": "approved", "limits": {}},
        ]
    });
    clients::upsert_client(
        &pool,
        &oauth.client_id,
        &body,
        client.version,
        &admin,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();
    let err = bblbb_backend::marketplace::checkout::create_intent(
        &pool,
        &buyer,
        &oauth.client_id,
        &new_offer.id,
        new_offer.version,
        "ord-limit-1",
        1,
        &format!("idem-l-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::DailyLimitExceeded));
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn offers_are_scoped_to_owning_client() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner_a = insert_user(&pool, "owner_a").await;
    let owner_b = insert_user(&pool, "owner_b").await;
    let (oauth_a, secret_a) = create_oauth_confidential(&pool, "a").await;
    let (oauth_b, secret_b) = create_oauth_confidential(&pool, "b").await;
    create_marketplace_client(&pool, &oauth_a.client_id, &owner_a, 0, None).await;
    create_marketplace_client(&pool, &oauth_b.client_id, &owner_b, 0, None).await;
    let principal_a = service_auth(
        &pool,
        &oauth_a.client_id,
        &secret_a,
        "marketplace.offer.write",
    )
    .await;
    let principal_b = service_auth(
        &pool,
        &oauth_b.client_id,
        &secret_b,
        "marketplace.offer.write",
    )
    .await;
    let offer_a = create_active_offer(&pool, &principal_a, "a-1", 100, None).await;

    // B 更新 A 的 Offer → 拒绝。
    let bump = json!({
        "external_offer_id": "a-1",
        "title": "hijack",
        "description": null,
        "currency_id": CURRENCY_COIN,
        "unit_amount": 1,
        "quantity_min": 1,
        "quantity_max": 1,
        "stock_policy": "unlimited",
        "stock_remaining": null,
        "status": "active",
    });
    let err = offers::update_offer(
        &pool,
        &principal_b,
        &offer_a.id,
        offer_a.version,
        &bump,
        now_millis(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MarketplaceError::Forbidden(_)));
    cleanup(&dir);
    close_pool(&pool).await;
}

/// 占位断言：错误码映射到稳定 Problem code（供 ERROR-CODES 注册表校验）。
#[test]
fn error_codes_are_stable() {
    assert_eq!(
        MarketplaceError::CheckoutUserMismatch.code(),
        "checkout_user_mismatch"
    );
    assert_eq!(
        MarketplaceError::CheckoutInteractionInvalid.code(),
        "checkout_interaction_invalid"
    );
    assert_eq!(
        MarketplaceError::InsufficientFunds.code(),
        "insufficient_funds"
    );
    assert_eq!(
        MarketplaceError::RefundExceedsPurchase.code(),
        "refund_exceeds_purchase"
    );
    assert_eq!(
        MarketplaceError::MarketplaceDisabled("x".into()).code(),
        "marketplace_disabled"
    );
    assert_eq!(
        MarketplaceError::InvalidClient("x".into()).code(),
        "marketplace_invalid_client"
    );
    assert_eq!(
        MarketplaceError::WebhookInvalidSignature.code(),
        "webhook_invalid_signature"
    );
    assert_eq!(
        MarketplaceError::MerchantBalanceInsufficient.code(),
        "merchant_balance_insufficient"
    );
    // 403/409 状态映射（Problem 响应）。
    let resp =
        marketplace_error_to_app(MarketplaceError::CheckoutUserMismatch, "r1").into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::FORBIDDEN);
    let resp = marketplace_error_to_app(MarketplaceError::CheckoutInteractionInvalid, "r2")
        .into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::CONFLICT);
    let resp =
        marketplace_error_to_app(MarketplaceError::RefundExceedsPurchase, "r3").into_response();
    assert_eq!(resp.status(), axum::http::StatusCode::CONFLICT);
}
