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

/// SMTP 发件抽象（测试用 RecordingSender；生产由 HTTP 中继实现，见 `relay`）。
pub trait EmailSender: Send + Sync {
    /// 投递一封邮件；`Err` 返回 Provider 错误（SMTP 应答码等）。
    ///
    /// 以 desugar 形式声明（而非 `async fn`）：`async_fn_in_trait` lint
    /// 在 `-D warnings` 下拒绝公共 trait 的 `async fn`（auto trait
    /// bounds 不可指定），desugar 显式给出 `+ Send`。
    fn send(
        &self,
        to: &str,
        subject: &str,
        body: &str,
    ) -> impl std::future::Future<Output = Result<(), ProviderError>> + Send;
}

/// 数据库中的 SMTP 配置（0064 site_settings 扩展）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DbSmtpConfig {
    pub enabled: bool,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub pass: String,
    pub from_email: String,
    pub from_name: String,
    pub encryption: String,
}

#[derive(sqlx::FromRow)]
struct DbSmtpRow {
    smtp_enabled: i64,
    smtp_host: String,
    smtp_port: i64,
    smtp_user: String,
    smtp_pass: String,
    smtp_from_email: String,
    smtp_from_name: String,
    smtp_encryption: String,
}

/// 从 site_settings 读取当前数据库中的 SMTP 配置。
///
/// `settings_key` 为 `BBLBB__SETTINGS_ENCRYPTION_KEY`（P0 整改：smtp_pass
/// 静态加密 `enc1:` 密文；历史明文原样透传，解密失败按未配置处理）。
pub async fn load_smtp_config_from_db(
    pool: &DatabasePool,
    settings_key: &str,
) -> Result<Option<DbSmtpConfig>, sqlx::Error> {
    let sql = "SELECT smtp_enabled, smtp_host, smtp_port, smtp_user, smtp_pass, smtp_from_email, smtp_from_name, smtp_encryption FROM site_settings WHERE id = 'singleton'";
    let row: Option<DbSmtpRow> = match pool {
        Either::Left(p) => sqlx::query_as(sql).fetch_optional(p).await?,
        Either::Right(p) => sqlx::query_as(sql).fetch_optional(p).await?,
    };
    Ok(row.map(|r| DbSmtpConfig {
        enabled: r.smtp_enabled != 0,
        host: r.smtp_host,
        port: r.smtp_port.clamp(1, 65535) as u16,
        user: r.smtp_user,
        pass: crate::config::secret_crypto::decrypt_setting(settings_key, &r.smtp_pass),
        from_email: r.smtp_from_email,
        from_name: r.smtp_from_name,
        encryption: r.smtp_encryption,
    }))
}

/// 记录式发件器（测试断言调用参数；可选失败脚本）。
#[derive(Debug, Default)]
pub struct RecordingSender {
    pub calls: std::sync::Mutex<Vec<(String, String, String)>>,
    pub failures: std::sync::Mutex<Vec<ProviderError>>,
}

impl EmailSender for RecordingSender {
    async fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), ProviderError> {
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
pub async fn deliver_email_job<S: EmailSender + Sync>(
    pool: &DatabasePool,
    worker_id: &str,
    job_id: &str,
    sender: &S,
    settings_key: &str,
) -> Result<(), EmailError> {
    let row: Option<(String, i64)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT payload, payload_version FROM jobs WHERE id = ?")
                .bind(job_id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT payload, payload_version FROM jobs WHERE id = ?")
                .bind(job_id)
                .fetch_optional(p)
                .await?
        }
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
    // 换绑确认等定向邮件：params.to_email 覆盖默认收件人（新邮箱确认前
    // users.email_normalized 仍是旧邮箱，GA 邮箱换绑）
    let recipient = params
        .get("to_email")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or(recipient);

    let params = match expand_verification_params(pool, settings_key, template_key, &params).await {
        Ok(Some(p)) => p,
        Ok(None) => params,
        Err(reason) => {
            // token 已消费/过期/无法解密：永久失败（重试无意义）
            let safe = sanitize_log("", "verification link unavailable", &reason);
            let _ = fail_job(
                pool,
                worker_id,
                job_id,
                &safe,
                RetryClass::Permanent,
                &email_retry_policy(),
            )
            .await;
            return Err(EmailError::Invalid(safe));
        }
    };

    let rendered = render(template_key, &params);
    let body = format!(
        "{}\n\n{}\n\n（此邮件由 BBLBB 自动发送，请勿直接回复）",
        rendered.body.as_deref().unwrap_or_default(),
        "如非本人操作请及时修改密码。"
    );

    match sender.send(&recipient, &rendered.title, &body).await {
        Ok(()) => {
            let _ = complete_job(pool, worker_id, job_id).await;
            Ok(())
        }
        Err(provider_err) => {
            let class = provider_err.classify();
            let retry_class = class.retry_class().unwrap_or(RetryClass::Permanent);
            let detail = provider_detail(&provider_err);
            let safe = sanitize_log(&recipient, &rendered.title, &detail);
            let _ = fail_job(
                pool,
                worker_id,
                job_id,
                &safe,
                retry_class,
                &email_retry_policy(),
            )
            .await;
            Err(EmailError::Invalid(safe))
        }
    }
}

/// 邮件投递前的一次性链接展开（GA P0-2 收尾）。
///
/// 对 `email.verification` / `email.password_reset` 模板：用 params 里的
/// `token_id` 查 token 密文列（`token_encrypted`，GA migration 0083），以
/// settings key 解密明文，构造站点一次性链接注入 `verify_url`/`reset_url`。
/// payload 全程不携带明文 token（M01-JOBS-12）。
///
/// 返回 None：模板不涉及链接（原样透传 params）；`Some(Err)`：token 缺失/
/// 无法解密（调用方按永久失败处理——token 已消费或过期，重试无意义）。
pub async fn expand_verification_params(
    pool: &DatabasePool,
    settings_key: &str,
    template_key: TemplateKey,
    params: &serde_json::Map<String, Value>,
) -> Result<Option<serde_json::Map<String, Value>>, String> {
    use crate::notifications::templates::TemplateKey as TK;
    let (table, url_path, url_key) = match template_key {
        TK::EmailVerification => (
            "email_verification_tokens",
            "/verify-email?token=",
            "verify_url",
        ),
        TK::PasswordReset => (
            "password_reset_tokens",
            "/password-reset/confirm?token=",
            "reset_url",
        ),
        TK::EmailChange => (
            "email_change_tokens",
            "/email-change/confirm?token=",
            "confirm_url",
        ),
        _ => return Ok(None),
    };
    let token_id = params
        .get("token_id")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| "verification params missing token_id".to_string())?;
    let sql = format!("SELECT token_encrypted FROM {table} WHERE id = ?");
    let sealed: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(&sql)
                .bind(token_id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(&sql)
                .bind(token_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| format!("token lookup failed: {e}"))?;
    let sealed = sealed.ok_or_else(|| "token not found (expired or consumed)".to_string())?;
    let token = crate::auth::token::open_token(settings_key, &sealed)
        .ok_or_else(|| "token cannot be decrypted (expired or key rotated)".to_string())?;
    let origin = std::env::var("BBLBB__PUBLIC_ORIGIN")
        .unwrap_or_default()
        .trim_end_matches('/')
        .to_string();
    let mut out = params.clone();
    out.insert(
        url_key.to_string(),
        Value::String(format!("{origin}{url_path}{}", token)),
    );
    Ok(Some(out))
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
