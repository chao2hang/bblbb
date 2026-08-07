//! M07-LEDGER：账本与账户内核。
//!
//! 构建在 0047 迁移之上：
//!
//! - [`apply_operation`]（M07-LEDGER-04/05/06/07）：debit/credit/freeze/
//!   unfreeze/adjust/reversal 统一原子入口；`(idempotency_scope, idempotency_key)`
//!   唯一键 + `request_hash` 实现「同键重放返回原流水、不同摘要冲突」；
//!   余额不足/负数/溢出/并发双扣全部回滚。
//! - 禁止项（M07-LEDGER-05）：无充值/提现/现金兑换/普通用户转账/现实价值
//!   承诺——命令面只有站内积分 kind，`Transfer` 直接被拒。
//! - 并发（M07-LEDGER-03）：SQLite `BEGIN IMMEDIATE` 整体写锁 + version
//!   乐观更新；MySQL/MariaDB `SELECT ... FOR UPDATE` 行锁 + version 校验。
//! - [`admin_grant`]（M07-LEDGER-09）：管理员发放要求 reason、`points.adjust`
//!   权限、审计与可配置双人复核。
//! - 奖励撤销/退款只追加反向补偿流水（M07-LEDGER-10），不更新/删除历史。

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::db::DatabasePool;
use crate::events::types::POINTS_OPERATION_COMPLETED;
use crate::outbox::{enqueue, now_millis};

/// 内置货币（0047 种子，三库一致）。
pub const CURRENCY_EXP: &str = "01911fd5-0047-0000-0000-000000000001";
pub const CURRENCY_COIN: &str = "01911fd5-0047-0000-0000-000000000002";

/// 账本操作类型（0047 CHECK 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LedgerKind {
    Award,
    Consume,
    ShopPurchase,
    Transfer,
    Freeze,
    Unfreeze,
    Adjust,
    Reversal,
}

impl LedgerKind {
    pub const ALL: [LedgerKind; 8] = [
        LedgerKind::Award,
        LedgerKind::Consume,
        LedgerKind::ShopPurchase,
        LedgerKind::Transfer,
        LedgerKind::Freeze,
        LedgerKind::Unfreeze,
        LedgerKind::Adjust,
        LedgerKind::Reversal,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            LedgerKind::Award => "award",
            LedgerKind::Consume => "consume",
            LedgerKind::ShopPurchase => "shop_purchase",
            LedgerKind::Transfer => "transfer",
            LedgerKind::Freeze => "freeze",
            LedgerKind::Unfreeze => "unfreeze",
            LedgerKind::Adjust => "adjust",
            LedgerKind::Reversal => "reversal",
        }
    }

    pub fn parse(s: &str) -> Option<LedgerKind> {
        Self::ALL.into_iter().find(|k| k.as_str() == s)
    }
}

/// 账本命令（每次调用对应一条 operation + 一条或多条 transaction）。
#[derive(Debug, Clone)]
pub struct LedgerCommand {
    pub idempotency_scope: String,
    pub idempotency_key: String,
    pub kind: LedgerKind,
    /// 系统操作可为 None。
    pub actor_id: Option<String>,
    pub user_id: String,
    pub currency_id: String,
    pub delta_balance: i64,
    pub delta_frozen: i64,
    pub source_type: Option<String>,
    pub source_id: Option<String>,
    pub memo: String,
    /// 补偿/撤销时引用原 operation（只追加，不更新历史）。
    pub reverses_operation_id: Option<String>,
}

impl LedgerCommand {
    /// 规范化的请求摘要（幂等冲突检测：同 key 不同摘要 → Conflict）。
    pub fn request_hash(&self) -> String {
        let canonical = format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.kind.as_str(),
            self.user_id,
            self.currency_id,
            self.delta_balance,
            self.delta_frozen,
            self.source_type.as_deref().unwrap_or(""),
            self.source_id.as_deref().unwrap_or("")
        );
        let digest = Sha256::digest(canonical.as_bytes());
        hex::encode(digest)
    }
}

/// 账本错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LedgerError {
    Db(String),
    NotFound(String),
    Invalid(String),
    /// 余额不足（可用或冻结余额不足以支付）。
    InsufficientBalance,
    /// 余额为负（非负货币或冻结余额转负）。
    NegativeBalance,
    /// 溢出（checked_add 失败）。
    Overflow,
    /// 并发修改：version 不匹配（双扣/双写）。
    ConcurrentModification,
    /// 幂等冲突：同 key 不同请求摘要。
    IdempotencyConflict,
    Forbidden(String),
}

impl From<sqlx::Error> for LedgerError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for LedgerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "ledger db error: {msg}"),
            Self::NotFound(msg) => write!(f, "ledger not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid ledger command: {msg}"),
            Self::InsufficientBalance => write!(f, "insufficient balance"),
            Self::NegativeBalance => write!(f, "negative balance"),
            Self::Overflow => write!(f, "ledger overflow"),
            Self::ConcurrentModification => write!(f, "concurrent modification"),
            Self::IdempotencyConflict => write!(f, "idempotency key reused with different payload"),
            Self::Forbidden(msg) => write!(f, "ledger forbidden: {msg}"),
        }
    }
}

impl std::error::Error for LedgerError {}

/// 一条不可变流水。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LedgerTxRow {
    pub id: String,
    pub operation_id: String,
    pub user_id: String,
    pub currency_id: String,
    pub delta_balance: i64,
    pub delta_frozen: i64,
    pub balance_after: i64,
    pub frozen_after: i64,
    pub created_at: i64,
}

/// operation 结果（重放时返回原流水）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationResult {
    pub operation_id: String,
    pub transactions: Vec<LedgerTxRow>,
}

/// 命令校验（M07-LEDGER-04/05）。
pub fn validate_command(cmd: &LedgerCommand) -> Result<(), LedgerError> {
    if cmd.kind == LedgerKind::Transfer {
        return Err(LedgerError::Invalid(
            "普通用户转账不可用（M07-LEDGER-05）".to_string(),
        ));
    }
    if cmd.delta_balance == 0 && cmd.delta_frozen == 0 {
        return Err(LedgerError::Invalid("no-op command".to_string()));
    }
    if matches!(cmd.kind, LedgerKind::Adjust | LedgerKind::Reversal) && cmd.memo.trim().is_empty() {
        return Err(LedgerError::Invalid(
            "memo required for adjust/reversal".to_string(),
        ));
    }
    // freeze/unfreeze 在可用与冻结之间平移：总额守恒。
    if matches!(cmd.kind, LedgerKind::Freeze | LedgerKind::Unfreeze)
        && cmd.delta_balance.checked_add(cmd.delta_frozen) != Some(0)
    {
        return Err(LedgerError::Invalid(
            "freeze/unfreeze must conserve total balance".to_string(),
        ));
    }
    // 禁止现实价值承诺类操作：不允许任何「现金/提现/兑换」形态。
    let lower = cmd.memo.to_lowercase();
    for banned in [
        "提现",
        "充值",
        "现金",
        "兑换法币",
        "real-world",
        "cash out",
        "withdraw to",
    ] {
        if lower.contains(banned) {
            return Err(LedgerError::Invalid(
                "real-value / cash operations are prohibited".to_string(),
            ));
        }
    }
    Ok(())
}

/// 账户状态（锁内读取）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountState {
    pub balance: i64,
    pub frozen_balance: i64,
    pub version: i64,
    pub allow_negative: bool,
}

/// 应用 delta（纯函数，可测）：余额/冻结转负、溢出在此判定。
pub fn apply_delta(
    account: AccountState,
    delta_balance: i64,
    delta_frozen: i64,
) -> Result<(i64, i64), LedgerError> {
    let new_balance = account
        .balance
        .checked_add(delta_balance)
        .ok_or(LedgerError::Overflow)?;
    let new_frozen = account
        .frozen_balance
        .checked_add(delta_frozen)
        .ok_or(LedgerError::Overflow)?;
    if new_frozen < 0 {
        return Err(LedgerError::NegativeBalance);
    }
    if new_balance < 0 && !account.allow_negative {
        return Err(LedgerError::InsufficientBalance);
    }
    Ok((new_balance, new_frozen))
}

/// 读取账户（不存在返回 NotFound）。
pub async fn get_account(
    pool: &DatabasePool,
    user_id: &str,
    currency_id: &str,
) -> Result<AccountState, LedgerError> {
    let row: Option<(i64, i64, i64)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT balance, frozen_balance, version FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(currency_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT balance, frozen_balance, version FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(currency_id)
        .fetch_optional(p)
        .await?,
    };
    let Some((balance, frozen_balance, version)) = row else {
        return Err(LedgerError::NotFound("point account not found".to_string()));
    };
    Ok(AccountState {
        balance,
        frozen_balance,
        version,
        allow_negative: false,
    })
}

/// 应用账本命令（M07-LEDGER-04/06/07）。
///
/// 原子流程：校验 → 幂等（同键重放/冲突）→ 锁账户 → 校验余额 → version
/// 乐观更新 → 写不可变流水 → 提交。任何失败回滚。
///
/// `explicit_auto_deref`：sqlx `Executor` 只实现于 `&mut Connection`，
/// `&mut PoolConnection` 不满足，`&mut *conn` 是必要显式解引用（clippy 误报）。
#[allow(clippy::explicit_auto_deref)]
pub async fn apply_operation(
    pool: &DatabasePool,
    cmd: LedgerCommand,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    validate_command(&cmd)?;
    let hash = cmd.request_hash();

    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            // SQLite：BEGIN IMMEDIATE 获得整体写锁（M07-LEDGER-03），
            // 避免读-改-写交错；配合 version 乐观更新双保险。
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<OperationResult, LedgerError> = async {
                // 1) 幂等：先查同键已有流水。
                if let Some(existing) = existing_operation_sqlite(&mut *conn, &cmd, &hash).await? {
                    return Ok(existing);
                }
                // 2) 插入 operation（BEGIN IMMEDIATE 下无并发写者）。
                let operation_id = uuid::Uuid::now_v7().to_string();
                insert_operation_sqlite(&mut *conn, &operation_id, &cmd, &hash, now).await?;
                // 2.5) 懒建账户（幂等 INSERT OR IGNORE；版本 0 起步）。
                sqlx::query(
                    "INSERT OR IGNORE INTO point_accounts
                         (user_id, currency_id, balance, frozen_balance, version, updated_at)
                     VALUES (?, ?, 0, 0, 0, ?)",
                )
                .bind(&cmd.user_id)
                .bind(&cmd.currency_id)
                .bind(now)
                .execute(&mut *conn)
                .await?;
                // 3) 读账户 + 校验。
                let (balance, frozen_balance, version) =
                    account_sqlite(&mut *conn, &cmd.user_id, &cmd.currency_id).await?;
                let allow_negative = currency_allow_negative_sqlite(&mut *conn, &cmd.currency_id)
                    .await?;
                let (new_balance, new_frozen) = apply_delta(
                    AccountState {
                        balance,
                        frozen_balance,
                        version,
                        allow_negative,
                    },
                    cmd.delta_balance,
                    cmd.delta_frozen,
                )?;
                // 4) version 乐观更新：rows==0 → 并发双写。
                let affected = sqlx::query(
                    "UPDATE point_accounts SET balance = ?, frozen_balance = ?, version = version + 1, updated_at = ?
                     WHERE user_id = ? AND currency_id = ? AND version = ?",
                )
                .bind(new_balance)
                .bind(new_frozen)
                .bind(now)
                .bind(&cmd.user_id)
                .bind(&cmd.currency_id)
                .bind(version)
                .execute(&mut *conn)
                .await?
                .rows_affected();
                if affected != 1 {
                    return Err(LedgerError::ConcurrentModification);
                }
                // 5) 不可变流水。
                insert_transaction_sqlite(
                    &mut *conn,
                    &operation_id,
                    &cmd,
                    now,
                    balance,
                    frozen_balance,
                    new_balance,
                    new_frozen,
                )
                .await?;
                Ok(OperationResult {
                    operation_id: operation_id.clone(),
                    transactions: vec![LedgerTxRow {
                        id: uuid::Uuid::now_v7().to_string(),
                        operation_id: operation_id.clone(),
                        user_id: cmd.user_id.clone(),
                        currency_id: cmd.currency_id.clone(),
                        delta_balance: cmd.delta_balance,
                        delta_frozen: cmd.delta_frozen,
                        balance_after: new_balance,
                        frozen_after: new_frozen,
                        created_at: now,
                    }],
                })
            }
            .await;
            match outcome {
                Ok(result) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    notify_operation(pool, &result, &cmd).await;
                    Ok(result)
                }
                Err(err) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(err)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let outcome: Result<OperationResult, LedgerError> = async {
                if let Some(existing) = existing_operation_mysql(&mut tx, &cmd, &hash).await? {
                    return Ok(existing);
                }
                let operation_id = uuid::Uuid::now_v7().to_string();
                // 唯一键兜底：并发同键会唯一冲突 → 重查返回原流水或冲突。
                if let Err(insert_err) =
                    insert_operation_mysql(&mut tx, &operation_id, &cmd, &hash, now).await
                {
                    if is_duplicate_key(&insert_err) {
                        if let Some(existing) =
                            existing_operation_mysql(&mut tx, &cmd, &hash).await?
                        {
                            return Ok(existing);
                        }
                        return Err(LedgerError::IdempotencyConflict);
                    }
                    return Err(LedgerError::from(insert_err));
                }
                // 行锁：SELECT ... FOR UPDATE（M07-LEDGER-03）。
                sqlx::query(
                    "INSERT IGNORE INTO point_accounts
                         (user_id, currency_id, balance, frozen_balance, version, updated_at)
                     VALUES (?, ?, 0, 0, 0, ?)",
                )
                .bind(&cmd.user_id)
                .bind(&cmd.currency_id)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                let (balance, frozen_balance, version) =
                    account_for_update_mysql(&mut tx, &cmd.user_id, &cmd.currency_id).await?;
                let allow_negative = currency_allow_negative_mysql(&mut tx, &cmd.currency_id).await?;
                let (new_balance, new_frozen) = apply_delta(
                    AccountState {
                        balance,
                        frozen_balance,
                        version,
                        allow_negative,
                    },
                    cmd.delta_balance,
                    cmd.delta_frozen,
                )?;
                let affected = sqlx::query(
                    "UPDATE point_accounts SET balance = ?, frozen_balance = ?, version = version + 1, updated_at = ?
                     WHERE user_id = ? AND currency_id = ? AND version = ?",
                )
                .bind(new_balance)
                .bind(new_frozen)
                .bind(now)
                .bind(&cmd.user_id)
                .bind(&cmd.currency_id)
                .bind(version)
                .execute(&mut *tx)
                .await?
                .rows_affected();
                if affected != 1 {
                    return Err(LedgerError::ConcurrentModification);
                }
                insert_transaction_mysql(
                    &mut tx,
                    &operation_id,
                    &cmd,
                    now,
                    balance,
                    frozen_balance,
                    new_balance,
                    new_frozen,
                )
                .await?;
                Ok(OperationResult {
                    operation_id: operation_id.clone(),
                    transactions: vec![LedgerTxRow {
                        id: uuid::Uuid::now_v7().to_string(),
                        operation_id: operation_id.clone(),
                        user_id: cmd.user_id.clone(),
                        currency_id: cmd.currency_id.clone(),
                        delta_balance: cmd.delta_balance,
                        delta_frozen: cmd.delta_frozen,
                        balance_after: new_balance,
                        frozen_after: new_frozen,
                        created_at: now,
                    }],
                })
            }
            .await;
            match outcome {
                Ok(result) => {
                    tx.commit().await?;
                    notify_operation(pool, &result, &cmd).await;
                    Ok(result)
                }
                Err(err) => {
                    let _ = tx.rollback().await;
                    Err(err)
                }
            }
        }
    }
}

/// 在调用方事务内执行账本操作（M07-SHOP-02 / M06-DOWNLOAD-04 同事务需求）。
///
/// 与 [`apply_operation`] 语义一致（幂等/锁账户/version 乐观更新/不可变流水），
/// 但不管理事务生命周期：调用方负责 `BEGIN IMMEDIATE`（SQLite）或事务 begin
/// （MySQL），并在成功后统一 commit；失败时整体回滚（含本函数已写部分）。
/// 调用方在提交前调用本函数，返回的 `OperationResult.operation_id` 用于关联
/// 订单/授权等业务行。
///
/// `explicit_auto_deref` 说明同 [`apply_operation`]：`&mut PoolConnection`/
/// `&mut Transaction` 需显式解引用才能作为 sqlx Executor。
#[allow(clippy::explicit_auto_deref)]
pub async fn apply_operation_in_sqlite_tx(
    conn: &mut sqlx::SqliteConnection,
    cmd: LedgerCommand,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    validate_command(&cmd)?;
    let hash = cmd.request_hash();
    // 幂等：同键同摘要返回原流水；同键不同摘要冲突。
    if let Some(existing) = existing_operation_sqlite(conn, &cmd, &hash).await? {
        return Ok(existing);
    }
    let operation_id = uuid::Uuid::now_v7().to_string();
    insert_operation_sqlite(conn, &operation_id, &cmd, &hash, now).await?;
    sqlx::query(
        "INSERT OR IGNORE INTO point_accounts
             (user_id, currency_id, balance, frozen_balance, version, updated_at)
         VALUES (?, ?, 0, 0, 0, ?)",
    )
    .bind(&cmd.user_id)
    .bind(&cmd.currency_id)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    let (balance, frozen_balance, version) =
        account_sqlite(conn, &cmd.user_id, &cmd.currency_id).await?;
    let allow_negative = currency_allow_negative_sqlite(conn, &cmd.currency_id).await?;
    let (new_balance, new_frozen) = apply_delta(
        AccountState {
            balance,
            frozen_balance,
            version,
            allow_negative,
        },
        cmd.delta_balance,
        cmd.delta_frozen,
    )?;
    let affected = sqlx::query(
        "UPDATE point_accounts SET balance = ?, frozen_balance = ?, version = version + 1, updated_at = ?
         WHERE user_id = ? AND currency_id = ? AND version = ?",
    )
    .bind(new_balance)
    .bind(new_frozen)
    .bind(now)
    .bind(&cmd.user_id)
    .bind(&cmd.currency_id)
    .bind(version)
    .execute(&mut *conn)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(LedgerError::ConcurrentModification);
    }
    insert_transaction_sqlite(
        conn,
        &operation_id,
        &cmd,
        now,
        balance,
        frozen_balance,
        new_balance,
        new_frozen,
    )
    .await?;
    Ok(OperationResult {
        operation_id: operation_id.clone(),
        transactions: vec![LedgerTxRow {
            id: uuid::Uuid::now_v7().to_string(),
            operation_id: operation_id.clone(),
            user_id: cmd.user_id.clone(),
            currency_id: cmd.currency_id.clone(),
            delta_balance: cmd.delta_balance,
            delta_frozen: cmd.delta_frozen,
            balance_after: new_balance,
            frozen_after: new_frozen,
            created_at: now,
        }],
    })
}

/// MySQL/MariaDB 版本：调用方已 begin 事务，本函数在事务内执行账本操作。
#[allow(clippy::explicit_auto_deref)]
pub async fn apply_operation_in_mysql_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    cmd: LedgerCommand,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    validate_command(&cmd)?;
    let hash = cmd.request_hash();
    if let Some(existing) = existing_operation_mysql(tx, &cmd, &hash).await? {
        return Ok(existing);
    }
    let operation_id = uuid::Uuid::now_v7().to_string();
    if let Err(insert_err) = insert_operation_mysql(tx, &operation_id, &cmd, &hash, now).await {
        if is_duplicate_key(&insert_err) {
            if let Some(existing) = existing_operation_mysql(tx, &cmd, &hash).await? {
                return Ok(existing);
            }
            return Err(LedgerError::IdempotencyConflict);
        }
        return Err(LedgerError::from(insert_err));
    }
    sqlx::query(
        "INSERT IGNORE INTO point_accounts
             (user_id, currency_id, balance, frozen_balance, version, updated_at)
         VALUES (?, ?, 0, 0, 0, ?)",
    )
    .bind(&cmd.user_id)
    .bind(&cmd.currency_id)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    let (balance, frozen_balance, version) =
        account_for_update_mysql(tx, &cmd.user_id, &cmd.currency_id).await?;
    let allow_negative = currency_allow_negative_mysql(tx, &cmd.currency_id).await?;
    let (new_balance, new_frozen) = apply_delta(
        AccountState {
            balance,
            frozen_balance,
            version,
            allow_negative,
        },
        cmd.delta_balance,
        cmd.delta_frozen,
    )?;
    let affected = sqlx::query(
        "UPDATE point_accounts SET balance = ?, frozen_balance = ?, version = version + 1, updated_at = ?
         WHERE user_id = ? AND currency_id = ? AND version = ?",
    )
    .bind(new_balance)
    .bind(new_frozen)
    .bind(now)
    .bind(&cmd.user_id)
    .bind(&cmd.currency_id)
    .bind(version)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    if affected != 1 {
        return Err(LedgerError::ConcurrentModification);
    }
    insert_transaction_mysql(
        tx,
        &operation_id,
        &cmd,
        now,
        balance,
        frozen_balance,
        new_balance,
        new_frozen,
    )
    .await?;
    Ok(OperationResult {
        operation_id: operation_id.clone(),
        transactions: vec![LedgerTxRow {
            id: uuid::Uuid::now_v7().to_string(),
            operation_id: operation_id.clone(),
            user_id: cmd.user_id.clone(),
            currency_id: cmd.currency_id.clone(),
            delta_balance: cmd.delta_balance,
            delta_frozen: cmd.delta_frozen,
            balance_after: new_balance,
            frozen_after: new_frozen,
            created_at: now,
        }],
    })
}

/// 奖励/入账（credit）。
pub async fn credit(
    pool: &DatabasePool,
    cmd: LedgerCommand,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    let mut cmd = cmd;
    cmd.kind = LedgerKind::Award;
    apply_operation(pool, cmd, now).await
}

/// 消费/扣减（debit）。
pub async fn debit(
    pool: &DatabasePool,
    cmd: LedgerCommand,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    let mut cmd = cmd;
    cmd.kind = LedgerKind::Consume;
    apply_operation(pool, cmd, now).await
}

/// 冻结：可用 → 冻结（总额守恒）。
pub async fn freeze(
    pool: &DatabasePool,
    mut cmd: LedgerCommand,
    amount: i64,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    cmd.kind = LedgerKind::Freeze;
    cmd.delta_balance = -amount;
    cmd.delta_frozen = amount;
    apply_operation(pool, cmd, now).await
}

/// 解冻：冻结 → 可用。
pub async fn unfreeze(
    pool: &DatabasePool,
    mut cmd: LedgerCommand,
    amount: i64,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    cmd.kind = LedgerKind::Unfreeze;
    cmd.delta_balance = amount;
    cmd.delta_frozen = -amount;
    apply_operation(pool, cmd, now).await
}

/// 读取原 operation 的流水（撤销/补偿用）。
async fn load_operation_deltas(
    pool: &DatabasePool,
    operation_id: &str,
) -> Result<(i64, i64), LedgerError> {
    let row: Option<(i64, i64)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT delta_balance, delta_frozen FROM point_transactions WHERE operation_id = ? LIMIT 1",
        )
        .bind(operation_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT delta_balance, delta_frozen FROM point_transactions WHERE operation_id = ? LIMIT 1",
        )
        .bind(operation_id)
        .fetch_optional(p)
        .await?,
    };
    let Some((d_balance, d_frozen)) = row else {
        return Err(LedgerError::NotFound(
            "original operation not found".to_string(),
        ));
    };
    Ok((d_balance, d_frozen))
}

/// 奖励撤销/退款：只写反向补偿流水（M07-LEDGER-10），不更新/删除历史。
pub async fn reversal(
    pool: &DatabasePool,
    scope: &str,
    key: &str,
    actor_id: Option<&str>,
    original_operation_id: &str,
    reason: &str,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    if reason.trim().is_empty() {
        return Err(LedgerError::Invalid("reason required".to_string()));
    }
    let (d_balance, d_frozen) = load_operation_deltas(pool, original_operation_id).await?;
    let cmd = LedgerCommand {
        idempotency_scope: scope.to_string(),
        idempotency_key: key.to_string(),
        kind: LedgerKind::Reversal,
        actor_id: actor_id.map(|s| s.to_string()),
        user_id: String::new(), // 由下面查询原 transaction 的 user/currency 决定
        currency_id: String::new(),
        delta_balance: -d_balance,
        delta_frozen: -d_frozen,
        source_type: Some("operation".to_string()),
        source_id: Some(original_operation_id.to_string()),
        memo: reason.to_string(),
        reverses_operation_id: Some(original_operation_id.to_string()),
    };
    // 从原流水取 user_id/currency_id。
    let (user_id, currency_id) = original_account(pool, original_operation_id).await?;
    let cmd = LedgerCommand {
        user_id,
        currency_id,
        ..cmd
    };
    apply_operation(pool, cmd, now).await
}

/// 补偿（人工账务修正，只追加）：kind=adjust，要求 memo。
pub async fn compensation(
    pool: &DatabasePool,
    cmd: LedgerCommand,
    now: i64,
) -> Result<OperationResult, LedgerError> {
    let mut cmd = cmd;
    cmd.kind = LedgerKind::Adjust;
    apply_operation(pool, cmd, now).await
}

async fn original_account(
    pool: &DatabasePool,
    operation_id: &str,
) -> Result<(String, String), LedgerError> {
    let row: Option<(String, String)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT user_id, currency_id FROM point_transactions WHERE operation_id = ? LIMIT 1",
        )
        .bind(operation_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT user_id, currency_id FROM point_transactions WHERE operation_id = ? LIMIT 1",
        )
        .bind(operation_id)
        .fetch_optional(p)
        .await?,
    };
    row.ok_or_else(|| LedgerError::NotFound("original operation not found".to_string()))
}

/// 写入余额快照（M07-LEDGER-01）。
pub async fn snapshot_balance(
    pool: &DatabasePool,
    user_id: &str,
    currency_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), LedgerError> {
    let account = get_account(pool, user_id, currency_id).await?;
    let id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO point_balance_snapshots (id, user_id, currency_id, balance, frozen_balance, snapshot_at, reason)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(currency_id)
            .bind(account.balance)
            .bind(account.frozen_balance)
            .bind(now)
            .bind(reason)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO point_balance_snapshots (id, user_id, currency_id, balance, frozen_balance, snapshot_at, reason)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(currency_id)
            .bind(account.balance)
            .bind(account.frozen_balance)
            .bind(now)
            .bind(reason)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

/// 管理员发放（M07-LEDGER-09）：reason + `points.adjust` 权限 + 审计 +
/// 可配置双人复核（`dual_review` 开启时要求第二审批人）。
pub async fn admin_grant(
    pool: &DatabasePool,
    actor_id: &str,
    input: AdminGrantInput,
    now: i64,
    dual_review: bool,
    second_approver: Option<&str>,
) -> Result<OperationResult, LedgerError> {
    if input.reason.trim().is_empty() {
        return Err(LedgerError::Invalid("reason required".to_string()));
    }
    if input.amount <= 0 {
        return Err(LedgerError::Invalid(
            "grant amount must be positive".to_string(),
        ));
    }
    if dual_review {
        let second = second_approver.filter(|s| *s != actor_id).ok_or_else(|| {
            LedgerError::Invalid("dual review requires a distinct second approver".to_string())
        })?;
        let _ = AuditEntry::user_action(actor_id, "ledger.admin_grant.second_approval")
            .with_target("user", &input.user_id)
            .with_effective_role("administrator")
            .with_reason(&input.reason)
            .with_policy_version(AUTHZ_POLICY_VERSION)
            .with_metadata(json!({ "second_approver": second }))
            .record(pool)
            .await;
    }
    let decision = authorize_action(pool, actor_id, "points.adjust", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(LedgerError::Db)?;
    if !decision.is_allowed() {
        return Err(LedgerError::Forbidden(
            "points.adjust permission required".to_string(),
        ));
    }
    let cmd = LedgerCommand {
        idempotency_scope: input.idempotency_scope,
        idempotency_key: input.idempotency_key,
        kind: LedgerKind::Award,
        actor_id: Some(actor_id.to_string()),
        user_id: input.user_id,
        currency_id: input.currency_id,
        delta_balance: input.amount,
        delta_frozen: 0,
        source_type: Some("admin_grant".to_string()),
        source_id: None,
        memo: input.reason.clone(),
        reverses_operation_id: None,
    };
    let result = apply_operation(pool, cmd, now).await?;
    let _ = AuditEntry::user_action(actor_id, "ledger.admin_grant")
        .with_target("user", &result.transactions[0].user_id)
        .with_effective_role("administrator")
        .with_reason(&input.reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "operation_id": result.operation_id }))
        .record(pool)
        .await;
    Ok(result)
}

/// 管理员发放输入。
#[derive(Debug, Clone)]
pub struct AdminGrantInput {
    pub user_id: String,
    pub currency_id: String,
    pub amount: i64,
    pub reason: String,
    pub idempotency_scope: String,
    pub idempotency_key: String,
}

/// 查询余额投影（安全：只含可用/冻结/总余额）。
pub async fn balance_projection(
    pool: &DatabasePool,
    user_id: &str,
    currency_id: &str,
) -> Result<Value, LedgerError> {
    let account = get_account(pool, user_id, currency_id).await?;
    Ok(json!({
        "user_id": user_id,
        "currency_id": currency_id,
        "balance": account.balance,
        "frozen_balance": account.frozen_balance,
        "total": account.balance + account.frozen_balance,
    }))
}

/// 供路由使用的助手：当前 Unix 毫秒。
pub fn now() -> i64 {
    now_millis()
}

async fn notify_operation(pool: &DatabasePool, result: &OperationResult, cmd: &LedgerCommand) {
    let tx = &result.transactions[0];
    let _ = enqueue(
        pool,
        POINTS_OPERATION_COMPLETED,
        json!({
            "operation_id": result.operation_id,
            "user_id": tx.user_id,
            "currency_id": tx.currency_id,
            "kind": cmd.kind.as_str(),
            "delta_balance": tx.delta_balance,
            "delta_frozen": tx.delta_frozen,
        }),
    )
    .await;
}

fn is_duplicate_key(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => {
            let code = db.code().map(|c| c.to_string()).unwrap_or_default();
            code == "23000" || code == "1062" || code.contains("2067") || code.contains("1555")
        }
        _ => false,
    }
}

// ─── SQLite 内部助手 ─────────────────────────────────────────────────────

async fn existing_operation_sqlite(
    conn: &mut sqlx::SqliteConnection,
    cmd: &LedgerCommand,
    hash: &str,
) -> Result<Option<OperationResult>, LedgerError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT id, request_hash FROM point_operations
         WHERE idempotency_scope = ? AND idempotency_key = ?",
    )
    .bind(&cmd.idempotency_scope)
    .bind(&cmd.idempotency_key)
    .fetch_optional(&mut *conn)
    .await?;
    let Some((operation_id, stored_hash)) = row else {
        return Ok(None);
    };
    if stored_hash != hash {
        return Err(LedgerError::IdempotencyConflict);
    }
    let txs = load_transactions_sqlite(&mut *conn, &operation_id).await?;
    Ok(Some(OperationResult {
        operation_id,
        transactions: txs,
    }))
}

async fn insert_operation_sqlite(
    conn: &mut sqlx::SqliteConnection,
    operation_id: &str,
    cmd: &LedgerCommand,
    hash: &str,
    now: i64,
) -> Result<(), LedgerError> {
    sqlx::query(
        "INSERT INTO point_operations
             (id, idempotency_scope, idempotency_key, request_hash, kind, actor_id, source_type, source_id, reverses_operation_id, memo, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(operation_id)
    .bind(&cmd.idempotency_scope)
    .bind(&cmd.idempotency_key)
    .bind(hash)
    .bind(cmd.kind.as_str())
    .bind(&cmd.actor_id)
    .bind(&cmd.source_type)
    .bind(&cmd.source_id)
    .bind(&cmd.reverses_operation_id)
    .bind(&cmd.memo)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn account_sqlite(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    currency_id: &str,
) -> Result<(i64, i64, i64), LedgerError> {
    let row: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT balance, frozen_balance, version FROM point_accounts WHERE user_id = ? AND currency_id = ?",
    )
    .bind(user_id)
    .bind(currency_id)
    .fetch_optional(&mut *conn)
    .await?;
    row.ok_or_else(|| LedgerError::NotFound("point account not found".to_string()))
}

async fn currency_allow_negative_sqlite(
    conn: &mut sqlx::SqliteConnection,
    currency_id: &str,
) -> Result<bool, LedgerError> {
    let row: Option<i64> = sqlx::query_scalar("SELECT allow_negative FROM currencies WHERE id = ?")
        .bind(currency_id)
        .fetch_optional(&mut *conn)
        .await?;
    row.map(|v| v != 0)
        .ok_or_else(|| LedgerError::NotFound("currency not found".to_string()))
}

/// 不可变流水写入（SQLite）。参数多但均为同一事务内的既定字段。
#[allow(clippy::too_many_arguments)]
async fn insert_transaction_sqlite(
    conn: &mut sqlx::SqliteConnection,
    operation_id: &str,
    cmd: &LedgerCommand,
    now: i64,
    balance_before: i64,
    frozen_before: i64,
    balance_after: i64,
    frozen_after: i64,
) -> Result<(), LedgerError> {
    sqlx::query(
        "INSERT INTO point_transactions
             (id, operation_id, user_id, currency_id, delta_balance, delta_frozen, balance_after, frozen_after, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(operation_id)
    .bind(&cmd.user_id)
    .bind(&cmd.currency_id)
    .bind(cmd.delta_balance)
    .bind(cmd.delta_frozen)
    .bind(balance_after)
    .bind(frozen_after)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    let _ = (balance_before, frozen_before);
    Ok(())
}

async fn load_transactions_sqlite(
    conn: &mut sqlx::SqliteConnection,
    operation_id: &str,
) -> Result<Vec<LedgerTxRow>, LedgerError> {
    let rows: Vec<TxRow> = sqlx::query_as::<_, TxRow>(
        "SELECT id, operation_id, user_id, currency_id, delta_balance, delta_frozen, balance_after, frozen_after, created_at
         FROM point_transactions WHERE operation_id = ? ORDER BY created_at",
    )
    .bind(operation_id)
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows.into_iter().map(TxRow::into_model).collect())
}

// ─── MySQL/MariaDB 内部助手 ─────────────────────────────────────────────

async fn existing_operation_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    cmd: &LedgerCommand,
    hash: &str,
) -> Result<Option<OperationResult>, LedgerError> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT id, request_hash FROM point_operations
         WHERE idempotency_scope = ? AND idempotency_key = ?",
    )
    .bind(&cmd.idempotency_scope)
    .bind(&cmd.idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((operation_id, stored_hash)) = row else {
        return Ok(None);
    };
    if stored_hash != hash {
        return Err(LedgerError::IdempotencyConflict);
    }
    let txs = load_transactions_mysql(&mut *tx, &operation_id).await?;
    Ok(Some(OperationResult {
        operation_id,
        transactions: txs,
    }))
}

async fn insert_operation_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    operation_id: &str,
    cmd: &LedgerCommand,
    hash: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO point_operations
             (id, idempotency_scope, idempotency_key, request_hash, kind, actor_id, source_type, source_id, reverses_operation_id, memo, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(operation_id)
    .bind(&cmd.idempotency_scope)
    .bind(&cmd.idempotency_key)
    .bind(hash)
    .bind(cmd.kind.as_str())
    .bind(&cmd.actor_id)
    .bind(&cmd.source_type)
    .bind(&cmd.source_id)
    .bind(&cmd.reverses_operation_id)
    .bind(&cmd.memo)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn account_for_update_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    currency_id: &str,
) -> Result<(i64, i64, i64), LedgerError> {
    let row: Option<(i64, i64, i64)> = sqlx::query_as(
        "SELECT balance, frozen_balance, version FROM point_accounts
         WHERE user_id = ? AND currency_id = ? FOR UPDATE",
    )
    .bind(user_id)
    .bind(currency_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.ok_or_else(|| LedgerError::NotFound("point account not found".to_string()))
}

async fn currency_allow_negative_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    currency_id: &str,
) -> Result<bool, LedgerError> {
    let row: Option<i64> = sqlx::query_scalar("SELECT allow_negative FROM currencies WHERE id = ?")
        .bind(currency_id)
        .fetch_optional(&mut **tx)
        .await?;
    row.map(|v| v != 0)
        .ok_or_else(|| LedgerError::NotFound("currency not found".to_string()))
}

/// 不可变流水写入（MySQL/MariaDB）。参数多但均为同一事务内的既有字段。
#[allow(clippy::too_many_arguments)]
async fn insert_transaction_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    operation_id: &str,
    cmd: &LedgerCommand,
    now: i64,
    balance_before: i64,
    frozen_before: i64,
    balance_after: i64,
    frozen_after: i64,
) -> Result<(), LedgerError> {
    sqlx::query(
        "INSERT INTO point_transactions
             (id, operation_id, user_id, currency_id, delta_balance, delta_frozen, balance_after, frozen_after, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(operation_id)
    .bind(&cmd.user_id)
    .bind(&cmd.currency_id)
    .bind(cmd.delta_balance)
    .bind(cmd.delta_frozen)
    .bind(balance_after)
    .bind(frozen_after)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    let _ = (balance_before, frozen_before);
    Ok(())
}

async fn load_transactions_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    operation_id: &str,
) -> Result<Vec<LedgerTxRow>, LedgerError> {
    let rows: Vec<TxRow> = sqlx::query_as::<_, TxRow>(
        "SELECT id, operation_id, user_id, currency_id, delta_balance, delta_frozen, balance_after, frozen_after, created_at
         FROM point_transactions WHERE operation_id = ? ORDER BY created_at",
    )
    .bind(operation_id)
    .fetch_all(&mut **tx)
    .await?;
    Ok(rows.into_iter().map(TxRow::into_model).collect())
}

/// 流水行（跨库同构）。
#[derive(sqlx::FromRow)]
struct TxRow {
    id: String,
    operation_id: String,
    user_id: String,
    currency_id: String,
    delta_balance: i64,
    delta_frozen: i64,
    balance_after: i64,
    frozen_after: i64,
    created_at: i64,
}

impl TxRow {
    fn into_model(self) -> LedgerTxRow {
        LedgerTxRow {
            id: self.id,
            operation_id: self.operation_id,
            user_id: self.user_id,
            currency_id: self.currency_id,
            delta_balance: self.delta_balance,
            delta_frozen: self.delta_frozen,
            balance_after: self.balance_after,
            frozen_after: self.frozen_after,
            created_at: self.created_at,
        }
    }
}
