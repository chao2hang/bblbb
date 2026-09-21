//! 信任等级 SQL 仓储（SQLite / MySQL 双方言）。
//!
//! 行为统计表带 user 维度唯一键，写入一律 `INSERT OR IGNORE` /
//! `INSERT IGNORE` 天然幂等；聚合查询用标准 SQL，双方言共用。
//! 时间统一 Unix 毫秒（`outbox::now_millis`），「天」统一 UTC
//! `YYYY-MM-DD` 字符串（`utc_day_of`），避免方言日期函数。
//!
//! 点赞多样性口径（linux.do Wiki）：
//! - 收到的赞：不同点赞人 ≥ 收赞数 / user_div，不同天数 ≥ 收赞数 / day_div；
//! - 送出的赞：不同收赞人 ≥ 送赞数 / user_div，不同天数 ≥ 送赞数 / day_div。

use sqlx::Either;

use super::rules::{CumulativeStats, WindowStats};
use crate::db::pool::DatabasePool;
use crate::outbox::now_millis;

/// 每请求阅读心跳上限（秒）；超过部分丢弃。
pub const MAX_READ_HEARTBEAT_SECONDS: i64 = 60;
/// 每人每日阅读时长计入上限（秒）；防刷，超出不再累计。
pub const MAX_READ_SECONDS_PER_DAY: i64 = 7200;

type TrustResult<T> = Result<T, String>;

/// 记录一次当日访问（一天一行，幂等）。
pub async fn record_visit(pool: &DatabasePool, user_id: &str, day: &str) -> TrustResult<()> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("INSERT OR IGNORE INTO trust_visits (user_id, visit_day, first_seen_at) VALUES (?, ?, ?)")
                .bind(user_id)
                .bind(day)
                .bind(now)
                .execute(p)
                .await
                .map(|_| ())
        }
        Either::Right(p) => {
            sqlx::query("INSERT IGNORE INTO trust_visits (user_id, visit_day, first_seen_at) VALUES (?, ?, ?)")
                .bind(user_id)
                .bind(day)
                .bind(now)
                .execute(p)
                .await
                .map(|_| ())
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 记录进入话题（user×post 唯一，幂等）。
pub async fn record_topic_view(
    pool: &DatabasePool,
    user_id: &str,
    post_id: &str,
) -> TrustResult<()> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("INSERT OR IGNORE INTO trust_topic_views (user_id, post_id, first_viewed_at) VALUES (?, ?, ?)")
                .bind(user_id)
                .bind(post_id)
                .bind(now)
                .execute(p)
                .await
                .map(|_| ())
        }
        Either::Right(p) => {
            sqlx::query("INSERT IGNORE INTO trust_topic_views (user_id, post_id, first_viewed_at) VALUES (?, ?, ?)")
                .bind(user_id)
                .bind(post_id)
                .bind(now)
                .execute(p)
                .await
                .map(|_| ())
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// 记录阅读楼层（user×comment 唯一，幂等；调用方保证楼层属于 post_id）。
pub async fn record_comment_reads(
    pool: &DatabasePool,
    user_id: &str,
    post_id: &str,
    comment_ids: &[String],
) -> TrustResult<()> {
    if comment_ids.is_empty() {
        return Ok(());
    }
    let now = now_millis();
    for comment_id in comment_ids {
        match pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT OR IGNORE INTO trust_comment_reads (user_id, comment_id, post_id, first_read_at) VALUES (?, ?, ?, ?)",
                )
                .bind(user_id)
                .bind(comment_id)
                .bind(post_id)
                .bind(now)
                .execute(p)
                .await
                .map(|_| ())
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT IGNORE INTO trust_comment_reads (user_id, comment_id, post_id, first_read_at) VALUES (?, ?, ?, ?)",
                )
                .bind(user_id)
                .bind(comment_id)
                .bind(post_id)
                .bind(now)
                .execute(p)
                .await
                .map(|_| ())
            }
        }
        .map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// 累计阅读时长（服务端钳制：单请求 ≤60s，每人每日 ≤7200s）。
/// 返回当日累计秒数。
pub async fn add_read_time(
    pool: &DatabasePool,
    user_id: &str,
    seconds: i64,
    day: &str,
) -> TrustResult<i64> {
    let seconds = seconds.clamp(0, MAX_READ_HEARTBEAT_SECONDS);
    if seconds <= 0 {
        return Ok(0);
    }
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let total: i64 = sqlx::query_scalar(
                "INSERT INTO trust_read_time (user_id, read_day, seconds, updated_at) VALUES (?, ?, ?, ?)
                 ON CONFLICT(user_id, read_day) DO UPDATE SET
                     seconds = MIN(? + trust_read_time.seconds, ?),
                     updated_at = excluded.updated_at
                 RETURNING seconds",
            )
            .bind(user_id)
            .bind(day)
            .bind(seconds)
            .bind(now)
            .bind(seconds)
            .bind(MAX_READ_SECONDS_PER_DAY)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(total)
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO trust_read_time (user_id, read_day, seconds, updated_at) VALUES (?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE
                     seconds = LEAST(? + trust_read_time.seconds, ?),
                     updated_at = VALUES(updated_at)",
            )
            .bind(user_id)
            .bind(day)
            .bind(seconds)
            .bind(now)
            .bind(seconds)
            .bind(MAX_READ_SECONDS_PER_DAY)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            let total: i64 = sqlx::query_scalar(
                "SELECT seconds FROM trust_read_time WHERE user_id = ? AND read_day = ?",
            )
            .bind(user_id)
            .bind(day)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string())?;
            Ok(total)
        }
    }
}

/// 当前信任等级（users.trust_level；缺列/缺行按 0）。
pub async fn current_level(pool: &DatabasePool, user_id: &str) -> TrustResult<i64> {
    let level: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
                .bind(user_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(level.unwrap_or(0))
}

/// 用户是否存在（未删除）。
pub async fn user_exists(pool: &DatabasePool, user_id: &str) -> TrustResult<bool> {
    let exists: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT 1 FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(user_id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT 1 FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(user_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| e.to_string())?;
    Ok(exists == Some(1))
}

/// 写入等级变更：users 缓存 + 只追加事件（事务）。
#[allow(clippy::too_many_arguments)]
pub async fn set_level(
    pool: &DatabasePool,
    user_id: &str,
    from_level: Option<i64>,
    to_level: i64,
    reason: &str,
    note: Option<&str>,
    created_by: Option<&str>,
) -> TrustResult<()> {
    let now = now_millis();
    let event_id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await.map_err(|e| e.to_string())?;
            sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut *conn)
                .await
                .map_err(|e| e.to_string())?;
            let outcome: TrustResult<()> = async {
                sqlx::query("UPDATE users SET trust_level = ?, trust_level_updated_at = ? WHERE id = ?")
                    .bind(to_level)
                    .bind(now)
                    .bind(user_id)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| e.to_string())?;
                sqlx::query(
                    "INSERT INTO trust_level_events (id, user_id, from_level, to_level, reason, note, created_by, created_at)
                     VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
                )
                .bind(&event_id)
                .bind(user_id)
                .bind(from_level)
                .bind(to_level)
                .bind(reason)
                .bind(note)
                .bind(created_by)
                .bind(now)
                .execute(&mut *conn)
                .await
                .map_err(|e| e.to_string())?;
                Ok(())
            }
            .await;
            match outcome {
                Ok(()) => {
                    sqlx::query("COMMIT")
                        .execute(&mut *conn)
                        .await
                        .map_err(|e| e.to_string())?;
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    return Err(e);
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await.map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE users SET trust_level = ?, trust_level_updated_at = ? WHERE id = ?",
            )
            .bind(to_level)
            .bind(now)
            .bind(user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "INSERT INTO trust_level_events (id, user_id, from_level, to_level, reason, note, created_by, created_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&event_id)
            .bind(user_id)
            .bind(from_level)
            .bind(to_level)
            .bind(reason)
            .bind(note)
            .bind(created_by)
            .bind(now)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
            tx.commit().await.map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 最近一次「到达某级」的事件时间（毫秒）；无则 None。
pub async fn last_to_level_event(
    pool: &DatabasePool,
    user_id: &str,
    to_level: i64,
) -> TrustResult<Option<i64>> {
    let at: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT created_at FROM trust_level_events WHERE user_id = ? AND to_level = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(to_level)
        .fetch_optional(p)
        .await,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT created_at FROM trust_level_events WHERE user_id = ? AND to_level = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(to_level)
        .fetch_optional(p)
        .await,
    }
    .map_err(|e| e.to_string())?;
    Ok(at)
}

/// 规则行（trust_level_rules 投影）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuleRow {
    pub level: i64,
    pub name: String,
    pub summary: Option<String>,
    pub requirements_json: Option<String>,
    pub is_enabled: bool,
    pub version: i64,
}

pub async fn load_rules(pool: &DatabasePool) -> TrustResult<Vec<RuleRow>> {
    // 全量行（含停用）：评估引擎需要感知 is_enabled 才能真正停用某级
    // （此前只取启用行，parse_rules 又对缺失等级回填代码默认值，
    // 导致「停用」实际不生效）；管理列表也需要展示停用行。
    let sql = "SELECT level, name, summary, requirements_json, is_enabled, version
               FROM trust_level_rules ORDER BY level";
    match pool {
        Either::Left(p) => sqlx::query_as::<_, RuleRow>(sql).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as::<_, RuleRow>(sql).fetch_all(p).await,
    }
    .map_err(|e| e.to_string())
}

/// 管理端更新单级规则（全列 + version 乐观锁）。
///
/// 返回更新后的行；`None` = version 冲突（0 行受影响）或规则行不存在
/// （调用方先经 `load_rules` 确认存在，路由层统一映射 409）。
#[allow(clippy::too_many_arguments)]
pub async fn update_rule(
    pool: &DatabasePool,
    level: i64,
    name: &str,
    summary: Option<&str>,
    requirements_json: &str,
    is_enabled: bool,
    expected_version: i64,
) -> TrustResult<Option<RuleRow>> {
    let now = now_millis();
    let enabled_i64 = i64::from(is_enabled);
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE trust_level_rules
             SET name = ?, summary = ?, requirements_json = ?, is_enabled = ?,
                 version = version + 1, updated_at = ?
             WHERE level = ? AND version = ?",
        )
        .bind(name)
        .bind(summary)
        .bind(requirements_json)
        .bind(enabled_i64)
        .bind(now)
        .bind(level)
        .bind(expected_version)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE trust_level_rules
             SET name = ?, summary = ?, requirements_json = ?, is_enabled = ?,
                 version = version + 1, updated_at = ?
             WHERE level = ? AND version = ?",
        )
        .bind(name)
        .bind(summary)
        .bind(requirements_json)
        .bind(enabled_i64)
        .bind(now)
        .bind(level)
        .bind(expected_version)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
    };
    if affected == 0 {
        return Ok(None);
    }
    load_rule(pool, level).await
}

/// 恢复单级规则为代码内置默认（name/summary/requirements/is_enabled=1；
/// version + 1）。规则行缺失时插入默认行（幂等兜底）。
pub async fn reset_rule(pool: &DatabasePool, level: i64) -> TrustResult<RuleRow> {
    let idx = level.clamp(0, 4) as usize;
    let (name, summary) = super::rules::DEFAULT_RULE_META[idx];
    let (requirements_json, _) = super::rules::DEFAULT_REQUIREMENTS[idx];
    let now = now_millis();
    let existing = load_rule(pool, level).await?;
    if existing.is_none() {
        match pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO trust_level_rules (level, name, summary, requirements_json, is_enabled, version, updated_at)
                     VALUES (?, ?, ?, ?, 1, 1, ?)",
                )
                .bind(level)
                .bind(name)
                .bind(summary)
                .bind(requirements_json)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT INTO trust_level_rules (level, name, summary, requirements_json, is_enabled, version, updated_at)
                     VALUES (?, ?, ?, ?, 1, 1, ?)",
                )
                .bind(level)
                .bind(name)
                .bind(summary)
                .bind(requirements_json)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
            }
        }
        return load_rule(pool, level)
            .await?
            .ok_or_else(|| "rule row disappeared after reset insert".to_string());
    }
    let enabled_i64 = 1i64;
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE trust_level_rules
             SET name = ?, summary = ?, requirements_json = ?, is_enabled = ?,
                 version = version + 1, updated_at = ?
             WHERE level = ?",
        )
        .bind(name)
        .bind(summary)
        .bind(requirements_json)
        .bind(enabled_i64)
        .bind(now)
        .bind(level)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE trust_level_rules
             SET name = ?, summary = ?, requirements_json = ?, is_enabled = ?,
                 version = version + 1, updated_at = ?
             WHERE level = ?",
        )
        .bind(name)
        .bind(summary)
        .bind(requirements_json)
        .bind(enabled_i64)
        .bind(now)
        .bind(level)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
    };
    if affected == 0 {
        return Err("rule reset affected no rows".to_string());
    }
    load_rule(pool, level)
        .await?
        .ok_or_else(|| "rule row disappeared after reset".to_string())
}

/// 读取单级规则行（不存在 → None）。
async fn load_rule(pool: &DatabasePool, level: i64) -> TrustResult<Option<RuleRow>> {
    let sql = "SELECT level, name, summary, requirements_json, is_enabled, version
               FROM trust_level_rules WHERE level = ?";
    match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, RuleRow>(sql)
                .bind(level)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, RuleRow>(sql)
                .bind(level)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| e.to_string())
}

/// 每级用户数（users.trust_level 聚合，管理视图用）。
pub async fn level_user_counts(pool: &DatabasePool) -> TrustResult<Vec<(i64, i64)>> {
    let sql =
        "SELECT trust_level, COUNT(*) FROM users WHERE deleted_at IS NULL GROUP BY trust_level";
    match pool {
        Either::Left(p) => sqlx::query_as::<_, (i64, i64)>(sql).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as::<_, (i64, i64)>(sql).fetch_all(p).await,
    }
    .map_err(|e| e.to_string())
}

/// 累计口径统计。
pub async fn cumulative_stats(pool: &DatabasePool, user_id: &str) -> TrustResult<CumulativeStats> {
    let topics_entered = count(
        pool,
        "SELECT COUNT(*) FROM trust_topic_views WHERE user_id = ?",
        user_id,
    )
    .await?;
    let posts_read = count(
        pool,
        "SELECT COUNT(*) FROM trust_comment_reads WHERE user_id = ?",
        user_id,
    )
    .await?;
    let time_read_seconds: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COALESCE(SUM(seconds), 0) FROM trust_read_time WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT CAST(COALESCE(SUM(seconds), 0) AS SIGNED) FROM trust_read_time WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| e.to_string())?;
    let days_visited = count(
        pool,
        "SELECT COUNT(*) FROM trust_visits WHERE user_id = ?",
        user_id,
    )
    .await?;
    let topics_replied_to = count(
        pool,
        "SELECT COUNT(DISTINCT post_id) FROM comments WHERE author_id = ? AND status != 'deleted'",
        user_id,
    )
    .await?;
    let likes_given = count(
        pool,
        // 反应唯一写入路径是 user_reactions（多态：post/comment），见
        // reactions/service.rs；post_reactions/comment_reactions 为遗留空表。
        "SELECT COUNT(*) FROM user_reactions WHERE user_id = ? AND reaction = 'like'",
        user_id,
    )
    .await?;
    let likes_received = count(
        pool,
        "SELECT COUNT(*) FROM user_reactions ur
         JOIN posts p ON ur.target_type = 'post' AND p.id = ur.target_id
         WHERE p.author_id = ? AND ur.reaction = 'like'",
        user_id,
    )
    .await?
        + count(
            pool,
            "SELECT COUNT(*) FROM user_reactions ur
         JOIN comments c ON ur.target_type = 'comment' AND c.id = ur.target_id
         WHERE c.author_id = ? AND ur.reaction = 'like'",
            user_id,
        )
        .await?;
    Ok(CumulativeStats {
        topics_entered,
        posts_read,
        time_read_seconds,
        days_visited,
        likes_given,
        likes_received,
        topics_replied_to,
    })
}

/// 窗口口径统计（TL3）。`window_days` 取 TL3 规则；
/// 点赞多样性在 Rust 侧对 (对方用户, created_at) 行去重计算。
pub async fn window_stats(
    pool: &DatabasePool,
    user_id: &str,
    window_days: i64,
) -> TrustResult<WindowStats> {
    let now = now_millis();
    let window_start = now - window_days.max(0) * 86_400_000;
    // no_sanction_months：TL3 规则固定 6 个月，按 30 天/月近似。
    let sanction_start = now - 6 * 30 * 86_400_000;
    let window_day = utc_day_of(window_start);

    let visit_days: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM trust_visits WHERE user_id = ? AND visit_day >= ?",
            )
            .bind(user_id)
            .bind(&window_day)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM trust_visits WHERE user_id = ? AND visit_day >= ?",
            )
            .bind(user_id)
            .bind(&window_day)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| e.to_string())?;

    let topics_replied_to = count_since(
        pool,
        "SELECT COUNT(DISTINCT post_id) FROM comments WHERE author_id = ? AND status != 'deleted' AND created_at >= ?",
        user_id,
        window_start,
    )
    .await?;

    let topics_created_in_window = count_since_no_user(
        pool,
        "SELECT COUNT(*) FROM posts WHERE created_at >= ? AND deleted_at IS NULL AND status != 'draft'",
        window_start,
    )
    .await?;
    let posts_created_in_window = count_since_no_user(
        pool,
        "SELECT COUNT(*) FROM comments WHERE created_at >= ? AND status != 'deleted'",
        window_start,
    )
    .await?;

    let topics_viewed_in_window = count_since(
        pool,
        "SELECT COUNT(*) FROM trust_topic_views v JOIN posts p ON p.id = v.post_id
         WHERE v.user_id = ? AND p.created_at >= ? AND p.deleted_at IS NULL",
        user_id,
        window_start,
    )
    .await?;
    let posts_read_in_window = count_since(
        pool,
        "SELECT COUNT(*) FROM trust_comment_reads r JOIN comments c ON c.id = r.comment_id
         WHERE r.user_id = ? AND c.created_at >= ? AND c.status != 'deleted'",
        user_id,
        window_start,
    )
    .await?;

    // 收到的赞（窗口内，按内容作者归属）：行级取回后在 Rust 去重。
    let mut received: Vec<(String, i64)> = Vec::new();
    received.extend(
        fetch_pairs(
            pool,
            "SELECT ur.user_id, ur.created_at FROM user_reactions ur
             JOIN posts p ON ur.target_type = 'post' AND p.id = ur.target_id
             WHERE p.author_id = ? AND ur.reaction = 'like' AND ur.created_at >= ?",
            user_id,
            window_start,
        )
        .await?,
    );
    received.extend(
        fetch_pairs(
            pool,
            "SELECT ur.user_id, ur.created_at FROM user_reactions ur
             JOIN comments c ON ur.target_type = 'comment' AND c.id = ur.target_id
             WHERE c.author_id = ? AND ur.reaction = 'like' AND ur.created_at >= ?",
            user_id,
            window_start,
        )
        .await?,
    );
    let likes_received = received.len() as i64;
    let likes_received_users = distinct_users(&received);
    let likes_received_days = distinct_days(&received);

    // 送出的赞（窗口内）：多样性按不同收赞人与不同天数。
    // COALESCE 前先限定目标类型，避免未知 target_type 解码出 NULL 字符串。
    let given = fetch_pairs(
        pool,
        "SELECT COALESCE(p.author_id, c.author_id), ur.created_at FROM user_reactions ur
         LEFT JOIN posts p ON ur.target_type = 'post' AND p.id = ur.target_id
         LEFT JOIN comments c ON ur.target_type = 'comment' AND c.id = ur.target_id
         WHERE ur.user_id = ? AND ur.reaction = 'like' AND ur.created_at >= ?
           AND ur.target_type IN ('post', 'comment')",
        user_id,
        window_start,
    )
    .await?;
    let likes_given = given.len() as i64;
    let likes_given_users = distinct_users(&given);
    let likes_given_days = distinct_days(&given);

    let confirmed_flags: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(DISTINCT r.id) FROM reports r
             LEFT JOIN posts p ON r.target_type = 'post' AND p.id = r.target_id
             LEFT JOIN comments c ON r.target_type = 'comment' AND c.id = r.target_id
             WHERE r.status = 'resolved' AND r.created_at >= ?
               AND ((r.target_type = 'post' AND p.author_id = ?)
                 OR (r.target_type = 'comment' AND c.author_id = ?)
                 OR (r.target_type = 'user' AND r.target_id = ?))",
            )
            .bind(window_start)
            .bind(user_id)
            .bind(user_id)
            .bind(user_id)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(DISTINCT r.id) FROM reports r
             LEFT JOIN posts p ON r.target_type = 'post' AND p.id = r.target_id
             LEFT JOIN comments c ON r.target_type = 'comment' AND c.id = r.target_id
             WHERE r.status = 'resolved' AND r.created_at >= ?
               AND ((r.target_type = 'post' AND p.author_id = ?)
                 OR (r.target_type = 'comment' AND c.author_id = ?)
                 OR (r.target_type = 'user' AND r.target_id = ?))",
            )
            .bind(window_start)
            .bind(user_id)
            .bind(user_id)
            .bind(user_id)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| e.to_string())?;

    let sanctioned_recently: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM sanctions WHERE user_id = ? AND kind IN ('mute', 'ban')
             AND revoked_at IS NULL AND starts_at >= ?",
            )
            .bind(user_id)
            .bind(sanction_start)
            .fetch_one(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM sanctions WHERE user_id = ? AND kind IN ('mute', 'ban')
             AND revoked_at IS NULL AND starts_at >= ?",
            )
            .bind(user_id)
            .bind(sanction_start)
            .fetch_one(p)
            .await
        }
    }
    .map_err(|e| e.to_string())?;

    Ok(WindowStats {
        window_days,
        visit_days,
        topics_replied_to,
        topics_created_in_window,
        topics_viewed_in_window,
        posts_created_in_window,
        posts_read_in_window,
        likes_received,
        likes_received_users,
        likes_received_days,
        likes_given,
        likes_given_users,
        likes_given_days,
        confirmed_flags,
        sanctioned_recently: sanctioned_recently > 0,
    })
}

fn distinct_users(rows: &[(String, i64)]) -> i64 {
    rows.iter()
        .map(|(u, _)| u)
        .collect::<std::collections::BTreeSet<_>>()
        .len() as i64
}

fn distinct_days(rows: &[(String, i64)]) -> i64 {
    rows.iter()
        .map(|(_, at)| utc_day_of(*at))
        .collect::<std::collections::BTreeSet<_>>()
        .len() as i64
}

async fn count(pool: &DatabasePool, sql: &str, user_id: &str) -> TrustResult<i64> {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).bind(user_id).fetch_one(p).await,
        Either::Right(p) => sqlx::query_scalar(sql).bind(user_id).fetch_one(p).await,
    }
    .map_err(|e| e.to_string())
}

/// 双参数（user_id, since）标量计数。
async fn count_since(
    pool: &DatabasePool,
    sql: &str,
    user_id: &str,
    since: i64,
) -> TrustResult<i64> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar(sql)
                .bind(user_id)
                .bind(since)
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(sql)
                .bind(user_id)
                .bind(since)
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| e.to_string())
}

/// 单参数（since）标量计数。
async fn count_since_no_user(pool: &DatabasePool, sql: &str, since: i64) -> TrustResult<i64> {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).bind(since).fetch_one(p).await,
        Either::Right(p) => sqlx::query_scalar(sql).bind(since).fetch_one(p).await,
    }
    .map_err(|e| e.to_string())
}

/// 双参数 (user_id, since) 的 (字符串, 时间) 行对。
async fn fetch_pairs(
    pool: &DatabasePool,
    sql: &str,
    user_id: &str,
    since: i64,
) -> TrustResult<Vec<(String, i64)>> {
    match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, (String, i64)>(sql)
                .bind(user_id)
                .bind(since)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, (String, i64)>(sql)
                .bind(user_id)
                .bind(since)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| e.to_string())
}

/// Unix 毫秒 → UTC `YYYY-MM-DD`。
pub fn utc_day_of(now_ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(now_ms)
        .unwrap_or_else(|| chrono::DateTime::from_timestamp_millis(0).expect("epoch"))
        .format("%Y-%m-%d")
        .to_string()
}

/// 当前 UTC 日（供 service/hooks 使用）。
pub fn utc_day_now() -> String {
    utc_day_of(now_millis())
}
