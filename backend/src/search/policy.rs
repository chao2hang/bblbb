//! 搜索退出与管理员索引策略（M08-INDEX-03）。
//!
//! - 作者逐帖退出：`posts.search_index_opt_out`（退出搜索引擎公开索引）与
//!   `posts.ai_summary_opt_out`（退出 AI 摘要生成），逐帖设置即 bump
//!   `posts.updated_at` 并幂等入队索引 Job（[`enqueue_index_job`]）；
//! - 管理员全站/板块策略：`search_site_index_policy`（单行）与
//!   `board_index_policies`（按板块），值域 `allow`/`deny`；
//! - **优先级：管理员 deny 覆盖作者 allow**（CRAWLER-POLICY.md §1/§3）——
//!   [`effective_admin_policy`] 取 site 与 board 的并集，任一 deny 即 deny；
//! - 策略变更副作用：[`set_site_policy`] / [`set_board_policy`] / [`set_post_opt_out`]
//!   在事务内更新行并 bump `updated_at`（策略 revision 单调性来源，
//!   docs/SEARCH.md §5），随后对受影响帖子幂等入队索引 Job。
//!
//! 领域边界：本模块是 service 层（可用 sqlx），纯裁决在 [`crate::search::gate`]。

use sqlx::Either;

use crate::db::DatabasePool;
use crate::search::index_job::enqueue_index_job;

/// 管理员策略字面量：遵循作者选择。
pub const POLICY_ALLOW: &str = "allow";
/// 管理员策略字面量：强制退出索引（优先于作者 allow）。
pub const POLICY_DENY: &str = "deny";

/// 管理员索引策略行（site 或 board）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdminIndexPolicy {
    pub search_index: String,
    pub ai_summary: String,
    /// 行 updated_at（策略 revision 输入）。
    pub updated_at: i64,
    pub version: i64,
}

impl AdminIndexPolicy {
    /// 合并 site 与 board 策略：任一 deny 即 deny（管理员关闭优先）。
    pub fn effective(site: &Option<AdminIndexPolicy>, board: &Option<AdminIndexPolicy>) -> Self {
        let denied = |f: fn(&AdminIndexPolicy) -> &str| {
            site.as_ref().is_some_and(|p| f(p) == POLICY_DENY)
                || board.as_ref().is_some_and(|p| f(p) == POLICY_DENY)
        };
        let search_index = if denied(|p| &p.search_index) {
            POLICY_DENY.to_string()
        } else {
            POLICY_ALLOW.to_string()
        };
        let ai_summary = if denied(|p| &p.ai_summary) {
            POLICY_DENY.to_string()
        } else {
            POLICY_ALLOW.to_string()
        };
        AdminIndexPolicy {
            search_index,
            ai_summary,
            updated_at: site
                .as_ref()
                .map(|p| p.updated_at)
                .into_iter()
                .chain(board.as_ref().map(|p| p.updated_at))
                .max()
                .unwrap_or(0),
            version: 1,
        }
    }
}

/// 策略行投影（site/board 同构：key, search_index, ai_summary, version, updated_by, updated_at）。
type PolicyRow = (String, String, String, i64, String, i64);

/// 读取全站索引策略（无行 = 全部 allow，不建行——惰性创建发生在写）。
pub async fn load_site_policy(pool: &DatabasePool) -> Result<Option<AdminIndexPolicy>, String> {
    let row: Option<PolicyRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, PolicyRow>(
            "SELECT scope_key, search_index, ai_summary, version, updated_by, updated_at
                 FROM search_site_index_policy WHERE scope_key = 'site'",
        )
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, PolicyRow>(
            "SELECT scope_key, search_index, ai_summary, version, updated_by, updated_at
                 FROM search_site_index_policy WHERE scope_key = 'site'",
        )
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    Ok(row.map(
        |(_, search_index, ai_summary, version, _, updated_at)| AdminIndexPolicy {
            search_index,
            ai_summary,
            version,
            updated_at,
        },
    ))
}

/// 读取指定板块的索引策略（无行 = allow）。
pub async fn load_board_policy(
    pool: &DatabasePool,
    board_id: &str,
) -> Result<Option<AdminIndexPolicy>, String> {
    let row: Option<PolicyRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, PolicyRow>(
            "SELECT board_id, search_index, ai_summary, version, updated_by, updated_at
                 FROM board_index_policies WHERE board_id = ?",
        )
        .bind(board_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, PolicyRow>(
            "SELECT board_id, search_index, ai_summary, version, updated_by, updated_at
                 FROM board_index_policies WHERE board_id = ?",
        )
        .bind(board_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    Ok(row.map(
        |(_, search_index, ai_summary, version, _, updated_at)| AdminIndexPolicy {
            search_index,
            ai_summary,
            version,
            updated_at,
        },
    ))
}

fn validate_policy_value(value: &str) -> Result<(), String> {
    if value == POLICY_ALLOW || value == POLICY_DENY {
        Ok(())
    } else {
        Err(format!(
            "policy value must be 'allow' or 'deny', got {value:?}"
        ))
    }
}

/// 设置全站索引策略（单行 upsert + bump updated_at + 入队全部帖子索引 Job）。
pub async fn set_site_policy(
    pool: &DatabasePool,
    search_index: &str,
    ai_summary: &str,
    actor: &str,
    now: i64,
) -> Result<(), String> {
    validate_policy_value(search_index)?;
    validate_policy_value(ai_summary)?;
    upsert_site_policy(pool, search_index, ai_summary, actor, now).await?;
    // 全站策略变更影响全部帖子：幂等入队（dedup 合并待处理 Job）。
    enqueue_all_posts(pool).await?;
    Ok(())
}

/// 设置板块索引策略（upsert + bump updated_at + 入队该板块帖子索引 Job）。
pub async fn set_board_policy(
    pool: &DatabasePool,
    board_id: &str,
    search_index: &str,
    ai_summary: &str,
    actor: &str,
    now: i64,
) -> Result<(), String> {
    validate_policy_value(search_index)?;
    validate_policy_value(ai_summary)?;
    upsert_board_policy(pool, board_id, search_index, ai_summary, actor, now).await?;
    enqueue_board_posts(pool, board_id).await?;
    Ok(())
}

/// 作者逐帖退出：`search_index_opt_out` / `ai_summary_opt_out`。
/// 写 posts 列 + bump updated_at（策略 revision 输入）+ 入队该帖索引 Job。
pub async fn set_post_opt_out(
    pool: &DatabasePool,
    post_id: &str,
    search_index_opt_out: bool,
    ai_summary_opt_out: bool,
    now: i64,
) -> Result<bool, String> {
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE posts
             SET search_index_opt_out = ?, ai_summary_opt_out = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(search_index_opt_out as i64)
        .bind(ai_summary_opt_out as i64)
        .bind(now)
        .bind(post_id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE posts
             SET search_index_opt_out = ?, ai_summary_opt_out = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(search_index_opt_out as i64)
        .bind(ai_summary_opt_out as i64)
        .bind(now)
        .bind(post_id)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
    };
    if affected == 0 {
        return Ok(false);
    }
    enqueue_index_job(pool, "post", post_id).await?;
    Ok(true)
}

async fn upsert_site_policy(
    pool: &DatabasePool,
    search_index: &str,
    ai_summary: &str,
    actor: &str,
    now: i64,
) -> Result<(), String> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO search_site_index_policy
                     (scope_key, search_index, ai_summary, version, updated_by, updated_at)
                 VALUES ('site', 'allow', 'allow', 1, ?, ?)",
            )
            .bind(actor)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE search_site_index_policy
                 SET search_index = ?, ai_summary = ?, version = version + 1,
                     updated_by = ?, updated_at = ?
                 WHERE scope_key = 'site'",
            )
            .bind(search_index)
            .bind(ai_summary)
            .bind(actor)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO search_site_index_policy
                     (scope_key, search_index, ai_summary, version, updated_by, updated_at)
                 VALUES ('site', 'allow', 'allow', 1, ?, ?)",
            )
            .bind(actor)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE search_site_index_policy
                 SET search_index = ?, ai_summary = ?, version = version + 1,
                     updated_by = ?, updated_at = ?
                 WHERE scope_key = 'site'",
            )
            .bind(search_index)
            .bind(ai_summary)
            .bind(actor)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

async fn upsert_board_policy(
    pool: &DatabasePool,
    board_id: &str,
    search_index: &str,
    ai_summary: &str,
    actor: &str,
    now: i64,
) -> Result<(), String> {
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO board_index_policies
                     (board_id, search_index, ai_summary, version, updated_by, updated_at)
                 VALUES (?, 'allow', 'allow', 1, ?, ?)",
            )
            .bind(board_id)
            .bind(actor)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE board_index_policies
                 SET search_index = ?, ai_summary = ?, version = version + 1,
                     updated_by = ?, updated_at = ?
                 WHERE board_id = ?",
            )
            .bind(search_index)
            .bind(ai_summary)
            .bind(actor)
            .bind(now)
            .bind(board_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO board_index_policies
                     (board_id, search_index, ai_summary, version, updated_by, updated_at)
                 VALUES (?, 'allow', 'allow', 1, ?, ?)",
            )
            .bind(board_id)
            .bind(actor)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
            sqlx::query(
                "UPDATE board_index_policies
                 SET search_index = ?, ai_summary = ?, version = version + 1,
                     updated_by = ?, updated_at = ?
                 WHERE board_id = ?",
            )
            .bind(search_index)
            .bind(ai_summary)
            .bind(actor)
            .bind(now)
            .bind(board_id)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 对全部帖子幂等入队索引 Job（全站策略变更；dedup 合并待处理 Job）。
async fn enqueue_all_posts(pool: &DatabasePool) -> Result<(), String> {
    let ids: Vec<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM posts")
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_scalar("SELECT id FROM posts")
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?,
    };
    for id in ids {
        enqueue_index_job(pool, "post", &id).await?;
    }
    Ok(())
}

/// 对指定板块的帖子幂等入队索引 Job（板块策略变更）。
async fn enqueue_board_posts(pool: &DatabasePool, board_id: &str) -> Result<(), String> {
    let ids: Vec<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM posts WHERE board_id = ?")
            .bind(board_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_scalar("SELECT id FROM posts WHERE board_id = ?")
            .bind(board_id)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?,
    };
    for id in ids {
        enqueue_index_job(pool, "post", &id).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::now_millis;

    #[test]
    fn effective_merges_deny_over_allow() {
        let site = Some(AdminIndexPolicy {
            search_index: POLICY_DENY.to_string(),
            ai_summary: POLICY_ALLOW.to_string(),
            updated_at: 100,
            version: 2,
        });
        let board = Some(AdminIndexPolicy {
            search_index: POLICY_ALLOW.to_string(),
            ai_summary: POLICY_ALLOW.to_string(),
            updated_at: 200,
            version: 1,
        });
        let eff = AdminIndexPolicy::effective(&site, &board);
        assert_eq!(eff.search_index, POLICY_DENY);
        assert_eq!(eff.ai_summary, POLICY_ALLOW);
        assert_eq!(eff.updated_at, 200, "revision 取最大 updated_at");
    }

    #[test]
    fn effective_allows_when_no_deny() {
        let site = Some(AdminIndexPolicy {
            search_index: POLICY_ALLOW.to_string(),
            ai_summary: POLICY_ALLOW.to_string(),
            updated_at: 1,
            version: 1,
        });
        let eff = AdminIndexPolicy::effective(&site, &None);
        assert_eq!(eff.search_index, POLICY_ALLOW);
        assert_eq!(eff.ai_summary, POLICY_ALLOW);

        let none = AdminIndexPolicy::effective(&None, &None);
        assert_eq!(none.search_index, POLICY_ALLOW);
        assert_eq!(none.updated_at, 0);
    }

    #[test]
    fn policy_value_validation() {
        assert!(validate_policy_value(POLICY_ALLOW).is_ok());
        assert!(validate_policy_value(POLICY_DENY).is_ok());
        assert!(validate_policy_value("maybe").is_err());
    }

    #[test]
    fn now_millis_is_positive() {
        assert!(now_millis() > 1_700_000_000_000);
    }
}
