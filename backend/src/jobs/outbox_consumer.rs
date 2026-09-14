//! 通用 Outbox 消费者（P0 整改）。
//!
//! docs/JOBS.md §2/§11：业务事务写入 `outbox_events` 后必须由后台消费者
//! 投递。此前 `fetch_pending`/`consume_in_tx`/`mark_sent_in_tx` 只有定义、
//! 没有生产调用方，事件会永久停留 `pending`。
//!
//! 消费语义（M01-JOBS-06，至少一次 + 幂等去重）：
//! 1. `fetch_pending` 取一批到期事件；
//! 2. CAS 认领：`status: pending → processing`（多 worker 竞争只有一方成功）；
//! 3. 单事务内：`consume_in_tx` 去重标记 → 领域副作用 → `mark_sent_in_tx`
//!    置 `sent`，整体提交；
//! 4. 副作用失败：整事务回滚（去重标记与副作用一起消失），`mark_failed`
//!    累计 attempts 并按退避重试，超过 max_attempts 进入 `failed`。
//!
//! 已注册副作用：
//! - `user.registered.v1` → SMTP 启用时入队 `email.deliver` 任务（验证邮件）；
//!   未配置 SMTP 时记录 warn 并置 sent（避免无 transport 的死信噪音）；
//! - 其余已注册事件 → 幂等标记已投递（占位消费者；各领域里程碑接管后
//!   在 [`side_effect`] 中追加真实副作用）。

use std::time::Duration;

use tokio::sync::watch;

use crate::db::pool::DatabasePool;
use crate::events;
use crate::outbox::{self, OutboxTx};
use sqlx::Either;

/// Outbox 消费者轮询间隔。
pub const POLL_INTERVAL: Duration = Duration::from_secs(5);

/// 单批最大事件数（与 `fetch_pending` 上限一致）。
const BATCH_LIMIT: i64 = 20;

/// 运行 Outbox 消费循环，直到停机信号到达。
///
/// `settings_key` 为 `BBLBB__SETTINGS_ENCRYPTION_KEY`（SMTP 凭据解密）。
pub async fn run(pool: DatabasePool, settings_key: String, mut shutdown: watch::Receiver<bool>) {
    let mut tick = tokio::time::interval(POLL_INTERVAL);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        if *shutdown.borrow() {
            tracing::info!("outbox consumer stopping (shutdown)");
            break;
        }
        tokio::select! {
            _ = shutdown.changed() => continue,
            _ = tick.tick() => {}
        }
        match process_batch(&pool, &settings_key, BATCH_LIMIT).await {
            Ok(0) => {}
            Ok(n) => tracing::debug!(processed = n, "outbox batch processed"),
            Err(error) => {
                tracing::warn!(%error, "outbox consumer batch failed; retrying next tick")
            }
        }
    }
}

/// 处理一批到期事件；返回实际处理（含跳过）的事件数。
pub async fn process_batch(
    pool: &DatabasePool,
    settings_key: &str,
    limit: i64,
) -> Result<usize, sqlx::Error> {
    let events = outbox::fetch_pending(pool, limit).await?;
    let mut processed = 0usize;
    for event in events {
        // CAS 认领：pending → processing；0 行 = 已被其他消费者抢走，跳过。
        let claimed = claim(pool, &event.id).await?;
        if claimed == 0 {
            processed += 1;
            continue;
        }
        match handle(pool, settings_key, &event).await {
            Ok(()) => processed += 1,
            Err(error) => {
                tracing::warn!(
                    event_id = %event.id,
                    event_type = %event.event_type,
                    %error,
                    "outbox side effect failed; scheduling retry"
                );
                outbox::mark_failed(pool, &event.id, &error.to_string()).await?;
                processed += 1;
            }
        }
    }
    Ok(processed)
}

/// CAS 认领：`pending → processing`。
async fn claim(pool: &DatabasePool, event_id: &str) -> Result<u64, sqlx::Error> {
    let sql = "UPDATE outbox_events SET status = 'processing'
               WHERE id = ? AND status = 'pending'";
    Ok(match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(event_id)
            .execute(p)
            .await?
            .rows_affected(),
        Either::Right(p) => sqlx::query(sql)
            .bind(event_id)
            .execute(p)
            .await?
            .rows_affected(),
    })
}

/// 消费单个事件：事务内去重标记 + 副作用 + 标记已投递，整体提交。
async fn handle(
    pool: &DatabasePool,
    settings_key: &str,
    event: &outbox::OutboxEvent,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match pool {
        Either::Left(p) => {
            let mut tx = OutboxTx::Left(p.begin().await?);
            consume_and_apply(&mut tx, pool, settings_key, event).await?;
            match tx {
                Either::Left(t) => t.commit().await?,
                Either::Right(_) => unreachable!(),
            }
        }
        Either::Right(p) => {
            let mut tx = OutboxTx::Right(p.begin().await?);
            consume_and_apply(&mut tx, pool, settings_key, event).await?;
            match tx {
                Either::Left(_) => unreachable!(),
                Either::Right(t) => t.commit().await?,
            }
        }
    }
    Ok(())
}

/// 事务内：去重标记（首投才执行副作用）→ 副作用 → `mark_sent_in_tx`。
async fn consume_and_apply(
    tx: &mut OutboxTx<'_>,
    pool: &DatabasePool,
    settings_key: &str,
    event: &outbox::OutboxEvent,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    if !outbox::consume_in_tx(tx, &event.id, "outbox-consumer").await? {
        // 重复投递：去重标记已存在，跳过副作用（但事件本身仍需置 sent）。
        outbox::mark_sent_in_tx(tx, &event.id).await?;
        return Ok(());
    }
    side_effect(pool, settings_key, event).await?;
    outbox::mark_sent_in_tx(tx, &event.id).await?;
    Ok(())
}

/// 领域副作用分发。未知事件幂等置 sent（占位消费者），不静默丢弃：
/// 事件保留 `sent` 状态 + 审计/日志可追溯。
async fn side_effect(
    pool: &DatabasePool,
    settings_key: &str,
    event: &outbox::OutboxEvent,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    match event.event_type.as_str() {
        events::types::USER_REGISTERED => {
            let user_id = event.payload.get("user_id").and_then(|v| v.as_str());
            let Some(user_id) = user_id else {
                return Err("user.registered payload missing user_id".into());
            };
            // SMTP 未配置时不入队邮件任务（transport 为桩，入队只会死信）。
            // settings_key 由 main 传入（P0 整改：smtp_pass 静态加密）。
            let smtp = crate::email::service::load_smtp_config_from_db(pool, settings_key).await?;
            let smtp_ready = smtp
                .as_ref()
                .map(|c| c.enabled && !c.host.is_empty())
                .unwrap_or(false);
            if !smtp_ready {
                tracing::warn!(
                    event_id = %event.id,
                    user_id = %user_id,
                    "user.registered: SMTP not configured in site settings; verification email skipped"
                );
                return Ok(());
            }
            crate::email::service::enqueue_email(
                pool,
                user_id,
                crate::notifications::templates::TemplateKey::SecurityNotice,
                serde_json::from_value(serde_json::json!({ "kind": "email_verification" }))?,
                Some("email_verification"),
                event
                    .payload
                    .get("email_verification_token_id")
                    .and_then(|v| v.as_str()),
                outbox::now_millis(),
            )
            .await
            .map_err(|e| -> Box<dyn std::error::Error + Send + Sync> {
                match e {
                    crate::email::service::EmailError::Db(m)
                    | crate::email::service::EmailError::Invalid(m)
                    | crate::email::service::EmailError::NotFound(m) => m.into(),
                }
            })?;
            Ok(())
        }
        other => {
            // 占位消费者：标记已投递并保留可观测日志；领域里程碑在此追加副作用。
            tracing::debug!(event_type = %other, event_id = %event.id, "outbox event delivered (no-op consumer)");
            Ok(())
        }
    }
}
