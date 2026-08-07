//! M07-LEVELS-05/06/07：活动奖励领取引擎、配置与撤销。
//!
//! 构建在 0050 迁移之上（`activity_rules`/`activity_claims`）：
//!
//! - **幂等去重**（M07-LEVELS-05）：`(rule_id, user_id, deduplication_key)`
//!   唯一约束 + 账本 `(scope, key)` 幂等，并发开页/刷新共享同一领取结果；
//!   `deduplication_key` 约定：签到 `{user_id}:{activity_day}:check_in`，
//!   反应 `{user_id}:{target_type}:{target_id}:{reaction}`，
//!   内容 `{user_id}:{kind}:{target_id}`。
//! - **每日奖励上限**：`rule.daily_limit`（按 `activity_day` 计数）。
//! - **延迟确认/撤销**（M07-LEVELS-06）：奖励先落账本+claim，管理员/审核可
//!   [`revoke_claim`]——只追加反向补偿流水（ledger `reversal`），claim 置
//!   `revoked`，不删除历史。
//! - **反刷**（M07-LEVELS-07）：排除自赞（`user_id == target_owner_id`）、
//!   撤赞重赞（同 dedup 周期唯一）、被处罚用户（`users.status` 非
//!   active/restricted）、失效规则；批量账号由路由层
//!   [`crate::ratelimit::RateLimiter`] 按 IP/设备限流。
//! - 管理配置（站点时区、默认签到规则、奖励开关）与任务（自定义任务奖励）
//!   存于 `activity_rules`（config 标记在 `conditions_json`），每次更新
//!   `version+1` 并写审计。

use serde_json::{json, Value};
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::db::DatabasePool;
use crate::economy::activity::checkin::{
    activity_day_for, claimed_on_day, resolve_user_timezone, streak_days, TzResolution,
    TIMEZONE_VERSION,
};
use crate::economy::ledger::service::{
    apply_operation, get_account, reversal, LedgerCommand, LedgerError, LedgerKind, CURRENCY_EXP,
};
use crate::economy::levels;
use crate::events::types::ACTIVITY_CLAIMED;
use crate::outbox::{enqueue, now_millis};

/// 默认站点时区（管理员配置缺省值）。
pub const DEFAULT_SITE_TIMEZONE: &str = "Asia/Shanghai";
/// 允许的规则类型（0050 CHECK 一致）。
pub const RULE_KINDS: &[&str] = &[
    "check_in",
    "task",
    "reaction",
    "post",
    "comment",
    "leaderboard",
];
/// 账本 idempotency scope（活动域）。
const LEDGER_SCOPE: &str = "activity";

/// 活动域错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ActivityError {
    Db(String),
    NotFound(String),
    Invalid(String),
    /// 同 key 已领取（幂等重放）。
    AlreadyClaimed,
    /// 未达条件/命中风控/每日上限/冷却。
    NotEligible(String),
    Ledger(String),
}

impl From<sqlx::Error> for ActivityError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl From<LedgerError> for ActivityError {
    fn from(e: LedgerError) -> Self {
        Self::Ledger(e.to_string())
    }
}

impl From<levels::LevelError> for ActivityError {
    fn from(e: levels::LevelError) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for ActivityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "activity db error: {msg}"),
            Self::NotFound(msg) => write!(f, "activity not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid activity input: {msg}"),
            Self::AlreadyClaimed => write!(f, "already claimed"),
            Self::NotEligible(msg) => write!(f, "activity not eligible: {msg}"),
            Self::Ledger(msg) => write!(f, "activity ledger error: {msg}"),
        }
    }
}

impl std::error::Error for ActivityError {}

/// 奖励金额（货币 code + 整数最小单位）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RewardValue {
    pub currency: String,
    pub amount: i64,
}

impl RewardValue {
    pub fn to_value(&self) -> Value {
        json!({ "currency": self.currency, "amount": self.amount })
    }
}

/// 活动规则行（`activity_rules`）。
#[derive(Debug, Clone)]
pub struct ActivityRuleRow {
    pub id: String,
    pub kind: String,
    pub currency_id: String,
    pub amount: i64,
    pub daily_limit: Option<i64>,
    pub cooldown_seconds: Option<i64>,
    pub conditions_json: Option<String>,
    pub version: i64,
    pub is_enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 活动配置（存于默认签到规则的 `conditions_json`，`config:true` 标记）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityConfig {
    pub site_timezone: String,
    pub timezone_version: String,
    pub check_in_enabled: bool,
    pub check_in_amount: i64,
    pub check_in_daily_limit: i64,
    pub rewards_enabled: bool,
    pub version: i64,
}

impl ActivityConfig {
    pub fn to_value(&self) -> Value {
        json!({
            "site_timezone": self.site_timezone,
            "timezone_version": self.timezone_version,
            "check_in": {
                "enabled": self.check_in_enabled,
                "amount": self.check_in_amount,
                "daily_limit": self.check_in_daily_limit,
            },
            "rewards_enabled": self.rewards_enabled,
            "version": self.version,
        })
    }
}

/// 单次领取结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimOutcome {
    /// 本次调用是否新产生奖励。
    pub claimed: bool,
    pub amount: i64,
    pub currency_id: String,
    pub operation_id: Option<String>,
}

/// 签到结果（`POST /api/v1/activity/visit`）。
#[derive(Debug, Clone)]
pub struct CheckInOutcome {
    pub first_today: bool,
    pub checked_in_today: bool,
    pub streak_days: i64,
    pub today_earned: Vec<RewardValue>,
    pub point_operation_id: Option<String>,
    pub activity_day: String,
    pub timezone: TzResolution,
}

/// 配置更新输入。
#[derive(Debug, Clone, Default)]
pub struct ActivityConfigUpdate {
    pub site_timezone: Option<String>,
    pub check_in_enabled: Option<bool>,
    pub check_in_amount: Option<i64>,
    pub check_in_daily_limit: Option<i64>,
    pub rewards_enabled: Option<bool>,
    pub reason: String,
}

/// 任务（自定义活动规则）创建/更新输入。
#[derive(Debug, Clone, Default)]
pub struct TaskInput {
    pub kind: Option<String>,
    pub amount: Option<i64>,
    pub currency_id: Option<String>,
    pub daily_limit: Option<i64>,
    pub cooldown_seconds: Option<i64>,
    pub conditions_json: Option<String>,
    pub is_enabled: Option<bool>,
}

// ─── 规则/配置读取 ─────────────────────────────────────────────────────

/// `activity_rules` 行元组（sqlx 列序；别名避免复杂类型 lint）。
type RuleRowTuple = (
    String,
    String,
    String,
    i64,
    Option<i64>,
    Option<i64>,
    Option<String>,
    i64,
    i64,
    i64,
    i64,
);

/// 从行元组构造规则。
#[allow(clippy::too_many_arguments)]
fn rule_from_row(
    id: String,
    kind: String,
    currency_id: String,
    amount: i64,
    daily_limit: Option<i64>,
    cooldown_seconds: Option<i64>,
    conditions_json: Option<String>,
    version: i64,
    is_enabled: i64,
    created_at: i64,
    updated_at: i64,
) -> ActivityRuleRow {
    ActivityRuleRow {
        id,
        kind,
        currency_id,
        amount,
        daily_limit,
        cooldown_seconds,
        conditions_json,
        version,
        is_enabled: is_enabled != 0,
        created_at,
        updated_at,
    }
}

/// 读取配置规则（`kind='check_in'` 且 `conditions_json` 含 `config:true`）。
async fn get_config_rule(pool: &DatabasePool) -> Result<Option<ActivityRuleRow>, ActivityError> {
    match pool {
        Either::Left(p) => {
            let row: Option<RuleRowTuple> = sqlx::query_as(
                "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
                 FROM activity_rules
                 WHERE kind = 'check_in' AND conditions_json LIKE '%\"config\":true%'
                 ORDER BY created_at, id LIMIT 1",
            )
            .fetch_optional(p)
            .await?;
            Ok(row.map(|r| rule_from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10)))
        }
        Either::Right(p) => {
            let row: Option<RuleRowTuple> = sqlx::query_as(
                "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
                 FROM activity_rules
                 WHERE kind = 'check_in' AND conditions_json LIKE '%\"config\":true%'
                 ORDER BY created_at, id LIMIT 1",
            )
            .fetch_optional(p)
            .await?;
            Ok(row.map(|r| rule_from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10)))
        }
    }
}

fn build_config_from_rule(rule: &ActivityRuleRow) -> ActivityConfig {
    let cond: Value = rule
        .conditions_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok())
        .unwrap_or(Value::Null);
    let obj = cond.as_object().cloned().unwrap_or_default();
    let site_timezone = obj
        .get("site_timezone")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_SITE_TIMEZONE)
        .to_string();
    let check_in_enabled = obj
        .get("check_in_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let rewards_enabled = obj
        .get("rewards_enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    ActivityConfig {
        site_timezone,
        timezone_version: TIMEZONE_VERSION.to_string(),
        check_in_enabled,
        check_in_amount: rule.amount,
        check_in_daily_limit: rule.daily_limit.unwrap_or(1),
        rewards_enabled,
        version: rule.version,
    }
}

/// 读取活动配置（未初始化时引导默认）。
pub async fn get_activity_config(pool: &DatabasePool) -> Result<ActivityConfig, ActivityError> {
    let rule = get_config_rule(pool)
        .await?
        .ok_or_else(|| ActivityError::NotFound("activity config not initialized".to_string()))?;
    Ok(build_config_from_rule(&rule))
}

/// 引导默认配置（幂等）：等级方案 + 默认签到规则。首次调用时创建。
///
/// 并发首次访问（新库上线瞬间）可能同时进入：SQLite 走 `BEGIN IMMEDIATE`
/// 整体写锁 + 事务内复查；MySQL 用 `INSERT ... SELECT ... WHERE NOT EXISTS`
/// 原子防重，保证只存在一条 config 规则（否则同日并发会重复领取两条规则）。
pub async fn ensure_default_activity_config(
    pool: &DatabasePool,
    now: i64,
) -> Result<ActivityConfig, ActivityError> {
    levels::ensure_default_scheme(pool, now)
        .await
        .map_err(ActivityError::from)?;
    let conditions = json!({
        "config": true,
        "site_timezone": DEFAULT_SITE_TIMEZONE,
        "timezone_version": TIMEZONE_VERSION,
        "check_in_enabled": true,
        "rewards_enabled": true,
    });
    let conditions_str =
        serde_json::to_string(&conditions).map_err(|e| ActivityError::Invalid(e.to_string()))?;
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<(), ActivityError> = async {
                let exists: Option<String> = sqlx::query_scalar(
                    "SELECT id FROM activity_rules
                     WHERE kind = 'check_in' AND conditions_json LIKE '%\"config\":true%' LIMIT 1",
                )
                .fetch_optional(&mut *conn)
                .await?;
                if exists.is_none() {
                    sqlx::query(
                        "INSERT INTO activity_rules
                             (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
                         VALUES (?, 'check_in', ?, 10, 1, NULL, ?, 1, 1, ?, ?)",
                    )
                    .bind(uuid::Uuid::now_v7().to_string())
                    .bind(CURRENCY_EXP)
                    .bind(&conditions_str)
                    .bind(now)
                    .bind(now)
                    .execute(&mut *conn)
                    .await?;
                }
                Ok(())
            }
            .await;
            match outcome {
                Ok(()) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    return Err(e);
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO activity_rules
                     (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
                 SELECT ?, 'check_in', ?, 10, 1, NULL, ?, 1, 1, ?, ?
                 WHERE NOT EXISTS (
                     SELECT 1 FROM activity_rules
                     WHERE kind = 'check_in' AND conditions_json LIKE '%\"config\":true%'
                 )",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(CURRENCY_EXP)
            .bind(&conditions_str)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
    }
    get_activity_config(pool).await
}

/// 启用中的签到规则（含 config 规则与运营追加的签到奖励规则）。
async fn list_enabled_check_in_rules(
    pool: &DatabasePool,
) -> Result<Vec<ActivityRuleRow>, ActivityError> {
    let rows: Vec<RuleRowTuple> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
             FROM activity_rules WHERE kind = 'check_in' AND is_enabled = 1
             ORDER BY created_at, id",
        )
        .fetch_all(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
             FROM activity_rules WHERE kind = 'check_in' AND is_enabled = 1
             ORDER BY created_at, id",
        )
        .fetch_all(p)
        .await?,
    };
    Ok(rows
        .into_iter()
        .map(|r| rule_from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10))
        .collect())
}

async fn load_rule(pool: &DatabasePool, rule_id: &str) -> Result<ActivityRuleRow, ActivityError> {
    match pool {
        Either::Left(p) => {
            let row: Option<RuleRowTuple> = sqlx::query_as(
                "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
                 FROM activity_rules WHERE id = ?",
            )
            .bind(rule_id)
            .fetch_optional(p)
            .await?;
            row.map(|r| rule_from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10))
                .ok_or_else(|| ActivityError::NotFound("rule not found".to_string()))
        }
        Either::Right(p) => {
            let row: Option<RuleRowTuple> = sqlx::query_as(
                "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
                 FROM activity_rules WHERE id = ?",
            )
            .bind(rule_id)
            .fetch_optional(p)
            .await?;
            row.map(|r| rule_from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10))
                .ok_or_else(|| ActivityError::NotFound("rule not found".to_string()))
        }
    }
}

async fn currency_code(pool: &DatabasePool, currency_id: &str) -> Result<String, ActivityError> {
    let code: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT code FROM currencies WHERE id = ?")
                .bind(currency_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT code FROM currencies WHERE id = ?")
                .bind(currency_id)
                .fetch_optional(p)
                .await?
        }
    };
    code.ok_or_else(|| ActivityError::NotFound("currency not found".to_string()))
}

// ─── 反刷：账号状态与自赞 ─────────────────────────────────────────────

/// 被处罚/未验证用户不可领取奖励（M07-LEVELS-07）。
async fn check_user_eligible(pool: &DatabasePool, user_id: &str) -> Result<(), ActivityError> {
    let status: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT status FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT status FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await?
        }
    };
    match status.as_deref() {
        Some("active") | Some("restricted") => Ok(()),
        _ => Err(ActivityError::NotEligible(
            "account not allowed for activity rewards".to_string(),
        )),
    }
}

// ─── 领取引擎 ─────────────────────────────────────────────────────────

/// 已领取（granted）计数（每日上限，M07-LEVELS-05）。
async fn count_granted(
    pool: &DatabasePool,
    rule_id: &str,
    user_id: &str,
    activity_day: &str,
) -> Result<i64, ActivityError> {
    let count: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM activity_claims
             WHERE rule_id = ? AND user_id = ? AND activity_day = ? AND status = 'granted'",
            )
            .bind(rule_id)
            .bind(user_id)
            .bind(activity_day)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM activity_claims
             WHERE rule_id = ? AND user_id = ? AND activity_day = ? AND status = 'granted'",
            )
            .bind(rule_id)
            .bind(user_id)
            .bind(activity_day)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(count.unwrap_or(0))
}

/// 该规则最近一次领取时间（冷却用）。
async fn last_claim_time(
    pool: &DatabasePool,
    rule_id: &str,
    user_id: &str,
) -> Result<Option<i64>, ActivityError> {
    match pool {
        Either::Left(p) => Ok(sqlx::query_scalar(
            "SELECT MAX(created_at) FROM activity_claims WHERE rule_id = ? AND user_id = ?",
        )
        .bind(rule_id)
        .bind(user_id)
        .fetch_optional(p)
        .await?
        .flatten()),
        Either::Right(p) => Ok(sqlx::query_scalar(
            "SELECT MAX(created_at) FROM activity_claims WHERE rule_id = ? AND user_id = ?",
        )
        .bind(rule_id)
        .bind(user_id)
        .fetch_optional(p)
        .await?
        .flatten()),
    }
}

/// 领取（去重原子写，M07-LEVELS-05）。`deduplication_key` 见模块文档。
///
/// 幂等语义：先查已有 claim——存在则重放（pending 占位自动补完成，崩溃恢复），
/// 不存在才做「每日上限/冷却 → 原子插入 → 账本」。`INSERT OR IGNORE` 唯一键
/// `(rule_id,user_id,key)` 兜底并发竞态；账本 `(scope,key)` 幂等保证同 key
/// 只奖励一次。重放/并发绝不触碰每日上限分支（不会误报 daily limit reached）。
pub async fn claim_rule(
    pool: &DatabasePool,
    rule: &ActivityRuleRow,
    user_id: &str,
    activity_day: &str,
    deduplication_key: &str,
    now: i64,
) -> Result<ClaimOutcome, ActivityError> {
    if !rule.is_enabled {
        return Err(ActivityError::NotEligible("rule disabled".to_string()));
    }
    check_user_eligible(pool, user_id).await?;

    // 幂等重放/崩溃补完成：已有 claim 优先。
    if let Some(existing) = load_claim(pool, &rule.id, user_id, deduplication_key).await? {
        if existing.point_operation_id.starts_with("pending:") {
            // 崩溃遗留：补完成（账本幂等，不会重复奖励）。
            let op_id =
                grant_via_ledger(pool, rule, user_id, deduplication_key, now, &existing.id).await?;
            update_claim_operation(pool, &existing.id, &op_id).await?;
            emit_claimed(pool, rule, user_id, deduplication_key, activity_day, &op_id).await;
            return Ok(ClaimOutcome {
                claimed: true,
                amount: rule.amount,
                currency_id: rule.currency_id.clone(),
                operation_id: Some(op_id),
            });
        }
        return Err(ActivityError::AlreadyClaimed);
    }

    // 每日上限 + 冷却（仅对新领取生效；重放走上面分支）。
    if let Some(limit) = rule.daily_limit {
        if limit <= 0 {
            return Err(ActivityError::NotEligible(
                "rule has no daily quota".to_string(),
            ));
        }
        let granted = count_granted(pool, &rule.id, user_id, activity_day).await?;
        if granted >= limit {
            return Err(ActivityError::NotEligible(
                "daily limit reached".to_string(),
            ));
        }
    }
    if let Some(cooldown) = rule.cooldown_seconds {
        if cooldown > 0 {
            if let Some(last) = last_claim_time(pool, &rule.id, user_id).await? {
                if last + cooldown * 1000 > now {
                    return Err(ActivityError::NotEligible(
                        "reward cooldown active".to_string(),
                    ));
                }
            }
        }
    }

    let claim_id = uuid::Uuid::now_v7().to_string();
    let pending_op = format!("pending:{claim_id}");
    let inserted = insert_claim_ignore(
        pool,
        &claim_id,
        &rule.id,
        user_id,
        activity_day,
        deduplication_key,
        &pending_op,
        now,
    )
    .await?;

    if !inserted {
        // 并发竞态：另一请求先插入同 key → 幂等重放。
        let existing = load_claim(pool, &rule.id, user_id, deduplication_key)
            .await?
            .ok_or_else(|| ActivityError::NotFound("claim row not found".to_string()))?;
        if existing.point_operation_id.starts_with("pending:") {
            // 崩溃遗留：补完成（账本幂等，不会重复奖励）。
            let op_id =
                grant_via_ledger(pool, rule, user_id, deduplication_key, now, &existing.id).await?;
            update_claim_operation(pool, &existing.id, &op_id).await?;
            emit_claimed(pool, rule, user_id, deduplication_key, activity_day, &op_id).await;
            return Ok(ClaimOutcome {
                claimed: true,
                amount: rule.amount,
                currency_id: rule.currency_id.clone(),
                operation_id: Some(op_id),
            });
        }
        return Err(ActivityError::AlreadyClaimed);
    }

    let op_id = grant_via_ledger(pool, rule, user_id, deduplication_key, now, &claim_id).await?;
    update_claim_operation(pool, &claim_id, &op_id).await?;
    emit_claimed(pool, rule, user_id, deduplication_key, activity_day, &op_id).await;
    Ok(ClaimOutcome {
        claimed: true,
        amount: rule.amount,
        currency_id: rule.currency_id.clone(),
        operation_id: Some(op_id),
    })
}

struct ClaimRow {
    id: String,
    point_operation_id: String,
}

async fn load_claim(
    pool: &DatabasePool,
    rule_id: &str,
    user_id: &str,
    deduplication_key: &str,
) -> Result<Option<ClaimRow>, ActivityError> {
    match pool {
        Either::Left(p) => {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT id, point_operation_id FROM activity_claims
                 WHERE rule_id = ? AND user_id = ? AND deduplication_key = ?",
            )
            .bind(rule_id)
            .bind(user_id)
            .bind(deduplication_key)
            .fetch_optional(p)
            .await?;
            Ok(row.map(|(id, point_operation_id)| ClaimRow {
                id,
                point_operation_id,
            }))
        }
        Either::Right(p) => {
            let row: Option<(String, String)> = sqlx::query_as(
                "SELECT id, point_operation_id FROM activity_claims
                 WHERE rule_id = ? AND user_id = ? AND deduplication_key = ?",
            )
            .bind(rule_id)
            .bind(user_id)
            .bind(deduplication_key)
            .fetch_optional(p)
            .await?;
            Ok(row.map(|(id, point_operation_id)| ClaimRow {
                id,
                point_operation_id,
            }))
        }
    }
}

/// 原子写 claim（唯一键去重；返回是否新插入）。
#[allow(clippy::too_many_arguments)]
async fn insert_claim_ignore(
    pool: &DatabasePool,
    claim_id: &str,
    rule_id: &str,
    user_id: &str,
    activity_day: &str,
    deduplication_key: &str,
    point_operation_id: &str,
    now: i64,
) -> Result<bool, ActivityError> {
    let affected = match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO activity_claims
                     (id, rule_id, user_id, activity_day, deduplication_key, point_operation_id, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'granted', ?, ?)",
            )
            .bind(claim_id)
            .bind(rule_id)
            .bind(user_id)
            .bind(activity_day)
            .bind(deduplication_key)
            .bind(point_operation_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?
            .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO activity_claims
                     (id, rule_id, user_id, activity_day, deduplication_key, point_operation_id, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, 'granted', ?, ?)",
            )
            .bind(claim_id)
            .bind(rule_id)
            .bind(user_id)
            .bind(activity_day)
            .bind(deduplication_key)
            .bind(point_operation_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?
            .rows_affected()
        }
    };
    Ok(affected == 1)
}

/// 账本发放（M07-LEVELS-05 奖励写入；amount=0 规则只记 claim 不写流水）。
async fn grant_via_ledger(
    pool: &DatabasePool,
    rule: &ActivityRuleRow,
    user_id: &str,
    deduplication_key: &str,
    now: i64,
    fallback_id: &str,
) -> Result<String, ActivityError> {
    if rule.amount <= 0 {
        return Ok(format!("zero:{fallback_id}"));
    }
    let cmd = LedgerCommand {
        idempotency_scope: LEDGER_SCOPE.to_string(),
        idempotency_key: format!("{user_id}:{}:{deduplication_key}", rule.id),
        kind: LedgerKind::Award,
        actor_id: None,
        user_id: user_id.to_string(),
        currency_id: rule.currency_id.clone(),
        delta_balance: rule.amount,
        delta_frozen: 0,
        source_type: Some("activity".to_string()),
        source_id: Some(rule.id.clone()),
        memo: format!("{} 奖励", rule.kind),
        reverses_operation_id: None,
    };
    let result = apply_operation(pool, cmd, now).await?;
    Ok(result.operation_id)
}

/// claim 行关联真实流水（仅更新 pending 占位，避免覆盖并发完成者）。
async fn update_claim_operation(
    pool: &DatabasePool,
    claim_id: &str,
    operation_id: &str,
) -> Result<(), ActivityError> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE activity_claims SET point_operation_id = ?, updated_at = ?
                 WHERE id = ? AND point_operation_id LIKE 'pending:%'",
            )
            .bind(operation_id)
            .bind(now)
            .bind(claim_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE activity_claims SET point_operation_id = ?, updated_at = ?
                 WHERE id = ? AND point_operation_id LIKE 'pending:%'",
            )
            .bind(operation_id)
            .bind(now)
            .bind(claim_id)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

async fn emit_claimed(
    pool: &DatabasePool,
    rule: &ActivityRuleRow,
    user_id: &str,
    deduplication_key: &str,
    activity_day: &str,
    operation_id: &str,
) {
    let _ = enqueue(
        pool,
        ACTIVITY_CLAIMED,
        json!({
            "rule_id": rule.id,
            "user_id": user_id,
            "currency_id": rule.currency_id,
            "amount": rule.amount,
            "deduplication_key": deduplication_key,
            "activity_day": activity_day,
            "point_operation_id": operation_id,
        }),
    )
    .await;
}

// ─── 签到流程（M07-LEVELS-03/04/05）───────────────────────────────────

/// 签到领取：用户时区日界线 → 逐条领取启用签到规则 → 重建等级。
pub async fn claim_check_in(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<CheckInOutcome, ActivityError> {
    let config = ensure_default_activity_config(pool, now).await?;
    let tz = resolve_user_timezone(pool, user_id, &config.site_timezone).await?;
    let activity_day = activity_day_for(tz.offset_secs, now);

    let mut earned: Vec<RewardValue> = Vec::new();
    let mut first_operation: Option<String> = None;
    let mut any_claimed = false;

    if config.check_in_enabled && config.rewards_enabled {
        let rules = list_enabled_check_in_rules(pool).await?;
        for rule in rules {
            let dedup_key = format!("{user_id}:{activity_day}:check_in");
            match claim_rule(pool, &rule, user_id, &activity_day, &dedup_key, now).await {
                Ok(outcome) if outcome.claimed => {
                    any_claimed = true;
                    if first_operation.is_none() {
                        first_operation = outcome.operation_id.clone();
                    }
                    let code = currency_code(pool, &outcome.currency_id).await?;
                    earned.push(RewardValue {
                        currency: code,
                        amount: outcome.amount,
                    });
                }
                Ok(_) => {}
                Err(ActivityError::AlreadyClaimed) => {}
                Err(e) => return Err(e),
            }
        }
    }

    if any_claimed {
        // 等级重建：只写 user_levels 缓存与 level_events，不改账本与历史。
        let _ = levels::recompute_level(pool, user_id, "activity.check_in", now).await;
    }

    let checked_in_today = any_claimed || claimed_on_day(pool, user_id, &activity_day).await?;
    // 重放/并发场景：返回原领取的流水号（OpenAPI point_operation_id）。
    if first_operation.is_none() && checked_in_today {
        first_operation = latest_check_in_operation(pool, user_id, &activity_day).await?;
    }
    let streak = streak_days(pool, user_id, &activity_day).await?;

    Ok(CheckInOutcome {
        first_today: any_claimed,
        checked_in_today,
        streak_days: streak,
        today_earned: earned,
        point_operation_id: first_operation,
        activity_day,
        timezone: tz,
    })
}

/// 当日最近一条签到领取的流水号（非 pending；重放响应用）。
async fn latest_check_in_operation(
    pool: &DatabasePool,
    user_id: &str,
    activity_day: &str,
) -> Result<Option<String>, ActivityError> {
    let row: Option<Option<String>> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT ac.point_operation_id
             FROM activity_claims ac
             JOIN activity_rules ar ON ar.id = ac.rule_id
             WHERE ac.user_id = ? AND ac.activity_day = ? AND ar.kind = 'check_in'
               AND ac.status = 'granted' AND ac.point_operation_id NOT LIKE 'pending:%'
             ORDER BY ac.created_at DESC, ac.id DESC LIMIT 1",
            )
            .bind(user_id)
            .bind(activity_day)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT ac.point_operation_id
             FROM activity_claims ac
             JOIN activity_rules ar ON ar.id = ac.rule_id
             WHERE ac.user_id = ? AND ac.activity_day = ? AND ar.kind = 'check_in'
               AND ac.status = 'granted' AND ac.point_operation_id NOT LIKE 'pending:%'
             ORDER BY ac.created_at DESC, ac.id DESC LIMIT 1",
            )
            .bind(user_id)
            .bind(activity_day)
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row.flatten())
}

// ─── 反刷规则（M07-LEVELS-07）────────────────────────────────────────

/// 有效点赞奖励：排除自赞；同目标+反应去重（撤赞重赞不重复奖励）。
pub async fn claim_reaction_reward(
    pool: &DatabasePool,
    user_id: &str,
    target_owner_id: &str,
    target_type: &str,
    target_id: &str,
    reaction: &str,
    now: i64,
) -> Result<ClaimOutcome, ActivityError> {
    if user_id == target_owner_id {
        return Err(ActivityError::NotEligible(
            "self-reaction rewards are excluded".to_string(),
        ));
    }
    let config = ensure_default_activity_config(pool, now).await?;
    if !config.rewards_enabled {
        return Err(ActivityError::NotEligible("rewards disabled".to_string()));
    }
    let rule = list_first_enabled_rule(pool, "reaction")
        .await?
        .ok_or_else(|| ActivityError::NotEligible("no reaction rule configured".to_string()))?;
    let tz = resolve_user_timezone(pool, user_id, &config.site_timezone).await?;
    let activity_day = activity_day_for(tz.offset_secs, now);
    let dedup_key = format!("{user_id}:{target_type}:{target_id}:{reaction}");
    claim_rule(pool, &rule, user_id, &activity_day, &dedup_key, now).await
}

/// 内容奖励（发帖/有效回复）：同目标去重，防删除重发重复奖励。
pub async fn claim_content_reward(
    pool: &DatabasePool,
    user_id: &str,
    kind: &str,
    target_id: &str,
    now: i64,
) -> Result<ClaimOutcome, ActivityError> {
    if !matches!(kind, "post" | "comment") {
        return Err(ActivityError::Invalid(format!("unsupported kind {kind}")));
    }
    let config = ensure_default_activity_config(pool, now).await?;
    if !config.rewards_enabled {
        return Err(ActivityError::NotEligible("rewards disabled".to_string()));
    }
    let rule = list_first_enabled_rule(pool, kind)
        .await?
        .ok_or_else(|| ActivityError::NotEligible(format!("no {kind} rule configured")))?;
    let tz = resolve_user_timezone(pool, user_id, &config.site_timezone).await?;
    let activity_day = activity_day_for(tz.offset_secs, now);
    let dedup_key = format!("{user_id}:{kind}:{target_id}");
    claim_rule(pool, &rule, user_id, &activity_day, &dedup_key, now).await
}

async fn list_first_enabled_rule(
    pool: &DatabasePool,
    kind: &str,
) -> Result<Option<ActivityRuleRow>, ActivityError> {
    match pool {
        Either::Left(p) => {
            let row: Option<RuleRowTuple> = sqlx::query_as(
                "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
                 FROM activity_rules WHERE kind = ? AND is_enabled = 1
                 ORDER BY created_at, id LIMIT 1",
            )
            .bind(kind)
            .fetch_optional(p)
            .await?;
            Ok(row.map(|r| rule_from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10)))
        }
        Either::Right(p) => {
            let row: Option<RuleRowTuple> = sqlx::query_as(
                "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
                 FROM activity_rules WHERE kind = ? AND is_enabled = 1
                 ORDER BY created_at, id LIMIT 1",
            )
            .bind(kind)
            .fetch_optional(p)
            .await?;
            Ok(row.map(|r| rule_from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10)))
        }
    }
}

// ─── 撤销（M07-LEVELS-06）────────────────────────────────────────────

/// 撤销奖励：只追加反向补偿流水（ledger reversal）+ claim 置 `revoked`，
/// 不删除/修改历史。`zero:`（未写流水）与 `pending:`（未完成）行直接置
/// revoked。
pub async fn revoke_claim(
    pool: &DatabasePool,
    actor_id: &str,
    claim_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), ActivityError> {
    if reason.trim().is_empty() {
        return Err(ActivityError::Invalid("reason required".to_string()));
    }
    let (_rule_id, _user_id, op_id, status): (String, String, String, String) = match pool {
        Either::Left(p) => {
            let row: Option<(String, String, String, String)> = sqlx::query_as(
                "SELECT rule_id, user_id, point_operation_id, status FROM activity_claims WHERE id = ?",
            )
            .bind(claim_id)
            .fetch_optional(p)
            .await?;
            row.ok_or_else(|| ActivityError::NotFound("claim not found".to_string()))?
        }
        Either::Right(p) => {
            let row: Option<(String, String, String, String)> = sqlx::query_as(
                "SELECT rule_id, user_id, point_operation_id, status FROM activity_claims WHERE id = ?",
            )
            .bind(claim_id)
            .fetch_optional(p)
            .await?;
            row.ok_or_else(|| ActivityError::NotFound("claim not found".to_string()))?
        }
    };
    if status == "revoked" {
        return Err(ActivityError::Invalid("claim already revoked".to_string()));
    }
    if !op_id.starts_with("pending:") && !op_id.starts_with("zero:") {
        reversal(
            pool,
            LEDGER_SCOPE,
            &format!("revoke:{op_id}"),
            Some(actor_id),
            &op_id,
            reason,
            now,
        )
        .await?;
    }
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE activity_claims SET status = 'revoked', updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(claim_id)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE activity_claims SET status = 'revoked', updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(claim_id)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

// ─── 汇总投影（M07-LEVELS-01/02/09）──────────────────────────────────

/// 活动汇总：今日签到状态、连续天数、等级（服务端裁决权益）、经验余额。
pub async fn activity_summary(
    pool: &DatabasePool,
    user_id: &str,
    now: i64,
) -> Result<Value, ActivityError> {
    let config = ensure_default_activity_config(pool, now).await?;
    let tz = resolve_user_timezone(pool, user_id, &config.site_timezone).await?;
    let activity_day = activity_day_for(tz.offset_secs, now);

    // 等级新鲜度：以 exp 余额重建缓存（幂等，缓存失效不改账本与历史）。
    let _ = levels::recompute_level(pool, user_id, "activity.summary", now).await;
    let level = levels::level_projection(pool, user_id)
        .await
        .map_err(ActivityError::from)?;
    let checked_in_today = claimed_on_day(pool, user_id, &activity_day).await?;
    let streak = streak_days(pool, user_id, &activity_day).await?;
    let balance = match get_account(pool, user_id, CURRENCY_EXP).await {
        Ok(account) => account.balance,
        Err(LedgerError::NotFound(_)) => 0,
        Err(_) => 0,
    };

    Ok(json!({
        "activity_day": activity_day,
        "checked_in_today": checked_in_today,
        "streak_days": streak,
        "level": level,
        "experience": {
            "currency": "exp",
            "balance": balance,
        },
        "config": {
            "site_timezone": config.site_timezone,
            "timezone_version": TIMEZONE_VERSION,
        },
        "timezone": tz.to_value(),
    }))
}

// ─── 管理：任务与配置（M07-LEVELS-06/09）──────────────────────────────

/// 活动规则行 → 管理端 JSON 投影。
pub fn rule_to_value(rule: &ActivityRuleRow) -> Value {
    json!({
        "id": rule.id,
        "kind": rule.kind,
        "currency": rule.currency_id,
        "amount": rule.amount,
        "daily_limit": rule.daily_limit,
        "cooldown_seconds": rule.cooldown_seconds,
        "conditions": rule.conditions_json.as_deref().and_then(|s| serde_json::from_str::<Value>(s).ok()),
        "version": rule.version,
        "is_enabled": rule.is_enabled,
        "created_at": rule.created_at,
        "updated_at": rule.updated_at,
    })
}

/// 全部活动规则（管理端任务列表）。
pub async fn list_activity_tasks(pool: &DatabasePool) -> Result<Value, ActivityError> {
    let rows: Vec<RuleRowTuple> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
             FROM activity_rules ORDER BY created_at, id",
        )
        .fetch_all(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at
             FROM activity_rules ORDER BY created_at, id",
        )
        .fetch_all(p)
        .await?,
    };
    let items: Vec<Value> = rows
        .into_iter()
        .map(|r| rule_from_row(r.0, r.1, r.2, r.3, r.4, r.5, r.6, r.7, r.8, r.9, r.10))
        .map(|r| rule_to_value(&r))
        .collect();
    Ok(json!({ "items": items }))
}

fn validate_task_input(input: &TaskInput) -> Result<(), ActivityError> {
    if let Some(kind) = &input.kind {
        if !RULE_KINDS.contains(&kind.as_str()) {
            return Err(ActivityError::Invalid(format!(
                "kind must be one of {RULE_KINDS:?}"
            )));
        }
    }
    if let Some(amount) = input.amount {
        if amount < 0 {
            return Err(ActivityError::Invalid("amount must be >= 0".to_string()));
        }
    }
    if let Some(limit) = input.daily_limit {
        if limit <= 0 {
            return Err(ActivityError::Invalid(
                "daily_limit must be > 0".to_string(),
            ));
        }
    }
    if let Some(cooldown) = input.cooldown_seconds {
        if cooldown < 0 {
            return Err(ActivityError::Invalid(
                "cooldown_seconds must be >= 0".to_string(),
            ));
        }
    }
    Ok(())
}

/// 创建自定义任务奖励（reason + 审计，事务内；M07-LEVELS-06）。
pub async fn create_activity_task(
    pool: &DatabasePool,
    actor_id: &str,
    reason: &str,
    input: &TaskInput,
    now: i64,
) -> Result<ActivityRuleRow, ActivityError> {
    if reason.trim().is_empty() {
        return Err(ActivityError::Invalid("reason required".to_string()));
    }
    validate_task_input(input)?;
    let kind = input
        .kind
        .clone()
        .ok_or_else(|| ActivityError::Invalid("kind is required".to_string()))?;
    let amount = input.amount.unwrap_or(0);
    let currency_id = input
        .currency_id
        .clone()
        .unwrap_or_else(|| CURRENCY_EXP.to_string());
    let daily_limit = input.daily_limit;
    let cooldown_seconds = input.cooldown_seconds;
    let conditions_json = input.conditions_json.clone();
    let is_enabled = if input.is_enabled.unwrap_or(true) {
        1
    } else {
        0
    };
    let id = uuid::Uuid::now_v7().to_string();

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO activity_rules
                     (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&kind)
            .bind(&currency_id)
            .bind(amount)
            .bind(daily_limit)
            .bind(cooldown_seconds)
            .bind(&conditions_json)
            .bind(is_enabled)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            let audit = AuditEntry::user_action(actor_id, "admin.activity.task_create")
                .with_target("activity_rule", &id)
                .with_effective_role("administrator")
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .with_metadata(json!({ "kind": kind, "amount": amount }));
            let mut otx = crate::outbox::OutboxTx::Left(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "INSERT INTO activity_rules
                     (id, kind, currency_id, amount, daily_limit, cooldown_seconds, conditions_json, version, is_enabled, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, 1, ?, ?, ?)",
            )
            .bind(&id)
            .bind(&kind)
            .bind(&currency_id)
            .bind(amount)
            .bind(daily_limit)
            .bind(cooldown_seconds)
            .bind(&conditions_json)
            .bind(is_enabled)
            .bind(now)
            .bind(now)
            .execute(&mut *tx)
            .await?;
            let audit = AuditEntry::user_action(actor_id, "admin.activity.task_create")
                .with_target("activity_rule", &id)
                .with_effective_role("administrator")
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .with_metadata(json!({ "kind": kind, "amount": amount }));
            let mut otx = crate::outbox::OutboxTx::Right(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    load_rule(pool, &id).await
}

/// 更新任务规则（version+1 + reason + 审计，事务内）。
pub async fn update_activity_task(
    pool: &DatabasePool,
    actor_id: &str,
    reason: &str,
    task_id: &str,
    input: &TaskInput,
    now: i64,
) -> Result<ActivityRuleRow, ActivityError> {
    if reason.trim().is_empty() {
        return Err(ActivityError::Invalid("reason required".to_string()));
    }
    validate_task_input(input)?;
    let current = load_rule(pool, task_id).await?;
    let kind = input.kind.clone().unwrap_or(current.kind);
    let amount = input.amount.unwrap_or(current.amount);
    let currency_id = input.currency_id.clone().unwrap_or(current.currency_id);
    let daily_limit = input.daily_limit.or(current.daily_limit);
    let cooldown_seconds = input.cooldown_seconds.or(current.cooldown_seconds);
    let conditions_json = input.conditions_json.clone().or(current.conditions_json);
    let is_enabled = input.is_enabled.unwrap_or(current.is_enabled);
    let new_version = current.version + 1;
    let is_enabled_num = if is_enabled { 1 } else { 0 };

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "UPDATE activity_rules SET kind = ?, currency_id = ?, amount = ?, daily_limit = ?, cooldown_seconds = ?, conditions_json = ?, version = ?, is_enabled = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(&kind)
            .bind(&currency_id)
            .bind(amount)
            .bind(daily_limit)
            .bind(cooldown_seconds)
            .bind(&conditions_json)
            .bind(new_version)
            .bind(is_enabled_num)
            .bind(now)
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
            let audit = AuditEntry::user_action(actor_id, "admin.activity.task_update")
                .with_target("activity_rule", task_id)
                .with_effective_role("administrator")
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .with_metadata(json!({ "kind": kind, "amount": amount, "version": new_version }));
            let mut otx = crate::outbox::OutboxTx::Left(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "UPDATE activity_rules SET kind = ?, currency_id = ?, amount = ?, daily_limit = ?, cooldown_seconds = ?, conditions_json = ?, version = ?, is_enabled = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(&kind)
            .bind(&currency_id)
            .bind(amount)
            .bind(daily_limit)
            .bind(cooldown_seconds)
            .bind(&conditions_json)
            .bind(new_version)
            .bind(is_enabled_num)
            .bind(now)
            .bind(task_id)
            .execute(&mut *tx)
            .await?;
            let audit = AuditEntry::user_action(actor_id, "admin.activity.task_update")
                .with_target("activity_rule", task_id)
                .with_effective_role("administrator")
                .with_reason(reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .with_metadata(json!({ "kind": kind, "amount": amount, "version": new_version }));
            let mut otx = crate::outbox::OutboxTx::Right(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    load_rule(pool, task_id).await
}

/// 更新活动配置（version+1 + reason + 审计，事务内；M07-LEVELS-09）。
pub async fn update_activity_config(
    pool: &DatabasePool,
    actor_id: &str,
    input: &ActivityConfigUpdate,
    now: i64,
) -> Result<ActivityConfig, ActivityError> {
    if input.reason.trim().is_empty() {
        return Err(ActivityError::Invalid("reason required".to_string()));
    }
    if let Some(amount) = input.check_in_amount {
        if amount < 0 {
            return Err(ActivityError::Invalid(
                "check_in amount must be >= 0".to_string(),
            ));
        }
    }
    if let Some(limit) = input.check_in_daily_limit {
        if limit <= 0 {
            return Err(ActivityError::Invalid(
                "check_in daily_limit must be > 0".to_string(),
            ));
        }
    }
    if let Some(tz) = &input.site_timezone {
        if tz.trim().is_empty() {
            return Err(ActivityError::Invalid(
                "site_timezone must not be empty".to_string(),
            ));
        }
    }

    let config = get_activity_config(pool).await?;
    let new_conditions = json!({
        "config": true,
        "site_timezone": input.site_timezone.clone().unwrap_or_else(|| config.site_timezone.clone()),
        "timezone_version": TIMEZONE_VERSION,
        "check_in_enabled": input.check_in_enabled.unwrap_or(config.check_in_enabled),
        "rewards_enabled": input.rewards_enabled.unwrap_or(config.rewards_enabled),
    });
    let new_conditions_str = serde_json::to_string(&new_conditions)
        .map_err(|e| ActivityError::Invalid(e.to_string()))?;
    let new_amount = input.check_in_amount.unwrap_or(config.check_in_amount);
    let new_daily_limit = input
        .check_in_daily_limit
        .unwrap_or(config.check_in_daily_limit);
    let new_version = config.version + 1;
    let rule_id = get_config_rule(pool)
        .await?
        .ok_or_else(|| ActivityError::NotFound("activity config not initialized".to_string()))?
        .id;

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "UPDATE activity_rules SET amount = ?, daily_limit = ?, conditions_json = ?, version = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(new_amount)
            .bind(new_daily_limit)
            .bind(&new_conditions_str)
            .bind(new_version)
            .bind(now)
            .bind(&rule_id)
            .execute(&mut *tx)
            .await?;
            let audit = AuditEntry::user_action(actor_id, "admin.activity.config_update")
                .with_target("config", "activity")
                .with_effective_role("administrator")
                .with_reason(&input.reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .with_metadata(json!({ "version": new_version }));
            let mut otx = crate::outbox::OutboxTx::Left(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "UPDATE activity_rules SET amount = ?, daily_limit = ?, conditions_json = ?, version = ?, updated_at = ?
                 WHERE id = ?",
            )
            .bind(new_amount)
            .bind(new_daily_limit)
            .bind(&new_conditions_str)
            .bind(new_version)
            .bind(now)
            .bind(&rule_id)
            .execute(&mut *tx)
            .await?;
            let audit = AuditEntry::user_action(actor_id, "admin.activity.config_update")
                .with_target("config", "activity")
                .with_effective_role("administrator")
                .with_reason(&input.reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .with_metadata(json!({ "version": new_version }));
            let mut otx = crate::outbox::OutboxTx::Right(tx);
            audit.record_in_tx(&mut otx).await?;
            match otx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    get_activity_config(pool).await
}
