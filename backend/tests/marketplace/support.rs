#![allow(dead_code)]
//! M12 共享测试造数助手（SQLite 真库 + 全量迁移 + 线上服务函数）。
//!
//! 所有操作走 `bblbb_backend::marketplace::*` 线上代码路径，不重实现被测逻辑。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::economy::ledger::service as ledger;
use bblbb_backend::economy::ledger::service::LedgerCommand;
use bblbb_backend::marketplace::clients::{self, MarketplaceClient, ServicePrincipal};
use bblbb_backend::marketplace::offers::{self, OfferRow};
use bblbb_backend::marketplace::webhooks;
use bblbb_backend::oidc::clients::OAuthClient;
use sqlx::Either;

pub use bblbb_backend::economy::ledger::service::CURRENCY_COIN;
pub use bblbb_backend::outbox::now_millis;

/// Webhook Secret 加密主密钥（测试常量；对应 config.marketplace_webhook_encryption_key）。
pub const WEBHOOK_MASTER_KEY: &str = "test-marketplace-webhook-master-key";

pub async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-mp-{}", uuid::Uuid::now_v7()));
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

pub fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

pub async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

pub async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
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

pub async fn set_user_status(pool: &DatabasePool, user_id: &str, status: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET status = ? WHERE id = ?")
                .bind(status)
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 买方充值（走线上账本 credit）。
pub async fn credit_user(pool: &DatabasePool, user_id: &str, amount: i64) {
    let cmd = LedgerCommand {
        idempotency_scope: format!("test.credit.{user_id}"),
        idempotency_key: uuid::Uuid::now_v7().to_string(),
        kind: ledger::LedgerKind::Award,
        actor_id: None,
        user_id: user_id.to_string(),
        currency_id: CURRENCY_COIN.to_string(),
        delta_balance: amount,
        delta_frozen: 0,
        source_type: Some("test".to_string()),
        source_id: None,
        memo: "test credit".to_string(),
        reverses_operation_id: None,
    };
    ledger::credit(pool, cmd, now_millis()).await.unwrap();
}

/// 读取买方可用余额。
pub async fn balance_of(pool: &DatabasePool, user_id: &str) -> i64 {
    ledger::get_account(pool, user_id, CURRENCY_COIN)
        .await
        .map(|a| a.balance)
        .unwrap_or(0)
}

/// 创建 Confidential OAuth Client（走线上 oidc clients.create_client）。
pub async fn create_oauth_confidential(pool: &DatabasePool, tag: &str) -> (OAuthClient, String) {
    let input = bblbb_backend::oidc::clients::ClientCreateInput {
        name: format!("Market Test {tag}"),
        client_type: "confidential".into(),
        redirect_uris: vec!["https://merchant.example/cb".to_string()],
        post_logout_uris: vec![],
        scopes: vec!["openid".to_string(), "profile".to_string()],
    };
    let (client, secret) =
        bblbb_backend::oidc::clients::create_client(pool, &input, "test-admin", now_millis())
            .await
            .unwrap();
    (
        client,
        secret.expect("confidential client must return a secret"),
    )
}

/// 注册并批准 Marketplace Client（status=active + 全部 marketplace scope +
/// webhook URL + webhook secret 轮换）。
pub async fn create_marketplace_client(
    pool: &DatabasePool,
    oauth_client_id: &str,
    owner_user_id: &str,
    fee_bps: i64,
    webhook_url: Option<&str>,
) -> MarketplaceClient {
    let admin_id = insert_user(pool, "admin").await;
    let scopes: Vec<Value> = clients::ALL_SCOPES
        .iter()
        .map(|s| {
            json!({
                "scope": s,
                "status": "approved",
                "limits": if *s == "marketplace.checkout.create" {
                    json!({"max_amount_per_transaction": 100000, "max_amount_daily": 300000, "max_purchases_daily": 10})
                } else {
                    json!({})
                }
            })
        })
        .collect();
    let body = json!({
        "name": "Merchant Corp",
        "owner_user_id": owner_user_id,
        "terms_url": "https://merchant.example/terms",
        "privacy_url": "https://merchant.example/privacy",
        "webhook_url": webhook_url,
        "redirect_uris": ["https://merchant.example/cb"],
        "fee_bps": fee_bps,
        "status": "active",
        "scopes": scopes,
    });
    let client = clients::upsert_client(
        pool,
        oauth_client_id,
        &body,
        1,
        &admin_id,
        "admin",
        now_millis(),
    )
    .await
    .unwrap();
    if webhook_url.is_some() {
        clients::rotate_webhook_secret(
            pool,
            &client.client_id,
            &admin_id,
            "test setup",
            WEBHOOK_MASTER_KEY,
            now_millis(),
        )
        .await
        .unwrap();
    }
    client
}

/// 服务认证（Confidential Client secret）。
pub async fn service_auth(
    pool: &DatabasePool,
    client_id: &str,
    secret: &str,
    scope: &str,
) -> ServicePrincipal {
    clients::service_authenticate(pool, client_id, secret, scope)
        .await
        .unwrap()
}

/// 服务端登记并激活 Offer（单价 amount、库存、fee 由 Client 配置）。
pub async fn create_active_offer(
    pool: &DatabasePool,
    principal: &ServicePrincipal,
    external_id: &str,
    amount: i64,
    stock: Option<i64>,
) -> OfferRow {
    let stock_policy = if stock.is_some() {
        "finite"
    } else {
        "unlimited"
    };
    let body = json!({
        "external_offer_id": external_id,
        "title": format!("Gift {external_id}"),
        "description": "test offer",
        "currency_id": CURRENCY_COIN,
        "unit_amount": amount,
        "quantity_min": 1,
        "quantity_max": 1,
        "stock_policy": stock_policy,
        "stock_remaining": stock,
    });
    let offer = offers::create_offer(pool, principal, &body, now_millis())
        .await
        .unwrap();
    let activate = json!({
        "external_offer_id": offer.external_offer_id,
        "title": offer.title,
        "description": offer.description_safe,
        "currency_id": offer.currency_id,
        "unit_amount": offer.amount,
        "quantity_min": offer.quantity_min,
        "quantity_max": offer.quantity_max,
        "stock_policy": offer.stock_policy,
        "stock_remaining": offer.stock_remaining,
        "status": "active",
    });
    offers::update_offer(pool, principal, &offer.id, 1, &activate, now_millis())
        .await
        .unwrap()
}

/// 完整购买：创建 Intent + confirm（返回 purchase JSON）。
pub async fn buy_flow(
    pool: &DatabasePool,
    user_id: &str,
    client: &MarketplaceClient,
    offer: &OfferRow,
    quantity: i64,
) -> Value {
    let intent = bblbb_backend::marketplace::checkout::create_intent(
        pool,
        user_id,
        &client.client_id,
        &offer.id,
        offer.version,
        &format!("order-{}", uuid::Uuid::now_v7().simple()),
        quantity,
        &format!("idem-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap();
    let intent_id = intent["intent_id"].as_str().unwrap().to_string();
    let version = intent["version"].as_i64().unwrap();
    bblbb_backend::marketplace::checkout::confirm_intent(
        pool,
        user_id,
        &intent_id,
        version,
        &format!("confirm-{}", uuid::Uuid::now_v7().simple()),
    )
    .await
    .unwrap()
}

/// 读取 Webhook 明文 Secret（测试内部解密）。
pub fn webhook_secret_of(pool: &DatabasePool, client: &MarketplaceClient) -> String {
    let stored = client.webhook_secret_hash.as_deref().unwrap();
    let _ = pool;
    webhooks::decrypt_webhook_secret(WEBHOOK_MASTER_KEY, stored).unwrap()
}
