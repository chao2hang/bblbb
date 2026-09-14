//! 信任等级编排：评估引擎（升降级 + TL3 宽限）、进度视图、手动授予、
//! 行为事件钩子（best-effort，失败只记日志，不影响主流程）。
//!
//! 评估语义（移植 linux.do / Discourse）：
//! - 目标等级 = 1..=3 中满足全部条件的最高级（TL4 只能手动授予）；
//! - 当前 < 目标 → 晋升（reason='promotion'）；
//! - 当前 == 3 且目标 < 3 → TL3 滚动窗口不达标，降级回 2（成员）；
//!   但最近一次到达 3 级后的 2 周宽限期内不降级；
//! - 其他情况不变（1/2 级不降级）。

use serde::Serialize;
use sqlx::Either;

use super::rules::{
    level_met, requirement_items, RequirementItem, Requirements, DEFAULT_REQUIREMENTS,
};
use super::store::{self, RuleRow};
use crate::db::pool::DatabasePool;
use crate::outbox::now_millis;

/// TL3 降级宽限期：2 周（毫秒）。
pub const TL3_GRACE_MS: i64 = 14 * 24 * 3600 * 1000;

/// 领域错误（路由层映射为 AppError）。
#[derive(Debug)]
pub enum TrustError {
    Db(String),
    NotFound,
    Invalid(String),
}

impl std::fmt::Display for TrustError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrustError::Db(e) => write!(f, "trust store error: {e}"),
            TrustError::NotFound => write!(f, "user not found"),
            TrustError::Invalid(m) => write!(f, "{m}"),
        }
    }
}

type TrustResult<T> = Result<T, TrustError>;

fn db<E: std::fmt::Display>(e: E) -> TrustError {
    TrustError::Db(e.to_string())
}

/// 一次评估的结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Evaluation {
    pub from_level: i64,
    pub to_level: i64,
    pub changed: bool,
    /// promotion / demotion；未变化为空。
    pub reason: Option<&'static str>,
}

/// 规则集：level → 阈值（库内规则优先，缺失回退代码内置默认）。
fn parse_rules(rows: Vec<RuleRow>) -> std::collections::BTreeMap<i64, Requirements> {
    let mut map = std::collections::BTreeMap::new();
    for row in rows {
        let req = row
            .requirements_json
            .as_deref()
            .and_then(|j| Requirements::parse(j).ok())
            .unwrap_or_else(|| Requirements::default_for_level(row.level));
        map.insert(row.level, req);
    }
    for level in 0..=4i64 {
        map.entry(level).or_insert_with(|| {
            let idx = level as usize;
            let (json, _) = DEFAULT_REQUIREMENTS[idx];
            Requirements::parse(json).unwrap_or_default()
        });
    }
    map
}

/// 停用等级集合（is_enabled=0 的行）。停用语义：
/// - 该级不参与自动晋升目标选择（用户不会自动升入停用级）；
/// - TL3 停用时不做自动降级（该级脱离自动管理，存量用户保持不动）；
/// - 手动授予不受停用影响（管理员显式操作始终允许）。
fn disabled_levels(rows: &[RuleRow]) -> std::collections::BTreeSet<i64> {
    rows.iter()
        .filter(|r| !r.is_enabled)
        .map(|r| r.level)
        .collect()
}

/// 评估并落地用户信任等级。
pub async fn evaluate_user(pool: &DatabasePool, user_id: &str) -> TrustResult<Evaluation> {
    let current = store::current_level(pool, user_id).await.map_err(db)?;
    if current >= 4 {
        // TL4 仅手动管理，评估不再改动。
        return Ok(Evaluation {
            from_level: current,
            to_level: current,
            changed: false,
            reason: None,
        });
    }
    let rows = store::load_rules(pool).await.map_err(db)?;
    let disabled = disabled_levels(&rows);
    let rules = parse_rules(rows);
    let window_days = rules
        .get(&3)
        .map(|r| r.window_days)
        .filter(|d| *d > 0)
        .unwrap_or(100);
    let cum = store::cumulative_stats(pool, user_id).await.map_err(db)?;
    let win = store::window_stats(pool, user_id, window_days)
        .await
        .map_err(db)?;

    // 目标等级 = 1..=3 中未停用且满足全部条件的最高级。
    let target = (1..=3i64)
        .rev()
        .find(|l| {
            !disabled.contains(l)
                && rules
                    .get(l)
                    .map(|req| level_met(req, &cum, &win))
                    .unwrap_or(false)
        })
        .unwrap_or(0);

    if target > current {
        store::set_level(
            pool,
            user_id,
            Some(current),
            target,
            "promotion",
            None,
            None,
        )
        .await
        .map_err(db)?;
        return Ok(Evaluation {
            from_level: current,
            to_level: target,
            changed: true,
            reason: Some("promotion"),
        });
    }

    if current == 3 && !disabled.contains(&3) && target < 3 {
        // TL3 滚动窗口不达标 → 降级回成员；宽限期内不动。
        // TL3 规则被停用时该级脱离自动管理，不做降级。
        let last = store::last_to_level_event(pool, user_id, 3)
            .await
            .map_err(db)?;
        let now = now_millis();
        let in_grace = matches!(last, Some(at) if now < at.saturating_add(TL3_GRACE_MS));
        if !in_grace {
            store::set_level(pool, user_id, Some(current), 2, "demotion", None, None)
                .await
                .map_err(db)?;
            return Ok(Evaluation {
                from_level: current,
                to_level: 2,
                changed: true,
                reason: Some("demotion"),
            });
        }
    }

    Ok(Evaluation {
        from_level: current,
        to_level: current,
        changed: false,
        reason: None,
    })
}

/// 窗口信息（下一级为窗口口径时返回）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WindowInfo {
    pub days: i64,
    /// 窗口起点（Unix 毫秒）。
    pub since: i64,
}

/// 下一级摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct NextLevel {
    pub level: i64,
    pub name: String,
    pub summary: Option<String>,
    pub manual_only: bool,
    /// 全部条件是否已满足（manual_only 恒为 false）。
    pub eligible: bool,
    pub requirements: Vec<RequirementItem>,
}

/// 进度视图（GET /me/trust-level 响应体）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct TrustProgress {
    pub level: i64,
    pub name: String,
    pub summary: Option<String>,
    pub updated_at: Option<i64>,
    /// TL3 降级宽限期截止（毫秒）；仅 level==3 时返回。
    pub grace_until: Option<i64>,
    pub window: Option<WindowInfo>,
    pub next_level: Option<NextLevel>,
}

/// 计算用户进度视图（附带一次惰性评估，保证展示即真实）。
pub async fn progress(pool: &DatabasePool, user_id: &str) -> TrustResult<TrustProgress> {
    evaluate_user(pool, user_id).await.map_err(db)?;
    build_progress(pool, user_id).await
}

/// 只读进度视图（管理端复用；不触发评估）。
pub async fn build_progress(pool: &DatabasePool, user_id: &str) -> TrustResult<TrustProgress> {
    let current = store::current_level(pool, user_id).await.map_err(db)?;
    let rules_rows = store::load_rules(pool).await.map_err(db)?;
    let rules = parse_rules(rules_rows.clone());
    let disabled = disabled_levels(&rules_rows);
    let level_rule = rules_rows.iter().find(|r| r.level == current);
    let updated_at: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT trust_level_updated_at FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await
                .map_err(db)?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT trust_level_updated_at FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await
                .map_err(db)?
        }
    }
    .flatten();

    let grace_until = if current == 3 {
        store::last_to_level_event(pool, user_id, 3)
            .await
            .map_err(db)?
            .map(|at| at.saturating_add(TL3_GRACE_MS))
    } else {
        None
    };

    // 下一级（4 级只能手动，仍展示条件说明）；跳过停用等级——
    // 停用级不会成为自动晋升目标，展示其条件会造成「已达标却不升级」的误导。
    let next_level = if current < 4 {
        let mut next_opt: Option<NextLevel> = None;
        for next in (current + 1)..=4i64 {
            if disabled.contains(&next) {
                continue;
            }
            let req = rules
                .get(&next)
                .cloned()
                .unwrap_or_else(|| Requirements::default_for_level(next));
            let row = rules_rows.iter().find(|r| r.level == next);
            let window_days = req.window_days.max(0);
            let (cum, win) = if req.manual_only {
                (Default::default(), Default::default())
            } else if window_days > 0 {
                let w = store::window_stats(pool, user_id, window_days)
                    .await
                    .map_err(db)?;
                (Default::default(), w)
            } else {
                let c = store::cumulative_stats(pool, user_id).await.map_err(db)?;
                (c, Default::default())
            };
            let items = requirement_items(&req, &cum, &win);
            let eligible = !req.manual_only && items.iter().all(|i| i.met);
            next_opt = Some(NextLevel {
                level: next,
                name: row
                    .map(|r| r.name.clone())
                    .unwrap_or_else(|| super::rules::LEVEL_NAMES[next as usize].to_string()),
                summary: row.and_then(|r| r.summary.clone()),
                manual_only: req.manual_only,
                eligible,
                requirements: items,
            });
            break;
        }
        next_opt
    } else {
        None
    };

    // 窗口信息：当前级为 TL3 或下一级为窗口口径时展示。
    let window = if current == 3 {
        let wd = rules
            .get(&3)
            .map(|r| r.window_days)
            .filter(|d| *d > 0)
            .unwrap_or(100);
        Some(WindowInfo {
            days: wd,
            since: now_millis() - wd * 86_400_000,
        })
    } else {
        next_level.as_ref().and_then(|n| {
            rules
                .get(&n.level)
                .map(|r| r.window_days)
                .filter(|d| *d > 0)
                .map(|wd| WindowInfo {
                    days: wd,
                    since: now_millis() - wd * 86_400_000,
                })
        })
    };

    Ok(TrustProgress {
        level: current,
        name: level_rule
            .map(|r| r.name.clone())
            .unwrap_or_else(|| super::rules::LEVEL_NAMES[current.clamp(0, 4) as usize].to_string()),
        summary: level_rule.and_then(|r| r.summary.clone()),
        updated_at,
        grace_until,
        window,
        next_level,
    })
}

/// 阅读心跳：钳制后累计当日阅读时长，返回 (本次计入, 当日累计)。
pub async fn add_read_time(
    pool: &DatabasePool,
    user_id: &str,
    seconds: i64,
) -> TrustResult<(i64, i64)> {
    if seconds < 1 {
        return Err(TrustError::Invalid("seconds must be >= 1".into()));
    }
    let seconds = seconds.min(store::MAX_READ_HEARTBEAT_SECONDS);
    let total = store::add_read_time(pool, user_id, seconds, &store::utc_day_now())
        .await
        .map_err(db)?;
    Ok((seconds, total))
}

/// 管理员手动设置（TL4 唯一授予通道；0–4 皆可，需 reason）。
pub async fn manual_set(
    pool: &DatabasePool,
    admin_id: &str,
    target_user_id: &str,
    level: i64,
    note: &str,
) -> TrustResult<Evaluation> {
    if !(0..=4).contains(&level) {
        return Err(TrustError::Invalid("trust level must be 0..=4".into()));
    }
    if !store::user_exists(pool, target_user_id).await.map_err(db)? {
        return Err(TrustError::NotFound);
    }
    let current = store::current_level(pool, target_user_id)
        .await
        .map_err(db)?;
    if current == level {
        return Ok(Evaluation {
            from_level: current,
            to_level: current,
            changed: false,
            reason: None,
        });
    }
    store::set_level(
        pool,
        target_user_id,
        Some(current),
        level,
        "manual",
        Some(note),
        Some(admin_id),
    )
    .await
    .map_err(db)?;
    Ok(Evaluation {
        from_level: current,
        to_level: level,
        changed: true,
        reason: Some("manual"),
    })
}

// ───────────────────────── 行为事件钩子（best-effort） ─────────────────────────

/// 会话活跃：记录当日访问（每次会话续期至多一次，幂等）。
pub async fn on_session_active(pool: &DatabasePool, user_id: &str) {
    if let Err(e) = store::record_visit(pool, user_id, &store::utc_day_now()).await {
        tracing::warn!(user_id, error = %e, "trust: record visit failed");
    }
}

/// 进入话题：记录 + 惰性评估。
pub async fn on_topic_viewed(pool: &DatabasePool, user_id: &str, post_id: &str) {
    if let Err(e) = store::record_topic_view(pool, user_id, post_id).await {
        tracing::warn!(user_id, error = %e, "trust: record topic view failed");
        return;
    }
    if let Err(e) = evaluate_user(pool, user_id).await {
        tracing::warn!(user_id, error = %e, "trust: evaluate after topic view failed");
    }
}

/// 阅读楼层：记录 + 惰性评估。
pub async fn on_comments_read(
    pool: &DatabasePool,
    user_id: &str,
    post_id: &str,
    comment_ids: &[String],
) {
    if comment_ids.is_empty() {
        return;
    }
    if let Err(e) = store::record_comment_reads(pool, user_id, post_id, comment_ids).await {
        tracing::warn!(user_id, error = %e, "trust: record comment reads failed");
        return;
    }
    if let Err(e) = evaluate_user(pool, user_id).await {
        tracing::warn!(user_id, error = %e, "trust: evaluate after comment reads failed");
    }
}

/// 反应变化：评估行为人与内容作者（点赞变化影响双方统计）。
pub async fn on_reaction_changed(pool: &DatabasePool, actor_id: &str, owner_id: Option<&str>) {
    if let Err(e) = evaluate_user(pool, actor_id).await {
        tracing::warn!(actor_id, error = %e, "trust: evaluate actor after reaction failed");
    }
    if let Some(owner) = owner_id {
        if owner != actor_id {
            if let Err(e) = evaluate_user(pool, owner).await {
                tracing::warn!(owner, error = %e, "trust: evaluate owner after reaction failed");
            }
        }
    }
}

/// 处罚变化（mute/ban 创建或撤销）：评估目标用户。
pub async fn on_sanction_changed(pool: &DatabasePool, user_id: &str) {
    if let Err(e) = evaluate_user(pool, user_id).await {
        tracing::warn!(user_id, error = %e, "trust: evaluate after sanction change failed");
    }
}
