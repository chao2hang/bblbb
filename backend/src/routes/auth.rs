use axum::{
    body::Body,
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
        verification::{verify_email_token, VerifyEmailError},
    },
    domain::registration::{validate_register, RegisterRequest},
    error::AppError,
    ratelimit::{
        client_ip, REGISTER_ACCOUNT_LIMIT, REGISTER_IP_LIMIT, REGISTER_WINDOW_MS, RESEND_IP_LIMIT,
        RESEND_IP_WINDOW_MS, RESET_IP_LIMIT, RESET_IP_WINDOW_MS,
    },
    users::dto::Me,
};

/// M02-MFA-05：登录响应携带有效全局角色（未完成强制 enrollment 的 elevated
/// 账号聚合已降级为 member 基线——会话与 Me 均不宣称高权限）。
async fn effective_global_roles(pool: &crate::db::DatabasePool, user_id: &str) -> Vec<String> {
    crate::authz::roles::aggregate_permissions(pool, user_id, None)
        .await
        .map(|agg| agg.global_roles)
        .unwrap_or_default()
}

async fn current_trust_level(pool: &crate::db::DatabasePool, user_id: &str) -> i64 {
    match pool {
        sqlx::Either::Left(p) => sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(p)
            .await
            .ok()
            .flatten(),
        sqlx::Either::Right(p) => sqlx::query_scalar("SELECT trust_level FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_optional(p)
            .await
            .ok()
            .flatten(),
    }
    .unwrap_or(0)
    .clamp(0, 4)
}

// ─── DTO ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginRequest {
    /// 用户名或邮箱（不区分大小写）
    pub identifier: String,
    pub password: String,
    /// 「记住我」：勾选后签发 30 天会话（默认 7 天；M02-UX-03）。
    #[serde(default)]
    pub remember: Option<bool>,
}

/// 第二步 MFA 登录请求（M02-UX-03）：totp_code / recovery_code / passkey 三选一。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoginMfaRequest {
    /// 第一步登录返回的一次性 challenge token
    pub challenge_token: String,
    pub totp_code: Option<String>,
    pub recovery_code: Option<String>,
    /// 浏览器 navigator.credentials.get() 的原始 JSON（M02-MFA-PK，
    /// Passkey 作为第二因素与 TOTP/恢复码 OR 共存——恰好一个通过即可）
    pub passkey: Option<Value>,
}

/// 第一步登录的 MFA challenge 响应（M02-UX-03）：密码已验证，等待第二因素。
#[derive(Serialize)]
struct LoginMfaChallenge {
    mfa_required: bool,
    challenge_token: String,
    /// 该账号是否注册过 Passkey（true 时第二步可用 Passkey 断言，
    /// 也可继续用 TOTP/恢复码；M02-MFA-PK）
    passkey_available: bool,
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
        .route("/api/v1/auth/login/mfa", post(login_mfa))
        .route("/api/v1/auth/bootstrap", post(bootstrap_admin))
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

/// GET /api/v1/auth/csrf — 获取 CSRF token（M02-SESSION-07/08）
///
/// - 已认证：返回 Session 绑定 synchronizer token（由 session_id +
///   csrf_secret_hash 确定性派生，同一会话稳定）；
/// - 未认证：签发匿名预认证 CSRF 状态（M02-SESSION-08）——写入
///   `__Host-bblbb_csrf` cookie，返回由 (记录 id, csrf_secret_hash) 确定性
///   派生的 token；已有有效预认证 cookie 时复用（token 稳定）；
/// - 响应始终 `Cache-Control: private, no-store`（CSRF token 不得缓存）。
async fn get_csrf_token(
    State(state): State<AppState>,
    headers: HeaderMap,
    auth: AuthSession,
) -> Result<Response, AppError> {
    // 已认证：Session 绑定 synchronizer token（同一会话稳定）
    if let Some(csrf) = &auth.csrf_token {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(CsrfResponse {
                token: csrf.clone(),
            }),
        )
            .into_response());
    }

    // 未认证：匿名预认证 CSRF 状态（服务端可回溯，防 login CSRF）
    let request_id = "csrf";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let jar = CookieJar::from_headers(&headers);
    let existing = jar
        .get(crate::auth::preauth::PREAUTH_COOKIE_NAME)
        .map(|c| c.value().to_string());

    let (issued_cookie, token) = match existing {
        // 已有有效预认证状态：复用（浏览器已持有 cookie，仅返回稳定 token）
        Some(cookie_token) => match crate::auth::resolve_preauth(pool, &cookie_token).await {
            Ok(Some((id, secret_hash))) => (
                None,
                crate::auth::session::generate_csrf_token(&id, &secret_hash),
            ),
            _ => issue_preauth_state(pool, request_id).await?,
        },
        None => issue_preauth_state(pool, request_id).await?,
    };

    let mut response = Response::builder()
        .header(header::CACHE_CONTROL, "private, no-store")
        .header(header::CONTENT_TYPE, "application/json");
    if let Some(cookie_token) = issued_cookie {
        response = response.header(
            header::SET_COOKIE,
            crate::auth::build_preauth_cookie(&cookie_token).to_string(),
        );
    }
    Ok(response
        .body(Body::from(
            serde_json::to_string(&CsrfResponse { token }).unwrap(),
        ))
        .unwrap())
}

/// 签发新的预认证 CSRF 状态，返回 (新 cookie 令牌, 派生 CSRF token)。
async fn issue_preauth_state(
    pool: &crate::db::DatabasePool,
    request_id: &str,
) -> Result<(Option<String>, String), AppError> {
    let issued = crate::auth::issue_preauth(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok((Some(issued.cookie_token), issued.csrf_token))
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

    // 领域校验（M02-IDENTITY-03）→ 422 `validation_failed` + 字段级
    // errors[]（field/message_key），前端按字段渲染中文提示（契约
    // /auth/register 仅声明 201/422/429，不再返回 400 bad_request）。
    let registration = validate_register(&req).map_err(|e| {
        AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            "Unprocessable Entity",
            e.to_string(),
            request_id,
        )
        .with_errors(Some(json!([{
            "field": e.field(),
            "code": "validation_failed",
            "message_key": e.message_key(),
        }])))
    })?;

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

    if crate::users::blacklist::is_nickname_blacklisted(pool, &registration.username)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
    {
        return Err(AppError::bad_request(
            "该名称已被列入黑名单，禁止注册",
            request_id,
            None,
        ));
    }

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

/// POST /api/v1/auth/bootstrap — 首管理员引导（P0 整改）。
///
/// 匿名 + 一次性 bootstrap token（`--bootstrap` 生成）。校验顺序：
/// token 有效 → 实例未初始化（无 active administrator）→ 注册形状
/// （复用注册校验与密码强度）。成功创建 active administrator 并永久
/// 消费 token；重复初始化 409；无效 token 统一 422 防枚举。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BootstrapRequest {
    pub token: String,
    pub username: String,
    pub email: String,
    pub password: String,
}

/// bootstrap 端点限流：每 IP 5 次 / 小时（防 token 暴力猜测）。
const BOOTSTRAP_IP_LIMIT: u32 = 5;
/// bootstrap 端点限流窗口：1 小时。
const BOOTSTRAP_WINDOW_MS: i64 = 60 * 60 * 1000;

async fn bootstrap_admin(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<BootstrapRequest>,
) -> Result<Response, AppError> {
    let request_id = "auth_bootstrap";
    let now = crate::outbox::now_millis();
    let ip = client_ip(&headers);
    let limit = state.limiter.check(
        &format!("bootstrap:ip:{ip}"),
        BOOTSTRAP_IP_LIMIT,
        BOOTSTRAP_WINDOW_MS,
        now,
    );
    if !limit.allowed {
        return Err(AppError::rate_limited(
            "too many bootstrap attempts, try again later",
            request_id,
            limit.retry_after_secs,
            limit.limit,
            limit.remaining,
            limit.reset_at_ms / 1000,
        ));
    }

    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let Some(token_row_id) = crate::bootstrap::validate_bootstrap_token(pool, &req.token, now)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
    else {
        return Err(AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "bootstrap_token_invalid",
            "Unprocessable Entity",
            "invalid, expired or already used bootstrap token",
            request_id,
        ));
    };

    if crate::bootstrap::has_active_administrator(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
    {
        return Err(AppError::conflict(
            "bootstrap already completed: an active administrator exists",
            request_id,
        ));
    }

    // 注册形状校验 + 规范化（与 /auth/register 同规则同错误形状）。
    let registration = validate_register(&RegisterRequest {
        username: req.username.clone(),
        email: req.email.clone(),
        password: req.password.clone(),
    })
    .map_err(|e| {
        AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "validation_failed",
            "Unprocessable Entity",
            e.to_string(),
            request_id,
        )
        .with_errors(Some(json!([{
            "field": e.field(),
            "code": "validation_failed",
            "message_key": e.message_key(),
        }])))
    })?;
    let password_hash = hash_password(&registration.password)
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let user_id = crate::bootstrap::consume_and_create_admin(
        pool,
        &token_row_id,
        &crate::bootstrap::BootstrapAdminInput {
            username: registration.username_normalized.clone(),
            email: registration.email_normalized.clone(),
            password_hash,
            display_name: Some(registration.username.clone()),
        },
        now,
    )
    .await
    .map_err(|e| AppError::conflict(&e, request_id))?;

    tracing::info!(user_id = %user_id, "first administrator created via bootstrap");
    // 响应携带新建账号数据：私有数据不缓存（M00-FRONTEND-06，契约已声明）。
    let mut resp = (
        StatusCode::CREATED,
        Json(json!({ "ok": true, "user_id": user_id })),
    )
        .into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("private, no-store"),
    );
    Ok(resp)
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
        // 不存在/已消费/过期统一错误（防 token 枚举）；
        // 422 `verification_token_invalid`（契约 /auth/verify-email 仅声明 422）
        Err(VerifyEmailError::InvalidOrExpired) => Err(AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "verification_token_invalid",
            "Unprocessable Entity",
            "invalid or expired verification token",
            request_id,
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
        // 422 `validation_failed`（契约仅声明 202/422/429）+ email 字段提示
        return Err(email_validation_error(request_id));
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
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    match login_user(
        pool,
        &state.limiter,
        &identifier_normalized,
        &req.password,
        &ip,
        ua,
        req.remember.unwrap_or(false),
        request_id,
        &LoginLimits::default(),
    )
    .await
    {
        Ok(outcome) => {
            // 启用第二因素（TOTP 或 Passkey，M02-MFA-PK）：第一步只签发一次性
            // challenge（不写会话 Cookie），前端进入第二步 /auth/login/mfa。
            if outcome.mfa_required {
                let challenge_token = crate::auth::start_mfa_login(
                    pool,
                    &outcome.user_id,
                    req.remember.unwrap_or(false),
                )
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                // Passkey 入口提示：仅当服务端已配置且该账号有有效 Passkey
                let passkey_available = !state.config.passkey_rp_id.is_empty()
                    && crate::auth::passkey::has_active_passkey(pool, &outcome.user_id)
                        .await
                        .unwrap_or(false);
                return Ok((
                    StatusCode::OK,
                    [(header::CACHE_CONTROL, "private, no-store")],
                    Json(LoginMfaChallenge {
                        mfa_required: true,
                        challenge_token,
                        passkey_available,
                    }),
                )
                    .into_response());
            }

            let cookie = build_session_cookie(&outcome.session_token);
            let mfa_enabled = crate::auth::passkey::has_second_factor(pool, &outcome.user_id)
                .await
                .unwrap_or(false);
            let roles = effective_global_roles(pool, &outcome.user_id).await;
            let trust_level = current_trust_level(pool, &outcome.user_id).await;

            // 登录后自动签到（若配置开启）：
            if let Ok(cfg) = crate::economy::activity::service::get_activity_config(pool).await {
                if cfg.check_in_enabled && cfg.auto_check_in_enabled && cfg.rewards_enabled {
                    let _ = crate::economy::activity::service::claim_check_in(
                        pool,
                        &outcome.user_id,
                        crate::outbox::now_millis(),
                    )
                    .await;
                }
            }
            let me = Me {
                id: outcome.user_id,
                username: outcome.username,
                email: outcome.email,
                email_verified: outcome.email_verified,
                status: outcome.status,
                display_name: outcome.display_name,
                bio: None,
                signature: None,
                timezone: "UTC".to_string(),
                theme_name: None,
                email_visible_to: "nobody".to_string(),
                profile_visible_to: "everyone".to_string(),
                level: trust_level,
                roles,
                mfa_enabled,
                version: 1,
                presentation_tokens: None,
                avatar_attachment_id: None,
            };
            Ok((
                StatusCode::OK,
                [(header::SET_COOKIE, cookie.to_string())],
                Json(me),
            )
                .into_response())
        }
        // 不区分账号不存在/密码错误/账号被禁（防枚举）；
        // 稳定码 invalid_credentials（ERROR-CODES.md）——前端渲染
        // 「用户名或密码不正确」，不再落入 unauthorized 的「请先登录」文案。
        Err(LoginError::InvalidCredentials) => Err(AppError::with_code(
            StatusCode::UNAUTHORIZED,
            "invalid_credentials",
            "Unauthorized",
            "invalid credentials",
            request_id,
        )),
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

/// POST /api/v1/auth/login/mfa — 第二步 MFA 登录（M02-UX-03）
///
/// 用一次性 challenge + TOTP code 或恢复码完成登录：
/// - challenge 不存在/已消费/过期 → 422 `mfa_challenge_invalid`（统一，防枚举）；
/// - TOTP/恢复码错误 → 401 统一 `mfa_code_invalid`（不泄漏细节）；
/// - 成功 → 200 Me + 会话 Cookie。
///
/// 会话签发即 auth_verified_at=now（step-up 即刻满足，M02-MFA-07）。
async fn login_mfa(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(req): Json<LoginMfaRequest>,
) -> Result<Response, AppError> {
    let request_id = "login-mfa";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // 会话需记录设备 UA（设备列表展示 / 首见设备判定），与一步登录一致
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok());

    let has_code = req
        .totp_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some();
    let has_recovery = req
        .recovery_code
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .is_some();
    let has_passkey = req.passkey.as_ref().map(|v| !v.is_null()).unwrap_or(false);
    // 三选一（M02-MFA-PK）：都不给或给多个 → 统一 401（不泄漏校验规则细节；
    // 稳定码 mfa_code_invalid——与第二因素错误同一语义，防组合枚举）
    if has_code as u8 + has_recovery as u8 + has_passkey as u8 != 1 {
        return Err(AppError::with_code(
            StatusCode::UNAUTHORIZED,
            "mfa_code_invalid",
            "Unauthorized",
            "invalid MFA credentials",
            request_id,
        ));
    }

    if state.config.mfa_encryption_key.is_empty() {
        return Err(AppError::internal(
            "MFA encryption key is not configured (BBLBB__MFA_ENCRYPTION_KEY)",
            request_id,
        ));
    }

    // Passkey 实例：未配置（rp_id 为空）→ None，complete_mfa_login 内部统一
    // 以 InvalidCode 失败（不向未通过密码步的调用方泄漏服务端配置状态）
    let webauthn = crate::auth::passkey::build_webauthn(&state.config);

    match crate::auth::complete_mfa_login(
        pool,
        &req.challenge_token,
        req.totp_code.as_deref(),
        req.recovery_code.as_deref(),
        req.passkey.as_ref(),
        ua,
        state.config.mfa_encryption_key.as_bytes(),
        webauthn.as_ref(),
        request_id,
    )
    .await
    {
        Ok(completed) => {
            let cookie = build_session_cookie(&completed.session_token);
            // 第二步完成时 TOTP 必然已启用（mfa_required 由 has_confirmed_totp 判定）
            let roles = effective_global_roles(pool, &completed.user_id).await;
            let trust_level = current_trust_level(pool, &completed.user_id).await;

            // 登录后自动签到（若配置开启）：
            if let Ok(cfg) = crate::economy::activity::service::get_activity_config(pool).await {
                if cfg.check_in_enabled && cfg.auto_check_in_enabled && cfg.rewards_enabled {
                    let _ = crate::economy::activity::service::claim_check_in(
                        pool,
                        &completed.user_id,
                        crate::outbox::now_millis(),
                    )
                    .await;
                }
            }
            let me = Me {
                id: completed.user_id,
                username: completed.username,
                email: completed.email,
                email_verified: completed.email_verified,
                status: completed.status,
                display_name: completed.display_name,
                bio: None,
                signature: None,
                timezone: "UTC".to_string(),
                theme_name: None,
                email_visible_to: "nobody".to_string(),
                profile_visible_to: "everyone".to_string(),
                level: trust_level,
                roles,
                mfa_enabled: true,
                version: 1,
                presentation_tokens: None,
                avatar_attachment_id: None,
            };
            Ok((
                StatusCode::OK,
                [(header::SET_COOKIE, cookie.to_string())],
                Json(me),
            )
                .into_response())
        }
        // challenge 不存在/已消费/过期 → 422（契约仅声明 401/422/429）：
        // 稳定码 mfa_challenge_invalid——前端提示从第一步重新登录。
        Err(crate::auth::MfaLoginError::InvalidChallenge) => Err(AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "mfa_challenge_invalid",
            "Unprocessable Entity",
            "invalid or expired MFA challenge",
            request_id,
        )),
        // TOTP/恢复码错误 → 401 统一（不泄漏第二因素细节）；
        // 稳定码 mfa_code_invalid——前端提示「验证码不正确或已过期」，
        // 不再落入 unauthorized 的「请先登录」文案（M02-UX-03 用户反馈）。
        Err(crate::auth::MfaLoginError::InvalidCode) => Err(AppError::with_code(
            StatusCode::UNAUTHORIZED,
            "mfa_code_invalid",
            "Unauthorized",
            "invalid MFA credentials",
            request_id,
        )),
        Err(crate::auth::MfaLoginError::Database(e)) => Err(AppError::internal(e, request_id)),
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
        // 422 `validation_failed`（契约仅声明 202/422/429）+ email 字段提示
        return Err(email_validation_error(request_id));
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
        // 不存在/已消费/过期统一错误（防 token 枚举）；
        // 422 `reset_token_invalid`（契约 /auth/password-reset/confirm 仅声明 422）
        Err(ConfirmResetError::InvalidOrExpired) => Err(AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "reset_token_invalid",
            "Unprocessable Entity",
            "invalid or expired reset token",
            request_id,
        )),
        Err(ConfirmResetError::Database(e)) => Err(AppError::internal(e.to_string(), request_id)),
    }
}

// ─── 验证函数 ────────────────────────────────────────────────────────────────

#[allow(clippy::result_large_err)] // AppError 为全 handler 统一错误类型，体积固定可接受
fn validate_password(password: &str, request_id: &str) -> Result<(), AppError> {
    // 422 `validation_failed`（契约 /auth/password-reset/confirm 仅声明 422），
    // 附字段级 errors[] 供前端在 password 字段渲染中文提示。
    if password.len() < 8 {
        return Err(password_validation_error(
            "password_length",
            "password must be at least 8 characters",
            request_id,
        ));
    }
    if password.len() > 256 {
        return Err(password_validation_error(
            "password_length",
            "password must be at most 256 characters",
            request_id,
        ));
    }
    Ok(())
}

/// 密码字段校验失败 → 422 `validation_failed` + password 字段 message_key。
#[allow(clippy::result_large_err)]
fn password_validation_error(message_key: &str, detail: &str, request_id: &str) -> AppError {
    AppError::with_code(
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_failed",
        "Unprocessable Entity",
        detail,
        request_id,
    )
    .with_errors(Some(json!([{
        "field": "password",
        "code": "validation_failed",
        "message_key": message_key,
    }])))
}

/// 邮箱字段校验失败 → 422 `validation_failed` + email 字段 message_key。
#[allow(clippy::result_large_err)]
fn email_validation_error(request_id: &str) -> AppError {
    AppError::with_code(
        StatusCode::UNPROCESSABLE_ENTITY,
        "validation_failed",
        "Unprocessable Entity",
        "invalid email format",
        request_id,
    )
    .with_errors(Some(json!([{
        "field": "email",
        "code": "validation_failed",
        "message_key": "email_invalid",
    }])))
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
