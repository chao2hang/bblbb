use axum::{
    extract::{Path, State},
    response::Json,
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::error::AppError;
use crate::outbox::now_millis;

/// M06-DOWNLOAD 管理路由（admin_download 域 agent 填充）。
pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/admin/attachments/{id}/download-policy",
            get(get_admin_download_policy).patch(update_admin_download_policy),
        )
        .route(
            "/api/v1/admin/download-billing/config",
            get(get_billing_config).patch(update_billing_config),
        )
}

async fn admin_authorize(
    state: &AppState,
    auth: &AuthSession,
    request_id: &str,
) -> Result<(), AppError> {
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let decision = authorize_action(
        pool,
        &user.id,
        "download_billing.manage",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "download_billing.manage required",
            request_id,
        ));
    }
    Ok(())
}

async fn get_admin_download_policy(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_download_policy";
    admin_authorize(&state, &auth, request_id).await?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    match pool {
        sqlx::Either::Left(p) => {
            let row = sqlx::query(
                "SELECT id, mode, currency_id, amount, authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, grace_on_disable, version, is_enabled
                 FROM download_billing_policies WHERE scope_type = 'attachment' AND scope_id = ? ORDER BY version DESC LIMIT 1",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(row.map(policy_json).unwrap_or_else(
                || json!({ "attachment_id": id, "configured": false }),
            )))
        }
        sqlx::Either::Right(p) => {
            let row = sqlx::query(
                "SELECT id, mode, currency_id, amount, authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, grace_on_disable, version, is_enabled
                 FROM download_billing_policies WHERE scope_type = 'attachment' AND scope_id = ? ORDER BY version DESC LIMIT 1",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(row.map(policy_json_mysql).unwrap_or_else(
                || json!({ "attachment_id": id, "configured": false }),
            )))
        }
    }
}

fn policy_json(row: sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": row.get::<String,_>("id"),
        "mode": row.get::<String,_>("mode"),
        "currency_id": row.get::<Option<String>,_>("currency_id"),
        "amount": row.get::<i64,_>("amount"),
        "authorization_ttl_seconds": row.get::<i64,_>("authorization_ttl_seconds"),
        "daily_user_limit": row.get::<Option<i64>,_>("daily_user_limit"),
        "single_charge_limit": row.get::<Option<i64>,_>("single_charge_limit"),
        "attachment_revenue_limit": row.get::<Option<i64>,_>("attachment_revenue_limit"),
        "grace_on_disable": row.get::<i64,_>("grace_on_disable") != 0,
        "version": row.get::<i64,_>("version"),
        "is_enabled": row.get::<i64,_>("is_enabled") != 0,
    })
}

fn policy_json_mysql(row: sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": row.get::<String,_>("id"),
        "mode": row.get::<String,_>("mode"),
        "currency_id": row.get::<Option<String>,_>("currency_id"),
        "amount": row.get::<i64,_>("amount"),
        "authorization_ttl_seconds": row.get::<i64,_>("authorization_ttl_seconds"),
        "daily_user_limit": row.get::<Option<i64>,_>("daily_user_limit"),
        "single_charge_limit": row.get::<Option<i64>,_>("single_charge_limit"),
        "attachment_revenue_limit": row.get::<Option<i64>,_>("attachment_revenue_limit"),
        "grace_on_disable": row.get::<i64,_>("grace_on_disable") != 0,
        "version": row.get::<i64,_>("version"),
        "is_enabled": row.get::<i64,_>("is_enabled") != 0,
    })
}

#[derive(Deserialize)]
struct PolicyBody {
    mode: Option<String>,
    currency_id: Option<String>,
    amount: Option<i64>,
    authorization_ttl_seconds: Option<i64>,
    daily_user_limit: Option<i64>,
    single_charge_limit: Option<i64>,
    attachment_revenue_limit: Option<i64>,
    grace_on_disable: Option<bool>,
    is_enabled: Option<bool>,
    reason: Option<String>,
}

async fn update_admin_download_policy(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<PolicyBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_admin_download_policy";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, request_id).await?;
    let reason = body.reason.as_deref().unwrap_or("update download policy");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    if let Some(mode) = &body.mode {
        if ![
            "disabled",
            "free",
            "fixed",
            "inherit",
            "forced_free",
            "forced_paid",
        ]
        .contains(&mode.as_str())
        {
            return Err(AppError::bad_request("invalid mode", request_id, None));
        }
    }
    let now = now_millis();
    let policy_id = uuid::Uuid::now_v7().to_string();
    match pool {
        sqlx::Either::Left(p) => {
            // 新版本插入（不改旧行）：scope=attachment, scope_id=id, version=现有最大+1
            let max_version: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(version),0) FROM download_billing_policies WHERE scope_type='attachment' AND scope_id=?",
            )
            .bind(&id)
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let currency_id = body
                .currency_id
                .as_deref()
                .unwrap_or("01911fd5-0047-0000-0000-000000000002");
            sqlx::query(
                "INSERT INTO download_billing_policies
                     (id, scope_type, scope_id, mode, currency_id, amount, authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, grace_on_disable, version, is_enabled, created_at, updated_at)
                 VALUES (?, 'attachment', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&policy_id)
            .bind(&id)
            .bind(body.mode.as_deref().unwrap_or("free"))
            .bind(currency_id)
            .bind(body.amount.unwrap_or(0))
            .bind(body.authorization_ttl_seconds.unwrap_or(3600))
            .bind(body.daily_user_limit)
            .bind(body.single_charge_limit)
            .bind(body.attachment_revenue_limit)
            .bind(body.grace_on_disable.unwrap_or(true))
            .bind(max_version + 1)
            .bind(body.is_enabled.unwrap_or(true))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        sqlx::Either::Right(p) => {
            let max_version: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(version),0) FROM download_billing_policies WHERE scope_type='attachment' AND scope_id=?",
            )
            .bind(&id)
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let currency_id = body
                .currency_id
                .as_deref()
                .unwrap_or("01911fd5-0047-0000-0000-000000000002");
            sqlx::query(
                "INSERT INTO download_billing_policies
                     (id, scope_type, scope_id, mode, currency_id, amount, authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, grace_on_disable, version, is_enabled, created_at, updated_at)
                 VALUES (?, 'attachment', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&policy_id)
            .bind(&id)
            .bind(body.mode.as_deref().unwrap_or("free"))
            .bind(currency_id)
            .bind(body.amount.unwrap_or(0))
            .bind(body.authorization_ttl_seconds.unwrap_or(3600))
            .bind(body.daily_user_limit)
            .bind(body.single_charge_limit)
            .bind(body.attachment_revenue_limit)
            .bind(body.grace_on_disable.unwrap_or(true))
            .bind(max_version + 1)
            .bind(body.is_enabled.unwrap_or(true))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }
    AuditEntry::user_action(&user.id, "download.policy.update")
        .with_target("attachment", &id)
        .with_reason(reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(Json(
        json!({ "policy_id": policy_id, "attachment_id": id, "status": "updated" }),
    ))
}

async fn get_billing_config(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_billing_config";
    admin_authorize(&state, &auth, request_id).await?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    match pool {
        sqlx::Either::Left(p) => {
            let row = sqlx::query(
                "SELECT id, mode, currency_id, amount, authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, grace_on_disable, version, is_enabled
                 FROM download_billing_policies WHERE scope_type = 'site' AND scope_id IS NULL ORDER BY version DESC LIMIT 1",
            )
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(
                row.map(policy_json)
                    .unwrap_or_else(|| json!({ "configured": false })),
            ))
        }
        sqlx::Either::Right(p) => {
            let row = sqlx::query(
                "SELECT id, mode, currency_id, amount, authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, grace_on_disable, version, is_enabled
                 FROM download_billing_policies WHERE scope_type = 'site' AND scope_id IS NULL ORDER BY version DESC LIMIT 1",
            )
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            Ok(Json(
                row.map(policy_json_mysql)
                    .unwrap_or_else(|| json!({ "configured": false })),
            ))
        }
    }
}

async fn update_billing_config(
    State(state): State<AppState>,
    auth: AuthSession,
    axum::Json(body): axum::Json<PolicyBody>,
) -> Result<Json<Value>, AppError> {
    let request_id = "update_billing_config";
    let user = auth.require_auth(request_id)?;
    admin_authorize(&state, &auth, request_id).await?;
    let reason = body.reason.as_deref().unwrap_or("update billing config");
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let now = now_millis();
    let policy_id = uuid::Uuid::now_v7().to_string();
    match pool {
        sqlx::Either::Left(p) => {
            let max_version: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(version),0) FROM download_billing_policies WHERE scope_type='site' AND scope_id IS NULL",
            )
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let currency_id = body
                .currency_id
                .as_deref()
                .unwrap_or("01911fd5-0047-0000-0000-000000000002");
            sqlx::query(
                "INSERT INTO download_billing_policies
                     (id, scope_type, scope_id, mode, currency_id, amount, authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, grace_on_disable, version, is_enabled, created_at, updated_at)
                 VALUES (?, 'site', NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&policy_id)
            .bind(body.mode.as_deref().unwrap_or("free"))
            .bind(currency_id)
            .bind(body.amount.unwrap_or(0))
            .bind(body.authorization_ttl_seconds.unwrap_or(3600))
            .bind(body.daily_user_limit)
            .bind(body.single_charge_limit)
            .bind(body.attachment_revenue_limit)
            .bind(body.grace_on_disable.unwrap_or(true))
            .bind(max_version + 1)
            .bind(body.is_enabled.unwrap_or(true))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        sqlx::Either::Right(p) => {
            let max_version: i64 = sqlx::query_scalar(
                "SELECT COALESCE(MAX(version),0) FROM download_billing_policies WHERE scope_type='site' AND scope_id IS NULL",
            )
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            let currency_id = body
                .currency_id
                .as_deref()
                .unwrap_or("01911fd5-0047-0000-0000-000000000002");
            sqlx::query(
                "INSERT INTO download_billing_policies
                     (id, scope_type, scope_id, mode, currency_id, amount, authorization_ttl_seconds, daily_user_limit, single_charge_limit, attachment_revenue_limit, grace_on_disable, version, is_enabled, created_at, updated_at)
                 VALUES (?, 'site', NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&policy_id)
            .bind(body.mode.as_deref().unwrap_or("free"))
            .bind(currency_id)
            .bind(body.amount.unwrap_or(0))
            .bind(body.authorization_ttl_seconds.unwrap_or(3600))
            .bind(body.daily_user_limit)
            .bind(body.single_charge_limit)
            .bind(body.attachment_revenue_limit)
            .bind(body.grace_on_disable.unwrap_or(true))
            .bind(max_version + 1)
            .bind(body.is_enabled.unwrap_or(true))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }
    AuditEntry::user_action(&user.id, "download.billing_config.update")
        .with_reason(reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(Json(json!({ "policy_id": policy_id, "status": "updated" })))
}
