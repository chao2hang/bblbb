//! 安全通知（M02-MFA-08）：新设备、密码/MFA 变化、Session 撤销、恢复码使用。
//!
//! 每条安全通知在一个业务事务内写三处（同事务提交/回滚，M01-JOBS-02）：
//! 1. `notifications` 行：`type='system'` + `security_kind` 非空标记
//!    （M05-NOTIFY 偏好强制“安全通知不可被普通偏好完全关闭”）；
//! 2. 审计 `auth.security_notification`（含 kind，可追踪）；
//! 3. Outbox 事件 `auth.security_notification.v1`（邮件/站内异步送达，
//!    消费者按 event_id 幂等）。
//!
//! 触发点：登录新设备（login_user）、密码重置确认（confirm_password_reset）、
//! TOTP 启用/取消（confirm/cancel_enrollment）、逐设备撤销 Session
//! （revoke_session_by_id）、恢复码消费（consume_recovery_code）。

use serde_json::json;
use sqlx::Either;

use crate::{
    audit::AuditEntry,
    db::pool::DatabasePool,
    events,
    outbox::{enqueue_in_tx, now_millis, OutboxTx},
};

/// 安全通知类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecurityEvent {
    /// 新设备登录。
    NewDevice,
    /// 密码变化（密码重置等）。
    PasswordChanged,
    /// MFA 设置变化（TOTP 启用/取消）。
    MfaChanged,
    /// 会话被撤销（逐设备撤销）。
    SessionRevoked,
    /// 恢复码被使用（可能表明账号被他人接管）。
    RecoveryCodeUsed,
}

impl SecurityEvent {
    /// 稳定 kind 键：写入 `notifications.security_kind` / 审计 metadata /
    /// outbox payload，供偏好与模板引用。
    pub fn kind(&self) -> &'static str {
        match self {
            SecurityEvent::NewDevice => "new_device",
            SecurityEvent::PasswordChanged => "password_changed",
            SecurityEvent::MfaChanged => "mfa_changed",
            SecurityEvent::SessionRevoked => "session_revoked",
            SecurityEvent::RecoveryCodeUsed => "recovery_code_used",
        }
    }

    fn title(&self) -> &'static str {
        match self {
            SecurityEvent::NewDevice => "新设备登录",
            SecurityEvent::PasswordChanged => "密码已更改",
            SecurityEvent::MfaChanged => "MFA 设置已更改",
            SecurityEvent::SessionRevoked => "会话已撤销",
            SecurityEvent::RecoveryCodeUsed => "恢复码已使用",
        }
    }

    fn body(&self, detail: Option<&str>) -> String {
        let base = match self {
            SecurityEvent::NewDevice => {
                "您的账号从一台新设备登录。如非本人操作，请立即修改密码并检查登录会话。"
            }
            SecurityEvent::PasswordChanged => "您的账号密码已更改。如非本人操作，请立即重置密码。",
            SecurityEvent::MfaChanged => {
                "您的账号两步验证（MFA）设置已更改。如非本人操作，请立即处理。"
            }
            SecurityEvent::SessionRevoked => {
                "您的账号有登录会话被撤销。如非本人操作，请立即修改密码。"
            }
            SecurityEvent::RecoveryCodeUsed => {
                "您的账号使用了一个恢复码登录。如非本人操作，请立即修改密码并重新生成恢复码。"
            }
        };
        match detail.map(str::trim).filter(|d| !d.is_empty()) {
            Some(detail) => format!("{base}\n{detail}"),
            None => base.to_string(),
        }
    }
}

/// 在业务事务内创建一条安全通知（M02-MFA-08 核心）。
///
/// 同事务写：`notifications`（type='system' + security_kind）、审计
/// `auth.security_notification`、Outbox 事件 `auth.security_notification.v1`。
/// 返回通知记录 id。
pub async fn create_security_notification_in_tx<'e>(
    tx: &mut OutboxTx<'e>,
    user_id: &str,
    event: SecurityEvent,
    request_id: &str,
    detail: Option<&str>,
) -> Result<String, sqlx::Error> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let title = event.title();
    let body = event.body(detail);

    match tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO notifications (id, user_id, type, security_kind, title, body, created_at)
                 VALUES (?, ?, 'system', ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(event.kind())
            .bind(title)
            .bind(&body)
            .bind(now)
            .execute(&mut **t)
            .await?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO notifications (id, user_id, type, security_kind, title, body, created_at)
                 VALUES (?, ?, 'system', ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(event.kind())
            .bind(title)
            .bind(&body)
            .bind(now)
            .execute(&mut **t)
            .await?;
        }
    }

    AuditEntry::user_action(user_id, "auth.security_notification")
        .with_target("user", user_id)
        .with_request_id(request_id)
        .with_metadata(json!({ "kind": event.kind() }))
        .record_in_tx(tx)
        .await?;

    enqueue_in_tx(
        tx,
        events::types::AUTH_SECURITY_NOTIFICATION,
        json!({
            "user_id": user_id,
            "kind": event.kind(),
            "created_at": now,
        }),
    )
    .await?;

    Ok(id)
}

/// 独立事务包装：通知密码已更改（触发点：密码重置确认）。
pub async fn notify_password_changed(
    pool: &DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<String, sqlx::Error> {
    let mut tx = begin_tx(pool).await?;
    let id = create_security_notification_in_tx(
        &mut tx,
        user_id,
        SecurityEvent::PasswordChanged,
        request_id,
        None,
    )
    .await?;
    commit_tx(tx).await?;
    Ok(id)
}

/// 独立事务包装：通知 MFA 设置变化（触发点：TOTP 启用/取消）。
pub async fn notify_mfa_changed(
    pool: &DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<String, sqlx::Error> {
    let mut tx = begin_tx(pool).await?;
    let id = create_security_notification_in_tx(
        &mut tx,
        user_id,
        SecurityEvent::MfaChanged,
        request_id,
        None,
    )
    .await?;
    commit_tx(tx).await?;
    Ok(id)
}

/// 独立事务包装：通知会话被撤销（触发点：逐设备撤销）。
pub async fn notify_session_revoked(
    pool: &DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<String, sqlx::Error> {
    let mut tx = begin_tx(pool).await?;
    let id = create_security_notification_in_tx(
        &mut tx,
        user_id,
        SecurityEvent::SessionRevoked,
        request_id,
        None,
    )
    .await?;
    commit_tx(tx).await?;
    Ok(id)
}

/// 独立事务包装：通知恢复码被使用（触发点：恢复码消费）。
pub async fn notify_recovery_code_used(
    pool: &DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<String, sqlx::Error> {
    let mut tx = begin_tx(pool).await?;
    let id = create_security_notification_in_tx(
        &mut tx,
        user_id,
        SecurityEvent::RecoveryCodeUsed,
        request_id,
        None,
    )
    .await?;
    commit_tx(tx).await?;
    Ok(id)
}

/// 独立事务包装：通知新设备登录（触发点：登录成功且设备首次见到）。
pub async fn notify_new_device(
    pool: &DatabasePool,
    user_id: &str,
    ua: &str,
    request_id: &str,
) -> Result<String, sqlx::Error> {
    let mut tx = begin_tx(pool).await?;
    let id = create_security_notification_in_tx(
        &mut tx,
        user_id,
        SecurityEvent::NewDevice,
        request_id,
        Some(ua),
    )
    .await?;
    commit_tx(tx).await?;
    Ok(id)
}

/// 新设备登录通知（M02-MFA-08）：仅当该用户没有其它非撤销会话使用相同
/// 设备 UA 时发送（首次见到的设备）。返回是否已发送。
///
/// 无 UA / 空 UA → 无法判定设备，不通知。调用方应在 `create_session`
/// 之前先调用 [`has_device_seen`] 做“是否新设备”判定，避免新会话自身
/// 计入“已见设备”。
pub async fn notify_new_device_if_first_seen(
    pool: &DatabasePool,
    user_id: &str,
    ua: Option<&str>,
    request_id: &str,
) -> Result<bool, sqlx::Error> {
    let Some(ua) = ua.map(str::trim).filter(|u| !u.is_empty()) else {
        return Ok(false);
    };
    if has_device_seen(pool, user_id, ua).await? {
        return Ok(false);
    }
    notify_new_device(pool, user_id, ua, request_id).await?;
    Ok(true)
}

/// 该用户是否已有非撤销会话使用相同设备 UA（“已见设备”判定）。
pub async fn has_device_seen(
    pool: &DatabasePool,
    user_id: &str,
    ua: &str,
) -> Result<bool, sqlx::Error> {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_sessions
             WHERE user_id = ? AND revoked_at IS NULL AND user_agent = ?",
        )
        .bind(user_id)
        .bind(ua)
        .fetch_one(p)
        .await
        .map(|c| c > 0),
        Either::Right(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM user_sessions
             WHERE user_id = ? AND revoked_at IS NULL AND user_agent = ?",
        )
        .bind(user_id)
        .bind(ua)
        .fetch_one(p)
        .await
        .map(|c| c > 0),
    }
}

async fn begin_tx(pool: &DatabasePool) -> Result<OutboxTx<'_>, sqlx::Error> {
    match pool {
        Either::Left(p) => Ok(Either::Left(p.begin().await?)),
        Either::Right(p) => Ok(Either::Right(p.begin().await?)),
    }
}

async fn commit_tx(tx: OutboxTx<'_>) -> Result<(), sqlx::Error> {
    match tx {
        Either::Left(t) => t.commit().await,
        Either::Right(t) => t.commit().await,
    }
}
