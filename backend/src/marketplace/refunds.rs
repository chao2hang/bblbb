//! M12-REFUND：退款（只追加 reversal）与补偿。
//!
//! 规则（docs/MARKETPLACE-ACCOUNTING.md §6，docs/MARKETPLACE.md §7）：
//! - 已提交购买不 UPDATE/DELETE、不把原流水改成失败；退款创建新的
//!   `reversal` operation、退款记录，引用原 purchase/operation；
//! - 同一购买的累计退款不得超过原购买金额；并发退款锁原 Purchase
//!   （FOR UPDATE）并由累计条件更新保证；
//! - 市场只能退款自己的交易；管理员强制退款要求 reason + recent-auth +
//!   限额 + 审计；
//! - 退款先使用商户该 Purchase 未结算的 pending 余额；已结算时从
//!   merchant available 扣除；余额不足不允许账户变负 → 退款进入
//!   `requested` 并冻结 Client 新销售，由管理员补足/冲正后重试；
//! - 平台费默认按退款比例返还（`fee_refundable=false` 的 Intent 不返）；
//! - 每次退款满足恒等式 `Σ(delta_balance + delta_pending + delta_frozen) = 0`。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::economy::ledger::service as ledger;
use crate::economy::ledger::service::{LedgerCommand, LedgerKind};
use crate::events::types::MARKETPLACE_REFUND_SUCCEEDED;
use crate::idempotency::{self, FailureCachePolicy, IdempotencyKey, IdempotencyOutcome};
use crate::marketplace::checkout::{get_purchase, purchase_json, PurchaseRow};
use crate::marketplace::clients::{self, MarketplaceClient};
use crate::marketplace::webhooks;
use crate::marketplace::{now_millis, MarketplaceError};

/// Refund 行。
#[derive(Debug, Clone)]
pub struct RefundRow {
    pub id: String,
    pub purchase_id: String,
    pub client_id: String,
    pub amount: i64,
    pub status: String,
    pub reason_code: String,
    pub reason: Option<String>,
    pub merchant_refund_id: String,
    pub reversal_operation_id: Option<String>,
    pub refunded_by: String,
    pub refunded_by_type: String,
    pub idempotency_scope: String,
    pub idempotency_key: String,
    pub created_at: i64,
    pub processed_at: Option<i64>,
}

const REFUND_COLUMNS: &str = "id, purchase_id, client_id, amount, status, reason_code, reason, \
     merchant_refund_id, reversal_operation_id, refunded_by, refunded_by_type, idempotency_scope, \
     idempotency_key, created_at, processed_at";

fn refund_from_sqlite(row: &sqlx::sqlite::SqliteRow) -> RefundRow {
    RefundRow {
        id: row.get("id"),
        purchase_id: row.get("purchase_id"),
        client_id: row.get("client_id"),
        amount: row.get("amount"),
        status: row.get("status"),
        reason_code: row.get("reason_code"),
        reason: row.get("reason"),
        merchant_refund_id: row.get("merchant_refund_id"),
        reversal_operation_id: row.get("reversal_operation_id"),
        refunded_by: row.get("refunded_by"),
        refunded_by_type: row.get("refunded_by_type"),
        idempotency_scope: row.get("idempotency_scope"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
        processed_at: row.get("processed_at"),
    }
}

fn refund_from_mysql(row: &sqlx::mysql::MySqlRow) -> RefundRow {
    RefundRow {
        id: row.get("id"),
        purchase_id: row.get("purchase_id"),
        client_id: row.get("client_id"),
        amount: row.get("amount"),
        status: row.get("status"),
        reason_code: row.get("reason_code"),
        reason: row.get("reason"),
        merchant_refund_id: row.get("merchant_refund_id"),
        reversal_operation_id: row.get("reversal_operation_id"),
        refunded_by: row.get("refunded_by"),
        refunded_by_type: row.get("refunded_by_type"),
        idempotency_scope: row.get("idempotency_scope"),
        idempotency_key: row.get("idempotency_key"),
        created_at: row.get("created_at"),
        processed_at: row.get("processed_at"),
    }
}

pub fn refund_json(r: &RefundRow) -> Value {
    json!({
        "id": r.id,
        "purchase_id": r.purchase_id,
        "client_id": r.client_id,
        "amount": r.amount,
        "status": r.status,
        "reason_code": r.reason_code,
        "reason": r.reason,
        "merchant_refund_id": r.merchant_refund_id,
        "reversal_operation_id": r.reversal_operation_id,
        "refunded_by": r.refunded_by,
        "refunded_by_type": r.refunded_by_type,
        "created_at": r.created_at,
        "processed_at": r.processed_at,
    })
}

pub async fn get_refund(
    pool: &DatabasePool,
    id: &str,
) -> Result<Option<RefundRow>, MarketplaceError> {
    let sql = format!("SELECT {REFUND_COLUMNS} FROM refunds WHERE id = ?");
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| refund_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(id)
            .fetch_optional(p)
            .await?
            .map(|r| refund_from_mysql(&r)),
    };
    Ok(row)
}

pub async fn list_refunds(
    pool: &DatabasePool,
    purchase_id: Option<&str>,
    client_id: Option<&str>,
    after: Option<&str>,
    limit: i64,
) -> Result<Vec<Value>, MarketplaceError> {
    let limit = limit.clamp(1, 100);
    let clause = match (purchase_id, client_id) {
        (Some(_), _) => "WHERE purchase_id = ? AND id > ?",
        (None, Some(_)) => "WHERE client_id = ? AND id > ?",
        _ => {
            return Err(MarketplaceError::Invalid(
                "purchase_id or client_id required".into(),
            ))
        }
    };
    let sql = format!("SELECT {REFUND_COLUMNS} FROM refunds {clause} ORDER BY id ASC LIMIT ?");
    let key = purchase_id.or(client_id).unwrap_or("");
    let rows: Vec<Value> = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(key)
            .bind(after.unwrap_or(""))
            .bind(limit + 1)
            .fetch_all(p)
            .await?
            .iter()
            .map(|r| refund_json(&refund_from_sqlite(r)))
            .collect(),
        Either::Right(p) => sqlx::query(&sql)
            .bind(key)
            .bind(after.unwrap_or(""))
            .bind(limit + 1)
            .fetch_all(p)
            .await?
            .iter()
            .map(|r| refund_json(&refund_from_mysql(r)))
            .collect(),
    };
    Ok(rows.into_iter().take(limit as usize).collect())
}

/// 退款输入。
#[derive(Debug, Clone)]
pub struct RefundInput {
    pub amount: i64,
    pub reason_code: String,
    pub merchant_refund_id: String,
}

/// 解析并校验退款输入。
pub fn validate_refund_input(body: &Value) -> Result<RefundInput, MarketplaceError> {
    let amount = body.get("amount").and_then(Value::as_i64).unwrap_or(-1);
    let reason_code = body
        .get("reason_code")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let merchant_refund_id = body
        .get("merchant_refund_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if amount <= 0 {
        return Err(MarketplaceError::Invalid(
            "refund amount must be a positive integer".into(),
        ));
    }
    if merchant_refund_id.is_empty() || merchant_refund_id.len() > 128 {
        return Err(MarketplaceError::Invalid(
            "merchant_refund_id required (<=128 chars)".into(),
        ));
    }
    if reason_code.is_empty() || reason_code.len() > 64 {
        return Err(MarketplaceError::Invalid(
            "reason_code required (<=64 chars)".into(),
        ));
    }
    Ok(RefundInput {
        amount,
        reason_code,
        merchant_refund_id,
    })
}

fn refund_request_hash(actor_id: &str, purchase_id: &str, input: &RefundInput) -> String {
    use sha2::{Digest, Sha256};
    let canonical = format!(
        "{actor_id}|{purchase_id}|{}|{}|{}",
        input.amount, input.reason_code, input.merchant_refund_id
    );
    hex::encode(Sha256::digest(canonical.as_bytes()))
}

/// POST /marketplace/purchases/{id}/refund：Client 服务认证或管理员强制退款。
///
/// `principal` 为 `Some` 时是 Client 服务操作（只允许自己的 Purchase）；
/// `None` 时是管理员强制退款（route 层已做 reason + recent-auth + 审计）。
#[allow(clippy::too_many_arguments)]
pub async fn create_refund(
    pool: &DatabasePool,
    actor_id: &str,
    actor_type: &str,
    principal: Option<&MarketplaceClient>,
    purchase_id: &str,
    input: &RefundInput,
    idempotency_key: &str,
) -> Result<Value, MarketplaceError> {
    create_refund_inner(
        pool,
        actor_id,
        actor_id,
        actor_type,
        principal,
        purchase_id,
        input,
        idempotency_key,
    )
    .await
}

/// `ledger_actor_id` 必须是对应 `users(id)` 的真实用户（point_operations
/// actor_id FK；Client 服务退款使用 owner_user_id）。
#[allow(clippy::too_many_arguments)]
pub async fn create_refund_inner(
    pool: &DatabasePool,
    actor_id: &str,
    ledger_actor_id: &str,
    actor_type: &str,
    principal: Option<&MarketplaceClient>,
    purchase_id: &str,
    input: &RefundInput,
    idempotency_key: &str,
) -> Result<Value, MarketplaceError> {
    if !(16..=200).contains(&idempotency_key.len()) {
        return Err(MarketplaceError::Invalid(
            "Idempotency-Key must be 16..=200 chars".into(),
        ));
    }
    let scope_client = match principal {
        Some(c) => c.id.clone(),
        None => {
            let purchase = get_purchase(pool, purchase_id)
                .await?
                .ok_or_else(|| MarketplaceError::NotFound("purchase not found".into()))?;
            purchase.client_id.clone()
        }
    };
    let request_hash = refund_request_hash(actor_id, purchase_id, input);
    let idem = IdempotencyKey::new(
        "marketplace.refund",
        format!("{scope_client}.{idempotency_key}"),
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
            let result = execute_refund(
                pool,
                actor_id,
                ledger_actor_id,
                actor_type,
                principal,
                purchase_id,
                input,
                now_millis(),
            )
            .await;
            match result {
                Ok(view) => {
                    let refund_id = view["id"].as_str().unwrap_or("").to_string();
                    let _ = idempotency::complete(pool, &record_id, &refund_id).await;
                    Ok(view)
                }
                Err(e) => {
                    let _ = idempotency::mark_failed(pool, &record_id).await;
                    Err(e)
                }
            }
        }
        IdempotencyOutcome::Replay { response_reference } => {
            if let Some(refund_id) = response_reference {
                if let Some(refund) = get_refund(pool, &refund_id).await? {
                    return Ok(refund_json(&refund));
                }
            }
            Err(MarketplaceError::IdempotencyConflict)
        }
        IdempotencyOutcome::InProgress => Err(MarketplaceError::Invalid(
            "refund already in progress for this key".into(),
        )),
        IdempotencyOutcome::Conflict => Err(MarketplaceError::IdempotencyConflict),
        IdempotencyOutcome::Failed { .. } => Err(MarketplaceError::IdempotencyConflict),
    }
}

/// 共享校验：Client 作用域、状态、累计上限、平台费返还。
async fn refund_checks(
    pool: &DatabasePool,
    purchase: &PurchaseRow,
    principal: Option<&MarketplaceClient>,
    actor_type: &str,
    amount: i64,
) -> Result<(MarketplaceClient, bool), MarketplaceError> {
    if let Some(client) = principal {
        if purchase.client_id != client.id {
            return Err(MarketplaceError::Forbidden(
                "cannot refund another client's purchase".into(),
            ));
        }
    }
    if !matches!(actor_type, "client" | "admin") {
        return Err(MarketplaceError::Forbidden(
            "invalid refund actor type".into(),
        ));
    }
    if purchase.status == "refunded" {
        return Err(MarketplaceError::RefundNotAllowed(
            "purchase is already fully refunded".into(),
        ));
    }
    if !matches!(purchase.status.as_str(), "succeeded" | "partially_refunded") {
        return Err(MarketplaceError::RefundNotAllowed(
            "purchase is not refundable".into(),
        ));
    }
    // 累计上限（锁内条件更新兜底，此处先做精确校验）。
    let remaining = purchase.amount.saturating_sub(purchase.refunded_amount);
    if amount > remaining {
        return Err(MarketplaceError::RefundExceedsPurchase);
    }
    let client = clients::fetch_client_by_internal_id(pool, &purchase.client_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("marketplace client".into()))?;
    let fee_refundable =
        crate::marketplace::checkout::intent_fee_refundable(pool, &purchase.intent_id).await?;
    Ok((client, fee_refundable))
}

/// 退款事务本体。锁顺序：idempotency op → Purchase → 商户账户。
#[allow(clippy::explicit_auto_deref)]
#[allow(clippy::too_many_arguments)]
async fn execute_refund(
    pool: &DatabasePool,
    actor_id: &str,
    ledger_actor_id: &str,
    actor_type: &str,
    principal: Option<&MarketplaceClient>,
    purchase_id: &str,
    input: &RefundInput,
    now: i64,
) -> Result<Value, MarketplaceError> {
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<Value, MarketplaceError> = async {
                let purchase = load_purchase_sqlite(&mut conn, purchase_id).await?;
                let (client, fee_refundable) =
                    refund_checks(pool, &purchase, principal, actor_type, input.amount).await?;
                let fee_refund = if fee_refundable {
                    purchase.fee_amount * input.amount / purchase.amount.max(1)
                } else {
                    0
                };
                let merchant_portion = input.amount - fee_refund;

                let account = balance_account_sqlite(&mut conn, &purchase.client_id).await?;
                let sufficient =
                    account.available_balance + account.pending_balance >= merchant_portion;

                let refund_id = uuid::Uuid::now_v7().to_string();
                if !sufficient {
                    insert_refund_sqlite(
                        &mut conn,
                        &refund_id,
                        &purchase,
                        input,
                        actor_id,
                        actor_type,
                        "requested",
                        None,
                        now,
                    )
                    .await?;
                    freeze_new_sales_sqlite(&mut conn, &purchase.client_id, actor_id, now).await?;
                    let refund = refund_from_sqlite(
                        &sqlx::query(&format!(
                            "SELECT {REFUND_COLUMNS} FROM refunds WHERE id = ?"
                        ))
                        .bind(&refund_id)
                        .fetch_one(&mut *conn)
                        .await?,
                    );
                    return Ok(refund_json(&refund));
                }

                let refund_scope = format!("marketplace.refund.{purchase_id}");

                let buyer_reversal = LedgerCommand {
                    idempotency_scope: refund_scope.clone(),
                    idempotency_key: format!("buyer.{refund_id}"),
                    kind: LedgerKind::Reversal,
                    actor_id: Some(ledger_actor_id.to_string()),
                    user_id: purchase.user_id.clone(),
                    currency_id: purchase.currency_id.clone(),
                    delta_balance: input.amount,
                    delta_frozen: 0,
                    source_type: Some("marketplace_refund".to_string()),
                    source_id: Some(refund_id.clone()),
                    memo: format!("marketplace refund {refund_id}: {}", input.reason_code),
                    reverses_operation_id: Some(purchase.point_operation_id.clone()),
                };
                let buyer_op =
                    ledger::apply_operation_in_sqlite_tx(&mut conn, buyer_reversal, now).await?;

                let merchant_reversal = LedgerCommand {
                    idempotency_scope: refund_scope.clone(),
                    idempotency_key: format!("merchant.{refund_id}"),
                    kind: LedgerKind::Reversal,
                    actor_id: Some(ledger_actor_id.to_string()),
                    user_id: crate::marketplace::merchant_ledger_user(&purchase.client_id),
                    currency_id: purchase.currency_id.clone(),
                    delta_balance: -merchant_portion,
                    delta_frozen: 0,
                    source_type: Some("marketplace_refund".to_string()),
                    source_id: Some(refund_id.clone()),
                    memo: format!("marketplace merchant refund {refund_id}"),
                    reverses_operation_id: Some(purchase.merchant_operation_id.clone()),
                };
                let _merchant_op =
                    ledger::apply_operation_in_sqlite_tx(&mut conn, merchant_reversal, now).await?;

                let _fee_reversal_op = if fee_refund > 0 {
                    if let Some(fee_op_id) = &purchase.fee_operation_id {
                        let fee_reversal = LedgerCommand {
                            idempotency_scope: refund_scope.clone(),
                            idempotency_key: format!("fee.{refund_id}"),
                            kind: LedgerKind::Reversal,
                            actor_id: Some(ledger_actor_id.to_string()),
                            user_id: crate::marketplace::fee_ledger_user().to_string(),
                            currency_id: purchase.currency_id.clone(),
                            delta_balance: -fee_refund,
                            delta_frozen: 0,
                            source_type: Some("marketplace_refund".to_string()),
                            source_id: Some(refund_id.clone()),
                            memo: format!("marketplace fee refund {refund_id}"),
                            reverses_operation_id: Some(fee_op_id.clone()),
                        };
                        Some(
                            ledger::apply_operation_in_sqlite_tx(&mut conn, fee_reversal, now)
                                .await?,
                        )
                    } else {
                        None
                    }
                } else {
                    None
                };

                debit_refund_sqlite(
                    &mut conn,
                    &purchase.client_id,
                    merchant_portion,
                    account.version,
                    now,
                )
                .await?;

                let updated =
                    update_purchase_refunded_sqlite(&mut conn, &purchase, input.amount, now)
                        .await?;

                insert_refund_sqlite(
                    &mut conn,
                    &refund_id,
                    &purchase,
                    input,
                    actor_id,
                    actor_type,
                    "processed",
                    Some(&buyer_op.operation_id),
                    now,
                )
                .await?;

                AuditEntry::user_action(actor_id, "marketplace.refund")
                    .with_target("client", &client.client_id)
                    .with_target("purchase", &purchase.id)
                    .with_target("refund", &refund_id)
                    .with_reason(&input.reason_code)
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_sqlite(&mut conn)
                    .await
                    .map_err(MarketplaceError::from)?;

                let outbox_event_id = enqueue_refund_outbox_sqlite(
                    &mut conn,
                    json!({
                        "refund_id": refund_id,
                        "purchase_id": purchase.id,
                        "client_id": client.client_id,
                        "amount": input.amount,
                        "currency_id": purchase.currency_id,
                        "status": "processed",
                    }),
                )
                .await?;

                let hook_payload = webhooks::minimal_payload(
                    &outbox_event_id,
                    MARKETPLACE_REFUND_SUCCEEDED,
                    &json!({
                        "client_id": client.client_id,
                        "purchase_id": purchase.id,
                        "status": "processed",
                        "amount": input.amount,
                        "currency_id": purchase.currency_id,
                        "merchant_order_id": purchase.merchant_order_id,
                    }),
                );
                webhooks::register_delivery_sqlite(
                    &mut conn,
                    &purchase.client_id,
                    &outbox_event_id,
                    MARKETPLACE_REFUND_SUCCEEDED,
                    &hook_payload,
                    now,
                )
                .await?;

                let refund = refund_from_sqlite(
                    &sqlx::query(&format!(
                        "SELECT {REFUND_COLUMNS} FROM refunds WHERE id = ?"
                    ))
                    .bind(&refund_id)
                    .fetch_one(&mut *conn)
                    .await?,
                );
                let mut view = refund_json(&refund);
                view["purchase"] = purchase_json(&updated);
                Ok(view)
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
                let purchase = load_purchase_mysql(&mut tx, purchase_id).await?;
                let (client, fee_refundable) =
                    refund_checks(pool, &purchase, principal, actor_type, input.amount).await?;
                let fee_refund = if fee_refundable {
                    purchase.fee_amount * input.amount / purchase.amount.max(1)
                } else {
                    0
                };
                let merchant_portion = input.amount - fee_refund;

                let account = balance_account_mysql(&mut tx, &purchase.client_id).await?;
                let sufficient =
                    account.available_balance + account.pending_balance >= merchant_portion;

                let refund_id = uuid::Uuid::now_v7().to_string();
                if !sufficient {
                    insert_refund_mysql(
                        &mut tx,
                        &refund_id,
                        &purchase,
                        input,
                        actor_id,
                        actor_type,
                        "requested",
                        None,
                        now,
                    )
                    .await?;
                    freeze_new_sales_mysql(&mut tx, &purchase.client_id, actor_id, now).await?;
                    let refund = refund_from_mysql(
                        &sqlx::query(&format!(
                            "SELECT {REFUND_COLUMNS} FROM refunds WHERE id = ?"
                        ))
                        .bind(&refund_id)
                        .fetch_one(&mut *tx)
                        .await?,
                    );
                    return Ok(refund_json(&refund));
                }

                let refund_scope = format!("marketplace.refund.{purchase_id}");

                let buyer_reversal = LedgerCommand {
                    idempotency_scope: refund_scope.clone(),
                    idempotency_key: format!("buyer.{refund_id}"),
                    kind: LedgerKind::Reversal,
                    actor_id: Some(ledger_actor_id.to_string()),
                    user_id: purchase.user_id.clone(),
                    currency_id: purchase.currency_id.clone(),
                    delta_balance: input.amount,
                    delta_frozen: 0,
                    source_type: Some("marketplace_refund".to_string()),
                    source_id: Some(refund_id.clone()),
                    memo: format!("marketplace refund {refund_id}: {}", input.reason_code),
                    reverses_operation_id: Some(purchase.point_operation_id.clone()),
                };
                let buyer_op =
                    ledger::apply_operation_in_mysql_tx(&mut tx, buyer_reversal, now).await?;

                let merchant_reversal = LedgerCommand {
                    idempotency_scope: refund_scope.clone(),
                    idempotency_key: format!("merchant.{refund_id}"),
                    kind: LedgerKind::Reversal,
                    actor_id: Some(ledger_actor_id.to_string()),
                    user_id: crate::marketplace::merchant_ledger_user(&purchase.client_id),
                    currency_id: purchase.currency_id.clone(),
                    delta_balance: -merchant_portion,
                    delta_frozen: 0,
                    source_type: Some("marketplace_refund".to_string()),
                    source_id: Some(refund_id.clone()),
                    memo: format!("marketplace merchant refund {refund_id}"),
                    reverses_operation_id: Some(purchase.merchant_operation_id.clone()),
                };
                let _merchant_op =
                    ledger::apply_operation_in_mysql_tx(&mut tx, merchant_reversal, now).await?;

                let _fee_reversal_op = if fee_refund > 0 {
                    if let Some(fee_op_id) = &purchase.fee_operation_id {
                        let fee_reversal = LedgerCommand {
                            idempotency_scope: refund_scope.clone(),
                            idempotency_key: format!("fee.{refund_id}"),
                            kind: LedgerKind::Reversal,
                            actor_id: Some(ledger_actor_id.to_string()),
                            user_id: crate::marketplace::fee_ledger_user().to_string(),
                            currency_id: purchase.currency_id.clone(),
                            delta_balance: -fee_refund,
                            delta_frozen: 0,
                            source_type: Some("marketplace_refund".to_string()),
                            source_id: Some(refund_id.clone()),
                            memo: format!("marketplace fee refund {refund_id}"),
                            reverses_operation_id: Some(fee_op_id.clone()),
                        };
                        Some(ledger::apply_operation_in_mysql_tx(&mut tx, fee_reversal, now).await?)
                    } else {
                        None
                    }
                } else {
                    None
                };

                debit_refund_mysql(
                    &mut tx,
                    &purchase.client_id,
                    merchant_portion,
                    account.version,
                    now,
                )
                .await?;

                let updated =
                    update_purchase_refunded_mysql(&mut tx, &purchase, input.amount, now).await?;

                insert_refund_mysql(
                    &mut tx,
                    &refund_id,
                    &purchase,
                    input,
                    actor_id,
                    actor_type,
                    "processed",
                    Some(&buyer_op.operation_id),
                    now,
                )
                .await?;

                AuditEntry::user_action(actor_id, "marketplace.refund")
                    .with_target("client", &client.client_id)
                    .with_target("purchase", &purchase.id)
                    .with_target("refund", &refund_id)
                    .with_reason(&input.reason_code)
                    .with_policy_version(AUTHZ_POLICY_VERSION)
                    .record_into_mysql(&mut tx)
                    .await
                    .map_err(MarketplaceError::from)?;

                let outbox_event_id = enqueue_refund_outbox_mysql(
                    &mut tx,
                    json!({
                        "refund_id": refund_id,
                        "purchase_id": purchase.id,
                        "client_id": client.client_id,
                        "amount": input.amount,
                        "currency_id": purchase.currency_id,
                        "status": "processed",
                    }),
                )
                .await?;

                let hook_payload = webhooks::minimal_payload(
                    &outbox_event_id,
                    MARKETPLACE_REFUND_SUCCEEDED,
                    &json!({
                        "client_id": client.client_id,
                        "purchase_id": purchase.id,
                        "status": "processed",
                        "amount": input.amount,
                        "currency_id": purchase.currency_id,
                        "merchant_order_id": purchase.merchant_order_id,
                    }),
                );
                webhooks::register_delivery_mysql(
                    &mut tx,
                    &purchase.client_id,
                    &outbox_event_id,
                    MARKETPLACE_REFUND_SUCCEEDED,
                    &hook_payload,
                    now,
                )
                .await?;

                let refund = refund_from_mysql(
                    &sqlx::query(&format!(
                        "SELECT {REFUND_COLUMNS} FROM refunds WHERE id = ?"
                    ))
                    .bind(&refund_id)
                    .fetch_one(&mut *tx)
                    .await?,
                );
                let mut view = refund_json(&refund);
                view["purchase"] = purchase_json(&updated);
                Ok(view)
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

/// 管理员处理 `requested` 退款（补偿/冲正后重试；只处理指定 refund）。
pub async fn retry_requested_refund(
    pool: &DatabasePool,
    refund_id: &str,
    actor_id: &str,
    now: i64,
) -> Result<Value, MarketplaceError> {
    let refund = get_refund(pool, refund_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("refund not found".into()))?;
    if refund.status != "requested" {
        return Err(MarketplaceError::RefundNotAllowed(
            "refund is not in requested state".into(),
        ));
    }
    let purchase = get_purchase(pool, &refund.purchase_id)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("purchase not found".into()))?;
    let input = RefundInput {
        amount: refund.amount,
        reason_code: refund.reason_code.clone(),
        merchant_refund_id: refund.merchant_refund_id.clone(),
    };
    // 直接执行处理逻辑（跳过幂等记录，已存在 refund 行）。
    execute_refund(
        pool,
        actor_id,
        actor_id,
        "admin",
        None,
        &purchase.id,
        &input,
        now,
    )
    .await
}

// ─────────────────────────── SQLite 内部助手 ───────────────────────────

async fn load_purchase_sqlite(
    conn: &mut sqlx::SqliteConnection,
    id: &str,
) -> Result<PurchaseRow, MarketplaceError> {
    let sql = format!("SELECT {} FROM purchases WHERE id = ?", purchase_columns());
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut *conn)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("purchase not found".into()))?;
    Ok(purchase_from_row_sqlite(&row))
}

async fn load_purchase_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    id: &str,
) -> Result<PurchaseRow, MarketplaceError> {
    let sql = format!(
        "SELECT {} FROM purchases WHERE id = ? FOR UPDATE",
        purchase_columns()
    );
    let row = sqlx::query(&sql)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("purchase not found".into()))?;
    Ok(purchase_from_row_mysql(&row))
}

fn purchase_columns() -> &'static str {
    "id, intent_id, client_id, user_id, offer_id, offer_version, quantity, amount, fee_amount, \
     merchant_net, currency_id, status, refunded_amount, point_operation_id, merchant_operation_id, \
     fee_operation_id, merchant_order_id, created_at, updated_at"
}

pub fn purchase_from_row_sqlite(row: &sqlx::sqlite::SqliteRow) -> PurchaseRow {
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

pub fn purchase_from_row_mysql(row: &sqlx::mysql::MySqlRow) -> PurchaseRow {
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

async fn balance_account_sqlite(
    conn: &mut sqlx::SqliteConnection,
    client_id: &str,
) -> Result<crate::marketplace::balance::MerchantAccountRow, MarketplaceError> {
    let row = sqlx::query(
        "SELECT id, client_id, owner_user_id, currency_id, available_balance, pending_balance, frozen_balance, status, version, created_at, updated_at
         FROM marketplace_merchant_accounts WHERE client_id = ? AND currency_id = ?",
    )
    .bind(client_id)
    .bind(crate::economy::ledger::service::CURRENCY_COIN)
    .fetch_optional(&mut *conn)
    .await?
    .ok_or_else(|| MarketplaceError::NotFound("merchant account not found".into()))?;
    Ok(crate::marketplace::balance::account_from_row_sqlite(&row))
}

async fn balance_account_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    client_id: &str,
) -> Result<crate::marketplace::balance::MerchantAccountRow, MarketplaceError> {
    let row = sqlx::query(
        "SELECT id, client_id, owner_user_id, currency_id, available_balance, pending_balance, frozen_balance, status, version, created_at, updated_at
         FROM marketplace_merchant_accounts WHERE client_id = ? AND currency_id = ? FOR UPDATE",
    )
    .bind(client_id)
    .bind(crate::economy::ledger::service::CURRENCY_COIN)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| MarketplaceError::NotFound("merchant account not found".into()))?;
    Ok(crate::marketplace::balance::account_from_row_mysql(&row))
}

async fn debit_refund_sqlite(
    conn: &mut sqlx::SqliteConnection,
    client_id: &str,
    amount: i64,
    expected_version: i64,
    now: i64,
) -> Result<(), MarketplaceError> {
    let account = balance_account_sqlite(conn, client_id).await?;
    let from_pending = account.pending_balance.min(amount);
    let remaining = amount - from_pending;
    let from_available = if remaining > 0 {
        if account.available_balance < remaining {
            return Err(MarketplaceError::MerchantBalanceInsufficient);
        }
        remaining
    } else {
        0
    };
    let rows = sqlx::query(
        "UPDATE marketplace_merchant_accounts
         SET pending_balance = pending_balance - ?, available_balance = available_balance - ?,
             version = version + 1, updated_at = ?
         WHERE client_id = ? AND currency_id = ? AND version = ?",
    )
    .bind(from_pending)
    .bind(from_available)
    .bind(now)
    .bind(client_id)
    .bind(crate::economy::ledger::service::CURRENCY_COIN)
    .bind(expected_version)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if rows != 1 {
        return Err(MarketplaceError::Db(
            "concurrent merchant account modification".into(),
        ));
    }
    Ok(())
}

async fn debit_refund_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    client_id: &str,
    amount: i64,
    expected_version: i64,
    now: i64,
) -> Result<(), MarketplaceError> {
    let account = balance_account_mysql(tx, client_id).await?;
    let from_pending = account.pending_balance.min(amount);
    let remaining = amount - from_pending;
    let from_available = if remaining > 0 {
        if account.available_balance < remaining {
            return Err(MarketplaceError::MerchantBalanceInsufficient);
        }
        remaining
    } else {
        0
    };
    let rows = sqlx::query(
        "UPDATE marketplace_merchant_accounts
         SET pending_balance = pending_balance - ?, available_balance = available_balance - ?,
             version = version + 1, updated_at = ?
         WHERE client_id = ? AND currency_id = ? AND version = ?",
    )
    .bind(from_pending)
    .bind(from_available)
    .bind(now)
    .bind(client_id)
    .bind(crate::economy::ledger::service::CURRENCY_COIN)
    .bind(expected_version)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if rows != 1 {
        return Err(MarketplaceError::Db(
            "concurrent merchant account modification".into(),
        ));
    }
    Ok(())
}

/// 累计退款条件更新（锁内；`refunded_amount + ? <= amount` 兜底并发上限）。
async fn update_purchase_refunded_sqlite(
    conn: &mut sqlx::SqliteConnection,
    purchase: &PurchaseRow,
    amount: i64,
    now: i64,
) -> Result<PurchaseRow, MarketplaceError> {
    let new_refunded = purchase.refunded_amount + amount;
    let new_status = if new_refunded >= purchase.amount {
        "refunded"
    } else {
        "partially_refunded"
    };
    let rows = sqlx::query(
        "UPDATE purchases
         SET refunded_amount = refunded_amount + ?, status = ?, updated_at = ?
         WHERE id = ? AND refunded_amount + ? <= amount",
    )
    .bind(amount)
    .bind(new_status)
    .bind(now)
    .bind(&purchase.id)
    .bind(amount)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if rows != 1 {
        return Err(MarketplaceError::RefundExceedsPurchase);
    }
    let sql = format!("SELECT {} FROM purchases WHERE id = ?", purchase_columns());
    let row = sqlx::query(&sql)
        .bind(&purchase.id)
        .fetch_one(&mut *conn)
        .await?;
    Ok(purchase_from_row_sqlite(&row))
}

async fn update_purchase_refunded_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    purchase: &PurchaseRow,
    amount: i64,
    now: i64,
) -> Result<PurchaseRow, MarketplaceError> {
    let new_refunded = purchase.refunded_amount + amount;
    let new_status = if new_refunded >= purchase.amount {
        "refunded"
    } else {
        "partially_refunded"
    };
    let rows = sqlx::query(
        "UPDATE purchases
         SET refunded_amount = refunded_amount + ?, status = ?, updated_at = ?
         WHERE id = ? AND refunded_amount + ? <= amount",
    )
    .bind(amount)
    .bind(new_status)
    .bind(now)
    .bind(&purchase.id)
    .bind(amount)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if rows != 1 {
        return Err(MarketplaceError::RefundExceedsPurchase);
    }
    let sql = format!("SELECT {} FROM purchases WHERE id = ?", purchase_columns());
    let row = sqlx::query(&sql)
        .bind(&purchase.id)
        .fetch_one(&mut **tx)
        .await?;
    Ok(purchase_from_row_mysql(&row))
}

#[allow(clippy::too_many_arguments)]
async fn insert_refund_sqlite(
    conn: &mut sqlx::SqliteConnection,
    refund_id: &str,
    purchase: &PurchaseRow,
    input: &RefundInput,
    actor_id: &str,
    actor_type: &str,
    status: &str,
    reversal_operation_id: Option<&str>,
    now: i64,
) -> Result<(), MarketplaceError> {
    let scope = format!("marketplace.refund.{}", purchase.client_id);
    let idem_key = format!("refund.{refund_id}");
    let rows = sqlx::query(
        "INSERT INTO refunds
         (id, purchase_id, client_id, amount, status, reason_code, reason, merchant_refund_id,
          reversal_operation_id, refunded_by, refunded_by_type, idempotency_scope, idempotency_key,
          created_at, processed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(refund_id)
    .bind(&purchase.id)
    .bind(&purchase.client_id)
    .bind(input.amount)
    .bind(status)
    .bind(&input.reason_code)
    .bind(if status == "requested" {
        Some("merchant balance insufficient")
    } else {
        None
    })
    .bind(&input.merchant_refund_id)
    .bind(reversal_operation_id)
    .bind(actor_id)
    .bind(actor_type)
    .bind(&scope)
    .bind(&idem_key)
    .bind(now)
    .bind(if status == "processed" {
        Some(now)
    } else {
        None
    })
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if rows != 1 {
        return Err(MarketplaceError::Db("insert refund failed".into()));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_refund_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    refund_id: &str,
    purchase: &PurchaseRow,
    input: &RefundInput,
    actor_id: &str,
    actor_type: &str,
    status: &str,
    reversal_operation_id: Option<&str>,
    now: i64,
) -> Result<(), MarketplaceError> {
    let scope = format!("marketplace.refund.{}", purchase.client_id);
    let idem_key = format!("refund.{refund_id}");
    let rows = sqlx::query(
        "INSERT INTO refunds
         (id, purchase_id, client_id, amount, status, reason_code, reason, merchant_refund_id,
          reversal_operation_id, refunded_by, refunded_by_type, idempotency_scope, idempotency_key,
          created_at, processed_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(refund_id)
    .bind(&purchase.id)
    .bind(&purchase.client_id)
    .bind(input.amount)
    .bind(status)
    .bind(&input.reason_code)
    .bind(if status == "requested" {
        Some("merchant balance insufficient")
    } else {
        None
    })
    .bind(&input.merchant_refund_id)
    .bind(reversal_operation_id)
    .bind(actor_id)
    .bind(actor_type)
    .bind(&scope)
    .bind(&idem_key)
    .bind(now)
    .bind(if status == "processed" {
        Some(now)
    } else {
        None
    })
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if rows != 1 {
        return Err(MarketplaceError::Db("insert refund failed".into()));
    }
    Ok(())
}

/// 冻结新销售：Client status → disabled（阻止新 Intent/confirm；历史保留）。
async fn freeze_new_sales_sqlite(
    conn: &mut sqlx::SqliteConnection,
    client_id: &str,
    actor_id: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    let rows = sqlx::query(
        "UPDATE marketplace_clients SET status = 'disabled', version = version + 1, updated_by = ?, updated_at = ?
         WHERE id = ? AND status = 'active'",
    )
    .bind(actor_id)
    .bind(now)
    .bind(client_id)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    let _ = rows;
    Ok(())
}

async fn freeze_new_sales_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    client_id: &str,
    actor_id: &str,
    now: i64,
) -> Result<(), MarketplaceError> {
    let rows = sqlx::query(
        "UPDATE marketplace_clients SET status = 'disabled', version = version + 1, updated_by = ?, updated_at = ?
         WHERE id = ? AND status = 'active'",
    )
    .bind(actor_id)
    .bind(now)
    .bind(client_id)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    let _ = rows;
    Ok(())
}

async fn enqueue_refund_outbox_sqlite(
    conn: &mut sqlx::SqliteConnection,
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
    .bind(MARKETPLACE_REFUND_SUCCEEDED)
    .bind(&payload_str)
    .bind(now)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(id)
}

async fn enqueue_refund_outbox_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
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
    .bind(MARKETPLACE_REFUND_SUCCEEDED)
    .bind(&payload_str)
    .bind(now)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(id)
}
