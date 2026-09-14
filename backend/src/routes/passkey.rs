//! Passkey（WebAuthn/FIDO2）路由（M02-MFA-PK）：凭据管理与登录断言 options。
//!
//! - `GET /api/v1/auth/passkeys`：列出本人 Passkey（无敏感字段）；
//! - `POST /api/v1/auth/passkeys`：开始注册，返回 WebAuthn creation options；
//! - `POST /api/v1/auth/passkeys/confirm`：确认注册（浏览器凭据响应）；
//! - `DELETE /api/v1/auth/passkeys/{id}`：撤销本人 Passkey，
//!   高风险操作——要求近期认证（M02-MFA-07 step-up，与停用 TOTP 同级）；
//! - `POST /api/v1/auth/login/mfa/passkey/options`：两步登录第二步的 Passkey
//!   request options（预认证上下文，用一次性 MFA challenge 换取）。
//!
//! 全部走 CSRF 中间件（M02-SESSION-07/08，OpenAPI x-csrf: true）。注册/撤销
//! 成功发安全通知（`mfa_changed`，与 TOTP 启用/取消对齐）。错误统一稳定码
//! 不泄漏细节（防枚举）。

use axum::{
    extract::{Json, Path, State},
    response::Json as JsonResponse,
    routing::{delete, get, post},
    Router,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::{
    app::AppState,
    auth::{
        begin_passkey_login, begin_passkey_registration, build_webauthn,
        confirm_passkey_registration, is_step_up_required_for_session, list_passkeys, mark_step_up,
        notify_mfa_changed, revoke_passkey, AuthSession, PasskeyError, PasskeyInfo,
    },
    db::pool::DatabasePool,
    error::AppError,
};

pub fn router() -> Router<AppState> {
    Router::new()
        .route(
            "/api/v1/auth/passkeys",
            get(passkey_list).post(passkey_begin),
        )
        .route("/api/v1/auth/passkeys/confirm", post(passkey_confirm))
        .route("/api/v1/auth/passkeys/{id}", delete(passkey_revoke))
        .route(
            "/api/v1/auth/login/mfa/passkey/options",
            post(login_mfa_passkey_options),
        )
}

// ─────────────────────────── 管理（会话上下文） ───────────────────────────

/// GET /api/v1/auth/passkeys — 列出本人 Passkey
async fn passkey_list(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "passkey-list";
    let user = auth.require_auth(request_id)?;
    let pool = require_pool(&state, request_id)?;

    let items = list_passkeys(pool, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let body = PasskeyListResponse { passkeys: items };
    Ok(JsonResponse(serde_json::to_value(body).map_err(|e| {
        AppError::internal(e.to_string(), request_id)
    })?))
}

/// POST /api/v1/auth/passkeys — 开始注册（返回 creation options）
async fn passkey_begin(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "passkey-begin";
    let user = auth.require_auth(request_id)?;
    let pool = require_pool(&state, request_id)?;
    let webauthn = require_webauthn(&state, request_id)?;
    let display_name = user
        .display_name
        .as_deref()
        .unwrap_or(user.username.as_str());

    let ccr = begin_passkey_registration(pool, &webauthn, &user.id, &user.username, display_name)
        .await
        .map_err(|e| passkey_error(e, request_id))?;
    Ok(JsonResponse(serde_json::to_value(ccr).map_err(|e| {
        AppError::internal(e.to_string(), request_id)
    })?))
}

/// POST /api/v1/auth/passkeys/confirm — 确认注册
async fn passkey_confirm(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<PasskeyConfirmRequest>,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "passkey-confirm";
    let user = auth.require_auth(request_id)?;
    let pool = require_pool(&state, request_id)?;
    let webauthn = require_webauthn(&state, request_id)?;

    let info = confirm_passkey_registration(
        pool,
        &webauthn,
        &user.id,
        req.name.as_deref(),
        &req.credential,
    )
    .await
    .map_err(|e| passkey_error(e, request_id))?;

    // 安全通知尽力而为：失败不阻断（记 warn）
    if let Err(e) = notify_mfa_changed(pool, &user.id, request_id).await {
        tracing::warn!(user_id = %user.id, error = %e, "passkey security notification failed (registered)");
    }

    Ok(JsonResponse(json!({ "passkey": info })))
}

/// DELETE /api/v1/auth/passkeys/{id} — 撤销（step-up）
async fn passkey_revoke(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "passkey-revoke";
    let user = auth.require_auth(request_id)?;
    let pool = require_pool(&state, request_id)?;

    // 撤销第二因素为高风险操作：要求近期认证（M02-MFA-07）
    let token = require_step_up(&state, &jar, request_id).await?;

    let revoked = revoke_passkey(pool, &user.id, id.trim())
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if !revoked {
        return Err(AppError::not_found("passkey not found", request_id));
    }

    if let Err(e) = notify_mfa_changed(pool, &user.id, request_id).await {
        tracing::warn!(user_id = %user.id, error = %e, "passkey security notification failed (revoked)");
    }
    // 本次已重认证，刷新 step-up 窗口
    if let Some(token) = token {
        let _ = mark_step_up(pool, &token).await;
    }

    Ok(JsonResponse(json!({ "ok": true })))
}

// ─────────────────────────── 登录第二步（预认证上下文） ───────────────────────────

/// POST /api/v1/auth/login/mfa/passkey/options — 登录第二步 request options
///
/// 用第一步 /login 返回的一次性 MFA challenge 换取 WebAuthn request options。
/// challenge 无效/过期 → 422 `mfa_challenge_invalid`（与 /login/mfa 统一）；
/// 账号无有效 Passkey → 400 `passkey_unavailable`（调用方已通过密码步，
/// 泄漏面仅限本人账号状态）。
async fn login_mfa_passkey_options(
    State(state): State<AppState>,
    Json(req): Json<PasskeyLoginOptionsRequest>,
) -> Result<JsonResponse<Value>, AppError> {
    let request_id = "login-mfa-passkey-options";
    let pool = require_pool(&state, request_id)?;
    let webauthn = require_webauthn(&state, request_id)?;
    let token = req.challenge_token.trim();
    if token.is_empty() {
        return Err(AppError::with_code(
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            "mfa_challenge_invalid",
            "Unprocessable Entity",
            "invalid or expired MFA challenge",
            request_id,
        ));
    }

    let rcr = begin_passkey_login(pool, &webauthn, token)
        .await
        .map_err(|e| passkey_error(e, request_id))?;
    Ok(JsonResponse(serde_json::to_value(rcr).map_err(|e| {
        AppError::internal(e.to_string(), request_id)
    })?))
}

// ─────────────────────────── 请求/响应 DTO ───────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasskeyConfirmRequest {
    /// 用户可读标签（缺省「Passkey」）
    #[serde(default)]
    name: Option<String>,
    /// 浏览器 navigator.credentials.create() 的原始 JSON
    credential: Value,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PasskeyLoginOptionsRequest {
    challenge_token: String,
}

#[derive(serde::Serialize)]
struct PasskeyListResponse {
    passkeys: Vec<PasskeyInfo>,
}

// ─────────────────────────── 错误映射 / 公共助手 ───────────────────────────

fn require_pool<'a>(state: &'a AppState, request_id: &str) -> Result<&'a DatabasePool, AppError> {
    state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))
}

/// Passkey 未配置（rp_id 为空）→ 500 `passkey_not_configured`（运维配置问题，
/// 非用户错误；前端在 passkey_available=false 时不展示入口）。
fn require_webauthn(state: &AppState, request_id: &str) -> Result<webauthn_rs::Webauthn, AppError> {
    build_webauthn(&state.config).ok_or_else(|| {
        AppError::with_code(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "passkey_not_configured",
            "Internal Server Error",
            "passkey is not configured on this server",
            request_id,
        )
    })
}

/// PasskeyError → HTTP（统一稳定码，防枚举）：
/// - NotConfigured → 500 `passkey_not_configured`；
/// - NoPendingChallenge → 注册确认 400 `passkey_challenge_invalid` /
///   登录 options 422 `mfa_challenge_invalid`（由调用方区分）；
/// - NoCredentials → 400 `passkey_unavailable`；
/// - InvalidCredential → 400 `passkey_registration_invalid`（注册）/登录统一
///   `mfa_code_invalid`（与 TOTP/恢复码同语义，见 login_mfa）；
/// - LimitReached → 400 `passkey_limit_reached`；
/// - NotFound → 404；Database/Serialization → 500。
fn passkey_error(e: PasskeyError, request_id: &str) -> AppError {
    tracing::debug!(error = %e, "passkey error");
    match e {
        PasskeyError::NotConfigured => AppError::with_code(
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "passkey_not_configured",
            "Internal Server Error",
            "passkey is not configured on this server",
            request_id,
        ),
        PasskeyError::NoCredentials => AppError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "passkey_unavailable",
            "Bad Request",
            "no passkey registered for this account",
            request_id,
        ),
        PasskeyError::NoPendingChallenge => AppError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "passkey_challenge_invalid",
            "Bad Request",
            "invalid or expired passkey challenge",
            request_id,
        ),
        PasskeyError::InvalidCredential => AppError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "passkey_registration_invalid",
            "Bad Request",
            "invalid passkey registration response",
            request_id,
        ),
        PasskeyError::LimitReached => AppError::with_code(
            axum::http::StatusCode::BAD_REQUEST,
            "passkey_limit_reached",
            "Bad Request",
            "passkey limit reached",
            request_id,
        ),
        PasskeyError::NotFound => AppError::not_found("passkey not found", request_id),
        PasskeyError::Database(msg) => AppError::internal(msg, request_id),
        PasskeyError::Serialization(msg) => AppError::internal(msg, request_id),
    }
}

/// 高风险 Passkey 操作前置：会话必须处于近期认证窗口（M02-MFA-07）。
/// 返回会话 token（供成功后刷新 step-up 窗口）；要求时返回 403
/// `step_up_required`。与 routes/mfa.rs 的同名助手语义一致。
async fn require_step_up(
    state: &AppState,
    jar: &CookieJar,
    request_id: &str,
) -> Result<Option<String>, AppError> {
    let pool = require_pool(state, request_id)?;
    let Some(token) = jar.get(crate::auth::session::SESSION_COOKIE_NAME) else {
        return Err(AppError::unauthorized(
            "authentication required",
            request_id,
        ));
    };
    let token = token.value().to_string();
    let required = is_step_up_required_for_session(pool, &token, state.config.step_up_window_secs)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if required {
        return Err(AppError::step_up_required(request_id));
    }
    Ok(Some(token))
}
