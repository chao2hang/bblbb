use axum::{
    extract::State,
    http::{header, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{delete, get, post},
    Router,
};
use axum_extra::extract::CookieJar;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Either;

use crate::{
    app::AppState,
    auth::{
        password::{hash_password, verify_password, VerifyResult},
        session::{
            build_clear_session_cookie, build_session_cookie, create_session, revoke_session,
            AuthSession,
        },
        token::{generate_token, hash_token},
    },
    error::AppError,
};

// ─── DTO ─────────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegisterRequest {
    pub username: String,
    pub email: String,
    pub password: String,
}

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
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/session", delete(logout))
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
async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "register";

    // 验证输入
    validate_username(&req.username, request_id)?;
    validate_email(&req.email, request_id)?;
    validate_password(&req.password, request_id)?;

    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let username_normalized = req.username.to_lowercase();
    let email_normalized = req.email.to_lowercase();
    let password_hash =
        hash_password(&req.password).map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = Utc::now().timestamp();

    // 尝试插入用户（唯一约束会阻止重复）
    let insert_result: Result<(), sqlx::Error> = match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(&user_id)
            .bind(&username_normalized)
            .bind(&email_normalized)
            .bind(&password_hash)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(&user_id)
            .bind(&username_normalized)
            .bind(&email_normalized)
            .bind(&password_hash)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map(|_| ())
        }
    };

    if let Err(sqlx::Error::Database(ref e)) = insert_result {
        if e.is_unique_violation() {
            // 不泄漏用户名/邮箱是否已存在
            return Ok((StatusCode::CREATED, Json(json!({ "ok": true }))));
        }
    }
    insert_result.map_err(|e| AppError::internal(e.to_string(), request_id))?;

    // 创建验证 token
    let verify_token = generate_token();
    let token_hash = hash_token(&verify_token);
    let token_id = uuid::Uuid::now_v7().to_string();
    let expires_at = now + 24 * 60 * 60; // 24 小时

    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&token_id)
            .bind(&user_id)
            .bind(&token_hash)
            .bind(expires_at)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO email_verification_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&token_id)
            .bind(&user_id)
            .bind(&token_hash)
            .bind(expires_at)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    // TODO: 通过 Outbox 发送验证邮件（当前仅记录日志）
    tracing::info!(
        user_id = %user_id,
        "user registered, verification token generated"
    );

    Ok((StatusCode::CREATED, Json(json!({ "ok": true }))))
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

    let token_hash = hash_token(&req.token);
    let now = Utc::now().timestamp();

    match pool {
        Either::Left(p) => {
            // 查找未消费且未过期的 token
            let token_row = sqlx::query_as::<_, VerifyTokenRow>(
                "SELECT id, user_id FROM email_verification_tokens
                 WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            let token_row = token_row.ok_or_else(|| {
                AppError::bad_request("invalid or expired verification token", request_id, None)
            })?;

            // 原子消费 token 并激活用户
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE email_verification_tokens SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL")
                .bind(now)
                .bind(&token_row.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE users SET email_verified = 1, status = 'active', updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(&token_row.user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let token_row = sqlx::query_as::<_, VerifyTokenRow>(
                "SELECT id, user_id FROM email_verification_tokens
                 WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            let token_row = token_row.ok_or_else(|| {
                AppError::bad_request("invalid or expired verification token", request_id, None)
            })?;

            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE email_verification_tokens SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL")
                .bind(now)
                .bind(&token_row.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE users SET email_verified = 1, status = 'active', updated_at = ? WHERE id = ?")
                .bind(now)
                .bind(&token_row.user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    tracing::info!("email verified successfully");
    Ok(Json(GenericSuccess { ok: true }))
}

/// POST /api/v1/auth/login — 登录
async fn login(
    State(state): State<AppState>,
    _jar: CookieJar,
    Json(req): Json<LoginRequest>,
) -> Result<Response, AppError> {
    let request_id = "login";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let identifier_normalized = req.identifier.to_lowercase();

    // 查找用户（用户名或邮箱均可）— 常量时间失败
    let user_row = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, UserAuthRow>(
                "SELECT id, username_normalized, email_normalized, email_verified, status, display_name, password_hash
                 FROM users WHERE email_normalized = ? OR username_normalized = ?",
            )
            .bind(&identifier_normalized)
            .bind(&identifier_normalized)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, UserAuthRow>(
                "SELECT id, username_normalized, email_normalized, email_verified, status, display_name, password_hash
                 FROM users WHERE email_normalized = ? OR username_normalized = ?",
            )
            .bind(&identifier_normalized)
            .bind(&identifier_normalized)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    // 统一错误响应 — 不区分账号不存在、密码错误或账号状态
    let unified_error = || AppError::unauthorized("invalid credentials", request_id);

    let user = user_row.unwrap_or_else(|| UserAuthRow {
        id: String::new(),
        username_normalized: String::new(),
        email_normalized: String::new(),
        email_verified: 0,
        status: "pending".to_string(),
        display_name: None,
        password_hash: "$argon2id$v=19$m=19456,t=2,p=1$AAAAAAAAAAAAAAAAAAAAAA$AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".to_string(),
    });

    // 验证密码
    let verify_result = verify_password(&req.password, &user.password_hash);
    let auth_ok = matches!(verify_result, VerifyResult::Ok) && !user.id.is_empty();

    if !auth_ok {
        // 常量时间延迟（简化：总是执行验证操作）
        return Err(unified_error());
    }

    // 检查账号状态
    if user.status == "banned" {
        return Err(AppError::forbidden("account banned", request_id));
    }
    if user.status == "deleted" {
        return Err(unified_error());
    }

    // 创建会话
    let session_token = create_session(pool, &user.id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let cookie = build_session_cookie(&session_token);

    let me = MeResponse {
        id: user.id,
        username: user.username_normalized,
        email: user.email_normalized,
        email_verified: user.email_verified != 0,
        status: user.status,
        display_name: user.display_name,
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

/// DELETE /api/v1/auth/session — 登出
async fn logout(State(state): State<AppState>, jar: CookieJar) -> Result<Response, AppError> {
    let _request_id = "logout";

    if let Some(pool) = &state.db {
        if let Some(cookie) = jar.get(crate::auth::session::SESSION_COOKIE_NAME) {
            let token = cookie.value();
            let _ = revoke_session(pool, token).await;
        }
    }

    let clear_cookie = build_clear_session_cookie();
    Ok((
        StatusCode::NO_CONTENT,
        [(header::SET_COOKIE, clear_cookie.to_string())],
    )
        .into_response())
}

/// POST /api/v1/auth/password-reset — 请求密码重置
async fn request_password_reset(
    State(state): State<AppState>,
    Json(req): Json<PasswordResetRequest>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "password-reset";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let email_normalized = req.email.to_lowercase();
    let now = Utc::now().timestamp();

    // 查找用户（不泄漏是否存在）
    let user_id: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT id FROM users WHERE email_normalized = ? AND status != 'deleted'",
            )
            .bind(&email_normalized)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT id FROM users WHERE email_normalized = ? AND status != 'deleted'",
            )
            .bind(&email_normalized)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    if let Some(user_id) = user_id {
        // 失效旧 token
        match pool {
            Either::Left(p) => {
                sqlx::query("UPDATE password_reset_tokens SET consumed_at = ? WHERE user_id = ? AND consumed_at IS NULL")
                    .bind(now)
                    .bind(&user_id)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            Either::Right(p) => {
                sqlx::query("UPDATE password_reset_tokens SET consumed_at = ? WHERE user_id = ? AND consumed_at IS NULL")
                    .bind(now)
                    .bind(&user_id)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
        }

        // 创建新 token（30 分钟过期）
        let reset_token = generate_token();
        let token_hash = hash_token(&reset_token);
        let token_id = uuid::Uuid::now_v7().to_string();
        let expires_at = now + 30 * 60;

        match pool {
            Either::Left(p) => {
                sqlx::query(
                    "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&token_id)
                .bind(&user_id)
                .bind(&token_hash)
                .bind(expires_at)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            Either::Right(p) => {
                sqlx::query(
                    "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                     VALUES (?, ?, ?, ?, ?)",
                )
                .bind(&token_id)
                .bind(&user_id)
                .bind(&token_hash)
                .bind(expires_at)
                .bind(now)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
        }

        // TODO: 通过 Outbox 发送密码重置邮件
        tracing::info!(user_id = %user_id, "password reset token generated");
    }

    // 统一响应 — 不泄漏邮箱是否存在
    Ok((StatusCode::ACCEPTED, Json(json!({ "ok": true }))))
}

/// POST /api/v1/auth/password-reset/confirm — 确认密码重置
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

    let token_hash = hash_token(&req.token);
    let now = Utc::now().timestamp();

    // 查找有效 token
    let token_row = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, VerifyTokenRow>(
                "SELECT id, user_id FROM password_reset_tokens
                 WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .fetch_optional(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, VerifyTokenRow>(
                "SELECT id, user_id FROM password_reset_tokens
                 WHERE token_hash = ? AND consumed_at IS NULL AND expires_at > ?",
            )
            .bind(&token_hash)
            .bind(now)
            .fetch_optional(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let token_row = token_row
        .ok_or_else(|| AppError::bad_request("invalid or expired reset token", request_id, None))?;

    // 原子操作：消费 token、更新密码、撤销所有会话
    let password_hash =
        hash_password(&req.password).map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE password_reset_tokens SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL")
                .bind(now)
                .bind(&token_row.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                .bind(&password_hash)
                .bind(now)
                .bind(&token_row.user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(&token_row.user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE password_reset_tokens SET consumed_at = ? WHERE id = ? AND consumed_at IS NULL")
                .bind(now)
                .bind(&token_row.id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                .bind(&password_hash)
                .bind(now)
                .bind(&token_row.user_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(
                "UPDATE user_sessions SET revoked_at = ? WHERE user_id = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(&token_row.user_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    tracing::info!(user_id = %token_row.user_id, "password reset successful");
    Ok(Json(GenericSuccess { ok: true }))
}

// ─── 验证函数 ────────────────────────────────────────────────────────────────

fn validate_username(username: &str, request_id: &str) -> Result<(), AppError> {
    if username.len() < 3 || username.len() > 32 {
        return Err(AppError::bad_request(
            "username must be 3-32 characters",
            request_id,
            Some(json!({ "field": "username" })),
        ));
    }
    if !username
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(AppError::bad_request(
            "username may only contain alphanumeric characters, underscore and hyphen",
            request_id,
            Some(json!({ "field": "username" })),
        ));
    }
    // 保留名检查
    let reserved = [
        "admin",
        "root",
        "system",
        "moderator",
        "api",
        "auth",
        "www",
        "null",
    ];
    if reserved.contains(&username.to_lowercase().as_str()) {
        return Err(AppError::bad_request(
            "username is reserved",
            request_id,
            Some(json!({ "field": "username" })),
        ));
    }
    Ok(())
}

fn validate_email(email: &str, request_id: &str) -> Result<(), AppError> {
    if email.is_empty() || email.len() > 320 {
        return Err(AppError::bad_request(
            "invalid email",
            request_id,
            Some(json!({ "field": "email" })),
        ));
    }
    if !email.contains('@') || !email.contains('.') {
        return Err(AppError::bad_request(
            "invalid email format",
            request_id,
            Some(json!({ "field": "email" })),
        ));
    }
    Ok(())
}

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

// ─── 数据库行结构 ─────────────────────────────────────────────────────────────

#[derive(sqlx::FromRow)]
struct UserAuthRow {
    id: String,
    username_normalized: String,
    email_normalized: String,
    email_verified: i64,
    status: String,
    display_name: Option<String>,
    password_hash: String,
}

#[derive(sqlx::FromRow)]
struct VerifyTokenRow {
    id: String,
    user_id: String,
}
