//! M05-NOTIFY-07/08：邮件 Job 投递、重试/死信/重放与日志安全。
//!
//! - [`enqueue_email`]：以 `email.deliver` Job 入队；payload 只存
//!   `user_id` 引用与安全模板参数（无完整邮箱、无正文、无明文 token）。
//! - [`deliver_email_job`]：投递处理——成功 `complete_job`，失败经
//!   `ProviderError::classify` 转入 `fail_job`（临时→退避重试，永久→死信）。
//! - [`replay_email_job`]：管理员重放（`jobs::retry::replay_job`）。
//! - [`sanitize_log`]：掩码完整邮箱、剥离正文、脱敏 token 与 Provider
//!   响应后再写入日志/`last_error`（M05-NOTIFY-08）。

use serde_json::{json, Value};
use sqlx::Either;

use crate::db::DatabasePool;
use crate::jobs::classify::ProviderError;
use crate::jobs::payload::{redact_token, validate_mail_payload};
use crate::jobs::retry::{fail_job, replay_job, RetryClass, RetryPolicy};
use crate::jobs::worker::complete_job;
use crate::notifications::templates::{is_known_template, render, validate_params, TemplateKey};
use crate::outbox::now_millis;

/// 邮件队列与 Job kind。
pub const EMAIL_QUEUE: &str = "mail";
pub const EMAIL_JOB_KIND: &str = "email.deliver";

/// 邮件重试策略（指数退避，确定性 jitter=0 便于测试；SMTP 4xx 临时失败）。
pub fn email_retry_policy() -> RetryPolicy {
    RetryPolicy {
        base_delay_ms: 60_000,
        max_delay_ms: 3_600_000,
        jitter_ms: 0,
    }
}

/// 邮件服务错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailError {
    Db(String),
    Invalid(String),
    NotFound(String),
}

impl From<sqlx::Error> for EmailError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

/// SMTP 发件抽象（测试用 RecordingSender；生产由 SMTP 客户端实现）。
pub trait EmailSender: Send + Sync {
    /// 投递一封邮件；`Err` 返回 Provider 错误（SMTP 应答码等）。
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), ProviderError>;
}

/// 记录式发件器（测试断言调用参数；可选失败脚本）。
#[derive(Debug, Default)]
pub struct RecordingSender {
    pub calls: std::sync::Mutex<Vec<(String, String, String)>>,
    pub failures: std::sync::Mutex<Vec<ProviderError>>,
}

impl EmailSender for RecordingSender {
    fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), ProviderError> {
        {
            let mut guard = self.failures.lock().unwrap();
            if let Some(err) = guard.pop() {
                return Err(err);
            }
        }
        self.calls
            .lock()
            .unwrap()
            .push((to.to_string(), subject.to_string(), body.to_string()));
        Ok(())
    }
}

/// 入队邮件 Job（M05-NOTIFY-07）。
///
/// payload 只含 `user_id`/`template_key`/`params`/资源引用，经
/// [`validate_mail_payload`]（无明文 token）与 [`validate_params`]
/// （无隐藏正文/内部 note）双重校验。
pub async fn enqueue_email(
    pool: &DatabasePool,
    user_id: &str,
    template_key: TemplateKey,
    params: serde_json::Map<String, Value>,
    resource_type: Option<&str>,
    resource_id: Option<&str>,
    available_at: i64,
) -> Result<String, EmailError> {
    if !is_known_template(template_key.as_str()) {
        return Err(EmailError::Invalid(
            "unknown email template key".to_string(),
        ));
    }
    validate_params(&params).map_err(EmailError::Invalid)?;
    let payload = json!({
        "user_id": user_id,
        "template_key": template_key.as_str(),
        "params": params,
        "resource_type": resource_type,
        "resource_id": resource_id,
    });
    validate_mail_payload(&payload).map_err(|e| EmailError::Invalid(e.to_string()))?;

    let id = uuid::Uuid::now_v7().to_string();
    let payload_str = serde_json::to_string(&payload).unwrap_or_default();
    let dedup_key = format!(
        "email:{}:{}:{}",
        user_id,
        template_key.as_str(),
        resource_id.unwrap_or("none")
    );
    let now = now_millis();
    let inserted = match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO jobs
                     (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                      available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(EMAIL_QUEUE)
            .bind(EMAIL_JOB_KIND)
            .bind(&payload_str)
            .bind(available_at)
            .bind(&dedup_key)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?
            .rows_affected()
                == 1
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO jobs
                     (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                      available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(EMAIL_QUEUE)
            .bind(EMAIL_JOB_KIND)
            .bind(&payload_str)
            .bind(available_at)
            .bind(&dedup_key)
            .bind(now)
            .bind(now)
            .execute(p)
            .await?
            .rows_affected()
                == 1
        }
    };
    if !inserted {
        return Err(EmailError::Invalid(
            "duplicate email job already queued for this recipient/template".to_string(),
        ));
    }
    Ok(id)
}

/// 投递一封邮件（M05-NOTIFY-07）。
///
/// 从 payload 读 user_id，查库取完整邮箱（不写入 payload/日志）；
/// 渲染模板（安全参数）；成功后 `complete_job`，失败按 Provider 分类
/// `fail_job`（临时→退避重试，永久→死信）；`last_error` 经
/// [`sanitize_log`] 处理。
pub async fn deliver_email_job(
    pool: &DatabasePool,
    worker_id: &str,
    job_id: &str,
    sender: &dyn EmailSender,
) -> Result<(), EmailError> {
    let row: Option<(String, i64)> = match pool {
        Either::Left(p) => sqlx::query_as("SELECT payload, payload_version FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_optional(p)
            .await?,
        Either::Right(p) => sqlx::query_as("SELECT payload, payload_version FROM jobs WHERE id = ?")
            .bind(job_id)
            .fetch_optional(p)
            .await?,
    };
    let Some((payload_str, _version)) = row else {
        return Err(EmailError::NotFound("email job not found".to_string()));
    };
    let payload: Value =
        serde_json::from_str(&payload_str).map_err(|e| EmailError::Invalid(e.to_string()))?;
    let user_id = payload["user_id"]
        .as_str()
        .ok_or_else(|| EmailError::Invalid("payload missing user_id".to_string()))?
        .to_string();
    let template_key_str = payload["template_key"]
        .as_str()
        .ok_or_else(|| EmailError::Invalid("payload missing template_key".to_string()))?;
    let template_key = TemplateKey::parse(template_key_str)
        .ok_or_else(|| EmailError::Invalid("payload has unknown template_key".to_string()))?;
    let params = payload["params"]
        .as_object()
        .cloned()
        .ok_or_else(|| EmailError::Invalid("payload missing params".to_string()))?;

    let recipient: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT email_normalized FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT email_normalized FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some(recipient) = recipient else {
        let err = sanitize_log("", "user not found", "");
        let _ = fail_job(
            pool,
            worker_id,
            job_id,
            &err,
            RetryClass::Permanent,
            &email_retry_policy(),
        )
        .await;
        return Err(EmailError::NotFound("recipient user not found".to_string()));
    };

    let rendered = render(template_key, &params);
    let body = format!(
        "{}\n\n{}\n\n（此邮件由 BBLBB 自动发送，请勿直接回复）",
        rendered.body.as_deref().unwrap_or_default(),
        "如非本人操作请及时修改密码。"
    );

    match sender.send(&recipient, &rendered.title, &body) {
        Ok(()) => {
            let _ = complete_job(pool, worker_id, job_id).await;
            Ok(())
        }
        Err(provider_err) => {
            let class = provider_err.classify();
            let retry_class = class
                .retry_class()
                .unwrap_or(RetryClass::Permanent);
            let detail = provider_detail(&provider_err);
            let safe = sanitize_log(&recipient, &rendered.title, &detail);
            let _ = fail_job(pool, worker_id, job_id, &safe, retry_class, &email_retry_policy())
                .await;
            Err(EmailError::Invalid(safe))
        }
    }
}

/// 管理员重放邮件 Job（M05-NOTIFY-07）：dead → queued。
pub async fn replay_email_job(pool: &DatabasePool, job_id: &str) -> Result<bool, EmailError> {
    replay_job(pool, job_id).await.map_err(EmailError::from)
}

/// Provider 错误 → 安全摘要（不含响应原文）。
fn provider_detail(err: &ProviderError) -> String {
    match err {
        ProviderError::Smtp { code } => format!("smtp rejected (code {code})"),
        ProviderError::S3 { status } => format!("provider http {status}"),
        ProviderError::Timeout { operation } => format!("provider timeout ({operation})"),
        ProviderError::Connection => "provider connection failed".to_string(),
        ProviderError::Cancelled => "operation cancelled".to_string(),
    }
}

/// 日志安全（M05-NOTIFY-08）：完整邮箱掩码为 `a***@domain`、剥离正文、
/// 脱敏 token 与 Provider 响应原文。
pub fn sanitize_log(recipient: &str, subject: &str, detail: &str) -> String {
    let masked = mask_email(recipient);
    let mut text = format!("mail_delivery to={masked}");
    if !subject.is_empty() {
        text.push_str(&format!(" subject={}", redact_token(subject)));
    }
    if !detail.is_empty() {
        text.push_str(&format!(" detail={}", redact_token(detail)));
    }
    text
}

/// 掩码完整邮箱：`user@example.com` → `u***@example.com`。
fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) if !local.is_empty() => {
            let head = &local[..1];
            format!("{head}***@{domain}")
        }
        _ => "[redacted]".to_string(),
    }
}

/// 供路由/测试使用的助手：当前 Unix 毫秒。
pub fn now() -> i64 {
    now_millis()
}
