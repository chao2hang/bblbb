//! MFA 管理路由（M02-UX-06）：TOTP enrollment、恢复码一次展示、停用 MFA
//! 与 re-auth step-up。
//!
//! - `POST /api/v1/auth/mfa/enroll`：开始 enrollment，返回 otpauth URI +
//!   base32 secret（二维码所需最小数据，M02-MFA-02）；
//! - `POST /api/v1/auth/mfa/confirm`：`{ code }` 校验后原子启用；
//! - `DELETE /api/v1/auth/mfa/enrollment`：取消未完成 enrollment；
//! - `POST /api/v1/auth/mfa/recovery-codes`：生成新一组恢复码（只展示一次），
//!   高风险操作——要求近期认证（M02-MFA-07 step-up）；
//! - `DELETE /api/v1/auth/mfa`：停用已确认 TOTP（同时失效全部恢复码），
//!   同样要求 step-up；
//! - `POST /api/v1/auth/re-auth`：`{ password }` 重认证当前会话（step-up
//!   交互入口）。
//!
//! 全部走会话绑定 synchronizer CSRF（M02-SESSION-07，OpenAPI x-csrf: true）。
//! 统一错误映射：enrollment/code/恢复码错误不区分细节（防枚举），DB 错误 500。

use axum::{
    extract::{Json, State},
    response::Json as JsonResponse,
    routing::{delete, post},
    Router,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    app::AppState,
    auth::{
        begin_enrollment, cancel_enrollment, confirm_enrollment, disable_totp,
        generate_recovery_codes, is_step_up_required_for_session, mark_step_up,
        session::SESSION_COOKIE_NAME, verify_password, AuthSession, MfaError, VerifyResult,
        RECOVERY_CODE_COUNT,
    },
    db::pool::DatabasePool,
    error::AppError,
    outbox::now_millis,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/mfa/enroll", post(mfa_enroll))
        .route("/api/v1/auth/mfa/confirm", post(mfa_confirm))
        .route("/api/v1/auth/mfa/enrollment", delete(mfa_cancel))
        .route("/api/v1/auth/mfa/recovery-codes", post(mfa_recovery_codes))
        .route("/api/v1/auth/mfa", delete(mfa_disable))
        .route("/api/v1/auth/re-auth", post(re_auth))
}

/// POST /api/v1/auth/mfa/enroll — 开始 TOTP enrollment
async fn mfa_enroll(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<JsonResponse<TotpEnrollResponse>, AppError> {
    let request_id = "mfa-enroll";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    if state.config.mfa_encryption_key.is_empty() {
        return Err(AppError::internal(
            "MFA encryption key not configured",
            request_id,
        ));
    }

    let challenge = begin_enrollment(
        pool,
        &user.id,
        "BBLBB",
        &user.email,
        state.config.mfa_encryption_key.as_bytes(),
    )
    .await
    .map_err(mfa_db_error(request_id))?;

    Ok(JsonResponse(TotpEnrollResponse {
        otpauth_uri: challenge.otpauth_uri,
        secret_base32: challenge.secret_base32,
        issuer: challenge.issuer,
        account: challenge.account,
    }))
}

/// POST /api/v1/auth/mfa/confirm — 确认 enrollment（校验 6 位 code 后启用）
async fn mfa_confirm(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<MfaConfirmRequest>,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "mfa-confirm";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    if state.config.mfa_encryption_key.is_empty() {
        return Err(AppError::internal(
            "MFA encryption key not configured",
            request_id,
        ));
    }
    let code = req.code.trim();
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) {
        return Err(AppError::bad_request(
            "invalid TOTP code",
            request_id,
            Some(json!({ "field": "code" })),
        ));
    }

    confirm_enrollment(
        pool,
        &user.id,
        code,
        state.config.mfa_encryption_key.as_bytes(),
        (now_millis() / 1000) as u64,
    )
    .await
    .map_err(mfa_confirm_error(request_id))?;

    Ok(JsonResponse(json!({ "ok": true })))
}

/// DELETE /api/v1/auth/mfa/enrollment — 取消未完成 enrollment
async fn mfa_cancel(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "mfa-cancel";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let cancelled = cancel_enrollment(pool, &user.id)
        .await
        .map_err(mfa_db_error(request_id))?;
    if !cancelled {
        return Err(AppError::not_found("no pending enrollment", request_id));
    }
    Ok(JsonResponse(json!({ "ok": true })))
}

/// 高风险 MFA 操作前置：会话必须处于近期认证窗口（M02-MFA-07）。
/// 返回会话 token（供成功后刷新 step-up 窗口）；要求时返回 403
/// `step_up_required`（前端据此展示 re-auth 表单）。
async fn require_step_up(
    state: &AppState,
    jar: &CookieJar,
    request_id: &str,
) -> Result<Option<String>, AppError> {
    let Some(pool) = state.db.as_deref() else {
        return Err(AppError::internal("database not configured", request_id));
    };
    let Some(token) = jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string()) else {
        return Err(AppError::unauthorized(
            "authentication required",
            request_id,
        ));
    };
    let required = is_step_up_required_for_session(pool, &token, state.config.step_up_window_secs)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if required {
        return Err(AppError::step_up_required(request_id));
    }
    Ok(Some(token))
}

/// POST /api/v1/auth/mfa/recovery-codes — 生成新一组恢复码（只展示一次）
async fn mfa_recovery_codes(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "mfa-recovery-codes";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // Secret 操作：要求近期认证（M02-MFA-07）
    let token = require_step_up(&state, &jar, request_id).await?;

    let codes = generate_recovery_codes(pool, &user.id, RECOVERY_CODE_COUNT, request_id)
        .await
        .map_err(mfa_db_error(request_id))?;

    // 本次已重认证，刷新 step-up 窗口
    if let Some(token) = token {
        let _ = mark_step_up(pool, &token).await;
    }

    Ok(JsonResponse(
        json!({ "codes": codes, "only_shown_once": true }),
    ))
}

/// DELETE /api/v1/auth/mfa — 停用已确认 TOTP（含失效全部恢复码）
async fn mfa_disable(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "mfa-disable";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // 停用 MFA 为高风险操作：要求近期认证（M02-MFA-07）
    require_step_up(&state, &jar, request_id).await?;

    let disabled = disable_totp(pool, &user.id, request_id)
        .await
        .map_err(mfa_db_error(request_id))?;
    if !disabled {
        return Err(AppError::not_found("TOTP is not enabled", request_id));
    }
    Ok(JsonResponse(json!({ "ok": true })))
}

/// POST /api/v1/auth/re-auth — 重认证当前会话（step-up 交互入口）
///
/// 校验当前用户密码（错误统一 401，防枚举），成功后刷新会话
/// `auth_verified_at`（M02-MFA-07 mark_step_up），后续高风险操作 5 分钟
/// 内不再要求 step-up。
async fn re_auth(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    Json(req): Json<ReAuthRequest>,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "re-auth";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    if req.password.is_empty() {
        return Err(AppError::bad_request(
            "password is required",
            request_id,
            Some(json!({ "field": "password" })),
        ));
    }

    // 加载当前用户密码 hash 并验证
    let hash: Option<String> = match pool {
        DatabasePool::Left(p) => sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        DatabasePool::Right(p) => {
            sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
                .bind(&user.id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };
    let Some(hash) = hash else {
        return Err(AppError::unauthorized(
            "re-authentication failed",
            request_id,
        ));
    };
    if verify_password(&req.password, &hash) != VerifyResult::Ok {
        return Err(AppError::unauthorized(
            "re-authentication failed",
            request_id,
        ));
    }

    // 刷新当前会话 step-up 窗口
    let Some(token) = jar.get(SESSION_COOKIE_NAME).map(|c| c.value().to_string()) else {
        return Err(AppError::unauthorized(
            "authentication required",
            request_id,
        ));
    };
    mark_step_up(pool, &token)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(JsonResponse(json!({ "ok": true })))
}

/// confirm_enrollment 错误 → HTTP 映射（enrollment/code 细节统一，防枚举）。
fn mfa_confirm_error(request_id: &str) -> impl FnOnce(MfaError) -> AppError + '_ {
    move |e| match e {
        MfaError::NoPendingEnrollment | MfaError::AlreadyConfirmed | MfaError::InvalidCode => {
            AppError::bad_request("invalid or missing enrollment", request_id, None)
        }
        MfaError::Encryption => AppError::internal("MFA secret decryption failed", request_id),
        MfaError::Database(msg) => AppError::internal(msg, request_id),
        MfaError::TotpNotEnabled => {
            AppError::bad_request("invalid or missing enrollment", request_id, None)
        }
    }
}

/// 其余 MFA 服务错误 → HTTP 映射（DB 500，其余统一 400 不泄漏细节）。
fn mfa_db_error(request_id: &str) -> impl FnOnce(MfaError) -> AppError + '_ {
    move |e| match e {
        MfaError::Database(msg) => AppError::internal(msg, request_id),
        _ => AppError::bad_request("MFA operation failed", request_id, None),
    }
}

/// POST /api/v1/auth/mfa/enroll 响应（二维码所需最小数据）。
#[derive(serde::Serialize)]
struct TotpEnrollResponse {
    otpauth_uri: String,
    secret_base32: String,
    issuer: String,
    account: String,
}

/// POST /api/v1/auth/mfa/confirm 请求体。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MfaConfirmRequest {
    code: String,
}

/// POST /api/v1/auth/re-auth 请求体。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReAuthRequest {
    password: String,
}
