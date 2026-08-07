//! OAuth 用户同意（M11-CONSENT-01）：逐 Client × 逐 Scope consent、
//! 重新同意、撤销 + 安全通知。
//!
//! - `oauth_consents` 按 `(user_id, client_id, scope)` 唯一；撤销保留记录
//!   （`revoked_at`/`revoke_reason`），重新授权时复用同一行重新激活；
//! - 首次授权或新增 scope 写安全审计并通知用户（docs/AUTH-OIDC.md §15）；
//! - 撤销 consent 时同步撤销该 Client 的 Refresh Token family。

use serde_json::{json, Value};
use sqlx::Either;

use super::OidcError;

/// 已授权 scope 是否全部仍处于有效同意状态。
pub async fn consents_are_active(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    client_id: &str,
    scopes: &[String],
    now: i64,
) -> Result<bool, OidcError> {
    if scopes.is_empty() {
        return Ok(false);
    }
    let mut active = 0i64;
    for scope in scopes {
        let row: Option<(Option<i64>,)> = match pool {
            Either::Left(p) => {
                sqlx::query_as(
                    "SELECT revoked_at FROM oauth_consents WHERE user_id = ? AND client_id = ? AND scope = ?",
                )
                .bind(user_id)
                .bind(client_id)
                .bind(scope)
                .fetch_optional(p)
                .await?
            }
            Either::Right(p) => {
                sqlx::query_as(
                    "SELECT revoked_at FROM oauth_consents WHERE user_id = ? AND client_id = ? AND scope = ?",
                )
                .bind(user_id)
                .bind(client_id)
                .bind(scope)
                .fetch_optional(p)
                .await?
            }
        };
        match row {
            Some((None,)) => active += 1,
            Some((Some(revoked),)) if revoked > now => active += 1,
            _ => {}
        }
    }
    Ok(active as usize == scopes.len())
}

/// 当前有效同意的 scope 列表。
pub async fn active_consent_scopes(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    client_id: &str,
) -> Result<Vec<String>, OidcError> {
    let rows: Vec<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT scope FROM oauth_consents WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL ORDER BY scope",
            )
            .bind(user_id)
            .bind(client_id)
            .fetch_all(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT scope FROM oauth_consents WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL ORDER BY scope",
            )
            .bind(user_id)
            .bind(client_id)
            .fetch_all(p)
            .await?
        }
    };
    Ok(rows)
}

/// 授权（重新）同意：逐 scope 幂等激活，返回本次实际新增的 scope 列表。
pub async fn grant_consents(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    client_id: &str,
    scopes: &[String],
    now: i64,
) -> Result<Vec<String>, OidcError> {
    let active = active_consent_scopes(pool, user_id, client_id).await?;
    let newly_granted: Vec<String> = scopes
        .iter()
        .filter(|s| !active.contains(*s))
        .cloned()
        .collect();

    for scope in scopes {
        match pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO oauth_consents (id, user_id, client_id, scope, granted_at, revoked_at, revoke_reason)
                     VALUES (?, ?, ?, ?, ?, NULL, NULL)
                     ON CONFLICT(user_id, client_id, scope)
                     DO UPDATE SET granted_at = ?, revoked_at = NULL, revoke_reason = NULL",
                )
                .bind(uuid::Uuid::now_v7().to_string())
                .bind(user_id)
                .bind(client_id)
                .bind(scope)
                .bind(now)
                .bind(now)
                .execute(p)
                .await?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT INTO oauth_consents (id, user_id, client_id, scope, granted_at, revoked_at, revoke_reason)
                     VALUES (?, ?, ?, ?, ?, NULL, NULL)
                     ON DUPLICATE KEY UPDATE granted_at = VALUES(granted_at), revoked_at = NULL, revoke_reason = NULL",
                )
                .bind(uuid::Uuid::now_v7().to_string())
                .bind(user_id)
                .bind(client_id)
                .bind(scope)
                .bind(now)
                .bind(now)
                .execute(p)
                .await?;
            }
        }
    }
    Ok(newly_granted)
}

/// 撤销用户对某 Client 的全部 consent，并同步撤销该 Client 的 Refresh
/// Token family；写安全通知与审计。
pub async fn revoke_consents_for_client(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    client_internal_id: &str,
    reason: &str,
    request_id: &str,
    now: i64,
) -> Result<(), OidcError> {
    let affected = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE oauth_consents SET revoked_at = ?, revoke_reason = ?
                 WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(user_id)
        .bind(client_internal_id)
        .execute(p)
        .await?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE oauth_consents SET revoked_at = ?, revoke_reason = ?
                 WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL",
        )
        .bind(now)
        .bind(reason)
        .bind(user_id)
        .bind(client_internal_id)
        .execute(p)
        .await?
        .rows_affected(),
    };

    // 撤销该 Client 的 Refresh Token family（含未过期 Access Token）。
    super::tokens::revoke_families_for_user_client(
        pool,
        user_id,
        client_internal_id,
        "consent_revoked",
        now,
    )
    .await?;

    if affected > 0 {
        notify_oauth_security(
            pool,
            user_id,
            "oauth_consent_revoked",
            "oauth_client",
            client_internal_id,
        )
        .await?;
    }
    crate::audit::AuditEntry::user_action(user_id, "oauth.consent.revoke")
        .with_target("oauth_client", client_internal_id)
        .with_reason(reason)
        .with_request_id(request_id)
        .with_metadata(json!({ "consents_revoked": affected }))
        .record(pool)
        .await
        .map_err(|e| OidcError::Db(e.to_string()))?;
    Ok(())
}

/// OAuth 安全通知（security 类别，模板 `security.notice`）。
pub async fn notify_oauth_security(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    kind: &str,
    resource_type: &str,
    resource_id: &str,
) -> Result<(), OidcError> {
    use crate::notifications::model::NotificationCategory;
    use crate::notifications::service::{create_notification, CreateNotificationInput};
    use crate::notifications::templates::TemplateKey;
    use crate::outbox::now_millis;

    let mut params = serde_json::Map::new();
    params.insert("kind".to_string(), Value::String(kind.to_string()));
    create_notification(
        pool,
        CreateNotificationInput {
            user_id: user_id.to_string(),
            category: NotificationCategory::Security,
            template_key: TemplateKey::SecurityNotice,
            r#type: Some("system".to_string()),
            resource_type: Some(resource_type.to_string()),
            resource_id: Some(resource_id.to_string()),
            params,
        },
        now_millis(),
    )
    .await
    .map_err(|e| OidcError::Db(e.to_string()))?;
    Ok(())
}
