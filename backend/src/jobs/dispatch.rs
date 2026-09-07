//! Worker 模式任务分发（M15-PACKAGE-04 / M15-UPGRADE-06）。
//!
//! `bblbb-backend --worker` 运行时把 [`ClaimedJob`] 按 `kind` 分发给已实现的
//! 处理器：
//!
//! | kind | queue | handler |
//! |---|---|---|
//! | `search.index` | default | `search::index_job::handle_index_job` |
//! | `markdown.rerender` | default | `content::markdown::rerender::handle_rerender_job` |
//! | `content.publish` | default | `content::posts::publish_job::handle_publish_job` |
//! | `account_deletion` | default | `users::deletion::handle_account_deletion` |
//! | `email.deliver` | mail | 需要 SMTP sender（当前无生产 SMTP 客户端）；未配置时按临时失败返回，重试至 `max_attempts` 后进入 dead-letter |
//!
//! 未知 kind 按永久失败（dead-letter）处理：可观测、不静默丢弃。
//! worker 停机语义（停止领取、租约处理、总超时）由
//! [`worker_loop::run_worker`] 保证。

use crate::db::pool::DatabasePool;
use crate::jobs::retry::RetryClass;
use crate::jobs::worker::ClaimedJob;
use crate::jobs::worker_loop::JobOutcome;

/// `--worker` 模式默认领取的队列。
pub const WORKER_QUEUES: &[&str] = &["default", "mail"];

/// 按 `kind` 分发一个已领取的任务。
pub async fn dispatch_job(pool: &DatabasePool, job: ClaimedJob) -> JobOutcome {
    match job.kind.as_str() {
        "search.index" => crate::search::index_job::handle_index_job(pool, &job).await,
        "markdown.rerender" => {
            crate::content::markdown::rerender::handle_rerender_job(pool, &job).await
        }
        "content.publish" => {
            crate::content::posts::publish_job::handle_publish_job(pool, &job).await
        }
        "account_deletion" => crate::users::deletion::handle_account_deletion(pool, &job).await,
        "email.deliver" => {
            // 从数据库读取 SMTP 发信配置
            let smtp_conf = match crate::email::service::load_smtp_config_from_db(pool).await {
                Ok(c) => c,
                Err(e) => {
                    tracing::error!(job_id = %job.id, error = %e, "email.deliver job: failed to read smtp config from db");
                    None
                }
            };

            let (enabled, host_set) = match &smtp_conf {
                Some(conf) => (conf.enabled, !conf.host.is_empty()),
                None => (false, false),
            };

            if !enabled {
                tracing::warn!(job_id = %job.id, "email.deliver job: SMTP is disabled in database site settings");
                JobOutcome::Failed {
                    class: RetryClass::Transient,
                    error: "SMTP is disabled in database site settings".to_owned(),
                }
            } else if !host_set {
                tracing::warn!(job_id = %job.id, "email.deliver job: SMTP host is not configured in database site settings");
                JobOutcome::Failed {
                    class: RetryClass::Transient,
                    error: "SMTP host is empty in database site settings".to_owned(),
                }
            } else {
                let conf = smtp_conf.as_ref().unwrap();
                tracing::warn!(
                    job_id = %job.id,
                    host = %conf.host,
                    port = conf.port,
                    "email.deliver job: SMTP enabled in DB ({}:{}), network sender awaiting client transport",
                    conf.host,
                    conf.port
                );
                JobOutcome::Failed {
                    class: RetryClass::Transient,
                    error: format!(
                        "SMTP configured for {}:{} but network transport is not initialized",
                        conf.host, conf.port
                    ),
                }
            }
        }
        other => {
            tracing::error!(
                kind = %other,
                job_id = %job.id,
                "unknown job kind in worker dispatch; dead-lettering"
            );
            JobOutcome::Failed {
                class: RetryClass::Permanent,
                error: format!("unknown job kind: {other}"),
            }
        }
    }
}
