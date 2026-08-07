//! M12-CHECKOUT：user-bound Checkout Intent 与原子购买。
//!
//! 流程（docs/MARKETPLACE.md §4/§5，docs/MARKETPLACE-ACCOUNTING.md §3/§4）：
//! 1. 用户（Session）为已批准 Client 创建 5 分钟 TTL 的 Checkout Intent；
//!    请求体只接受 `client_id/offer_id/expected_offer_version/
//!    merchant_order_id/quantity`——user_id/amount/currency/merchant/balance
//!    全部由服务端从已批准报价快照派生；
//! 2. 托管确认页（Session + CSRF）显示准确金额/余额变化/授权期限；
//! 3. confirm 重读 Client、Scope、Offer、库存、用户状态、限额与 Intent expiry，
//!    在一个数据库事务内固定锁序原子执行；
//! 4. 买方扣款走不可变账本；商户 pending 入账 + 平台费同一事务；
//!    任一步失败整体回滚；响应只在提交后返回。
//!
//! 锁顺序（固定）：idempotency operation → checkout intent → offer/stock →
//! 买方 point account → 商户账户 → 平台费账户。
//! 幂等：create/confirm 强制 `Idempotency-Key`；同 key+摘要重放原结果，
//! 不同摘要 409（复用 `crate::idempotency`）。
//!
//! 认证决策（v1，M12 设计约束 #1）：OIDC scope 白名单冻结为
//! openid/profile/email（M11-CONSENT-06），不存在可用的 user-bound
//! marketplace.* Access Token，因此 Intent 创建使用 Session 认证（AuthSession
//! 绑定当前用户）；`client_id` 来自请求体，金额/用户/收款方全部服务端派生。
//! confirm 使用 Session + CSRF + intent/user/client 一致性校验
//! （`checkout_user_mismatch` 403 / `checkout_interaction_invalid` 409）。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::economy::ledger::service as ledger;
use crate::economy::ledger::service::{LedgerCommand, LedgerKind};
use crate::events::types::MARKETPLACE_PURCHASE_SUCCEEDED;
use crate::idempotency::{self, FailureCachePolicy, IdempotencyKey, IdempotencyOutcome};
use crate::marketplace::balance::MerchantAccountRow;
use crate::marketplace::clients;
use crate::marketplace::offers::{get_offer, OfferRow};
use crate::marketplace::webhooks;
use crate::marketplace::{now_millis, MarketplaceError, INTENT_TTL_MS};

/// Checkout Intent 行。
#[derive(Debug, Clone)]
pub struct IntentRow {
    pub id: String,
    pub client_id: String,
    pub user_id: String,
    pub offer_id: String,
    pub offer_version: i64,
    pub quantity: i64,
    pub amount: i64,
    pub fee_refundable: bool,
    pub currency_id: String,
    pub merchant_order_id: String,
    pub request_hash: String,
    pub expires_at: i64,
    pub status: String,
    pub consumed_at: Option<i64>,
    pub version: i64,
    pub idempotency_scope: String,
    pub idempotency_key: String,
    pub created_at: i64,
}

/// Purchase 行。
#[derive(Debug, Clone)]
pub struct PurchaseRow {
    pub id: String,
    pub intent_id: String,
    pub client_id: String,
    pub user_id: String,
    pub offer_id: String,
    pub offer_version: i64,
    pub quantity: i64,
    pub amount: i64,
    pub fee_amount: i64,
    pub merchant_net: i64,
    pub currency_id: String,
    pub status: String,
    pub refunded_amount: i64,
    pub point_operation_id: String,
    pub merchant_operation_id: String,
    pub fee_operation_id: Option<String>,
    pub merchant_order_id: String,
    pub created_at: i64,
    pub updated_at: i64,
}

const INTENT_COLUMNS: &str = "id, client_id, user_id, offer_id, offer_version, quantity, amount, \
     fee_refundable, currency_id, merchant_order_id, request_hash, expires_at, status, consumed_at, \
     version, idempotency_scope, idempotency_key, created_at";

const PURCHASE_COLUMNS: &str =
    "id, intent_id, client_id, user_id, offer_id, offer_version, quantity, \
     amount, fee_amount, merchant_net, currency_id, status, refunded_amount, point_operation_id, \
     merchant_operation_id, fee_operation_id, merchant_order_id, created_at, updated_at";

fn intent_from_sqlite(row: &sqlx::sqlite::SqliteRow) -> IntentRow {
    IntentRow {
        id: row.get("id"),
        client_id: row.get("client_id"),
        user_id: row.get("user_id"),
        offer_id: row.get("offer_id"),
        offer_version: row.get("offer_version"),
        quantity: row.get("quantity"),
        amount: row.get("amount"),
        fee_refundable: row.get::<i64, _>("fee_refundable") != 0,
        currency_id: row.get("currency_id"),
        merchant_order_id: row.get("merchant_order_id"),
        request_hash: row.get("request_hash"),
        expires_at: row.get("expires_at"),
        status: row.get("status"),
        consumed_at: row.get("consumed_at"),
        version: row.get("version"),
        idempotency_scope: row.get("idempotency_scope"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

fn intent_from_mysql(row: &sqlx::mysql::MySqlRow) -> IntentRow {
    IntentRow {
        id: row.get("id"),
        client_id: row.get("client_id"),
        user_id: row.get("user_id"),
        offer_id: row.get("offer_id"),
        offer_version: row.get("offer_version"),
        quantity: row.get("quantity"),
        amount: row.get("amount"),
        fee_refundable: row.get::<i64, _>("fee_refundable") != 0,
        currency_id: row.get("currency_id"),
        merchant_order_id: row.get("merchant_order_id"),
        request_hash: row.get("request_hash"),
        expires_at: row.get("expires_at"),
        status: row.get("status"),
        consumed_at: row.get("consumed_at"),
        version: row.get("version"),
        idempotency_scope: row.get("idempotency_scope"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
    }
}

fn purchase_from_sqlite(row: &sqlx::sqlite::SqliteRow) -> PurchaseRow {
    PurchaseRow {
        id: row.get("id"),
        intent_id: row.get("intent_id"),
        client_id: row.get("client_id"),
        user_id: row.get("user_id"),
        offer_id: row.get("offer_id"),
        offer_version: row.get("offer_version"),
        quantity: row.get("quantity"),
        amount: row.get("amount"),
        fee_amount: row.get("fee_amount"),
        merchant_net: row.get("merchant_net"),
        currency_id: row.get("currency_id"),
        status: row.get("status"),
        refunded_amount: row.get("refunded_amount"),
        point_operation_id: row.get("point_operation_id"),
        merchant_operation_id: row.get("merchant_operation_id"),
        fee_operation_id: row.get("fee_operation_id"),
        merchant_order_id: row.get("merchant_order_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn purchase_from_mysql(row: &sqlx::mysql::MySqlRow) -> PurchaseRow {
    PurchaseRow {
        id: row.get("id"),
        intent_id: row.get("intent_id"),
        client_id: row.get("client_id"),
        user_id: row.get("user_id"),
        offer_id: row.get("offer_id"),
        offer_version: row.get("offer_version"),
        quantity: row.get("quantity"),
        amount: row.get("amount"),
        fee_amount: row.get("fee_amount"),
        merchant_net: row.get("merchant_net"),
        currency_id: row.get("currency_id"),
        status: row.get("status"),
        refunded_amount: row.get("refunded_amount"),
        point_operation_id: row.get("point_operation_id"),
        merchant_operation_id: row.get("merchant_operation_id"),
        fee_operation_id: row.get("fee_operation_id"),
        merchant_order_id: row.get("merchant_order_id"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

/// 外部读取 Intent（确认页/查询）。不暴露幂等键。
pub fn intent_view_json(i: &IntentRow) -> Value {
    json!({
        "intent_id": i.id,
        "client_id": i.client_id,
        "offer_id": i.offer_id,
        "offer_version": i.offer_version,
        "quantity": i.quantity,
        "amount": i.amount,
        "currency_id": i.currency_id,
        "merchant_order_id": i.merchant_order_id,
        "expires_at": i.expires_at,
        "status": i.status,
        "version": i.version,
        "created_at": i.created_at,
    })
}

pub fn purchase_json(p: &PurchaseRow) -> Value {
    json!({
        "id": p.id,
        "intent_id": p.intent_id,
        "client_id": p.client_id,
        "user_id": p.user_id,
        "offer_id": p.offer_id,
        "offer_version": p.offer_version,
        "quantity": p.quantity,
        "amount": p.amount,
        "fee_amount": p.fee_amount,
        "merchant_net": p.merchant_net,
        "currency_id": p.currency_id,
        "status": p.status,
        "refunded_amount": p.refunded_amount,
        "merchant_order_id": p.merchant_order_id,
        "created_at": p.created_at,
        "updated_at": p.updated_at,
    })
}

// ─────────────────────────── 加载 ───────────────────────────

pub async fn load_intent(
    pool: &DatabasePool,
    id: &str,
) -> Result<Option<IntentRow>, MarketplaceError> {
    let sql = format!("SELECT {INTENT_COLUMNS} FROM checkout_intents WHERE id = ?");
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| intent_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| intent_from_mysql(&r)),
    };
    Ok(row)
}

pub async fn get_purchase(
    pool: &DatabasePool,
    id: &str,
) -> Result<Option<PurchaseRow>, MarketplaceError> {
    let sql = format!("SELECT {PURCHASE_COLUMNS} FROM purchases WHERE id = ?");
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| purchase_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| purchase_from_mysql(&r)),
    };
    Ok(row)
}

/// 按商户订单号查 Purchase（Client 服务端对账）。
pub async fn get_purchase_by_merchant_order(
    pool: &DatabasePool,
    client_id: &str,
    merchant_order_id: &str,
) -> Result<Option<PurchaseRow>, MarketplaceError> {
    let sql = format!(
        "SELECT {PURCHASE_COLUMNS} FROM purchases WHERE client_id = ? AND merchant_order_id = ?"
    );
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(client_id)
            .bind(merchant_order_id)
            .fetch_optional(p)
            .await?
            .map(|r| purchase_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(client_id)
            .bind(merchant_order_id)
            .fetch_optional(p)
            .await?
            .map(|r| purchase_from_mysql(&r)),
    };
    Ok(row)
}

/// 列表查询（用户本人 或 Client 服务端，互斥过滤）。
pub async fn list_purchases(
    pool: &DatabasePool,
    user_id: Option<&str>,
    client_id: Option<&str>,
    after: Option<&str>,
    limit: i64,
) -> Result<Vec<Value>, MarketplaceError> {
    let limit = limit.clamp(1, 100);
    let (clause, _n_binds) = match (user_id, client_id) {
        (Some(_), None) => ("WHERE user_id = ? AND id > ?", 2),
        (None, Some(_)) => ("WHERE client_id = ? AND id > ?", 2),
        _ => {
            return Err(MarketplaceError::Invalid(
                "user_id or client_id required".into(),
            ))
        }
    };
    let sql = format!("SELECT {PURCHASE_COLUMNS} FROM purchases {clause} ORDER BY id ASC LIMIT ?");
    let rows: Vec<Value> = match pool {
        Either::Left(p) => {
            let q = sqlx::query(&sql).bind(user_id.or(client_id).unwrap_or(""));
            let r = q
                .bind(after.unwrap_or(""))
                .bind(limit + 1)
                .fetch_all(p)
                .await?;
            r.iter()
                .map(|row| purchase_json(&purchase_from_sqlite(row)))
                .collect()
        }
        Either::Right(p) => {
            let q = sqlx::query(&sql).bind(user_id.or(client_id).unwrap_or(""));
            let r = q
                .bind(after.unwrap_or(""))
                .bind(limit + 1)
                .fetch_all(p)
                .await?;
            r.iter()
                .map(|row| purchase_json(&purchase_from_mysql(row)))
                .collect()
        }
    };
    Ok(rows.into_iter().take(limit as usize).collect())
}

// ─────────────────────────── 创建 Intent ───────────────────────────

/// POST /marketplace/checkout-intents：Session 用户为已批准 Client 创建
/// 短 TTL Intent。请求体字段全部服务端校验/派生；不接受 user_id/amount/
/// currency/merchant/balance。
#[allow(clippy::too_many_arguments)]
pub async fn create_intent(
    pool: &DatabasePool,
    user_id: &str,
    client_key: &str,
    offer_id: &str,
    expected_offer_version: i64,
    merchant_order_id: &str,
    quantity: i64,
    idempotency_key: &str,
) -> Result<Value, MarketplaceError> {
    if merchant_order_id.is_empty() || merchant_order_id.len() > 128 {
        return Err(MarketplaceError::Invalid(
            "merchant_order_id required (<=128 chars)".into(),
        ));
    }
    if !(16..=200).contains(&idempotency_key.len()) {
        return Err(MarketplaceError::Invalid(
            "Idempotency-Key must be 16..=200 chars".into(),
        ));
    }
    let now = now_millis();

    // 预检（幂等记录之外）：Client 与 Offer 必须可下单。
    let client = clients::fetch_client_by_client_id(pool, client_key)
        .await?
        .ok_or_else(|| MarketplaceError::InvalidClient("unknown client".into()))?;
    if !client.allows_new_sales() {
        return Err(MarketplaceError::MarketplaceDisabled(
            "marketplace client is not active".into(),
        ));
    }
    if !clients::scope_approved(pool, &client.id, "marketplace.checkout.create").await?
        || !clients::scope_approved(pool, &client.id, "marketplace.purchase").await?
    {
        return Err(MarketplaceError::MarketplaceDisabled(
            "checkout scopes not approved for this client".into(),
        ));
    }
    let offer = get_offer(pool, offer_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("offer not found".into()))?;
    if offer.client_id != client.id {
        return Err(MarketplaceError::Forbidden(
            "offer belongs to another client".into(),
        ));
    }
    if !offer.is_active() {
        return Err(MarketplaceError::OfferVersionChanged);
    }
    if offer.version != expected_offer_version {
        return Err(MarketplaceError::OfferVersionChanged);
    }
    if !(offer.quantity_min..=offer.quantity_max).contains(&quantity) {
        return Err(MarketplaceError::Invalid(format!(
            "quantity must be in {}..={}",
            offer.quantity_min, offer.quantity_max
        )));
    }
    let total = offer
        .amount
        .checked_mul(quantity)
        .ok_or_else(|| MarketplaceError::Invalid("amount overflow".into()))?;

    // 单笔限额在 Intent 创建即校验（MARKETPLACE.md §4 步骤 2）；日累计在
    // confirm 时按 Purchase 聚合校验。
    let limits = scope_limits(pool, &client.id).await?;
    if let Some(per_tx) = limits.0 {
        if total > per_tx {
            return Err(MarketplaceError::DailyLimitExceeded);
        }
    }

    let request_hash = intent_request_hash(
        user_id,
        client_key,
        offer_id,
        expected_offer_version,
        merchant_order_id,
        quantity,
    );
    // scope 固定（≤50 字符约束）；user/client 编码进 key 保证唯一。
    let idem = IdempotencyKey::new(
        "marketplace.intent",
        format!("{user_id}.{client_key}.{idempotency_key}"),
    )
    .map_err(|e| MarketplaceError::Invalid(e.to_string()))?;

    match idempotency::begin_or_replay(
        pool,
        &idem,
        &request_hash,
        24 * 3600 * 1000,
        FailureCachePolicy::Cache,
    )
    .await
    .map_err(|e| MarketplaceError::Db(e.to_string()))?
    {
        IdempotencyOutcome::Created { record_id } => {
            let intent_id = uuid::Uuid::now_v7().to_string();
            let expires_at = now + INTENT_TTL_MS;
            let rows = match pool {
                Either::Left(p) => sqlx::query(
                    "INSERT INTO checkout_intents
                     (id, client_id, user_id, offer_id, offer_version, quantity, amount, fee_refundable,
                      currency_id, merchant_order_id, request_hash, expires_at, status, consumed_at, version,
                      idempotency_scope, idempotency_key, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, 1, ?, ?, ?)",
                )
                .bind(&intent_id)
                .bind(&client.id)
                .bind(user_id)
                .bind(&offer.id)
                .bind(offer.version)
                .bind(quantity)
                .bind(total)
                .bind(true)
                .bind(&offer.currency_id)
                .bind(merchant_order_id)
                .bind(&request_hash)
                .bind(expires_at)
                .bind(&idem.scope)
                .bind(&idem.key)
                .bind(now)
                .execute(p)
                .await
                .map(|r| r.rows_affected()),
                Either::Right(p) => sqlx::query(
                    "INSERT INTO checkout_intents
                     (id, client_id, user_id, offer_id, offer_version, quantity, amount, fee_refundable,
                      currency_id, merchant_order_id, request_hash, expires_at, status, consumed_at, version,
                      idempotency_scope, idempotency_key, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', NULL, 1, ?, ?, ?)",
                )
                .bind(&intent_id)
                .bind(&client.id)
                .bind(user_id)
                .bind(&offer.id)
                .bind(offer.version)
                .bind(quantity)
                .bind(total)
                .bind(true)
                .bind(&offer.currency_id)
                .bind(merchant_order_id)
                .bind(&request_hash)
                .bind(expires_at)
                .bind(&idem.scope)
                .bind(&idem.key)
                .bind(now)
                .execute(p)
                .await
                .map(|r| r.rows_affected()),
            };
            if let Err(err) = rows {
                if is_unique_violation(&err) {
                    let _ = idempotency::mark_failed(pool, &record_id).await;
                    return Err(MarketplaceError::IdempotencyConflict);
                }
                let _ = idempotency::mark_failed(pool, &record_id).await;
                return Err(MarketplaceError::from(err));
            }
            let affected = rows.unwrap_or(0);
            if affected != 1 {
                let _ = idempotency::mark_failed(pool, &record_id).await;
                return Err(MarketplaceError::IdempotencyConflict);
            }
            let _ = idempotency::complete(pool, &record_id, &intent_id).await;
            let intent = load_intent(pool, &intent_id)
                .await?
                .ok_or_else(|| MarketplaceError::NotFound("intent".into()))?;
            Ok(intent_view_json(&intent))
        }
        IdempotencyOutcome::Replay { response_reference } => {
            if let Some(intent_id) = response_reference {
                if let Some(intent) = load_intent(pool, &intent_id).await? {
                    return Ok(intent_view_json(&intent));
                }
            }
            Err(MarketplaceError::IdempotencyConflict)
        }
        IdempotencyOutcome::InProgress => Err(MarketplaceError::Invalid(
            "intent creation already in progress for this key".into(),
        )),
        IdempotencyOutcome::Conflict => Err(MarketplaceError::IdempotencyConflict),
        IdempotencyOutcome::Failed { .. } => Err(MarketplaceError::IdempotencyConflict),
    }
}

fn intent_request_hash(
    user_id: &str,
    client_key: &str,
    offer_id: &str,
    expected_offer_version: i64,
    merchant_order_id: &str,
    quantity: i64,
) -> String {
    use sha2::{Digest, Sha256};
    let canonical = format!(
        "{user_id}|{client_key}|{offer_id}|{expected_offer_version}|{merchant_order_id}|{quantity}"
    );
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(err, sqlx::Error::Database(db) if db.is_unique_violation())
}

// ─────────────────────────── Confirm（原子购买） ───────────────────────────

/// 确认页投影（Session 用户本人可读）：显示商户/商品/数量/准确金额/余额
/// 变化/Scope/授权期限。Intent 本身不进 URL。
pub async fn intent_checkout_view(
    pool: &DatabasePool,
    viewer_user_id: &str,
    interaction_id: &str,
    now: i64,
) -> Result<Value, MarketplaceError> {
    let intent = load_intent(pool, interaction_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("checkout intent not found".into()))?;
    if intent.user_id != viewer_user_id {
        return Err(MarketplaceError::CheckoutUserMismatch);
    }
    if intent.status != "pending" {
        return Err(match intent.status.as_str() {
            "consumed" => MarketplaceError::CheckoutIntentConsumed,
            "denied" => MarketplaceError::CheckoutInteractionInvalid,
            _ => MarketplaceError::CheckoutIntentExpired,
        });
    }
    if intent.expires_at < now {
        return Err(MarketplaceError::CheckoutIntentExpired);
    }
    let client = clients::fetch_client_by_internal_id(pool, &intent.client_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("marketplace client".into()))?;
    let offer = get_offer(pool, &intent.offer_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("offer".into()))?;
    let account = ledger::get_account(pool, viewer_user_id, &intent.currency_id).await;
    let (balance, frozen) = match account {
        Ok(a) => (a.balance, a.frozen_balance),
        Err(ledger::LedgerError::NotFound(_)) => (0, 0),
        Err(e) => return Err(MarketplaceError::from(e)),
    };
    Ok(json!({
        "intent_id": intent.id,
        "interaction_id": intent.id,
        "version": intent.version,
        "client_id": client.client_id,
        "merchant_name": client.name,
        "terms_url": client.terms_url,
        "privacy_url": client.privacy_url,
        "offer_id": offer.id,
        "offer_title": offer.title,
        "offer_description": offer.description_safe,
        "offer_version": offer.version,
        "quantity": intent.quantity,
        "amount": intent.amount,
        "currency_id": intent.currency_id,
        "fee_bps": client.fee_bps,
        "fee_refundable": intent.fee_refundable,
        "scopes": ["marketplace.checkout.create", "marketplace.purchase"],
        "balance": balance,
        "frozen_balance": frozen,
        "balance_after": balance - intent.amount,
        "expires_at": intent.expires_at,
        "status": intent.status,
        "created_at": intent.created_at,
    }))
}

/// 取消 Intent（deny decision；幂等）。
pub async fn deny_intent(
    pool: &DatabasePool,
    user_id: &str,
    intent_id: &str,
    _now: i64,
) -> Result<Value, MarketplaceError> {
    let intent = load_intent(pool, intent_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("checkout intent not found".into()))?;
    if intent.user_id != user_id {
        return Err(MarketplaceError::CheckoutUserMismatch);
    }
    if intent.status == "consumed" {
        return Err(MarketplaceError::CheckoutIntentConsumed);
    }
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE checkout_intents SET status = 'denied', version = version + 1 WHERE id = ? AND status = 'pending'",
        )
        .bind(intent_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE checkout_intents SET status = 'denied', version = version + 1 WHERE id = ? AND status = 'pending'",
        )
        .bind(intent_id)
        .execute(p)
        .await?
        .rows_affected(),
    };
    let _ = rows;
    let updated = load_intent(pool, intent_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("intent".into()))?;
    Ok(json!({ "intent_id": intent_id, "status": updated.status, "version": updated.version }))
}

/// POST /checkout-intents/{id}/confirm：Session + CSRF + 一致性校验 +
/// 固定锁序原子购买。响应只在事务提交后返回。
///
/// `idempotency_key` 复用同一 HTTP 幂等键：同 key+摘要重放原 Purchase；
/// 不同摘要 409。Intent 的一次性由 `status='consumed'` 条件更新保证。
pub async fn confirm_intent(
    pool: &DatabasePool,
    user_id: &str,
    interaction_id: &str,
    expected_intent_version: i64,
    idempotency_key: &str,
) -> Result<Value, MarketplaceError> {
    if !(16..=200).contains(&idempotency_key.len()) {
        return Err(MarketplaceError::Invalid(
            "Idempotency-Key must be 16..=200 chars".into(),
        ));
    }
    let now = now_millis();
    let request_hash = confirm_request_hash(user_id, interaction_id, expected_intent_version);
    let idem = IdempotencyKey::new(
        "marketplace.confirm",
        format!("{user_id}.{idempotency_key}"),
    )
    .map_err(|e| MarketplaceError::Invalid(e.to_string()))?;

    match idempotency::begin_or_replay(
        pool,
        &idem,
        &request_hash,
        24 * 3600 * 1000,
        FailureCachePolicy::Cache,
    )
    .await
    .map_err(|e| MarketplaceError::Db(e.to_string()))?
    {
        IdempotencyOutcome::Created { record_id } => {
            let result =
                execute_purchase(pool, user_id, interaction_id, expected_intent_version, now).await;
            match result {
                Ok(view) => {
                    let purchase_id = view["id"].as_str().unwrap_or("").to_string();
                    let _ = idempotency::complete(pool, &record_id, &purchase_id).await;
                    Ok(view)
                }
                Err(e) => {
                    let _ = idempotency::mark_failed(pool, &record_id).await;
                    Err(e)
                }
            }
        }
        IdempotencyOutcome::Replay { response_reference } => {
            if let Some(purchase_id) = response_reference {
                if let Some(purchase) = get_purchase(pool, &purchase_id).await? {
                    return Ok(purchase_json(&purchase));
                }
            }
            Err(MarketplaceError::IdempotencyConflict)
        }
        IdempotencyOutcome::InProgress => Err(MarketplaceError::Invalid(
            "confirm already in progress for this key".into(),
        )),
        IdempotencyOutcome::Conflict => Err(MarketplaceError::IdempotencyConflict),
        IdempotencyOutcome::Failed { .. } => Err(MarketplaceError::IdempotencyConflict),
    }
}

fn confirm_request_hash(user_id: &str, interaction_id: &str, expected_version: i64) -> String {
    use sha2::{Digest, Sha256};
    let canonical = format!("{user_id}|{interaction_id}|{expected_version}");
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// 原子购买事务本体（SQLite `BEGIN IMMEDIATE` / MySQL 事务 + 固定锁顺序）。
#[allow(clippy::explicit_auto_deref)]
async fn execute_purchase(
    pool: &DatabasePool,
    user_id: &str,
    interaction_id: &str,
    expected_intent_version: i64,
    now: i64,
) -> Result<Value, MarketplaceError> {
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, MarketplaceError> = async {
                let intent = load_intent_sqlite(&mut conn, interaction_id).await?;
                validate_confirm_checks(&intent, user_id, interaction_id, expected_intent_version, now)?;
                let (client, offer) = load_client_and_offer_sqlite(&mut conn, &intent).await?;
                let limits = scope_limits_sqlite(&mut conn, &client.id).await?;
                validate_limits(&limits, &intent, pool, user_id).await?;
                check_stock_sqlite(&mut conn, &offer, intent.quantity).await?;
                check_user_status_sqlite(&mut conn, user_id).await?;
                ensure_ledger_users_sqlite(&mut conn, &intent.client_id, now).await?;

                let fee = intent.amount * client.fee_bps / 10_000;
                let merchant_net = intent.amount - fee;
                let purchase_id = uuid::Uuid::now_v7().to_string();
                let ledger_scope = format!("marketplace.purchase.{purchase_id}");

                // 买方扣款（不可变账本，事务内）。
                let buyer_cmd = LedgerCommand {
                    idempotency_scope: ledger_scope.clone(),
                    idempotency_key: "buyer".to_string(),
                    kind: LedgerKind::Consume,
                    actor_id: Some(user_id.to_string()),
                    user_id: user_id.to_string(),
                    currency_id: intent.currency_id.clone(),
                    delta_balance: -intent.amount,
                    delta_frozen: 0,
                    source_type: Some("marketplace_purchase".to_string()),
                    source_id: Some(purchase_id.clone()),
                    memo: format!("marketplace purchase {purchase_id}"),
                    reverses_operation_id: None,
                };
                let buyer_op = ledger::apply_operation_in_sqlite_tx(&mut conn, buyer_cmd, now).await?;

                // 商户入账（合成账户 merchant:{client_id}；同事务）。
                let merchant_cmd = LedgerCommand {
                    idempotency_scope: ledger_scope.clone(),
                    idempotency_key: "merchant".to_string(),
                    kind: LedgerKind::Award,
                    actor_id: None,
                    user_id: crate::marketplace::merchant_ledger_user(&intent.client_id),
                    currency_id: intent.currency_id.clone(),
                    delta_balance: merchant_net,
                    delta_frozen: 0,
                    source_type: Some("marketplace_purchase".to_string()),
                    source_id: Some(purchase_id.clone()),
                    memo: format!("marketplace merchant credit {purchase_id}"),
                    reverses_operation_id: None,
                };
                let merchant_op = ledger::apply_operation_in_sqlite_tx(&mut conn, merchant_cmd, now).await?;

                // 平台费（fee > 0 时；同一 operation 组）。
                let fee_op = if fee > 0 {
                    let fee_cmd = LedgerCommand {
                        idempotency_scope: ledger_scope.clone(),
                        idempotency_key: "fee".to_string(),
                        kind: LedgerKind::Award,
                        actor_id: None,
                        user_id: crate::marketplace::fee_ledger_user().to_string(),
                        currency_id: intent.currency_id.clone(),
                        delta_balance: fee,
                        delta_frozen: 0,
                        source_type: Some("marketplace_purchase".to_string()),
                        source_id: Some(purchase_id.clone()),
                        memo: format!("marketplace platform fee {purchase_id}"),
                        reverses_operation_id: None,
                    };
                    Some(ledger::apply_operation_in_sqlite_tx(&mut conn, fee_cmd, now).await?)
                } else {
                    None
                };

                // 商户运营余额：pending += merchant_net。
                credit_pending_sqlite(&mut conn, &intent.client_id, merchant_net, now).await?;

                // 消费 Intent（条件更新；并发只允许一次成功）。
                let consumed = sqlx::query(
                    "UPDATE checkout_intents SET status = 'consumed', consumed_at = ?, version = version + 1
                     WHERE id = ? AND status = 'pending'",
                )
                .bind(now)
                .bind(interaction_id)
                .execute(&mut *conn)
                .await?
                .rows_affected();
                if consumed != 1 {
                    return Err(MarketplaceError::CheckoutIntentConsumed);
                }

                // 扣库存（条件更新，不超卖）。
                if offer.stock_policy == "finite" {
                    let affected = sqlx::query(
                        "UPDATE offers SET stock_remaining = stock_remaining - ?, updated_at = ?
                         WHERE id = ? AND stock_remaining >= ?",
                    )
                    .bind(intent.quantity)
                    .bind(now)
                    .bind(&offer.id)
                    .bind(intent.quantity)
                    .execute(&mut *conn)
                    .await?
                    .rows_affected();
                    if affected != 1 {
                        return Err(MarketplaceError::OutOfStock);
                    }
                }

                // 写 Purchase。
                let insert = sqlx::query(
                    "INSERT INTO purchases
                     (id, intent_id, client_id, user_id, offer_id, offer_version, quantity, amount,
                      fee_amount, merchant_net, currency_id, status, refunded_amount, point_operation_id,
                      merchant_operation_id, fee_operation_id, merchant_order_id, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'succeeded', 0, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&purchase_id)
                .bind(&intent.id)
                .bind(&intent.client_id)
                .bind(user_id)
                .bind(&intent.offer_id)
                .bind(intent.offer_version)
                .bind(intent.quantity)
                .bind(intent.amount)
                .bind(fee)
                .bind(merchant_net)
                .bind(&intent.currency_id)
                .bind(&buyer_op.operation_id)
                .bind(&merchant_op.operation_id)
                .bind(fee_op.as_ref().map(|o| o.operation_id.clone()))
                .bind(&intent.merchant_order_id)
                .bind(now)
                .bind(now)
                .execute(&mut *conn)
                .await;
                if let Err(err) = insert {
                    if is_unique_violation(&err) {
                        return Err(MarketplaceError::CheckoutIntentConsumed);
                    }
                    return Err(MarketplaceError::from(err));
                }

                AuditEntry::user_action(user_id, "marketplace.purchase")
                    .with_target("client", &client.client_id)
                    .with_target("purchase", &purchase_id)
                    .with_reason("marketplace checkout")
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_sqlite(&mut conn)
                    .await
                    .map_err(MarketplaceError::from)?;

                // Outbox（同事务）：购买成功事件。
                let outbox_event_id = enqueue_outbox_sqlite(
                    &mut conn,
                    MARKETPLACE_PURCHASE_SUCCEEDED,
                    json!({
                        "purchase_id": purchase_id,
                        "client_id": client.client_id,
                        "user_id": user_id,
                        "amount": intent.amount,
                        "currency_id": intent.currency_id,
                        "merchant_order_id": intent.merchant_order_id,
                        "status": "succeeded",
                    }),
                )
                .await?;

                // Webhook 投递记录（post-commit 投递；payload 最小化）。
                let hook_payload = webhooks::minimal_payload(
                    &outbox_event_id,
                    MARKETPLACE_PURCHASE_SUCCEEDED,
                    &json!({
                        "client_id": client.client_id,
                        "purchase_id": purchase_id,
                        "status": "succeeded",
                        "amount": intent.amount,
                        "currency_id": intent.currency_id,
                        "merchant_order_id": intent.merchant_order_id,
                    }),
                );
                webhooks::register_delivery_sqlite(
                    &mut conn,
                    &intent.client_id,
                    &outbox_event_id,
                    MARKETPLACE_PURCHASE_SUCCEEDED,
                    &hook_payload,
                    now,
                )
                .await?;

                let purchase = purchase_from_sqlite(
                    &sqlx::query(&format!("SELECT {PURCHASE_COLUMNS} FROM purchases WHERE id = ?"))
                        .bind(&purchase_id)
                        .fetch_one(&mut *conn)
                        .await?,
                );
                Ok(purchase_json(&purchase))
            }
            .await;
            match outcome {
                Ok(v) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let outcome: Result<Value, MarketplaceError> = async {
                let intent = load_intent_mysql(&mut tx, interaction_id).await?;
                validate_confirm_checks(&intent, user_id, interaction_id, expected_intent_version, now)?;
                let (client, offer) = load_client_and_offer_mysql(&mut tx, &intent).await?;
                let limits = scope_limits_mysql(&mut tx, &client.id).await?;
                validate_limits(&limits, &intent, pool, user_id).await?;
                check_stock_mysql(&mut tx, &offer, intent.quantity).await?;
                check_user_status_mysql(&mut tx, user_id).await?;
                ensure_ledger_users_mysql(&mut tx, &intent.client_id, now).await?;

                let fee = intent.amount * client.fee_bps / 10_000;
                let merchant_net = intent.amount - fee;
                let purchase_id = uuid::Uuid::now_v7().to_string();
                let ledger_scope = format!("marketplace.purchase.{purchase_id}");

                let buyer_cmd = LedgerCommand {
                    idempotency_scope: ledger_scope.clone(),
                    idempotency_key: "buyer".to_string(),
                    kind: LedgerKind::Consume,
                    actor_id: Some(user_id.to_string()),
                    user_id: user_id.to_string(),
                    currency_id: intent.currency_id.clone(),
                    delta_balance: -intent.amount,
                    delta_frozen: 0,
                    source_type: Some("marketplace_purchase".to_string()),
                    source_id: Some(purchase_id.clone()),
                    memo: format!("marketplace purchase {purchase_id}"),
                    reverses_operation_id: None,
                };
                let buyer_op = ledger::apply_operation_in_mysql_tx(&mut tx, buyer_cmd, now).await?;

                let merchant_cmd = LedgerCommand {
                    idempotency_scope: ledger_scope.clone(),
                    idempotency_key: "merchant".to_string(),
                    kind: LedgerKind::Award,
                    actor_id: None,
                    user_id: crate::marketplace::merchant_ledger_user(&intent.client_id),
                    currency_id: intent.currency_id.clone(),
                    delta_balance: merchant_net,
                    delta_frozen: 0,
                    source_type: Some("marketplace_purchase".to_string()),
                    source_id: Some(purchase_id.clone()),
                    memo: format!("marketplace merchant credit {purchase_id}"),
                    reverses_operation_id: None,
                };
                let merchant_op = ledger::apply_operation_in_mysql_tx(&mut tx, merchant_cmd, now).await?;

                let fee_op = if fee > 0 {
                    let fee_cmd = LedgerCommand {
                        idempotency_scope: ledger_scope.clone(),
                        idempotency_key: "fee".to_string(),
                        kind: LedgerKind::Award,
                        actor_id: None,
                        user_id: crate::marketplace::fee_ledger_user().to_string(),
                        currency_id: intent.currency_id.clone(),
                        delta_balance: fee,
                        delta_frozen: 0,
                        source_type: Some("marketplace_purchase".to_string()),
                        source_id: Some(purchase_id.clone()),
                        memo: format!("marketplace platform fee {purchase_id}"),
                        reverses_operation_id: None,
                    };
                    Some(ledger::apply_operation_in_mysql_tx(&mut tx, fee_cmd, now).await?)
                } else {
                    None
                };

                credit_pending_mysql(&mut tx, &intent.client_id, merchant_net, now).await?;

                let consumed = sqlx::query(
                    "UPDATE checkout_intents SET status = 'consumed', consumed_at = ?, version = version + 1
                     WHERE id = ? AND status = 'pending'",
                )
                .bind(now)
                .bind(interaction_id)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if consumed != 1 {
                    return Err(MarketplaceError::CheckoutIntentConsumed);
                }

                if offer.stock_policy == "finite" {
                    let affected = sqlx::query(
                        "UPDATE offers SET stock_remaining = stock_remaining - ?, updated_at = ?
                         WHERE id = ? AND stock_remaining >= ?",
                    )
                    .bind(intent.quantity)
                    .bind(now)
                    .bind(&offer.id)
                    .bind(intent.quantity)
                    .execute(&mut *tx)
                    .await?
                    .rows_affected();
                    if affected != 1 {
                        return Err(MarketplaceError::OutOfStock);
                    }
                }

                let insert = sqlx::query(
                    "INSERT INTO purchases
                     (id, intent_id, client_id, user_id, offer_id, offer_version, quantity, amount,
                      fee_amount, merchant_net, currency_id, status, refunded_amount, point_operation_id,
                      merchant_operation_id, fee_operation_id, merchant_order_id, created_at, updated_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'succeeded', 0, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&purchase_id)
                .bind(&intent.id)
                .bind(&intent.client_id)
                .bind(user_id)
                .bind(&intent.offer_id)
                .bind(intent.offer_version)
                .bind(intent.quantity)
                .bind(intent.amount)
                .bind(fee)
                .bind(merchant_net)
                .bind(&intent.currency_id)
                .bind(&buyer_op.operation_id)
                .bind(&merchant_op.operation_id)
                .bind(fee_op.as_ref().map(|o| o.operation_id.clone()))
                .bind(&intent.merchant_order_id)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await;
                if let Err(err) = insert {
                    if is_unique_violation(&err) {
                        return Err(MarketplaceError::CheckoutIntentConsumed);
                    }
                    return Err(MarketplaceError::from(err));
                }

                AuditEntry::user_action(user_id, "marketplace.purchase")
                    .with_target("client", &client.client_id)
                    .with_target("purchase", &purchase_id)
                    .with_reason("marketplace checkout")
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_mysql(&mut tx)
                    .await
                    .map_err(MarketplaceError::from)?;

                let outbox_event_id = enqueue_outbox_mysql(
                    &mut tx,
                    MARKETPLACE_PURCHASE_SUCCEEDED,
                    json!({
                        "purchase_id": purchase_id,
                        "client_id": client.client_id,
                        "user_id": user_id,
                        "amount": intent.amount,
                        "currency_id": intent.currency_id,
                        "merchant_order_id": intent.merchant_order_id,
                        "status": "succeeded",
                    }),
                )
                .await?;

                let hook_payload = webhooks::minimal_payload(
                    &outbox_event_id,
                    MARKETPLACE_PURCHASE_SUCCEEDED,
                    &json!({
                        "client_id": client.client_id,
                        "purchase_id": purchase_id,
                        "status": "succeeded",
                        "amount": intent.amount,
                        "currency_id": intent.currency_id,
                        "merchant_order_id": intent.merchant_order_id,
                    }),
                );
                webhooks::register_delivery_mysql(
                    &mut tx,
                    &intent.client_id,
                    &outbox_event_id,
                    MARKETPLACE_PURCHASE_SUCCEEDED,
                    &hook_payload,
                    now,
                )
                .await?;

                let purchase = purchase_from_mysql(
                    &sqlx::query(&format!("SELECT {PURCHASE_COLUMNS} FROM purchases WHERE id = ?"))
                        .bind(&purchase_id)
                        .fetch_one(&mut *tx)
                        .await?,
                );
                Ok(purchase_json(&purchase))
            }
            .await;
            match outcome {
                Ok(v) => {
                    tx.commit().await?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = tx.rollback().await;
                    Err(e)
                }
            }
        }
    }
}

/// confirm 前的一致性校验（共用）。
fn validate_confirm_checks(
    intent: &IntentRow,
    user_id: &str,
    interaction_id: &str,
    expected_intent_version: i64,
    now: i64,
) -> Result<(), MarketplaceError> {
    if interaction_id != intent.id {
        return Err(MarketplaceError::CheckoutInteractionInvalid);
    }
    match intent.status.as_str() {
        "pending" => {}
        "consumed" => return Err(MarketplaceError::CheckoutIntentConsumed),
        "denied" => return Err(MarketplaceError::CheckoutInteractionInvalid),
        _ => return Err(MarketplaceError::CheckoutIntentExpired),
    }
    if intent.expires_at < now {
        return Err(MarketplaceError::CheckoutIntentExpired);
    }
    if intent.user_id != user_id {
        return Err(MarketplaceError::CheckoutUserMismatch);
    }
    if intent.version != expected_intent_version {
        return Err(MarketplaceError::VersionConflict {
            expected: expected_intent_version,
            current: intent.version,
        });
    }
    Ok(())
}

/// 读取 scope 限额（max_amount_per_transaction / max_amount_daily /
/// max_purchases_daily；未配置 = 不限）。
fn limits_from_json(limits_json: &str) -> (Option<i64>, Option<i64>, Option<i64>) {
    let v: Value = serde_json::from_str(limits_json).unwrap_or_else(|_| json!({}));
    (
        v.get("max_amount_per_transaction").and_then(Value::as_i64),
        v.get("max_amount_daily").and_then(Value::as_i64),
        v.get("max_purchases_daily").and_then(Value::as_i64),
    )
}

/// 限额校验（读意图金额与用户今日累计）。
async fn validate_limits(
    limits: &(Option<i64>, Option<i64>, Option<i64>),
    intent: &IntentRow,
    pool: &DatabasePool,
    user_id: &str,
) -> Result<(), MarketplaceError> {
    let (per_tx, daily_amount, daily_count) = limits;
    if let Some(per_tx) = per_tx {
        if intent.amount > *per_tx {
            return Err(MarketplaceError::DailyLimitExceeded);
        }
    }
    if daily_amount.is_some() || daily_count.is_some() {
        let today_start = crate::outbox::now_millis() - (crate::outbox::now_millis() % 86_400_000);
        let spent: i64 = match pool {
            Either::Left(p) => {
                sqlx::query_scalar(
                    "SELECT COALESCE(SUM(amount),0) FROM purchases WHERE user_id = ? AND client_id = ? AND created_at >= ?",
                )
                .bind(user_id)
                .bind(&intent.client_id)
                .bind(today_start)
                .fetch_one(p)
                .await?
            }
            Either::Right(p) => {
                sqlx::query_scalar(
                    "SELECT COALESCE(SUM(amount),0) FROM purchases WHERE user_id = ? AND client_id = ? AND created_at >= ?",
                )
                .bind(user_id)
                .bind(&intent.client_id)
                .bind(today_start)
                .fetch_one(p)
                .await?
            }
        };
        if let Some(limit) = daily_amount {
            if spent + intent.amount > *limit {
                return Err(MarketplaceError::DailyLimitExceeded);
            }
        }
        if let Some(limit) = daily_count {
            let count: i64 = match pool {
                Either::Left(p) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM purchases WHERE user_id = ? AND client_id = ? AND created_at >= ?",
                    )
                    .bind(user_id)
                    .bind(&intent.client_id)
                    .bind(today_start)
                    .fetch_one(p)
                    .await?
                }
                Either::Right(p) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM purchases WHERE user_id = ? AND client_id = ? AND created_at >= ?",
                    )
                    .bind(user_id)
                    .bind(&intent.client_id)
                    .bind(today_start)
                    .fetch_one(p)
                    .await?
                }
            };
            if count + 1 > *limit {
                return Err(MarketplaceError::DailyLimitExceeded);
            }
        }
    }
    Ok(())
}

// ─────────────────────────── SQLite 内部助手 ───────────────────────────

async fn load_intent_sqlite(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
) -> Result<IntentRow, MarketplaceError> {
    let sql = format!("SELECT {INTENT_COLUMNS} FROM checkout_intents WHERE id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("checkout intent not found".into()))?;
    Ok(intent_from_sqlite(&row))
}

async fn load_intent_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    id: &str,
) -> Result<IntentRow, MarketplaceError> {
    let sql = format!("SELECT {INTENT_COLUMNS} FROM checkout_intents WHERE id = ? FOR UPDATE");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("checkout intent not found".into()))?;
    Ok(intent_from_mysql(&row))
}

async fn load_client_and_offer_sqlite(
    conn: &mut sqlx::SqliteConnection,
    intent: &IntentRow,
) -> Result<(clients::MarketplaceClient, OfferRow), MarketplaceError> {
    let client = clients::fetch_client_by_internal_id_conn(conn, &intent.client_id)
        .await?
        .ok_or_else(|| MarketplaceError::InvalidClient("unknown client".into()))?;
    if !client.allows_new_sales() {
        return Err(MarketplaceError::MarketplaceDisabled(
            "marketplace client is not active".into(),
        ));
    }
    if !clients::scope_approved_conn(conn, &client.id, "marketplace.checkout.create").await?
        || !clients::scope_approved_conn(conn, &client.id, "marketplace.purchase").await?
    {
        return Err(MarketplaceError::MarketplaceDisabled(
            "checkout scopes not approved".into(),
        ));
    }
    let offer = load_offer_sqlite(conn, &intent.offer_id).await?;
    if !offer.is_active() || offer.version != intent.offer_version {
        return Err(MarketplaceError::OfferVersionChanged);
    }
    if offer.client_id != client.id {
        return Err(MarketplaceError::Forbidden(
            "offer belongs to another client".into(),
        ));
    }
    Ok((client, offer))
}

async fn load_client_and_offer_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    intent: &IntentRow,
) -> Result<(clients::MarketplaceClient, OfferRow), MarketplaceError> {
    let client = clients::fetch_client_by_internal_id_conn_mysql(tx, &intent.client_id)
        .await?
        .ok_or_else(|| MarketplaceError::InvalidClient("unknown client".into()))?;
    if !client.allows_new_sales() {
        return Err(MarketplaceError::MarketplaceDisabled(
            "marketplace client is not active".into(),
        ));
    }
    if !clients::scope_approved_conn_mysql(tx, &client.id, "marketplace.checkout.create").await?
        || !clients::scope_approved_conn_mysql(tx, &client.id, "marketplace.purchase").await?
    {
        return Err(MarketplaceError::MarketplaceDisabled(
            "checkout scopes not approved".into(),
        ));
    }
    let offer = load_offer_mysql(tx, &intent.offer_id).await?;
    if !offer.is_active() || offer.version != intent.offer_version {
        return Err(MarketplaceError::OfferVersionChanged);
    }
    if offer.client_id != client.id {
        return Err(MarketplaceError::Forbidden(
            "offer belongs to another client".into(),
        ));
    }
    Ok((client, offer))
}

async fn scope_limits_sqlite(
    conn: &mut sqlx::SqliteConnection,
    client_id: &str,
) -> Result<(Option<i64>, Option<i64>, Option<i64>), MarketplaceError> {
    let limits_json: Option<String> = sqlx::query_scalar(
        "SELECT limits_json FROM client_scopes WHERE client_id = ? AND scope = ?",
    )
    .bind(client_id)
    .bind("marketplace.checkout.create")
    .fetch_optional(&mut *conn)
    .await?;
    Ok(limits_from_json(limits_json.as_deref().unwrap_or("{}")))
}

/// 读取 scope 限额（池外，供 Intent 创建）。
async fn scope_limits(
    pool: &DatabasePool,
    client_id: &str,
) -> Result<(Option<i64>, Option<i64>, Option<i64>), MarketplaceError> {
    let limits_json: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT limits_json FROM client_scopes WHERE client_id = ? AND scope = ?",
            )
            .bind(client_id)
            .bind("marketplace.checkout.create")
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT limits_json FROM client_scopes WHERE client_id = ? AND scope = ?",
            )
            .bind(client_id)
            .bind("marketplace.checkout.create")
            .fetch_optional(p)
            .await?
        }
    };
    Ok(limits_from_json(limits_json.as_deref().unwrap_or("{}")))
}

async fn scope_limits_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    client_id: &str,
) -> Result<(Option<i64>, Option<i64>, Option<i64>), MarketplaceError> {
    let limits_json: Option<String> = sqlx::query_scalar(
        "SELECT limits_json FROM client_scopes WHERE client_id = ? AND scope = ?",
    )
    .bind(client_id)
    .bind("marketplace.checkout.create")
    .fetch_optional(&mut **tx)
    .await?;
    Ok(limits_from_json(limits_json.as_deref().unwrap_or("{}")))
}

async fn check_stock_sqlite(
    _conn: &mut sqlx::SqliteConnection,
    offer: &OfferRow,
    quantity: i64,
) -> Result<(), MarketplaceError> {
    if offer.stock_policy == "finite" {
        if let Some(stock) = offer.stock_remaining {
            if stock < quantity {
                return Err(MarketplaceError::OutOfStock);
            }
        }
    }
    Ok(())
}

async fn check_stock_mysql(
    _tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    offer: &OfferRow,
    quantity: i64,
) -> Result<(), MarketplaceError> {
    if offer.stock_policy == "finite" {
        if let Some(stock) = offer.stock_remaining {
            if stock < quantity {
                return Err(MarketplaceError::OutOfStock);
            }
        }
    }
    Ok(())
}

async fn check_user_status_sqlite(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
) -> Result<(), MarketplaceError> {
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&mut *conn)
        .await?;
    match status.as_deref() {
        Some("active") => Ok(()),
        Some("banned") | Some("pending_delete") | Some("deleted") => {
            Err(MarketplaceError::Forbidden("user cannot purchase".into()))
        }
        _ => Ok(()),
    }
}

async fn check_user_status_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
) -> Result<(), MarketplaceError> {
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM users WHERE id = ?")
        .bind(user_id)
        .fetch_optional(&mut **tx)
        .await?;
    match status.as_deref() {
        Some("active") => Ok(()),
        Some("banned") | Some("pending_delete") | Some("deleted") => {
            Err(MarketplaceError::Forbidden("user cannot purchase".into()))
        }
        _ => Ok(()),
    }
}

async fn load_offer_sqlite(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
) -> Result<OfferRow, MarketplaceError> {
    use crate::marketplace::offers::OFFER_COLUMNS_PUB;
    let sql = format!("SELECT {OFFER_COLUMNS_PUB} FROM offers WHERE id = ?");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("offer not found".into()))?;
    Ok(crate::marketplace::offers::offer_from_row_sqlite(&row))
}

async fn load_offer_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    id: &str,
) -> Result<OfferRow, MarketplaceError> {
    use crate::marketplace::offers::OFFER_COLUMNS_PUB;
    let sql = format!("SELECT {OFFER_COLUMNS_PUB} FROM offers WHERE id = ? FOR UPDATE");
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("offer not found".into()))?;
    Ok(crate::marketplace::offers::offer_from_row_mysql(&row))
}

/// 事务内确保商户/平台费账本用户行存在（point_accounts FK 要求真实 users 行；
/// 密码 '!' 无法登录，仅承载恒等式）。
async fn ensure_ledger_users_sqlite(
    conn: &mut sqlx::SqliteConnection,
    client_id: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    let merchant = crate::marketplace::merchant_ledger_user(client_id);
    let fee = crate::marketplace::fee_ledger_user();
    for (uid, label) in [(merchant.as_str(), "merchant"), (fee, "fee")] {
        sqlx::query(
            "INSERT OR IGNORE INTO users
             (id, username_normalized, email_normalized, password_hash, status, level, email_verified, created_at, updated_at)
             VALUES (?, ?, ?, '!', 'active', 0, 0, ?, ?)",
        )
        .bind(uid)
        .bind(uid)
        .bind(format!("{uid}@{label}.system.local"))
        .bind(now)
        .bind(now)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn ensure_ledger_users_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    client_id: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    let merchant = crate::marketplace::merchant_ledger_user(client_id);
    let fee = crate::marketplace::fee_ledger_user();
    for (uid, label) in [(merchant.as_str(), "merchant"), (fee, "fee")] {
        sqlx::query(
            "INSERT IGNORE INTO users
             (id, username_normalized, email_normalized, password_hash, status, level, email_verified, created_at, updated_at)
             VALUES (?, ?, ?, '!', 'active', 0, 0, ?, ?)",
        )
        .bind(uid)
        .bind(uid)
        .bind(format!("{uid}@{label}.system.local"))
        .bind(now)
        .bind(now)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn credit_pending_sqlite(
    conn: &mut sqlx::SqliteConnection,
    client_id: &str,
    amount: i64,
    now: i64,
) -> Result<(), MarketplaceError> {
    let rows = sqlx::query(
        "UPDATE marketplace_merchant_accounts
         SET pending_balance = pending_balance + ?, version = version + 1, updated_at = ?
         WHERE client_id = ? AND currency_id = ?",
    )
    .bind(amount)
    .bind(now)
    .bind(client_id)
    .bind(crate::economy::ledger::service::CURRENCY_COIN)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if rows != 1 {
        return Err(MarketplaceError::NotFound(
            "merchant account not found; approve the client first".into(),
        ));
    }
    Ok(())
}

async fn credit_pending_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    client_id: &str,
    amount: i64,
    now: i64,
) -> Result<(), MarketplaceError> {
    let rows = sqlx::query(
        "UPDATE marketplace_merchant_accounts
         SET pending_balance = pending_balance + ?, version = version + 1, updated_at = ?
         WHERE client_id = ? AND currency_id = ?",
    )
    .bind(amount)
    .bind(now)
    .bind(client_id)
    .bind(crate::economy::ledger::service::CURRENCY_COIN)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if rows != 1 {
        return Err(MarketplaceError::NotFound(
            "merchant account not found; approve the client first".into(),
        ));
    }
    Ok(())
}

async fn enqueue_outbox_sqlite(
    conn: &mut sqlx::SqliteConnection,
    event_type: &str,
    payload: Value,
) -> Result<String, MarketplaceError> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    sqlx::query(
        "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
         VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(&payload_str)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

async fn enqueue_outbox_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    event_type: &str,
    payload: Value,
) -> Result<String, MarketplaceError> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    sqlx::query(
        "INSERT INTO outbox_events (id, event_type, payload, payload_version, status, attempts, max_attempts, next_attempt_at, created_at)
         VALUES (?, ?, ?, 1, 'pending', 0, 5, ?, ?)",
    )
    .bind(&id)
    .bind(event_type)
    .bind(&payload_str)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}

/// 交易后商户账户摘要（对账/管理视图）。
pub async fn merchant_account_after_purchase(
    pool: &DatabasePool,
    client_id: &str,
) -> Result<Option<MerchantAccountRow>, MarketplaceError> {
    crate::marketplace::balance::get_account(
        pool,
        client_id,
        crate::economy::ledger::service::CURRENCY_COIN,
    )
    .await
}

/// Intent 的 fee_refundable 快照（退款按原结账快照判断平台费是否返还）。
pub async fn intent_fee_refundable(
    pool: &DatabasePool,
    intent_id: &str,
) -> Result<bool, MarketplaceError> {
    let v: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT fee_refundable FROM checkout_intents WHERE id = ?")
                .bind(intent_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT fee_refundable FROM checkout_intents WHERE id = ?")
                .bind(intent_id)
                .fetch_optional(p)
                .await?
        }
    };
    Ok(v.map(|x| x != 0).unwrap_or(true))
}
