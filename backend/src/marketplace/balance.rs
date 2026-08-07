//! M12-SCHEMA-05 / M12-REFUND-04：商户 available/pending/frozen 余额。
//!
//! 商户余额 `marketplace_merchant_accounts` 是运营余额；资金变动全部通过
//! 不可变账本 operation（`source_type=marketplace_purchase`,
//! `source_id=purchase_id`）以 `merchant:{client_id}` 合成账户记账，恒等式
//! `Σ(delta_balance + delta_pending + delta_frozen) = 0` 由对账校验。
//!
//! 结算（pending→available，7 天后）只移动拆分不改变总额，用 version 条件
//! 更新 + 审计记录，不直接改历史流水。冻结（Client 冻结）把 available+pending
//! 转入 frozen，同样不删除交易。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::economy::ledger::service as ledger;
use crate::economy::ledger::service::{LedgerCommand, LedgerKind, CURRENCY_COIN};
use crate::marketplace::MarketplaceError;

/// 商户账户行。
#[derive(Debug, Clone)]
pub struct MerchantAccountRow {
    pub id: String,
    pub client_id: String,
    pub owner_user_id: String,
    pub currency_id: String,
    pub available_balance: i64,
    pub pending_balance: i64,
    pub frozen_balance: i64,
    pub status: String,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

impl MerchantAccountRow {
    pub fn total(&self) -> i64 {
        self.available_balance + self.pending_balance + self.frozen_balance
    }
}

const ACCOUNT_COLUMNS: &str = "id, client_id, owner_user_id, currency_id, available_balance, \
     pending_balance, frozen_balance, status, version, created_at, updated_at";

fn account_from_sqlite(row: &sqlx::sqlite::SqliteRow) -> MerchantAccountRow {
    account_from_row_sqlite(row)
}

pub fn account_from_row_sqlite(row: &sqlx::sqlite::SqliteRow) -> MerchantAccountRow {
    MerchantAccountRow {
        id: row.get("id"),
        client_id: row.get("client_id"),
        owner_user_id: row.get("owner_user_id"),
        currency_id: row.get("currency_id"),
        available_balance: row.get("available_balance"),
        pending_balance: row.get("pending_balance"),
        frozen_balance: row.get("frozen_balance"),
        status: row.get("status"),
        version: row.get("version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn account_from_mysql(row: &sqlx::mysql::MySqlRow) -> MerchantAccountRow {
    account_from_row_mysql(row)
}

pub fn account_from_row_mysql(row: &sqlx::mysql::MySqlRow) -> MerchantAccountRow {
    MerchantAccountRow {
        id: row.get("id"),
        client_id: row.get("client_id"),
        owner_user_id: row.get("owner_user_id"),
        currency_id: row.get("currency_id"),
        available_balance: row.get("available_balance"),
        pending_balance: row.get("pending_balance"),
        frozen_balance: row.get("frozen_balance"),
        status: row.get("status"),
        version: row.get("version"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

pub async fn get_account(
    pool: &DatabasePool,
    client_id: &str,
    currency_id: &str,
) -> Result<Option<MerchantAccountRow>, MarketplaceError> {
    let sql = format!(
        "SELECT {ACCOUNT_COLUMNS} FROM marketplace_merchant_accounts WHERE client_id = ? AND currency_id = ?"
    );
    let row = match pool {
        Either::Left(p) => sqlx::query(&sql)
            .bind(client_id)
            .bind(currency_id)
            .fetch_optional(p)
            .await?
            .map(|r| account_from_sqlite(&r)),
        Either::Right(p) => sqlx::query(&sql)
            .bind(client_id)
            .bind(currency_id)
            .fetch_optional(p)
            .await?
            .map(|r| account_from_mysql(&r)),
    };
    Ok(row)
}

/// 商户余额视图（管理员/所有者；不含他人数据）。
pub async fn balance_view(pool: &DatabasePool, client_id: &str) -> Result<Value, MarketplaceError> {
    let account = get_account(pool, client_id, CURRENCY_COIN)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("merchant account not found".into()))?;
    Ok(json!({
        "client_id": client_id,
        "currency_id": account.currency_id,
        "available_balance": account.available_balance,
        "pending_balance": account.pending_balance,
        "frozen_balance": account.frozen_balance,
        "total": account.total(),
        "status": account.status,
        "version": account.version,
    }))
}

/// 购买入账：merchant pending_balance += net（条件更新 + version）。
pub async fn credit_pending(
    pool: &DatabasePool,
    client_id: &str,
    amount: i64,
    now: i64,
) -> Result<(), MarketplaceError> {
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET pending_balance = pending_balance + ?, version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ?",
        )
        .bind(amount)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET pending_balance = pending_balance + ?, version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ?",
        )
        .bind(amount)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::NotFound(
            "merchant account not found".into(),
        ));
    }
    Ok(())
}

/// 结算到期：pending → available（总额不变；version 条件更新 + 审计）。
///
/// 仅结算指定金额内（默认 7 天等待期由调用方 Job 判定）。退款/冻结中的
/// Client 不结算（调用方在 Job 中按状态过滤）。
pub async fn settle_pending(
    pool: &DatabasePool,
    client_id: &str,
    amount: i64,
    now: i64,
) -> Result<i64, MarketplaceError> {
    if amount <= 0 {
        return Err(MarketplaceError::Invalid(
            "settle amount must be > 0".into(),
        ));
    }
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET pending_balance = pending_balance - ?, available_balance = available_balance + ?,
                 version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ? AND pending_balance >= ?",
        )
        .bind(amount)
        .bind(amount)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .bind(amount)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET pending_balance = pending_balance - ?, available_balance = available_balance + ?,
                 version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ? AND pending_balance >= ?",
        )
        .bind(amount)
        .bind(amount)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .bind(amount)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::Invalid(
            "pending balance insufficient to settle".into(),
        ));
    }
    let _ = AuditEntry::user_action("system", "marketplace.settlement")
        .with_target("client", client_id)
        .with_metadata(json!({ "amount": amount, "currency_id": CURRENCY_COIN }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(amount)
}

/// 退款扣减：先 pending（未结算），再 available；不足时返回差额。
///
/// 返回值 `Ok((从 pending 扣, 从 available 扣))`。调用方在退款事务内调用；
/// 若金额不足应让退款进入 requested 状态（不能变负）。
pub async fn debit_refund(
    pool: &DatabasePool,
    client_id: &str,
    amount: i64,
    now: i64,
) -> Result<(i64, i64), MarketplaceError> {
    if amount <= 0 {
        return Err(MarketplaceError::Invalid(
            "refund amount must be > 0".into(),
        ));
    }
    let account = get_account(pool, client_id, CURRENCY_COIN)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("merchant account not found".into()))?;
    if account.status == "frozen" {
        return Err(MarketplaceError::MerchantBalanceInsufficient);
    }
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
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET pending_balance = pending_balance - ?, available_balance = available_balance - ?,
                 version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ? AND version = ?",
        )
        .bind(from_pending)
        .bind(from_available)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .bind(account.version)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET pending_balance = pending_balance - ?, available_balance = available_balance - ?,
                 version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ? AND version = ?",
        )
        .bind(from_pending)
        .bind(from_available)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .bind(account.version)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::Db(
            "concurrent merchant account modification".into(),
        ));
    }
    Ok((from_pending, from_available))
}

/// Client 冻结：available+pending → frozen（不删除交易；历史可查）。
pub async fn freeze_merchant_funds(
    pool: &DatabasePool,
    client_id: &str,
    reason: &str,
    actor_id: &str,
    now: i64,
) -> Result<MerchantAccountRow, MarketplaceError> {
    let account = get_account(pool, client_id, CURRENCY_COIN)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("merchant account not found".into()))?;
    let freeze_amount = account.available_balance + account.pending_balance;
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET available_balance = 0, pending_balance = 0,
                 frozen_balance = frozen_balance + ?, status = 'frozen',
                 version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ? AND version = ?",
        )
        .bind(freeze_amount)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .bind(account.version)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET available_balance = 0, pending_balance = 0,
                 frozen_balance = frozen_balance + ?, status = 'frozen',
                 version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ? AND version = ?",
        )
        .bind(freeze_amount)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .bind(account.version)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::Db(
            "concurrent merchant account modification".into(),
        ));
    }
    let _ = AuditEntry::user_action(actor_id, "marketplace.merchant_freeze")
        .with_target("client", client_id)
        .with_reason(reason)
        .with_metadata(json!({ "frozen_amount": freeze_amount }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    get_account(pool, client_id, CURRENCY_COIN)
        .await?
        .ok_or_else(|| MarketplaceError::NotFound("merchant account".into()))
}

/// 管理员补足/冲正（M12-REFUND-07 修复补偿）：追加补偿 operation +
/// 商户 available 增加。只追加，不编辑历史。
pub async fn admin_compensate(
    pool: &DatabasePool,
    actor_id: &str,
    client_id: &str,
    amount: i64,
    reason: &str,
    now: i64,
) -> Result<Value, MarketplaceError> {
    if amount <= 0 {
        return Err(MarketplaceError::Invalid(
            "compensate amount must be > 0".into(),
        ));
    }
    if reason.trim().is_empty() {
        return Err(MarketplaceError::Invalid("reason required".into()));
    }
    let ledger_user = crate::marketplace::merchant_ledger_user(client_id);
    let cmd = LedgerCommand {
        idempotency_scope: format!("marketplace.compensate.{client_id}"),
        idempotency_key: uuid::Uuid::now_v7().to_string(),
        kind: LedgerKind::Adjust,
        actor_id: Some(actor_id.to_string()),
        user_id: ledger_user,
        currency_id: CURRENCY_COIN.to_string(),
        delta_balance: amount,
        delta_frozen: 0,
        source_type: Some("marketplace_compensation".to_string()),
        source_id: Some(client_id.to_string()),
        memo: format!("merchant compensation: {reason}"),
        reverses_operation_id: None,
    };
    let op = ledger::apply_operation(pool, cmd, now).await?;
    // 商户 available 增加（金额进 available，等待结算窗口规则由运营决定）。
    let rows = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET available_balance = available_balance + ?, version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ?",
        )
        .bind(amount)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE marketplace_merchant_accounts
             SET available_balance = available_balance + ?, version = version + 1, updated_at = ?
             WHERE client_id = ? AND currency_id = ?",
        )
        .bind(amount)
        .bind(now)
        .bind(client_id)
        .bind(CURRENCY_COIN)
        .execute(p)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(MarketplaceError::NotFound("merchant account".into()));
    }
    let _ = AuditEntry::user_action(actor_id, "marketplace.merchant_compensate")
        .with_target("client", client_id)
        .with_reason(reason)
        .with_metadata(json!({ "amount": amount, "operation_id": op.operation_id }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    balance_view(pool, client_id).await
}
