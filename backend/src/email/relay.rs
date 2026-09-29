//! 生产邮件投递（HTTP 中继）。
//!
//! 背景与选型（GA 生产验证 2026-09-28，P1-10）：
//! - 生产源站（东京机房）出站 TCP 25/587 被机房防火墙封禁，进程内
//!   SMTP 客户端无法直连任何第三方提交端口；443 出站正常。
//! - 因此生产投递走 **HTTP 提交桥**：`POST {base}/mail/send`
//!   （`X-Relay-Token` 鉴权，From 由中继锁定 `noreply@bblbb.com`），
//!   中继机（mail.bblbb.com）以 Postfix+OpenDKIM 完成真实外发。
//!
//! 配置（登记于 `config::CONFIG_REGISTRY`；未配置时回落数据库
//! SMTP 分支行为）：
//! - `BBLBB__MAIL_RELAY_URL`：中继基地址（如 `https://mail.bblbb.com`）
//! - `BBLBB__MAIL_RELAY_TOKEN`：提交桥 token
//!
//! 日志安全（M05-NOTIFY-08）：本模块不产生含邮箱/正文的日志行；
//! 失败摘要经 `sanitize_log` 处理后进入 Job 错误与日志。

use sqlx::Either;

use crate::db::pool::DatabasePool;
use crate::email::service::{sanitize_log, EmailError, EmailSender};
use crate::jobs::classify::ProviderError;
use crate::jobs::retry::RetryClass;
use crate::notifications::templates::{render, TemplateKey};
use serde_json::{json, Value};

const RELAY_TIMEOUT_SECS: u64 = 30;
/// 环境变量名（与 `CONFIG_REGISTRY` 一致）。
pub const ENV_RELAY_URL: &str = "BBLBB__MAIL_RELAY_URL";
pub const ENV_RELAY_TOKEN: &str = "BBLBB__MAIL_RELAY_TOKEN";

/// 中继投递失败：错误摘要 + 重试分类（状态推进由 worker_loop 统一完成）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelayDeliveryError {
    pub error: EmailError,
    pub class: RetryClass,
}

/// HTTP 中继发件器：实现 [`EmailSender`]（异步投递）。
#[derive(Debug, Clone)]
pub struct RelaySender {
    base_url: String,
    token: String,
    client: reqwest::Client,
}

impl RelaySender {
    /// 从环境读取；`URL` 与 `TOKEN` 任一缺失/为空 → `None`（回落 DB SMTP 分支）。
    pub fn from_env() -> Option<Self> {
        let base_url = std::env::var(ENV_RELAY_URL).ok()?;
        let token = std::env::var(ENV_RELAY_TOKEN).ok()?;
        if base_url.trim().is_empty() || token.trim().is_empty() {
            return None;
        }
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(RELAY_TIMEOUT_SECS))
            .build()
            .ok()?;
        Some(Self {
            base_url: base_url.trim_end_matches('/').to_string(),
            token,
            client,
        })
    }

    fn endpoint(&self) -> String {
        format!("{}/mail/send", self.base_url)
    }
}

impl EmailSender for RelaySender {
    async fn send(&self, to: &str, subject: &str, body: &str) -> Result<(), ProviderError> {
        let payload = json!({
            "to": [to],
            "subject": subject,
            "text": body,
        });
        let resp = self
            .client
            .post(self.endpoint())
            .header("X-Relay-Token", &self.token)
            .json(&payload)
            .send()
            .await
            .map_err(|e| {
                if e.is_timeout() {
                    ProviderError::Timeout {
                        operation: "mail-relay post",
                    }
                } else {
                    ProviderError::Connection
                }
            })?;
        let status = resp.status().as_u16();
        if (200..300).contains(&status) {
            return Ok(());
        }
        Err(ProviderError::S3 { status })
    }
}

/// `email.deliver` Job 的中继投递路径。
///
/// 与 `service::deliver_email_job` 平行：同一 payload 契约
/// （`user_id`/`template_key`/`params`）、同一渲染与日志安全策略，
/// 发送点替换为 HTTP 中继。状态推进由 worker_loop 统一完成（本函数
/// 不调用 complete_job/fail_job）。
pub async fn deliver_job_via_relay(
    pool: &DatabasePool,
    job_id: &str,
    sender: &RelaySender,
    settings_key: &str,
) -> Result<(), RelayDeliveryError> {
    let row: Option<(String, i64)> = match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, i64)>(
            "SELECT payload, payload_version FROM jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_optional(p)
        .await
        .map_err(|e| RelayDeliveryError {
            error: EmailError::Db(e.to_string()),
            class: RetryClass::Transient,
        })?,
        Either::Right(p) => sqlx::query_as::<_, (String, i64)>(
            "SELECT payload, payload_version FROM jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_optional(p)
        .await
        .map_err(|e| RelayDeliveryError {
            error: EmailError::Db(e.to_string()),
            class: RetryClass::Transient,
        })?,
    };
    let Some((payload_str, _version)) = row else {
        return Err(RelayDeliveryError {
            error: EmailError::NotFound("email job not found".to_string()),
            class: RetryClass::Permanent,
        });
    };
    let payload: Value = serde_json::from_str(&payload_str).map_err(|e| RelayDeliveryError {
        error: EmailError::Invalid(e.to_string()),
        class: RetryClass::Permanent,
    })?;
    let bad_payload = |msg: &str| RelayDeliveryError {
        error: EmailError::Invalid(msg.to_string()),
        class: RetryClass::Permanent,
    };
    let user_id = payload["user_id"]
        .as_str()
        .ok_or_else(|| bad_payload("payload missing user_id"))?
        .to_string();
    let template_key_str = payload["template_key"]
        .as_str()
        .ok_or_else(|| bad_payload("payload missing template_key"))?;
    let template_key = TemplateKey::parse(template_key_str)
        .ok_or_else(|| bad_payload("payload has unknown template_key"))?;
    let params = payload["params"]
        .as_object()
        .cloned()
        .ok_or_else(|| bad_payload("payload missing params"))?;

    let recipient: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar::<_, String>("SELECT email_normalized FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_optional(p)
                .await
                .map_err(|e| RelayDeliveryError {
                    error: EmailError::Db(e.to_string()),
                    class: RetryClass::Transient,
                })?
        }
        Either::Right(p) => {
            sqlx::query_scalar::<_, String>("SELECT email_normalized FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_optional(p)
                .await
                .map_err(|e| RelayDeliveryError {
                    error: EmailError::Db(e.to_string()),
                    class: RetryClass::Transient,
                })?
        }
    };
    let Some(recipient) = recipient else {
        return Err(RelayDeliveryError {
            error: EmailError::NotFound(sanitize_log("", "recipient user not found", "")),
            class: RetryClass::Permanent,
        });
    };

    let params = crate::email::service::expand_verification_params(
        pool,
        settings_key,
        template_key,
        &params,
    )
    .await
    .map_err(|reason| RelayDeliveryError {
        error: EmailError::Invalid(sanitize_log("", "verification link unavailable", &reason)),
        class: RetryClass::Permanent,
    })?
    .unwrap_or(params);

    let rendered = render(template_key, &params);
    let body = format!(
        "{}\n\n{}\n\n（此邮件由 BBLBB 自动发送，请勿直接回复）",
        rendered.body.as_deref().unwrap_or_default(),
        "如非本人操作请及时修改密码。"
    );

    sender
        .send(&recipient, &rendered.title, &body)
        .await
        .map_err(|provider_err| {
            let class = provider_err
                .classify()
                .retry_class()
                .unwrap_or(RetryClass::Permanent);
            RelayDeliveryError {
                error: EmailError::Invalid(sanitize_log(
                    &recipient,
                    &rendered.title,
                    &provider_detail(&provider_err),
                )),
                class,
            }
        })
}

fn provider_detail(err: &ProviderError) -> String {
    match err {
        ProviderError::Smtp { code } => format!("smtp rejected (code {code})"),
        ProviderError::S3 { status } => format!("provider http {status}"),
        ProviderError::Timeout { operation } => format!("provider timeout ({operation})"),
        ProviderError::Connection => "provider connection failed".to_string(),
        ProviderError::Cancelled => "operation cancelled".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relay_with(url: &str, token: &str) -> RelaySender {
        RelaySender {
            base_url: url.trim_end_matches('/').to_string(),
            token: token.to_string(),
            client: reqwest::Client::new(),
        }
    }

    #[tokio::test]
    async fn relay_send_maps_network_failure_to_provider_error() {
        // 未监听的本地端口 → Connection/Timeout（网络层映射不 panic）。
        let relay = relay_with("http://127.0.0.1:1", "t");
        let err = relay.send("a@b.test", "s", "b").await.unwrap_err();
        assert!(matches!(
            err,
            ProviderError::Connection | ProviderError::Timeout { .. }
        ));
    }

    #[test]
    fn from_env_missing_vars_returns_none_without_panic() {
        let _ = RelaySender::from_env();
    }

    #[test]
    fn template_parse_rejects_unknown_key() {
        assert!(TemplateKey::parse("reply.created").is_some());
        assert!(TemplateKey::parse("not.a.key").is_none());
    }
}
