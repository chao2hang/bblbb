//! M04-MARKDOWN-05：渲染策略版本持久化与升级重渲染 Job。
//!
//! - 写路径（M04-POSTS 发布/编辑时调用）用 [`render_content`] 生成渲染产物：
//!   `{body_html, restricted_html, excerpt, renderer_version = 当前策略版本}`，
//!   随 post_contents/post_revisions 落库——存量行因此可与当前版本区分；
//! - 策略升级（[`policy::POLICY_VERSION`] 变化，renderer 或 sanitizer 任一
//!   递增）后，调用 [`enqueue_rerender_jobs`] 找出所有 `renderer_version !=
//!   当前` 的行（post_contents + post_revisions），为每行入队一个幂等 Job
//!   （kind `markdown.rerender`，dedup `markdown:rerender:{target}:{id}`）；
//! - [`handle_rerender_job`] 为 Worker 集成入口：按 payload `{target, id}`
//!   用**当前**策略重渲染 markdown 原文并覆盖渲染产物（markdown 快照与
//!   元数据不变）。行已删除或已是最新版本 → 幂等成功；无效 payload → 永久
//!   死信；数据库错误 → 瞬时重试。

use serde_json::{json, Value};
use sqlx::Either;

use crate::content::markdown::excerpt::render_excerpt;
use crate::content::markdown::policy::policy_version;
use crate::content::markdown::render_and_sanitize;
use crate::content::model::{PostContent, PostRevision};
use crate::content::repository::{
    get_post_revision, load_post_content, save_post_content, update_post_revision_rendered,
};
use crate::db::DatabasePool;
use crate::jobs::retry::RetryClass;
use crate::jobs::worker::ClaimedJob;
use crate::jobs::worker_loop::JobOutcome;
use crate::outbox::now_millis;

/// 重渲染 Job kind（worker 注册名）。
pub const RERENDER_JOB_KIND: &str = "markdown.rerender";

/// 重渲染 Job 队列。
const RERENDER_QUEUE: &str = "default";

/// 单次入队扫描的行数上限（分批执行，避免大库一次性扫描全部）。
const RERENDER_BATCH_LIMIT: i64 = 500;

/// 一次渲染的完整产物（写路径与重渲染 Job 共用的事实来源）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedContent {
    pub body_html: String,
    pub restricted_html: Option<String>,
    pub excerpt: String,
    pub renderer_version: String,
}

/// 用**当前**策略版本渲染正文：公开 HTML + 受限 HTML + 公开安全摘要。
///
/// - `body_html`/`restricted_html` 经完整管线（CommonMark → allowlist 清洗）；
/// - `excerpt` 只从公开正文提取（M04-MARKDOWN-06 语义）；
/// - `renderer_version` = [`policy_version`]（renderer+sanitizer 组合版本）。
pub fn render_content(body_markdown: &str, restricted_markdown: Option<&str>) -> RenderedContent {
    let body_html = render_and_sanitize(body_markdown);
    let restricted_html = restricted_markdown.map(render_and_sanitize);
    let excerpt = render_excerpt(body_markdown);
    RenderedContent {
        body_html,
        restricted_html,
        excerpt,
        renderer_version: policy_version(),
    }
}

// ---------------------------------------------------------------------------
// 陈旧行扫描与入队
// ---------------------------------------------------------------------------

struct StaleRow {
    target: &'static str,
    id: String,
}

/// 找出 `renderer_version != 当前策略版本` 的 post_contents 与 post_revisions
/// 行（最多 `limit` 条，先内容后修订）。
async fn list_stale_rows(pool: &DatabasePool, limit: i64) -> Result<Vec<StaleRow>, String> {
    let mut rows = Vec::new();
    let cap = limit.min(RERENDER_BATCH_LIMIT);
    match pool {
        Either::Left(p) => {
            let contents: Vec<(String,)> = sqlx::query_as(
                "SELECT post_id FROM post_contents WHERE renderer_version <> ? LIMIT ?",
            )
            .bind(policy_version())
            .bind(cap)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            let remaining = cap - contents.len() as i64;
            let revisions: Vec<(String,)> = if remaining > 0 {
                sqlx::query_as("SELECT id FROM post_revisions WHERE renderer_version <> ? LIMIT ?")
                    .bind(policy_version())
                    .bind(remaining)
                    .fetch_all(p)
                    .await
                    .map_err(|e| e.to_string())?
            } else {
                Vec::new()
            };
            rows.extend(contents.into_iter().map(|(id,)| StaleRow {
                target: "content",
                id,
            }));
            rows.extend(revisions.into_iter().map(|(id,)| StaleRow {
                target: "revision",
                id,
            }));
        }
        Either::Right(p) => {
            let contents: Vec<(String,)> = sqlx::query_as(
                "SELECT post_id FROM post_contents WHERE renderer_version <> ? LIMIT ?",
            )
            .bind(policy_version())
            .bind(cap)
            .fetch_all(p)
            .await
            .map_err(|e| e.to_string())?;
            let remaining = cap - contents.len() as i64;
            let revisions: Vec<(String,)> = if remaining > 0 {
                sqlx::query_as("SELECT id FROM post_revisions WHERE renderer_version <> ? LIMIT ?")
                    .bind(policy_version())
                    .bind(remaining)
                    .fetch_all(p)
                    .await
                    .map_err(|e| e.to_string())?
            } else {
                Vec::new()
            };
            rows.extend(contents.into_iter().map(|(id,)| StaleRow {
                target: "content",
                id,
            }));
            rows.extend(revisions.into_iter().map(|(id,)| StaleRow {
                target: "revision",
                id,
            }));
        }
    }
    Ok(rows)
}

/// 为陈旧行入队重渲染 Job（幂等：同一目标已有待处理 Job 则跳过）。
///
/// 返回本次新入队的数量。策略升级后调用；普通运行期通常返回 0。
pub async fn enqueue_rerender_jobs(pool: &DatabasePool, limit: i64) -> Result<usize, String> {
    let rows = list_stale_rows(pool, limit).await?;
    let mut enqueued = 0usize;
    for row in rows {
        let id = uuid::Uuid::now_v7().to_string();
        let payload = json!({ "target": row.target, "id": row.id });
        let dedup = format!("markdown:rerender:{}:{}", row.target, row.id);
        let now = now_millis();
        let inserted = match pool {
            Either::Left(p) => sqlx::query(
                "INSERT OR IGNORE INTO jobs
                     (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                      available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(RERENDER_QUEUE)
            .bind(RERENDER_JOB_KIND)
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
            .bind(RERENDER_QUEUE)
            .bind(RERENDER_JOB_KIND)
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

// ---------------------------------------------------------------------------
// Worker 集成入口
// ---------------------------------------------------------------------------

/// 处理 `markdown.rerender` Job：解析 payload 并映射为 [`JobOutcome`]。
pub async fn handle_rerender_job(pool: &DatabasePool, job: &ClaimedJob) -> JobOutcome {
    let target = match job.payload.get("target").and_then(Value::as_str) {
        Some("content") => "content",
        Some("revision") => "revision",
        _ => return permanent("markdown.rerender: invalid payload: missing/unknown target"),
    };
    let id = match job.payload.get("id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => id,
        _ => return permanent("markdown.rerender: invalid payload: missing id"),
    };
    let outcome = match target {
        "content" => rerender_content(pool, id).await,
        "revision" => rerender_revision(pool, id).await,
        _ => unreachable!("target 已在上方校验"),
    };
    match outcome {
        Ok(()) => JobOutcome::Succeeded,
        Err(e) => JobOutcome::Failed {
            class: RetryClass::Transient,
            error: format!("markdown.rerender: {e}"),
        },
    }
}

/// 用当前策略重渲染帖子的公开/受限正文并覆盖 `post_contents` 行。
async fn rerender_content(pool: &DatabasePool, post_id: &str) -> Result<(), String> {
    let Some(content) = load_post_content(pool, post_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        // 行已删除（帖子删除级联）→ 幂等成功
        return Ok(());
    };
    if content.renderer_version == policy_version() {
        // 已被并发 Job 更新到最新 → 幂等成功
        return Ok(());
    }
    let rendered = render_content(
        &content.body_markdown,
        content.restricted_markdown.as_deref(),
    );
    let updated = PostContent {
        post_id: content.post_id,
        body_markdown: content.body_markdown,
        body_html: rendered.body_html,
        restricted_markdown: content.restricted_markdown,
        restricted_html: rendered.restricted_html,
        renderer_version: rendered.renderer_version.to_string(),
        excerpt: rendered.excerpt,
        updated_at: now_millis(),
    };
    save_post_content(pool, &updated)
        .await
        .map_err(|e| e.to_string())
}

/// 用当前策略重渲染单条修订并覆盖渲染产物（markdown 快照不变）。
async fn rerender_revision(pool: &DatabasePool, revision_id: &str) -> Result<(), String> {
    let Some(revision) = get_post_revision(pool, revision_id)
        .await
        .map_err(|e| e.to_string())?
    else {
        return Ok(()); // 行已删除 → 幂等成功
    };
    if revision.renderer_version == policy_version() {
        return Ok(()); // 已是最新 → 幂等成功
    }
    let rendered = render_content(
        &revision.body_markdown,
        revision.restricted_markdown.as_deref(),
    );
    let updated = PostRevision {
        id: revision.id,
        post_id: revision.post_id,
        editor_id: revision.editor_id,
        body_markdown: revision.body_markdown,
        body_html: rendered.body_html,
        restricted_markdown: revision.restricted_markdown,
        restricted_html: rendered.restricted_html,
        renderer_version: rendered.renderer_version.to_string(),
        change_reason: revision.change_reason,
        version: revision.version,
        created_at: revision.created_at,
    };
    update_post_revision_rendered(pool, &updated)
        .await
        .map_err(|e| e.to_string())
}

fn permanent(error: &str) -> JobOutcome {
    JobOutcome::Failed {
        class: RetryClass::Permanent,
        error: error.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::markdown::policy::{policy_version, RENDERER_VERSION, SANITIZER_VERSION};

    #[test]
    fn policy_version_combines_both_versions() {
        let v = policy_version();
        assert_eq!(v, format!("{RENDERER_VERSION}+{SANITIZER_VERSION}"));
        assert!(v.starts_with("markdown-v1+ammonia-v1"));
    }

    #[test]
    fn render_content_tags_current_policy() {
        let out = render_content("# 标题\n\n正文 `code`", Some("> 受限"));
        assert!(
            out.body_html.contains("<h1 id=\"标题\">标题</h1>"),
            "{}",
            out.body_html
        );
        assert!(out
            .restricted_html
            .as_deref()
            .unwrap()
            .contains("<blockquote>"));
        assert_eq!(out.renderer_version, policy_version());
        assert!(!out.excerpt.is_empty());
    }

    #[test]
    fn render_content_without_restricted_keeps_none() {
        let out = render_content("正文", None);
        assert!(out.restricted_html.is_none());
    }
}
