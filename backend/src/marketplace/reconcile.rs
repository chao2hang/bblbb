//! M12-REFUND-07：增量对账、差异分类与恒等式校验。
//!
//! 对账窗口（`after_cursor`）内：
//! 1. 每个 Purchase 的三方账本 operation（买方扣款 / 商户入账 / 平台费）
//!    必须存在且金额与 Purchase 快照一致；
//! 2. 恒等式 `Σ(delta_balance + delta_pending + delta_frozen) = 0`；
//! 3. 商户运营余额（available+pending+frozen）与商户账本合成账户余额一致。
//!
//! 差异分类写入 `reconciliation_records.diffs_json`：
//! - `missing_ledger_op`：Purchase 引用的账本 operation 缺失；
//! - `amount_mismatch`：operation 金额与 Purchase 快照不一致；
//! - `identity_break`：窗口内三方 operation 之和不为 0；
//! - `merchant_balance_mismatch`：运营余额与账本余额不一致；
//! - `reversal_ok`：退款 reversal 满足恒等式。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::db::DatabasePool;
use crate::economy::ledger::service::CURRENCY_COIN;
use crate::marketplace::balance;
use crate::marketplace::MarketplaceError;

#[derive(Debug, Clone, PartialEq, Eq)]
enum DiffClass {
    MissingLedgerOp,
    AmountMismatch,
    IdentityBreak,
    MerchantBalanceMismatch,
}

impl DiffClass {
    fn as_str(&self) -> &'static str {
        match self {
            Self::MissingLedgerOp => "missing_ledger_op",
            Self::AmountMismatch => "amount_mismatch",
            Self::IdentityBreak => "identity_break",
            Self::MerchantBalanceMismatch => "merchant_balance_mismatch",
        }
    }
}

/// 增量对账（`after_cursor` 之后创建的 Purchase；cursor 使用 created_at 毫秒）。
pub async fn run_reconciliation(
    pool: &DatabasePool,
    client_id: &str,
    after_cursor: i64,
    now: i64,
) -> Result<Value, MarketplaceError> {
    let purchases = fetch_purchases_after(pool, client_id, after_cursor).await?;
    let mut diffs: Vec<Value> = Vec::new();
    let mut amount_sum: i64 = 0;
    let mut fee_sum: i64 = 0;
    let mut buyer_delta_sum: i64 = 0;
    let mut merchant_delta_sum: i64 = 0;
    let mut fee_delta_sum: i64 = 0;

    for purchase in &purchases {
        amount_sum += purchase.amount;
        fee_sum += purchase.fee_amount;
        // 买方 operation。
        let buyer = fetch_operation(pool, &purchase.point_operation_id).await?;
        match buyer {
            Some((delta_balance, user, source_type)) => {
                if delta_balance != -purchase.amount {
                    diffs.push(diff_entry(
                        &purchase.id,
                        DiffClass::AmountMismatch,
                        &format!("buyer delta {} != -{}", delta_balance, purchase.amount),
                    ));
                }
                if source_type.as_deref() != Some("marketplace_purchase") {
                    diffs.push(diff_entry(
                        &purchase.id,
                        DiffClass::MissingLedgerOp,
                        "buyer operation has wrong source_type",
                    ));
                }
                let _ = user;
                buyer_delta_sum += delta_balance;
            }
            None => diffs.push(diff_entry(
                &purchase.id,
                DiffClass::MissingLedgerOp,
                "buyer operation not found",
            )),
        }
        // 商户 operation。
        let merchant = fetch_operation(pool, &purchase.merchant_operation_id).await?;
        match merchant {
            Some((delta_balance, user, _)) => {
                let expected_user = crate::marketplace::merchant_ledger_user(client_id);
                if delta_balance != purchase.merchant_net {
                    diffs.push(diff_entry(
                        &purchase.id,
                        DiffClass::AmountMismatch,
                        &format!(
                            "merchant delta {} != {}",
                            delta_balance, purchase.merchant_net
                        ),
                    ));
                }
                if user != expected_user {
                    diffs.push(diff_entry(
                        &purchase.id,
                        DiffClass::AmountMismatch,
                        "merchant operation has wrong account",
                    ));
                }
                merchant_delta_sum += delta_balance;
            }
            None => diffs.push(diff_entry(
                &purchase.id,
                DiffClass::MissingLedgerOp,
                "merchant operation not found",
            )),
        }
        // 平台费 operation。
        if purchase.fee_amount > 0 {
            if let Some(fee_op_id) = &purchase.fee_operation_id {
                let fee = fetch_operation(pool, fee_op_id).await?;
                match fee {
                    Some((delta_balance, _, _)) => {
                        if delta_balance != purchase.fee_amount {
                            diffs.push(diff_entry(
                                &purchase.id,
                                DiffClass::AmountMismatch,
                                &format!("fee delta {} != {}", delta_balance, purchase.fee_amount),
                            ));
                        }
                        fee_delta_sum += delta_balance;
                    }
                    None => diffs.push(diff_entry(
                        &purchase.id,
                        DiffClass::MissingLedgerOp,
                        "fee operation not found",
                    )),
                }
            } else {
                diffs.push(diff_entry(
                    &purchase.id,
                    DiffClass::MissingLedgerOp,
                    "fee_operation_id missing but fee_amount > 0",
                ));
            }
        }
    }

    // 窗口恒等式。
    let window_identity = buyer_delta_sum + merchant_delta_sum + fee_delta_sum;
    if window_identity != 0 {
        diffs.push(diff_entry(
            "window",
            DiffClass::IdentityBreak,
            &format!(
                "buyer {buyer_delta_sum} + merchant {merchant_delta_sum} + fee {fee_delta_sum} != 0"
            ),
        ));
    }

    // 商户运营余额 vs 账本余额（含补偿/冲正）。
    let ledger_balance =
        fetch_account_balance(pool, &crate::marketplace::merchant_ledger_user(client_id)).await?;
    let account = balance::get_account(pool, client_id, CURRENCY_COIN).await?;
    if let (Some(account), Some(ledger_balance)) = (&account, ledger_balance) {
        if account.total() != ledger_balance {
            diffs.push(diff_entry(
                client_id,
                DiffClass::MerchantBalanceMismatch,
                &format!(
                    "operational {} != ledger {}",
                    account.total(),
                    ledger_balance
                ),
            ));
        }
    }

    let status = if diffs.is_empty() {
        "consistent"
    } else {
        "diff_found"
    };
    let record_id = uuid::Uuid::now_v7().to_string();
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "INSERT INTO reconciliation_records
             (id, client_id, after_cursor, purchases_count, amount_sum, fee_sum, ledger_delta_sum, status, diffs_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record_id)
        .bind(client_id)
        .bind(after_cursor)
        .bind(purchases.len() as i64)
        .bind(amount_sum)
        .bind(fee_sum)
        .bind(window_identity)
        .bind(status)
        .bind(serde_json::to_string(&diffs).unwrap_or_else(|_| "[]".into()))
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "INSERT INTO reconciliation_records
             (id, client_id, after_cursor, purchases_count, amount_sum, fee_sum, ledger_delta_sum, status, diffs_json, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&record_id)
        .bind(client_id)
        .bind(after_cursor)
        .bind(purchases.len() as i64)
        .bind(amount_sum)
        .bind(fee_sum)
        .bind(window_identity)
        .bind(status)
        .bind(serde_json::to_string(&diffs).unwrap_or_else(|_| "[]".into()))
        .bind(now)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::Db("insert reconciliation failed".into()));
    }
    Ok(json!({
        "id": record_id,
        "client_id": client_id,
        "after_cursor": after_cursor,
        "purchases_count": purchases.len(),
        "amount_sum": amount_sum,
        "fee_sum": fee_sum,
        "window_identity_sum": window_identity,
        "status": status,
        "diffs": diffs,
        "created_at": now,
    }))
}

fn diff_entry(target: &str, class: DiffClass, detail: &str) -> Value {
    json!({
        "target": target,
        "class": class.as_str(),
        "detail": detail,
    })
}

/// 拉取对账窗口内的 Purchase（含 Refund 聚合的窗口）。
async fn fetch_purchases_after(
    pool: &DatabasePool,
    client_id: &str,
    after_cursor: i64,
) -> Result<Vec<ReconcilePurchase>, MarketplaceError> {
    let sql = "SELECT id, client_id, user_id, amount, fee_amount, merchant_net, currency_id, \
               refunded_amount, point_operation_id, merchant_operation_id, fee_operation_id, created_at \
               FROM purchases WHERE client_id = ? AND created_at > ? ORDER BY created_at ASC";
    let rows: Vec<ReconcilePurchase> = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(client_id)
            .bind(after_cursor)
            .fetch_all(p)
            .await?
            .iter()
            .map(|r| ReconcilePurchase {
                id: r.get("id"),
                amount: r.get("amount"),
                fee_amount: r.get("fee_amount"),
                merchant_net: r.get("merchant_net"),
                point_operation_id: r.get("point_operation_id"),
                merchant_operation_id: r.get("merchant_operation_id"),
                fee_operation_id: r.get("fee_operation_id"),
            })
            .collect(),
        Either::Right(p) => sqlx::query(sql)
            .bind(client_id)
            .bind(after_cursor)
            .fetch_all(p)
            .await?
            .iter()
            .map(|r| ReconcilePurchase {
                id: r.get("id"),
                amount: r.get("amount"),
                fee_amount: r.get("fee_amount"),
                merchant_net: r.get("merchant_net"),
                point_operation_id: r.get("point_operation_id"),
                merchant_operation_id: r.get("merchant_operation_id"),
                fee_operation_id: r.get("fee_operation_id"),
            })
            .collect(),
    };
    Ok(rows)
}

struct ReconcilePurchase {
    id: String,
    amount: i64,
    fee_amount: i64,
    merchant_net: i64,
    point_operation_id: String,
    merchant_operation_id: String,
    fee_operation_id: Option<String>,
}

/// 读取 operation 的 (delta_balance, user_id, source_type)。
async fn fetch_operation(
    pool: &DatabasePool,
    operation_id: &str,
) -> Result<Option<(i64, String, Option<String>)>, MarketplaceError> {
    let row: Option<(i64, String, Option<String>)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT pt.delta_balance, pt.user_id, po.source_type
             FROM point_transactions pt JOIN point_operations po ON po.id = pt.operation_id
             WHERE pt.operation_id = ? LIMIT 1",
            )
            .bind(operation_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT pt.delta_balance, pt.user_id, po.source_type
             FROM point_transactions pt JOIN point_operations po ON po.id = pt.operation_id
             WHERE pt.operation_id = ? LIMIT 1",
            )
            .bind(operation_id)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row)
}

/// 合成账户余额（point_accounts）。
async fn fetch_account_balance(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Option<i64>, MarketplaceError> {
    let row: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT balance + frozen_balance FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(CURRENCY_COIN)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT balance + frozen_balance FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(CURRENCY_COIN)
        .fetch_optional(p)
        .await?,
    };
    Ok(row)
}

/// 列出对账记录。
pub async fn list_reconciliations(
    pool: &DatabasePool,
    client_id: Option<&str>,
    after: Option<&str>,
    limit: i64,
) -> Result<Vec<Value>, MarketplaceError> {
    let limit = limit.clamp(1, 100);
    let sql = if client_id.is_some() {
        "SELECT id, client_id, after_cursor, purchases_count, amount_sum, fee_sum, ledger_delta_sum, status, diffs_json, created_at
         FROM reconciliation_records WHERE client_id = ? AND id > ? ORDER BY id ASC LIMIT ?"
    } else {
        "SELECT id, client_id, after_cursor, purchases_count, amount_sum, fee_sum, ledger_delta_sum, status, diffs_json, created_at
         FROM reconciliation_records WHERE id > ? ORDER BY id ASC LIMIT ?"
    };
    let rows: Vec<Value> = match pool {
        Either::Left(p) => {
            let r = if let Some(c) = client_id {
                sqlx::query(sql)
                    .bind(c)
                    .bind(after.unwrap_or(""))
                    .bind(limit + 1)
                    .fetch_all(p)
                    .await?
            } else {
                sqlx::query(sql)
                    .bind(after.unwrap_or(""))
                    .bind(limit + 1)
                    .fetch_all(p)
                    .await?
            };
            r.iter()
                .map(|row| {
                    json!({
                        "id": row.get::<String,_>("id"),
                        "client_id": row.get::<String,_>("client_id"),
                        "after_cursor": row.get::<i64,_>("after_cursor"),
                        "purchases_count": row.get::<i64,_>("purchases_count"),
                        "amount_sum": row.get::<i64,_>("amount_sum"),
                        "fee_sum": row.get::<i64,_>("fee_sum"),
                        "ledger_delta_sum": row.get::<i64,_>("ledger_delta_sum"),
                        "status": row.get::<String,_>("status"),
                        "diffs": serde_json::from_str::<Value>(&row.get::<String,_>("diffs_json")).unwrap_or_else(|_| json!([])),
                        "created_at": row.get::<i64,_>("created_at"),
                    })
                })
                .collect()
        }
        Either::Right(p) => {
            let r = if let Some(c) = client_id {
                sqlx::query(sql)
                    .bind(c)
                    .bind(after.unwrap_or(""))
                    .bind(limit + 1)
                    .fetch_all(p)
                    .await?
            } else {
                sqlx::query(sql)
                    .bind(after.unwrap_or(""))
                    .bind(limit + 1)
                    .fetch_all(p)
                    .await?
            };
            r.iter()
                .map(|row| {
                    json!({
                        "id": row.get::<String,_>("id"),
                        "client_id": row.get::<String,_>("client_id"),
                        "after_cursor": row.get::<i64,_>("after_cursor"),
                        "purchases_count": row.get::<i64,_>("purchases_count"),
                        "amount_sum": row.get::<i64,_>("amount_sum"),
                        "fee_sum": row.get::<i64,_>("fee_sum"),
                        "ledger_delta_sum": row.get::<i64,_>("ledger_delta_sum"),
                        "status": row.get::<String,_>("status"),
                        "diffs": serde_json::from_str::<Value>(&row.get::<String,_>("diffs_json")).unwrap_or_else(|_| json!([])),
                        "created_at": row.get::<i64,_>("created_at"),
                    })
                })
                .collect()
        }
    };
    Ok(rows.into_iter().take(limit as usize).collect())
}
