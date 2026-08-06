//! M04-POSTS-06：scheduled 发布 Job（kind `content.publish`）。
//!
//! - [`enqueue_due_publish_jobs`]：扫描 `posts WHERE status='draft' AND
//!   scheduled_at <= now`，为每个到期帖子入队幂等 Job（dedup
//!   `content:publish:post:{id}`）；
//! - [`handle_publish_job`]：worker 集成入口——**执行时再次运行全部授权与
//!   等级校验**（[`publish_scheduled_post`] 内 `publish_preflight`），再事务
//!   切换 published + 板块计数 + 搜索索引入队；帖子非 scheduled/已发布 →
//!   幂等成功；无效 payload → 永久死信。

use serde_json::{json, Value};
use sqlx::Either;

use crate::content::posts::service::publish_scheduled_post;
use crate::db::DatabasePool;
use crate::jobs::retry::RetryClass;
use crate::jobs::worker::ClaimedJob;
use crate::jobs::worker_loop::JobOutcome;
use crate::outbox::now_millis;

/// 定时发布 Job kind（worker 注册名）。
pub const PUBLISH_JOB_KIND: &str = "content.publish";

const PUBLISH_QUEUE: &str = "default";
const PUBLISH_BATCH_LIMIT: i64 = 200;

/// 为到期的 scheduled 帖子入队发布 Job；返回新入队数量。
pub async fn enqueue_due_publish_jobs(pool: &DatabasePool, limit: i64) -> Result<usize, String> {
    let now = now_millis();
    let cap = limit.min(PUBLISH_BATCH_LIMIT);
    let ids: Vec<(String,)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT id FROM posts WHERE status = 'draft' AND scheduled_at IS NOT NULL AND scheduled_at <= ? LIMIT ?",
        )
        .bind(now)
        .bind(cap)
        .fetch_all(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as(
            "SELECT id FROM posts WHERE status = 'draft' AND scheduled_at IS NOT NULL AND scheduled_at <= ? LIMIT ?",
        )
        .bind(now)
        .bind(cap)
        .fetch_all(p)
        .await
        .map_err(|e| e.to_string())?,
    };

    let mut enqueued = 0usize;
    for (post_id,) in ids {
        let id = uuid::Uuid::now_v7().to_string();
        let payload = json!({ "source": "post", "id": post_id });
        let dedup = format!("content:publish:post:{post_id}");
        let inserted = match pool {
            Either::Left(p) => sqlx::query(
                "INSERT OR IGNORE INTO jobs
                     (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                      available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(PUBLISH_QUEUE)
            .bind(PUBLISH_JOB_KIND)
            .bind(payload.to_string())
            .bind(now)
            .bind(&dedup)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected(),
            Either::Right(p) => sqlx::query(
                "INSERT IGNORE INTO jobs
                     (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                      available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(PUBLISH_QUEUE)
            .bind(PUBLISH_JOB_KIND)
            .bind(payload.to_string())
            .bind(now)
            .bind(&dedup)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected(),
        };
        if inserted > 0 {
            enqueued += 1;
        }
    }
    Ok(enqueued)
}

/// Worker 集成入口：处理 `content.publish` Job。
pub async fn handle_publish_job(pool: &DatabasePool, job: &ClaimedJob) -> JobOutcome {
    let source = match job.payload.get("source").and_then(Value::as_str) {
        Some("post") => "post",
        _ => return permanent("content.publish: invalid payload: source must be 'post'"),
    };
    let id = match job.payload.get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => id,
        _ => return permanent("content.publish: invalid payload: missing id"),
    };
    let result = match source {
        "post" => publish_scheduled_post(pool, id, now_millis()).await,
        _ => unreachable!(),
    };
    match result {
        Ok(_) => JobOutcome::Succeeded,
        Err(e) => {
            // 帖子非 scheduled/不存在 → 幂等成功（无需重试）；其余瞬时重试
            match e {
                crate::content::posts::service::PublishError::NotFound(_) => JobOutcome::Succeeded,
                other => JobOutcome::Failed {
                    class: RetryClass::Transient,
                    error: format!("content.publish: {other}"),
                },
            }
        }
    }
}

fn permanent(error: &str) -> JobOutcome {
    JobOutcome::Failed {
        class: RetryClass::Permanent,
        error: error.to_owned(),
    }
}
