//! 社交域成就服务（GAP-FIX 社交域）。
//!
//! - [`evaluate`]：解锁判定钩子——按用户当前统计（发帖/评论/被反应/连续
//!   签到/粉丝数）与成就阈值比较，满足即解锁（写 `user_achievements` +
//!   `notifications` type='badge'）。调用方（发帖/评论/反应/签到/关注成功
//!   路径）应 best-effort 调用：失败只 `tracing::warn`，不阻断业务；
//! - [`unlock`]：单条解锁（幂等——已解锁返回 `false`），管理侧手工授予
//!   （grant）复用同一入口，保证通知语义一致；
//! - [`load_user_stats`]：各条件维度的实时统计（跨方言 `?` 占位符）；
//! - 解锁写在一个事务内：SQLite 用 `BEGIN IMMEDIATE` 取写锁（SELECT 后
//!   INSERT 的检查-写入原子性），MySQL 用普通事务 + `user_achievements`
//!   复合主键兜底并发（重复插入以错误上浮，由调用方 warn 吞掉）。
//!
//! SQL 兼容 SQLite/MySQL：全部使用 `?` 占位符，无方言语法；时间戳毫秒。

use serde_json::json;
use sqlx::Either;

use crate::db::DatabasePool;
use crate::outbox::now_millis;

/// 成就装备槽上限（spec：max_slots = 3）。
pub const MAX_EQUIPPED_SLOTS: i64 = 3;

/// 成就条件类型封闭集（与迁移 CHECK 一致）。
pub const CONDITION_TYPES: &[&str] = &[
    "post_count",
    "comment_count",
    "reaction_received",
    "checkin_streak",
    "follower_count",
    "manual",
];

/// 成就定义行（`achievements` 表投影）。
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AchievementRow {
    pub id: String,
    pub code: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub condition_type: String,
    pub condition_threshold: i64,
    pub reward_exp: i64,
    pub reward_coin: i64,
    pub is_hidden: i64,
    pub is_enabled: i64,
    pub sort_order: i64,
    pub version: i64,
    pub created_at: i64,
    pub updated_at: i64,
}

/// 用户各条件维度的实时进度统计。
#[derive(Debug, Clone, Copy, Default)]
pub struct UserStats {
    pub post_count: i64,
    pub comment_count: i64,
    pub reaction_received: i64,
    pub checkin_streak: i64,
    pub follower_count: i64,
}

impl UserStats {
    /// 按条件类型取进度值（未知类型返回 0）。
    pub fn value_for(&self, condition_type: &str) -> i64 {
        match condition_type {
            "post_count" => self.post_count,
            "comment_count" => self.comment_count,
            "reaction_received" => self.reaction_received,
            "checkin_streak" => self.checkin_streak,
            "follower_count" => self.follower_count,
            _ => 0,
        }
    }
}

/// 读取用户各维度统计（全部公开计数，无方言特性）。
pub async fn load_user_stats(pool: &DatabasePool, user_id: &str) -> Result<UserStats, String> {
    let post_count = stats_scalar(
        pool,
        user_id,
        "SELECT COUNT(*) FROM posts WHERE author_id = ? AND status = 'published' AND deleted_at IS NULL",
    )
    .await?;
    let comment_count = stats_scalar(
        pool,
        user_id,
        "SELECT COUNT(*) FROM comments WHERE author_id = ? AND status = 'published' AND deleted_at IS NULL",
    )
    .await?;
    // 被反应数 = 本人帖子上收到的反应 + 本人评论上收到的反应。
    let reactions_on_posts = stats_scalar(
        pool,
        user_id,
        "SELECT COUNT(*) FROM user_reactions ur JOIN posts p ON p.id = ur.target_id
         WHERE ur.target_type = 'post' AND p.author_id = ?",
    )
    .await?;
    let reactions_on_comments = stats_scalar(
        pool,
        user_id,
        "SELECT COUNT(*) FROM user_reactions ur JOIN comments c ON c.id = ur.target_id
         WHERE ur.target_type = 'comment' AND c.author_id = ?",
    )
    .await?;
    let follower_count = stats_scalar(
        pool,
        user_id,
        "SELECT COUNT(*) FROM user_follows WHERE followee_id = ?",
    )
    .await?;

    // 连续签到天数：复用 M07-LEVELS 的日界线规则（用户时区 → 站点 → UTC）。
    let checkin_streak = {
        let tz = crate::economy::activity::checkin::resolve_user_timezone(pool, user_id, "")
            .await
            .map_err(|e| e.to_string())?;
        let today =
            crate::economy::activity::checkin::activity_day_for(tz.offset_secs, now_millis());
        crate::economy::activity::checkin::streak_days(pool, user_id, &today)
            .await
            .map_err(|e| e.to_string())?
    };

    Ok(UserStats {
        post_count,
        comment_count,
        reaction_received: reactions_on_posts + reactions_on_comments,
        checkin_streak,
        follower_count,
    })
}

/// 成就统计单条 COUNT（Either 池共用）。
async fn stats_scalar(pool: &DatabasePool, user_id: &str, sql: &str) -> Result<i64, String> {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string()),
        Either::Right(p) => sqlx::query_scalar::<_, i64>(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .map_err(|e| e.to_string()),
    }
}

/// 列出启用中的成就（公共目录；按 sort_order ASC, code ASC 稳定排序）。
pub async fn list_enabled_achievements(pool: &DatabasePool) -> Result<Vec<AchievementRow>, String> {
    let sql = "SELECT id, code, name, description, category, condition_type, condition_threshold,
                      reward_exp, reward_coin, is_hidden, is_enabled, sort_order, version, created_at, updated_at
               FROM achievements WHERE is_enabled = 1 ORDER BY sort_order ASC, code ASC";
    let rows: Vec<AchievementRow> = match pool {
        Either::Left(p) => sqlx::query_as(sql).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as(sql).fetch_all(p).await,
    }
    .map_err(|e| e.to_string())?;
    Ok(rows)
}

/// 解锁判定钩子（best-effort）：按当前统计解锁所有满足阈值的自动成就。
///
/// 返回本次新解锁的 code 列表（供测试/日志；调用方通常忽略）。
/// `manual` 类型成就不参与自动判定，只能经管理侧 grant 授予。
pub async fn evaluate(pool: &DatabasePool, user_id: &str) -> Result<Vec<String>, String> {
    let achievements = list_enabled_achievements(pool).await?;
    if achievements.is_empty() {
        return Ok(Vec::new());
    }
    let stats = load_user_stats(pool, user_id).await?;
    let mut unlocked_codes = Vec::new();
    for ach in &achievements {
        if ach.condition_type == "manual" {
            continue;
        }
        let progress = stats.value_for(&ach.condition_type);
        if progress >= ach.condition_threshold && unlock(pool, user_id, ach).await.unwrap_or(false)
        {
            unlocked_codes.push(ach.code.clone());
        }
    }
    Ok(unlocked_codes)
}

/// 解锁单个成就（幂等）：写 `user_achievements` + 通知（type='badge'）。
///
/// 返回是否**本次**新解锁（已解锁返回 `Ok(false)`，不重复通知）。
/// SQLite 侧 `BEGIN IMMEDIATE` 保证「查重 + 插入」原子；MySQL 侧复合主键
/// 兜底，极端并发下重复插入以数据库错误上浮（调用方 best-effort 吞掉）。
pub async fn unlock(
    pool: &DatabasePool,
    user_id: &str,
    achievement: &AchievementRow,
) -> Result<bool, String> {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            let mut conn = p.acquire().await.map_err(|e| format!("acquire: {e}"))?;
            sqlx::query("BEGIN IMMEDIATE")
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("begin: {e}"))?;
            let outcome: Result<bool, String> = async {
                let dup: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
                )
                .bind(user_id)
                .bind(&achievement.id)
                .fetch_one(&mut *conn)
                .await
                .map_err(|e| format!("dup check: {e}"))?;
                if dup > 0 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO user_achievements (user_id, achievement_id, unlocked_at, progress, equipped)
                     VALUES (?, ?, ?, ?, 0)",
                )
                .bind(user_id)
                .bind(&achievement.id)
                .bind(now)
                .bind(achievement.condition_threshold)
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("insert unlock: {e}"))?;
                insert_badge_notification(&mut conn, user_id, achievement, now).await?;
                Ok(true)
            }
            .await;
            match outcome {
                Ok(v) => {
                    sqlx::query("COMMIT")
                        .execute(&mut *conn)
                        .await
                        .map_err(|e| format!("commit: {e}"))?;
                    Ok(v)
                }
                Err(e) => {
                    let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                    Err(e)
                }
            }
        }
        Either::Right(p) => {
            let mut tx = p.begin().await.map_err(|e| format!("begin: {e}"))?;
            let outcome: Result<bool, String> = async {
                let dup: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*) FROM user_achievements WHERE user_id = ? AND achievement_id = ?",
                )
                .bind(user_id)
                .bind(&achievement.id)
                .fetch_one(&mut *tx)
                .await
                .map_err(|e| format!("dup check: {e}"))?;
                if dup > 0 {
                    return Ok(false);
                }
                sqlx::query(
                    "INSERT INTO user_achievements (user_id, achievement_id, unlocked_at, progress, equipped)
                     VALUES (?, ?, ?, ?, 0)",
                )
                .bind(user_id)
                .bind(&achievement.id)
                .bind(now)
                .bind(achievement.condition_threshold)
                .execute(&mut *tx)
                .await
                .map_err(|e| format!("insert unlock: {e}"))?;
                insert_badge_notification_mysql(&mut tx, user_id, achievement, now).await?;
                Ok(true)
            }
            .await;
            match outcome {
                Ok(v) => {
                    tx.commit().await.map_err(|e| format!("commit: {e}"))?;
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

/// 解锁通知（SQLite 连接内）：type='badge'，title=`恭喜解锁成就「{name}」`。
async fn insert_badge_notification(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    achievement: &AchievementRow,
    now: i64,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, body, created_at)
         VALUES (?, ?, 'badge', ?, ?, ?)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(user_id)
    .bind(format!("恭喜解锁成就「{}」", achievement.name))
    .bind(&achievement.description)
    .bind(now)
    .execute(conn)
    .await
    .map_err(|e| format!("insert notification: {e}"))?;
    Ok(())
}

/// 解锁通知（MySQL 事务内，同 SQLite 语义）。
async fn insert_badge_notification_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    achievement: &AchievementRow,
    now: i64,
) -> Result<(), String> {
    sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, body, created_at)
         VALUES (?, ?, 'badge', ?, ?, ?)",
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(user_id)
    .bind(format!("恭喜解锁成就「{}」", achievement.name))
    .bind(&achievement.description)
    .bind(now)
    .execute(&mut **tx)
    .await
    .map_err(|e| format!("insert notification: {e}"))?;
    Ok(())
}

/// 私信通知（独立小事务）：type='mention'，link='/messages'。
///
/// 供 conversations 路由在发送消息后给对方插入一条提醒；失败由调用方
/// best-effort 吞掉（warn 不阻断发消息本身）。
pub async fn notify_private_message(
    pool: &DatabasePool,
    recipient_id: &str,
    sender_username: &str,
) -> Result<(), String> {
    let now = now_millis();
    let sql = "INSERT INTO notifications (id, user_id, type, title, body, link, created_at)
               VALUES (?, ?, 'mention', ?, NULL, '/messages', ?)";
    let title = format!("{sender_username} 给你发来私信");
    let result = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(recipient_id)
            .bind(&title)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(recipient_id)
            .bind(&title)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ()),
    };
    result.map_err(|e| format!("insert dm notification: {e}"))
}

/// 成就 → 公共 JSON 投影（隐藏成就 description 脱敏为「隐藏成就」）。
pub fn achievement_public_json(a: &AchievementRow) -> serde_json::Value {
    json!({
        "code": a.code,
        "name": a.name,
        "description": if a.is_hidden != 0 { "隐藏成就" } else { a.description.as_str() },
        "category": a.category,
        "reward_exp": a.reward_exp,
        "reward_coin": a.reward_coin,
        "is_hidden": a.is_hidden != 0,
        "sort_order": a.sort_order,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn condition_types_cover_spec_set() {
        for expected in [
            "post_count",
            "comment_count",
            "reaction_received",
            "checkin_streak",
            "follower_count",
            "manual",
        ] {
            assert!(
                CONDITION_TYPES.contains(&expected),
                "缺少条件类型 {expected}"
            );
        }
    }

    #[test]
    fn stats_value_for_unknown_type_is_zero() {
        let stats = UserStats {
            post_count: 3,
            ..UserStats::default()
        };
        assert_eq!(stats.value_for("post_count"), 3);
        assert_eq!(stats.value_for("manual"), 0);
        assert_eq!(stats.value_for("nope"), 0);
    }
}
