use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    app::AppState,
    auth::{
        login::{login_user, LoginError, LoginLimits},
        password::hash_password,
        password_reset::{
            confirm_password_reset as confirm_reset_service,
            request_password_reset as request_reset_service, ConfirmResetError,
            PasswordResetLimits, RequestResetError,
        },
        registration::{register_user, RegisterUserError},
        resend::{resend_verification_email, ResendError, ResendLimits},
        session::{
            build_clear_session_cookie, build_session_cookie,
            list_sessions as list_sessions_service, revoke_all_sessions as revoke_all_service,
            revoke_session as revoke_session_service, revoke_session_by_id as revoke_by_id_service,
            AuthSession, DeviceSession,
        },
        token::generate_token,
        verification::{verify_email_token, VerifyEmailError},
    },
    domain::registration::{validate_register, RegisterRequest},
    error::AppError,
    ratelimit::{
        client_ip, REGISTER_ACCOUNT_LIMIT, REGISTER_IP_LIMIT, REGISTER_WINDOW_MS, RESEND_IP_LIMIT,
        RESEND_IP_WINDOW_MS, RESET_IP_LIMIT, RESET_IP_WINDOW_MS,
    },
};

// ─── DTO ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    /// 用户名或邮箱（不区分大小写）
    pub identifier: String,
    pub password: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenRequest {
    pub token: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetRequest {
    pub email: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResendVerificationRequest {
    pub email: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PasswordResetConfirm {
    pub token: String,
    pub password: String,
}

#[derive(Serialize)]
struct GenericSuccess {
    ok: bool,
}

#[derive(Serialize)]
struct CsrfResponse {
    token: String,
}

#[derive(Serialize)]
struct MeResponse {
    id: String,
    username: String,
    email: String,
    email_verified: bool,
    status: String,
    display_name: Option<String>,
    level: i64,
    roles: Vec<String>,
}

// ─── 路由 ────────────────────────────────────────────────────────────────────

/// 认证路由：注册、登录、验证邮箱、密码恢复、CSRF、Session
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/auth/csrf", get(get_csrf_token))
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/verify-email", post(verify_email))
        .route(
            "/api/v1/auth/resend-verification",
            post(resend_verification),
        )
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/session", delete(logout))
        .route(
            "/api/v1/auth/sessions",
            get(list_sessions).delete(logout_all),
        )
        .route("/api/v1/auth/sessions/{id}", delete(revoke_session))
        .route("/api/v1/auth/password-reset", post(request_password_reset))
        .route(
            "/api/v1/auth/password-reset/confirm",
            post(confirm_password_reset),
        )
}

// ─── 处理器 ──────────────────────────────────────────────────────────────────

/// GET /api/v1/auth/csrf — 获取 CSRF token
async fn get_csrf_token(
    State(_state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<CsrfResponse>, AppError> {
    let _request_id = "csrf";

    if let Some(csrf) = &auth.csrf_token {
        return Ok(Json(CsrfResponse {
            token: csrf.clone(),
        }));
    }

    // 未认证用户生成一次性预认证 CSRF token
    let token = generate_token();
    Ok(Json(CsrfResponse { token }))
}

/// POST /api/v1/auth/register — 注册新用户
///
/// 领域校验（M02-IDENTITY-03）→ 事务创建（M02-IDENTITY-05）→
/// 双维度限流（M02-IDENTITY-06）：同一事务写入 pending 用户、一次性验证
/// token（hash）、审计与验证邮件 Outbox；任何失败整事务回滚。唯一约束冲突
/// 与成功返回相同响应（不泄漏用户名/邮箱是否已存在）；每 IP 与每账号
/// （规范化邮箱）分别按小时限流，超限返回 429 `rate_limited` + Retry-After。
async fn register(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "register";

    let registration = validate_register(&req)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;

    // 双维度限流（先校验，再消费额度；超限不执行任何昂贵操作）
    let now_ms = crate::outbox::now_millis();
    let ip = client_ip(&headers);
    let ip_status = state.limiter.check(
        &format!("register:ip:{ip}"),
        REGISTER_IP_LIMIT,
        REGISTER_WINDOW_MS,
        now_ms,
    );
    if !ip_status.allowed {
        return Err(AppError::rate_limited(
            "too many registration attempts, try again later",
            request_id,
            ip_status.retry_after_secs,
            ip_status.limit,
            ip_status.remaining,
            ip_status.reset_at_ms / 1000,
        ));
    }
    let account_status = state.limiter.check(
        &format!("register:account:{}", registration.email_normalized),
        REGISTER_ACCOUNT_LIMIT,
        REGISTER_WINDOW_MS,
        now_ms,
    );
    if !account_status.allowed {
        return Err(AppError::rate_limited(
            "too many registration attempts, try again later",
            request_id,
            account_status.retry_after_secs,
            account_status.limit,
            account_status.remaining,
            account_status.reset_at_ms / 1000,
        ));
    }

    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    match register_user(pool, &registration, request_id).await {
        Ok(_) => Ok((StatusCode::CREATED, Json(json!({ "ok": true })))),
        // 不泄漏用户名/邮箱是否已存在：与成功响应完全一致
        Err(RegisterUserError::AlreadyExists) => {
            Ok((StatusCode::CREATED, Json(json!({ "ok": true }))))
        }
        Err(RegisterUserError::PasswordHashFailed(e)) => Err(AppError::internal(e, request_id)),
        Err(RegisterUserError::Database(e)) => Err(AppError::internal(e.to_string(), request_id)),
    }
}

/// POST /api/v1/auth/verify-email — 验证邮箱
async fn verify_email(
    State(state): State<AppState>,
    Json(req): Json<TokenRequest>,
) -> Result<Json<GenericSuccess>, AppError> {
    let request_id = "verify-email";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    match verify_email_token(
        pool,
        &req.token,
        state.config.new_user_cooldown_secs as i64,
        request_id,
    )
    .await
    {
        Ok(_) => {
            tracing::info!("email verified successfully");
            Ok(Json(GenericSuccess { ok: true }))
        }
        // 不存在/已消费/过期统一错误（防 token 枚举）
        Err(VerifyEmailError::InvalidOrExpired) => Err(AppError::bad_request(
            "invalid or expired verification token",
            request_id,
            None,
        )),
        Err(VerifyEmailError::Database(e)) => Err(AppError::internal(e.to_string(), request_id)),
    }
}

/// POST /api/v1/auth/resend-verification — 重发验证邮件
///
/// 统一响应（M02-IDENTITY-08）：邮箱不存在、已激活与正常重发都返回 202，
/// 不泄漏邮箱是否已注册/已验证。冷却（60s）与日上限（3 次）命中返回 429；
/// 每 IP 每小时 10 次防刷。
async fn resend_verification(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<ResendVerificationRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "resend-verification";

    let email_normalized = crate::auth::normalize_email(req.email.trim());
    if !valid_email_shape(&email_normalized) {
        return Err(AppError::bad_request(
            "invalid email format",
            request_id,
            Some(json!({ "field": "email" })),
        ));
    }

    // IP 维度防刷
    let ip = client_ip(&headers);
    let now_ms = crate::outbox::now_millis();
    let ip_status = state.limiter.check(
        &format!("resend:ip:{ip}"),
        RESEND_IP_LIMIT,
        RESEND_IP_WINDOW_MS,
        now_ms,
    );
    if !ip_status.allowed {
        return Err(AppError::rate_limited(
            "too many resend requests, try again later",
            request_id,
            ip_status.retry_after_secs,
            ip_status.limit,
            ip_status.remaining,
            ip_status.reset_at_ms / 1000,
        ));
    }

    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    match resend_verification_email(
        pool,
        &state.limiter,
        &email_normalized,
        request_id,
        &ResendLimits::default(),
    )
    .await
    {
        // 正常重发 / 邮箱不存在或已激活：统一 202（不泄漏）
        Ok(_) => Ok((StatusCode::ACCEPTED, Json(json!({ "ok": true })))),
        Err(ResendError::RateLimited {
            retry_after_secs,
            limit,
            remaining,
            reset_at_unix_secs,
        }) => Err(AppError::rate_limited(
            "too many resend requests, try again later",
            request_id,
            retry_after_secs,
            limit,
            remaining,
            reset_at_unix_secs,
        )),
        Err(ResendError::Database(e)) => Err(AppError::internal(e.to_string(), request_id)),
    }
}

/// POST /api/v1/auth/login — 登录
///
/// 常量时间失败 + 统一 invalid credentials（不区分账号不存在/密码错误/账号
/// 状态）；每 IP 10 次/分钟，每账号连续失败 5 次锁定 10 分钟（429）——
/// M02-SESSION-03。
async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    _jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let request_id = "login";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // identifier 规范化：含 @ 按邮箱，否则按用户名（NFKC + lowercase）
    let identifier = req.identifier.trim();
    let identifier_normalized = if identifier.contains('@') {
        crate::auth::normalize_email(identifier)
    } else {
        crate::auth::normalize_username(identifier)
    };
    let ip = client_ip(&headers);

    match login_user(
        pool,
        &state.limiter,
        &identifier_normalized,
        &req.password,
        &ip,
        &LoginLimits::default(),
    )
    .await
    {
        Ok(outcome) => {
            let cookie = build_session_cookie(&outcome.session_token);
            let me = MeResponse {
                id: outcome.user_id,
                username: outcome.username,
                email: outcome.email,
                email_verified: outcome.email_verified,
                status: outcome.status,
                display_name: outcome.display_name,
                level: 1,
                roles: vec![],
            };
            Ok((
                StatusCode::OK,
                [(header::SET_COOKIE, cookie.to_string())],
                Json(me),
            )
                .into_response())
        }
        // 不区分账号不存在/密码错误/账号被禁（防枚举）
        Err(LoginError::InvalidCredentials) => {
            Err(AppError::unauthorized("invalid credentials", request_id))
        }
        Err(LoginError::RateLimited {
            retry_after_secs,
            limit,
            remaining,
            reset_at_unix_secs,
        }) => Err(AppError::rate_limited(
            "too many login attempts, try again later",
            request_id,
            retry_after_secs,
            limit,
            remaining,
            reset_at_unix_secs,
        )),
        Err(LoginError::Database(e)) => Err(AppError::internal(e.to_string(), request_id)),
    }
}

/// DELETE /api/v1/auth/session — 登出
async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<Response, AppError> {
    let _request_id = "logout";

    if let Some(pool) = &state.db {
        if let Some(cookie) = jar.get(crate::auth::session::SESSION_COOKIE_NAME) {
            let token = cookie.value();
            let _ = revoke_session_service(pool, token).await;
        }
    }

    let clear_cookie = build_clear_session_cookie();
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_cookie.to_string())],
    )
        .into_response())
}

/// GET /api/v1/auth/sessions — 设备列表（M02-SESSION-05）
async fn list_sessions(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Vec<DeviceSession>>, AppError> {
    let request_id = "list-sessions";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let sessions = list_sessions_service(pool, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(Json(sessions))
}

/// DELETE /api/v1/auth/sessions — 全部登出（撤销全部设备 + 清当前 cookie）
async fn logout_all(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "logout-all";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    revoke_all_service(pool, &user.id, "logout_all")
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let clear_cookie = build_clear_session_cookie();
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_cookie.to_string())],
    )
        .into_response())
}

/// DELETE /api/v1/auth/sessions/{id} — 逐设备撤销（仅限本人会话）
async fn revoke_session(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(session_id): Path<String>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "revoke-session";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let revoked = revoke_by_id_service(pool, &user.id, &session_id, "revoked_by_user")
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if !revoked {
        return Err(AppError::not_found("session not found", request_id));
    }
    Ok((StatusCode::OK, Json(json!({ "ok": true }))))
}

/// POST /api/v1/auth/password-reset — 请求找回密码
///
/// 统一响应（M02-IDENTITY-10）：邮箱不存在/已删除与正常请求都返回 202，
/// 不泄漏邮箱是否已注册。每 IP 每小时 5 次 + 每账号冷却 60s / 日上限 3 次，
/// 超限返回 429。
async fn request_password_reset(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<PasswordResetRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "password-reset";

    let email_normalized = crate::auth::normalize_email(req.email.trim());
    if !valid_email_shape(&email_normalized) {
        return Err(AppError::bad_request(
            "invalid email format",
            request_id,
            Some(json!({ "field": "email" })),
        ));
    }

    // IP 维度限流
    let ip = client_ip(&headers);
    let now_ms = crate::outbox::now_millis();
    let ip_status = state.limiter.check(
        &format!("reset:ip:{ip}"),
        RESET_IP_LIMIT,
        RESET_IP_WINDOW_MS,
        now_ms,
    );
    if !ip_status.allowed {
        return Err(AppError::rate_limited(
            "too many password reset requests, try again later",
            request_id,
            ip_status.retry_after_secs,
            ip_status.limit,
            ip_status.remaining,
            ip_status.reset_at_ms / 1000,
        ));
    }

    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    match request_reset_service(
        pool,
        &state.limiter,
        &email_normalized,
        request_id,
        &PasswordResetLimits::default(),
    )
    .await
    {
        // 正常请求 / 邮箱不存在或已删除：统一 202（不泄漏）
        Ok(_) => Ok((StatusCode::ACCEPTED, Json(json!({ "ok": true })))),
        Err(RequestResetError::RateLimited {
            retry_after_secs,
            limit,
            remaining,
            reset_at_unix_secs,
        }) => Err(AppError::rate_limited(
            "too many password reset requests, try again later",
            request_id,
            retry_after_secs,
            limit,
            remaining,
            reset_at_unix_secs,
        )),
        Err(RequestResetError::Database(e)) => Err(AppError::internal(e.to_string(), request_id)),
    }
}

/// POST /api/v1/auth/password-reset/confirm — 确认密码重置
///
/// 单事务：原子消费 30 分钟一次性 token → 更新密码哈希 → 撤销该用户全部
/// Session → 审计；无效/已消费/过期统一 400（M02-IDENTITY-10）。
async fn confirm_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetConfirm>,
) -> Result<Json<GenericSuccess>, AppError> {
    let request_id = "password-reset-confirm";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    validate_password(&req.password, request_id)?;

    // 密码哈希在任何 token 检查前执行（耗时不泄漏 token 有效性，防枚举）
    let password_hash =
        hash_password(&req.password).map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match confirm_reset_service(pool, &req.token, &password_hash, request_id).await {
        Ok(outcome) => {
            tracing::info!(user_id = %outcome.user_id, "password reset successful");
            Ok(Json(GenericSuccess { ok: true }))
        }
        // 不存在/已消费/过期统一错误（防 token 枚举）
        Err(ConfirmResetError::InvalidOrExpired) => Err(AppError::bad_request(
            "invalid or expired reset token",
            request_id,
            None,
        )),
        Err(ConfirmResetError::Database(e)) => Err(AppError::internal(e.to_string(), request_id)),
    }
}

// ─── 验证函数 ────────────────────────────────────────────────────────────────

#[allow(clippy::result_large_err)] // AppError 为全 handler 统一错误类型，体积固定可接受
fn validate_password(password: &str, request_id: &str) -> Result<(), AppError> {
    if password.len() < 8 {
        return Err(AppError::bad_request(
            "password must be at least 8 characters",
            request_id,
            Some(json!({ "field": "password" })),
        ));
    }
    if password.len() > 256 {
        return Err(AppError::bad_request(
            "password must be at most 256 characters",
            request_id,
            Some(json!({ "field": "password" })),
        ));
    }
    Ok(())
}

/// 基础邮箱格式检查（规范化后：恰好一个 @、本地/域名非空、域名含 `.`）。
fn valid_email_shape(email: &str) -> bool {
    let mut parts = email.split('@');
    let local = parts.next().unwrap_or("");
    let domain = parts.next().unwrap_or("");
    if parts.next().is_some() || local.is_empty() || domain.is_empty() {
        return false;
    }
    if local.contains(char::is_whitespace) || domain.contains(char::is_whitespace) {
        return false;
    }
    domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.')
}
