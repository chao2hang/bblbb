//! M07-LEVELS-01/02：等级/经验模块。
//!
//! 构建在 0050 迁移之上（`level_schemes`/`levels`/`user_levels`/`level_events`）：
//!
//! - **经验来源**（M07-LEVELS-01）：签到、优质内容、有效点赞、回复、互动与
//!   活动奖励——全部通过账本 [`crate::economy::ledger::service`] 的 `exp`
//!   货币入账（`LedgerKind::Award`）。本模块只读余额与阈值，不自行改账。
//! - **等级公式**：等级 = `levels.threshold` 升序下「经验余额 ≥ 阈值」的最高
//!   一级；`level_schemes.is_active=1` 决定当前生效方案。
//! - **缓存失效**（M07-LEVELS-02）：`user_levels` 是可重建缓存，真实来源是
//!   `point_accounts` 的 exp 余额。`get_level` 读缓存；`recompute_level` 以
//!   余额对照阈值重建并同步 `users.level`（帖子可见性/等级门槛读取的缓存列），
//!   仅在等级变化时写 `level_events`（from/to/reason，只追加）。重建与缓存
//!   失效**不改变账本与历史奖励**——账本恒等式与补偿语义由 ledger 模块负责。
//! - **等级权益版本**（M07-LEVELS-01）：`benefits_json` 由服务端在投影时裁决
//!   （读库中 `levels.benefits_json`，从不信任客户端）；`benefits_version`
//!   由方案/等级行的 `updated_at` 最大值派生，供前端缓存比对。

use serde_json::{json, Value};
use sqlx::Either;

use crate::db::DatabasePool;
use crate::economy::ledger::service::{get_account, LedgerError, CURRENCY_EXP};
use crate::outbox::now_millis;

/// 内置默认等级方案（`ensure_default_scheme` 引导用；仅当库中无活跃方案时创建）。
pub const DEFAULT_SCHEME_NAME: &str = "default";
/// 默认方案货币 = 经验（真实来源账户）。
pub const DEFAULT_SCHEME_CURRENCY: &str = CURRENCY_EXP;

/// 等级错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LevelError {
    Db(String),
    NotFound(String),
    Invalid(String),
}

impl From<sqlx::Error> for LevelError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for LevelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Db(msg) => write!(f, "level db error: {msg}"),
            Self::NotFound(msg) => write!(f, "level not found: {msg}"),
            Self::Invalid(msg) => write!(f, "invalid level input: {msg}"),
        }
    }
}

impl std::error::Error for LevelError {}

/// 等级方案行（`level_schemes`）。
#[derive(Debug, Clone)]
pub struct LevelSchemeRow {
    pub id: String,
    pub name: String,
    pub currency_id: String,
    pub is_active: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 等级行（`levels`）。
#[derive(Debug, Clone)]
pub struct LevelRow {
    pub id: String,
    pub scheme_id: String,
    pub name: String,
    pub threshold: i64,
    pub sort_order: i64,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub benefits_json: Option<String>,
    pub updated_at: i64,
}

/// 等级快照（`user_levels` 缓存投影 + 服务端裁决的权益）。
#[derive(Debug, Clone)]
pub struct LevelSnapshot {
    pub user_id: String,
    pub scheme_id: String,
    pub level_id: String,
    pub name: String,
    pub sort_order: i64,
    pub threshold: i64,
    pub icon: Option<String>,
    pub color: Option<String>,
    pub benefits: Option<Value>,
    pub benefits_version: String,
    pub computed_from_balance: i64,
    pub updated_at: i64,
}

/// `recompute_level` 结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecomputeOutcome {
    /// 本次重建是否发生升降级（写入了 `level_events`）。
    pub changed: bool,
    /// 重建前的等级（无则为 None）。
    pub previous_level_id: Option<String>,
    /// 重建后的等级（无活跃方案/等级表为空则为 None）。
    pub current_level_id: Option<String>,
    pub from_level_id: Option<String>,
    pub to_level_id: Option<String>,
    /// 重建依据的经验余额。
    pub balance: i64,
    pub scheme_id: Option<String>,
}

/// 读取当前活跃方案（`is_active=1`；存在多个时取最先创建者）。
pub async fn get_active_scheme(pool: &DatabasePool) -> Result<Option<LevelSchemeRow>, LevelError> {
    let row: Option<(String, String, String, i64, i64, i64)> = match pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT id, name, currency_id, is_active, created_at, updated_at
             FROM level_schemes WHERE is_active = 1 ORDER BY created_at, id LIMIT 1",
            )
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_as(
                "SELECT id, name, currency_id, is_active, created_at, updated_at
             FROM level_schemes WHERE is_active = 1 ORDER BY created_at, id LIMIT 1",
            )
            .fetch_optional(p)
            .await?
        }
    };
    Ok(row.map(
        |(id, name, currency_id, is_active, created_at, updated_at)| LevelSchemeRow {
            id,
            name,
            currency_id,
            is_active: is_active != 0,
            created_at,
            updated_at,
        },
    ))
}

/// `levels` 行元组（sqlx 列序；别名避免复杂类型 lint）。
type LevelRowTuple = (
    String,
    String,
    String,
    i64,
    i64,
    Option<String>,
    Option<String>,
    Option<String>,
    i64,
);

/// `user_levels JOIN levels` 行元组（别名避免复杂类型 lint）。
type UserLevelTuple = (
    String,
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    i64,
    Option<String>,
    i64,
    i64,
    i64,
);

/// 列出方案全部等级（按 threshold 升序）。
pub async fn list_levels(
    pool: &DatabasePool,
    scheme_id: &str,
) -> Result<Vec<LevelRow>, LevelError> {
    let rows: Vec<LevelRowTuple> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT id, scheme_id, name, threshold, sort_order, icon, color, benefits_json, updated_at
             FROM levels WHERE scheme_id = ? ORDER BY threshold ASC, sort_order ASC",
        )
        .bind(scheme_id)
        .fetch_all(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT id, scheme_id, name, threshold, sort_order, icon, color, benefits_json, updated_at
             FROM levels WHERE scheme_id = ? ORDER BY threshold ASC, sort_order ASC",
        )
        .bind(scheme_id)
        .fetch_all(p)
        .await?,
    };
    Ok(rows
        .into_iter()
        .map(
            |(id, sid, name, threshold, sort_order, icon, color, benefits_json, updated_at)| {
                LevelRow {
                    id,
                    scheme_id: sid,
                    name,
                    threshold,
                    sort_order,
                    icon,
                    color,
                    benefits_json,
                    updated_at,
                }
            },
        )
        .collect())
}

/// 经验余额（账户不存在视为 0；EXP 非负）。
async fn exp_balance(pool: &DatabasePool, user_id: &str) -> i64 {
    match get_account(pool, user_id, CURRENCY_EXP).await {
        Ok(account) => account.balance,
        Err(LedgerError::NotFound(_)) => 0,
        Err(_) => 0,
    }
}

/// 等级权益版本：方案与等级行 `updated_at` 的最大值派生（服务端裁决版本）。
pub async fn benefits_version(pool: &DatabasePool, scheme_id: &str) -> Result<String, LevelError> {
    let max_updated: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT MAX(updated_at) FROM levels WHERE scheme_id = ?")
                .bind(scheme_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT MAX(updated_at) FROM levels WHERE scheme_id = ?")
                .bind(scheme_id)
                .fetch_optional(p)
                .await?
        }
    };
    Ok(match max_updated {
        Some(ts) => format!("lv-{ts}"),
        None => "lv-0".to_string(),
    })
}

/// 读 `user_levels` 缓存并投影（M07-LEVELS-02 快路径）。
///
/// 缓存缺失时惰性调用 [`recompute_level`] 重建（真实来源为 exp 余额）。
pub async fn get_level(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Option<LevelSnapshot>, LevelError> {
    let row: Option<UserLevelTuple> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT ul.user_id, ul.scheme_id, ul.level_id, l.name, l.icon, l.color, l.sort_order, l.threshold, l.benefits_json, l.updated_at, ul.computed_from_balance, ul.updated_at
             FROM user_levels ul
             JOIN levels l ON l.id = ul.level_id
             WHERE ul.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT ul.user_id, ul.scheme_id, ul.level_id, l.name, l.icon, l.color, l.sort_order, l.threshold, l.benefits_json, l.updated_at, ul.computed_from_balance, ul.updated_at
             FROM user_levels ul
             JOIN levels l ON l.id = ul.level_id
             WHERE ul.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await?,
    };
    let Some((
        uid,
        scheme_id,
        level_id,
        name,
        icon,
        color,
        sort_order,
        threshold,
        benefits_json,
        _level_updated,
        computed_from_balance,
        updated_at,
    )) = row
    else {
        // 缓存缺失 → 惰性重建一次。
        let _ = recompute_level(pool, user_id, "cache_miss", now_millis()).await?;
        return read_cached_level(pool, user_id).await;
    };
    let version = benefits_version(pool, &scheme_id).await?;
    Ok(Some(LevelSnapshot {
        user_id: uid,
        scheme_id,
        level_id,
        name,
        sort_order,
        threshold,
        icon,
        color,
        benefits: parse_benefits(benefits_json.as_deref()),
        benefits_version: version,
        computed_from_balance,
        updated_at,
    }))
}

/// 只读 `user_levels` 缓存（不触发重建；内部用）。
async fn read_cached_level(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Option<LevelSnapshot>, LevelError> {
    let row: Option<UserLevelTuple> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT ul.user_id, ul.scheme_id, ul.level_id, l.name, l.icon, l.color, l.sort_order, l.threshold, l.benefits_json, l.updated_at, ul.computed_from_balance, ul.updated_at
             FROM user_levels ul
             JOIN levels l ON l.id = ul.level_id
             WHERE ul.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT ul.user_id, ul.scheme_id, ul.level_id, l.name, l.icon, l.color, l.sort_order, l.threshold, l.benefits_json, l.updated_at, ul.computed_from_balance, ul.updated_at
             FROM user_levels ul
             JOIN levels l ON l.id = ul.level_id
             WHERE ul.user_id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await?,
    };
    let Some((
        uid,
        scheme_id,
        level_id,
        name,
        icon,
        color,
        sort_order,
        threshold,
        benefits_json,
        _level_updated,
        computed_from_balance,
        updated_at,
    )) = row
    else {
        return Ok(None);
    };
    let version = benefits_version(pool, &scheme_id).await?;
    Ok(Some(LevelSnapshot {
        user_id: uid,
        scheme_id,
        level_id,
        name,
        sort_order,
        threshold,
        icon,
        color,
        benefits: parse_benefits(benefits_json.as_deref()),
        benefits_version: version,
        computed_from_balance,
        updated_at,
    }))
}

fn parse_benefits(raw: Option<&str>) -> Option<Value> {
    raw.and_then(|s| serde_json::from_str(s).ok())
}

/// 以 exp 余额对照阈值重建 `user_levels`（M07-LEVELS-02）。
///
/// - 等级变化时写 `level_events`（from_level_id/to_level_id/reason，只追加）；
/// - 同步 `users.level` 缓存列（sort_order；供帖子可见性/等级门槛读取）；
/// - 等级未变但余额变化时仅刷新 `computed_from_balance`/`updated_at`；
/// - **不改变账本与历史奖励**（只写缓存与事件日志）。
pub async fn recompute_level(
    pool: &DatabasePool,
    user_id: &str,
    reason: &str,
    now: i64,
) -> Result<RecomputeOutcome, LevelError> {
    let balance = exp_balance(pool, user_id).await;
    let Some(scheme) = get_active_scheme(pool).await? else {
        return Ok(RecomputeOutcome {
            changed: false,
            previous_level_id: None,
            current_level_id: None,
            from_level_id: None,
            to_level_id: None,
            balance,
            scheme_id: None,
        });
    };
    let levels = list_levels(pool, &scheme.id).await?;
    let Some(target) = pick_level(&levels, balance) else {
        return Ok(RecomputeOutcome {
            changed: false,
            previous_level_id: None,
            current_level_id: None,
            from_level_id: None,
            to_level_id: None,
            balance,
            scheme_id: Some(scheme.id),
        });
    };

    // 读当前缓存（用于判断变化与事件 from/to）。
    let current: Option<(String, String, i64)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT level_id, scheme_id, computed_from_balance FROM user_levels WHERE user_id = ? AND scheme_id = ?",
        )
        .bind(user_id)
        .bind(&scheme.id)
        .fetch_optional(p)
        .await?,
        Either::Right(p) => sqlx::query_as(
            "SELECT level_id, scheme_id, computed_from_balance FROM user_levels WHERE user_id = ? AND scheme_id = ?",
        )
        .bind(user_id)
        .bind(&scheme.id)
        .fetch_optional(p)
        .await?,
    };

    match current {
        Some((level_id, _, computed_from)) if level_id == target.id && computed_from == balance => {
            // 完全未变：无写。
            Ok(RecomputeOutcome {
                changed: false,
                previous_level_id: Some(level_id.clone()),
                current_level_id: Some(level_id),
                from_level_id: None,
                to_level_id: None,
                balance,
                scheme_id: Some(scheme.id),
            })
        }
        Some((level_id, _, _)) => {
            // 等级或余额变化：刷新缓存 + （如变化）事件。
            let changed = level_id != target.id;
            write_cache_and_maybe_event(
                pool, user_id, &scheme.id, &level_id, &target.id, balance, reason, now, changed,
            )
            .await?;
            Ok(RecomputeOutcome {
                changed,
                previous_level_id: Some(level_id.clone()),
                current_level_id: Some(target.id.clone()),
                from_level_id: if changed { Some(level_id) } else { None },
                to_level_id: if changed {
                    Some(target.id.clone())
                } else {
                    None
                },
                balance,
                scheme_id: Some(scheme.id),
            })
        }
        None => {
            // 首次：初始升级事件（from = NULL）。
            write_cache_and_maybe_event(
                pool, user_id, &scheme.id, "", &target.id, balance, reason, now, true,
            )
            .await?;
            Ok(RecomputeOutcome {
                changed: true,
                previous_level_id: None,
                current_level_id: Some(target.id.clone()),
                from_level_id: None,
                to_level_id: Some(target.id.clone()),
                balance,
                scheme_id: Some(scheme.id),
            })
        }
    }
}

/// 纯函数：阈值表 → 目标等级（阈值升序，取「balance ≥ threshold」最高一级；
/// 低于所有阈值时取最低等级）。
fn pick_level(levels: &[LevelRow], balance: i64) -> Option<&LevelRow> {
    let mut best: Option<&LevelRow> = None;
    for l in levels {
        if balance >= l.threshold {
            best = Some(l);
        }
    }
    best.or_else(|| levels.first())
}

/// 写 `user_levels`（upsert）+ `users.level` + （如变化）`level_events`。
///
/// `explicit_auto_deref`：sqlx `Executor` 只实现于 `&mut Connection`，`&mut
/// PoolConnection` 需显式解引用（与 ledger 模块同一约定，clippy 误报）。
#[allow(clippy::explicit_auto_deref)]
#[allow(clippy::too_many_arguments)]
async fn write_cache_and_maybe_event(
    pool: &DatabasePool,
    user_id: &str,
    scheme_id: &str,
    from_level_id: &str,
    to_level_id: &str,
    balance: i64,
    reason: &str,
    now: i64,
    write_event: bool,
) -> Result<(), LevelError> {
    let from_level_id = if from_level_id.is_empty() {
        None
    } else {
        Some(from_level_id.to_string())
    };
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<(), LevelError> = async {
                upsert_user_level_sqlite(&mut *conn, user_id, scheme_id, to_level_id, balance, now)
                    .await?;
                if let Some(from) = &from_level_id {
                    insert_level_event_sqlite(
                        &mut *conn,
                        user_id,
                        scheme_id,
                        Some(from),
                        to_level_id,
                        reason,
                        now,
                    )
                    .await?;
                } else if write_event {
                    insert_level_event_sqlite(
                        &mut *conn,
                        user_id,
                        scheme_id,
                        None,
                        to_level_id,
                        reason,
                        now,
                    )
                    .await?;
                }
                sync_users_level_sqlite(&mut *conn, user_id, to_level_id, now).await?;
                Ok(())
            }
            .await;
            match outcome {
                Ok(()) => {
                    sqlx::query("COMMIT").execute(&mut *conn).await?;
                    Ok(())
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            upsert_user_level_mysql(&mut tx, user_id, scheme_id, to_level_id, balance, now).await?;
            if write_event {
                insert_level_event_mysql(
                    &mut tx,
                    user_id,
                    scheme_id,
                    from_level_id.clone(),
                    to_level_id,
                    reason,
                    now,
                )
                .await?;
            }
            sync_users_level_mysql(&mut tx, user_id, to_level_id, now).await?;
            tx.commit().await?;
            Ok(())
        }
    }
}

async fn upsert_user_level_sqlite(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    scheme_id: &str,
    level_id: &str,
    balance: i64,
    now: i64,
) -> Result<(), LevelError> {
    sqlx::query(
        "INSERT INTO user_levels (user_id, scheme_id, level_id, computed_from_balance, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON CONFLICT (user_id, scheme_id) DO UPDATE SET
           level_id = excluded.level_id,
           computed_from_balance = excluded.computed_from_balance,
           updated_at = excluded.updated_at",
    )
    .bind(user_id)
    .bind(scheme_id)
    .bind(level_id)
    .bind(balance)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn upsert_user_level_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    scheme_id: &str,
    level_id: &str,
    balance: i64,
    now: i64,
) -> Result<(), LevelError> {
    sqlx::query(
        "INSERT INTO user_levels (user_id, scheme_id, level_id, computed_from_balance, updated_at)
         VALUES (?, ?, ?, ?, ?)
         ON DUPLICATE KEY UPDATE
           level_id = VALUES(level_id),
           computed_from_balance = VALUES(computed_from_balance),
           updated_at = VALUES(updated_at)",
    )
    .bind(user_id)
    .bind(scheme_id)
    .bind(level_id)
    .bind(balance)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_level_event_sqlite(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    scheme_id: &str,
    from_level_id: Option<&str>,
    to_level_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), LevelError> {
    sqlx::query(
        "INSERT INTO level_events (id, user_id, scheme_id, from_level_id, to_level_id, reason, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(user_id)
    .bind(scheme_id)
    .bind(from_level_id)
    .bind(to_level_id)
    .bind(reason)
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

async fn insert_level_event_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    scheme_id: &str,
    from_level_id: Option<String>,
    to_level_id: &str,
    reason: &str,
    now: i64,
) -> Result<(), LevelError> {
    sqlx::query(
        "INSERT INTO level_events (id, user_id, scheme_id, from_level_id, to_level_id, reason, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(user_id)
    .bind(scheme_id)
    .bind(from_level_id)
    .bind(to_level_id)
    .bind(reason)
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn sync_users_level_sqlite(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    level_id: &str,
    now: i64,
) -> Result<(), LevelError> {
    // users.level 存的是等级 sort_order（数值）；此处从 levels 取。
    let sort_order: Option<i64> = sqlx::query_scalar("SELECT sort_order FROM levels WHERE id = ?")
        .bind(level_id)
        .fetch_optional(&mut *conn)
        .await?;
    if let Some(order) = sort_order {
        sqlx::query(
            "UPDATE users SET level = ?, level_updated_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(order)
        .bind(now)
        .bind(now)
        .bind(user_id)
        .execute(&mut *conn)
        .await?;
    }
    Ok(())
}

async fn sync_users_level_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    level_id: &str,
    now: i64,
) -> Result<(), LevelError> {
    let sort_order: Option<i64> = sqlx::query_scalar("SELECT sort_order FROM levels WHERE id = ?")
        .bind(level_id)
        .fetch_optional(&mut **tx)
        .await?;
    if let Some(order) = sort_order {
        sqlx::query(
            "UPDATE users SET level = ?, level_updated_at = ?, updated_at = ? WHERE id = ?",
        )
        .bind(order)
        .bind(now)
        .bind(now)
        .bind(user_id)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

/// 等级投影（summary 用）：服务端裁决权益 + 经验余额。无方案/等级返回 None。
pub async fn level_projection(
    pool: &DatabasePool,
    user_id: &str,
) -> Result<Option<Value>, LevelError> {
    let Some(level) = get_level(pool, user_id).await? else {
        return Ok(None);
    };
    Ok(Some(json!({
        "level_id": level.level_id,
        "name": level.name,
        "sort_order": level.sort_order,
        "threshold": level.threshold,
        "icon": level.icon,
        "color": level.color,
        "benefits": level.benefits,
        "benefits_version": level.benefits_version,
        "computed_from_balance": level.computed_from_balance,
    })))
}

/// 引导默认方案（幂等；仅当库中无活跃方案时创建 `default` 方案 + 10 级阈值）。
///
/// 迁移不预置等级数据，运营可在后台配置；测试与首次启动通过本函数自举。
#[allow(clippy::explicit_auto_deref)]
pub async fn ensure_default_scheme(pool: &DatabasePool, now: i64) -> Result<(), LevelError> {
    if get_active_scheme(pool).await?.is_some() {
        return Ok(());
    }
    let scheme_id = uuid::Uuid::now_v7().to_string();
    let name = DEFAULT_SCHEME_NAME;
    let currency_id = DEFAULT_SCHEME_CURRENCY;
    // 默认阈值曲线：0/100/300/600/1000/1500/2100/2800/3600/4500。
    let thresholds: [(i64, &str); 10] = [
        (0, "L1"),
        (100, "L2"),
        (300, "L3"),
        (600, "L4"),
        (1000, "L5"),
        (1500, "L6"),
        (2100, "L7"),
        (2800, "L8"),
        (3600, "L9"),
        (4500, "L10"),
    ];
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await?;
            sqlx::query("BEGIN IMMEDIATE").execute(&mut *conn).await?;
            let outcome: Result<(), LevelError> = async {
                if get_active_scheme_from_conn(&mut *conn).await?.is_some() {
                    return Ok(());
                }
                sqlx::query(
                    "INSERT INTO level_schemes (id, name, currency_id, is_active, created_at, updated_at)
                     VALUES (?, ?, ?, 1, ?, ?)",
                )
                .bind(&scheme_id)
                .bind(name)
                .bind(currency_id)
                .bind(now)
                .bind(now)
                .execute(&mut *conn)
                .await?;
                for (i, (threshold, lname)) in thresholds.iter().enumerate() {
                    sqlx::query(
                        "INSERT INTO levels (id, scheme_id, name, threshold, sort_order, icon, color, benefits_json, created_at, updated_at)
                         VALUES (?, ?, ?, ?, ?, NULL, NULL, ?, ?, ?)",
                    )
                    .bind(uuid::Uuid::now_v7().to_string())
                    .bind(&scheme_id)
                    .bind(lname)
                    .bind(threshold)
                    .bind((i + 1) as i64)
                    .bind(serde_json::to_string(&json!({
                        "badge": null,
                        "max_visibility": 1,
                        "perks": [],
                    })).unwrap_or_else(|_| "{}".to_string()))
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
                    Ok(())
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            let existing: Option<String> =
                sqlx::query_scalar("SELECT id FROM level_schemes WHERE is_active = 1 LIMIT 1")
                    .fetch_optional(&mut *tx)
                    .await?;
            if existing.is_none() {
                sqlx::query(
                    "INSERT INTO level_schemes (id, name, currency_id, is_active, created_at, updated_at)
                     VALUES (?, ?, ?, 1, ?, ?)",
                )
                .bind(&scheme_id)
                .bind(name)
                .bind(currency_id)
                .bind(now)
                .bind(now)
                .execute(&mut *tx)
                .await?;
                for (i, (threshold, lname)) in thresholds.iter().enumerate() {
                    sqlx::query(
                        "INSERT INTO levels (id, scheme_id, name, threshold, sort_order, icon, color, benefits_json, created_at, updated_at)
                         VALUES (?, ?, ?, ?, ?, NULL, NULL, ?, ?, ?)",
                    )
                    .bind(uuid::Uuid::now_v7().to_string())
                    .bind(&scheme_id)
                    .bind(lname)
                    .bind(threshold)
                    .bind((i + 1) as i64)
                    .bind(serde_json::to_string(&json!({
                        "badge": null,
                        "max_visibility": 1,
                        "perks": [],
                    })).unwrap_or_else(|_| "{}".to_string()))
                    .bind(now)
                    .bind(now)
                    .execute(&mut *tx)
                    .await?;
                }
            }
            tx.commit().await?;
            Ok(())
        }
    }
}

async fn get_active_scheme_from_conn(
    conn: &mut sqlx::SqliteConnection,
) -> Result<Option<String>, LevelError> {
    let id: Option<String> =
        sqlx::query_scalar("SELECT id FROM level_schemes WHERE is_active = 1 LIMIT 1")
            .fetch_optional(&mut *conn)
            .await?;
    Ok(id)
}
