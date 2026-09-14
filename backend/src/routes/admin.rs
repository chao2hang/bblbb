use axum::{
    body::Body,
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, Request, StatusCode},
    middleware::Next,
    response::{IntoResponse, Json, Response},
    routing::{delete, get, patch, post, put},
    Router,
};
use axum_extra::extract::CookieJar;
use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::auth::token::hash_token;
use crate::authz::decision::AUTHZ_POLICY_VERSION;
use crate::authz::enforce::authorize_action;
use crate::boards::admin::{create_board, update_board, BoardCreateInput, BoardUpdateInput};
use crate::error::AppError;
use crate::outbox::now_millis;
use crate::tags::admin::{create_tag, update_tag};
use crate::video::{load_policy, update_provider_policy, Provider, VideoError};

/// 管理权限门（M03-AUTHZ-05）：admin.manage + 账号状态实时门。
async fn require_admin(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, "admin.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(crate::authz::enforce::deny_to_error(
            crate::authz::enforce::denied_reason(&decision)
                .unwrap_or(crate::authz::decision::DenyReason::DefaultDeny),
            request_id,
        ));
    }
    Ok(())
}

/// 高风险管理操作必填 reason。
#[allow(clippy::result_large_err)] // AppError 为统一错误类型
fn required_reason(body: &Value, request_id: &str) -> Result<String, AppError> {
    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin operation",
            request_id,
            None,
        ));
    }
    Ok(reason)
}

/// 管理写响应统一 `Cache-Control: private, no-store`（M00-FRONTEND-06，
/// 契约已对写操作声明该响应头）。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

async fn private_cache_middleware(request: Request<Body>, next: Next) -> Response {
    private_no_store(next.run(request).await)
}

/// 管理后台路由
///
/// M6/M7 域路由委托给按域分区的子模块（admin_storage / admin_download /
/// admin_activity / admin_shop），由对应域 agent 填充，避免单文件并发冲突。
use super::{admin_activity, admin_download, admin_plugins, admin_shop, admin_storage};

pub fn router() -> Router<AppState> {
    Router::new()
        // 用户管理
        .route(
            "/api/v1/admin/users",
            get(list_admin_users).post(create_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}",
            get(get_admin_user).patch(update_admin_user),
        )
        .route(
            "/api/v1/admin/users/{id}/randomize-nickname",
            post(randomize_admin_user_nickname),
        )
        .route(
            "/api/v1/admin/nickname-blacklist",
            get(list_admin_nickname_blacklist).post(add_admin_nickname_blacklist),
        )
        .route(
            "/api/v1/admin/nickname-blacklist/{id}",
            delete(delete_admin_nickname_blacklist),
        )
        // 角色管理
        .route(
            "/api/v1/admin/roles",
            get(list_admin_roles).post(create_admin_role),
        )
        .route(
            "/api/v1/admin/roles/{id}",
            get(get_admin_role).patch(update_admin_role),
        )
        // 板块管理
        .route(
            "/api/v1/admin/boards",
            get(list_admin_boards).post(create_admin_board),
        )
        .route(
            "/api/v1/admin/boards/{id}",
            get(get_admin_board).patch(update_admin_board),
        )
        // 板块角色 assignment（P1 整改：0022 已落库但无管理 API）
        .route(
            "/api/v1/admin/boards/{id}/roles",
            get(list_board_role_assignments).post(assign_board_role),
        )
        .route(
            "/api/v1/admin/boards/{id}/roles/{user_id}/{role_name}",
            axum::routing::delete(revoke_board_role),
        )
        // 标签管理
        .route(
            "/api/v1/admin/tags",
            get(list_admin_tags).post(create_admin_tag),
        )
        .route(
            "/api/v1/admin/tags/{id}",
            get(get_admin_tag).patch(update_admin_tag),
        )
        // M6 存储配额（admin_storage 域 agent 填充）
        .merge(admin_storage::router())
        // M6 下载计费（admin_download 域 agent 填充）
        .merge(admin_download::router())
        // M7 活跃任务（admin_activity 域 agent 填充）
        .merge(admin_activity::router())
        // M7 商城（admin_shop 域 agent 填充）
        .merge(admin_shop::router())
        // AI 管理
        .route(
            "/api/v1/admin/ai/config",
            get(get_admin_ai_config).patch(update_ai_config),
        )
        .route("/api/v1/admin/ai/providers/test", post(test_ai_provider))
        .route("/api/v1/admin/ai/tasks", get(list_ai_tasks))
        .route(
            "/api/v1/admin/ai/tasks/{id}/cancel",
            post(cancel_ai_task_admin),
        )
        .route("/api/v1/admin/ai/tasks/{id}/retry", post(retry_ai_task))
        // 视频管理
        .route("/api/v1/admin/video/policies", get(list_video_policies))
        .route("/api/v1/admin/video/policies/test", post(test_video_policy))
        .route(
            "/api/v1/admin/video/policies/{provider}",
            get(get_video_policy).patch(update_video_policy),
        )
        // OAuth 客户端
        .route(
            "/api/v1/admin/oauth-clients",
            get(list_oauth_clients).post(create_oauth_client),
        )
        .route(
            "/api/v1/admin/oauth-clients/{id}",
            get(get_oauth_client).patch(update_oauth_client),
        )
        // Marketplace 管理
        .route(
            "/api/v1/admin/marketplace/clients",
            get(list_marketplace_clients),
        )
        .route(
            "/api/v1/admin/marketplace/clients/{id}",
            get(get_marketplace_client).patch(update_marketplace_client),
        )
        .route(
            "/api/v1/admin/marketplace/clients/{id}/rotate-webhook-secret",
            post(rotate_webhook_secret),
        )
        .route(
            "/api/v1/admin/marketplace/clients/{id}/emergency-disable",
            post(emergency_disable_client),
        )
        .route(
            "/api/v1/admin/marketplace/offers",
            get(list_marketplace_offers),
        )
        .route(
            "/api/v1/admin/marketplace/transactions",
            get(list_marketplace_transactions),
        )
        .route(
            "/api/v1/admin/marketplace/webhook-deliveries",
            get(list_webhook_deliveries),
        )
        .route(
            "/api/v1/admin/marketplace/webhook-deliveries/{id}/replay",
            post(replay_webhook_delivery),
        )
        .route(
            "/api/v1/admin/marketplace/reconciliation/run",
            post(run_reconciliation),
        )
        .route(
            "/api/v1/admin/marketplace/refunds/{id}/retry",
            post(retry_requested_refund),
        )
        // 主题（管理端，公开端在 themes.rs）
        .route("/api/v1/admin/themes", get(list_themes))
        .route(
            "/api/v1/admin/themes/data-packages",
            post(upload_theme_package),
        )
        .route("/api/v1/admin/themes/default", put(set_default_theme))
        .route("/api/v1/admin/themes/{name}", delete(delete_theme))
        .route(
            "/api/v1/admin/themes/{name}/settings",
            patch(update_theme_settings),
        )
        .layer(axum::middleware::from_fn(private_cache_middleware))
        // 配置型插件（M13-PLUGIN；不在冻结 193 契约中，随 PLUGIN.md 发布）
        .merge(admin_plugins::router())
}

/// 领域权限门（M13-ADMIN-02）：`user.manage` / `role.manage` 等细分权限。
async fn require_permission(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    permission: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, permission, None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(crate::authz::enforce::deny_to_error(
            crate::authz::enforce::denied_reason(&decision)
                .unwrap_or(crate::authz::decision::DenyReason::DefaultDeny),
            request_id,
        ));
    }
    Ok(())
}

/// 管理用户行投影（GET /api/v1/admin/users 契约；与 users/dto.rs 的
/// `AdminUser` 一致——不含密码哈希/恢复码/会话等凭据）。
async fn admin_user_json(pool: &crate::db::DatabasePool, row: &sqlx::sqlite::SqliteRow) -> Value {
    let user_id: String = row.get("id");
    let roles = admin_roles_for_user(pool, &user_id)
        .await
        .unwrap_or_default();
    // B 币余额（point_accounts 实时余额；无账户 = 0）。
    let coin_balance = admin_user_balance(pool, &user_id).await;
    json!({
        "id": row.get::<String,_>("id"),
        "username": row.get::<String,_>("username_normalized"),
        "email": row.get::<String,_>("email_normalized"),
        "email_verified": row.get::<i64,_>("email_verified") != 0,
        "status": row.get::<String,_>("status"),
        "display_name": row.get::<Option<String>,_>("display_name"),
        "level": row.get::<i64,_>("level"),
        "trust_level": row.get::<i64,_>("trust_level"),
        "roles": roles,
        "coin_balance": coin_balance,
        "created_at": row.get::<i64,_>("created_at"),
        "updated_at": row.get::<i64,_>("updated_at"),
        "last_login_at": row.get::<Option<i64>,_>("last_login_at"),
        "delete_requested_at": row.get::<Option<i64>,_>("delete_requested_at"),
        "deleted_at": row.get::<Option<i64>,_>("deleted_at"),
        "version": row.get::<i64,_>("version"),
    })
}

/// 用户 B 币实时余额（point_accounts × currencies.code；无账户 = 0）。
async fn admin_user_balance(pool: &crate::db::DatabasePool, user_id: &str) -> i64 {
    let sql = "SELECT c.code AS code, COALESCE(pa.balance, 0) AS balance
         FROM currencies c
         LEFT JOIN point_accounts pa ON pa.currency_id = c.id AND pa.user_id = ?";
    let rows = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, (String, i64)>(sql)
                .bind(user_id)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, (String, i64)>(sql)
                .bind(user_id)
                .fetch_all(p)
                .await
        }
    };
    if let Ok(rows) = rows {
        return rows
            .into_iter()
            .find_map(|(code, balance)| (code == "coin").then_some(balance))
            .unwrap_or(0);
    }
    0
}

async fn admin_user_json_mysql(
    pool: &crate::db::DatabasePool,
    row: &sqlx::mysql::MySqlRow,
) -> Value {
    let user_id: String = row.get("id");
    let roles = admin_roles_for_user(pool, &user_id)
        .await
        .unwrap_or_default();
    let coin_balance = admin_user_balance(pool, &user_id).await;
    json!({
        "id": row.get::<String,_>("id"),
        "username": row.get::<String,_>("username_normalized"),
        "email": row.get::<String,_>("email_normalized"),
        "email_verified": row.get::<i64,_>("email_verified") != 0,
        "status": row.get::<String,_>("status"),
        "display_name": row.get::<Option<String>,_>("display_name"),
        "level": row.get::<i64,_>("level"),
        "trust_level": row.get::<i64,_>("trust_level"),
        "roles": roles,
        "coin_balance": coin_balance,
        "created_at": row.get::<i64,_>("created_at"),
        "updated_at": row.get::<i64,_>("updated_at"),
        "last_login_at": row.get::<Option<i64>,_>("last_login_at"),
        "delete_requested_at": row.get::<Option<i64>,_>("delete_requested_at"),
        "deleted_at": row.get::<Option<i64>,_>("deleted_at"),
        "version": row.get::<i64,_>("version"),
    })
}

/// 用户生效全局角色名（管理视图；M13-ADMIN-02）。
async fn admin_roles_for_user(
    pool: &crate::db::DatabasePool,
    user_id: &str,
) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT r.name FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = ? ORDER BY r.name",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), "listAdminUsersRoles"))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT r.name FROM user_roles ur JOIN roles r ON r.id = ur.role_id WHERE ur.user_id = ? ORDER BY r.name",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), "listAdminUsersRoles"))?,
    };
    Ok(rows.into_iter().map(|(name,)| name).collect())
}

/// GET /api/v1/admin/users — 分页用户列表（user.manage；管理 DTO 不含凭据）。
async fn list_admin_users(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "listAdminUsers";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "user.manage", request_id).await?;
    let after = params.get("after").and_then(|v| v.parse::<i64>().ok());
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30)
        .clamp(1, 200);
    // 筛选参数（GAP-FIX 筛选补齐）：q（用户名/邮箱模糊）、status（精确）、
    // level（精确）。q 使用 like_escape + ESCAPE '!'（SQLite/MySQL 一致）。
    let q_pattern = params
        .get("q")
        .map(|raw| raw.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let escaped = s.replace('!', "!!").replace('%', "!%").replace('_', "!_");
            format!("%{escaped}%")
        });
    // status 值域即 users.status CHECK（0001）。
    let status_filter = params
        .get("status")
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());
    if let Some(status) = status_filter.as_deref() {
        if ![
            "pending",
            "active",
            "restricted",
            "banned",
            "pending_delete",
            "deleted",
        ]
        .contains(&status)
        {
            return Err(AppError::bad_request(
                "status must be one of pending|active|restricted|banned|pending_delete|deleted",
                request_id,
                None,
            ));
        }
    }
    // level 参数继续兼容既有 API；底层筛选使用 users.trust_level。
    let level_filter = params.get("level").and_then(|v| v.parse::<i64>().ok());
    if level_filter.is_some() && level_filter.unwrap() < 0 {
        return Err(AppError::bad_request(
            "level must be a non-negative integer",
            request_id,
            None,
        ));
    }
    let mut items = Vec::new();
    let mut next_cursor: Option<i64> = None;
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(
                "SELECT id, username_normalized, email_normalized, email_verified, status, display_name, trust_level AS level, trust_level, created_at, updated_at, last_login_at, delete_requested_at, deleted_at, version
                 FROM users WHERE deleted_at IS NULL AND (? IS NULL OR created_at < ?)
                   AND (? IS NULL OR username_normalized LIKE ? ESCAPE '!' OR email_normalized LIKE ? ESCAPE '!')
                   AND (? IS NULL OR status = ?)
                   AND (? IS NULL OR trust_level = ?)
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(after)
            .bind(after)
            .bind(&q_pattern)
            .bind(&q_pattern)
            .bind(&q_pattern)
            .bind(&status_filter)
            .bind(&status_filter)
            .bind(level_filter)
            .bind(level_filter)
            .bind(limit + 1)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for (i, row) in rows.iter().enumerate() {
                if i as i64 >= limit {
                    next_cursor = Some(row.get("created_at"));
                    break;
                }
                items.push(admin_user_json(pool, row).await);
            }
        }
        Either::Right(p) => {
            let rows = sqlx::query(
                "SELECT id, username_normalized, email_normalized, email_verified, status, display_name, trust_level AS level, trust_level, created_at, updated_at, last_login_at, delete_requested_at, deleted_at, version
                 FROM users WHERE deleted_at IS NULL AND (? IS NULL OR created_at < ?)
                   AND (? IS NULL OR username_normalized LIKE ? ESCAPE '!' OR email_normalized LIKE ? ESCAPE '!')
                   AND (? IS NULL OR status = ?)
                   AND (? IS NULL OR trust_level = ?)
                 ORDER BY created_at DESC LIMIT ?",
            )
            .bind(after)
            .bind(after)
            .bind(&q_pattern)
            .bind(&q_pattern)
            .bind(&q_pattern)
            .bind(&status_filter)
            .bind(&status_filter)
            .bind(level_filter)
            .bind(level_filter)
            .bind(limit + 1)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for (i, row) in rows.iter().enumerate() {
                if i as i64 >= limit {
                    next_cursor = Some(row.get("created_at"));
                    break;
                }
                items.push(admin_user_json_mysql(pool, row).await);
            }
        }
    }
    Ok(Json(json!({ "items": items, "next_cursor": next_cursor })))
}

/// POST /api/v1/admin/users — 管理员创建用户（pending 状态；随机初始密码仅
/// 用于占位，用户须走密码重置；reason + recent-auth + 审计）。
async fn create_admin_user(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "createAdminUser";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "user.manage", request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;

    let username = body
        .get("username")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("username required", request_id, None))?
        .trim()
        .to_string();
    let email = body
        .get("email")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("email required", request_id, None))?
        .trim()
        .to_string();
    let display_name = body
        .get("display_name")
        .and_then(Value::as_str)
        .map(str::to_string);
    if username.is_empty() || email.is_empty() || !email.contains('@') || username.len() > 64 {
        return Err(AppError::bad_request(
            "invalid username/email",
            request_id,
            None,
        ));
    }
    if crate::users::blacklist::is_nickname_blacklisted(pool, &username)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
    {
        return Err(AppError::bad_request(
            "该用户名已被列入黑名单",
            request_id,
            None,
        ));
    }
    if let Some(ref dn) = display_name {
        if crate::users::blacklist::is_nickname_blacklisted(pool, dn)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
        {
            return Err(AppError::bad_request(
                "该昵称已被列入黑名单，禁止使用",
                request_id,
                None,
            ));
        }
    }
    // 随机占位密码（不可知；用户走密码重置激活）。
    let placeholder = uuid::Uuid::now_v7().to_string();
    let password_hash = crate::auth::hash_password(&placeholder)
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let now = crate::outbox::now_millis();
    let user_id = uuid::Uuid::now_v7().to_string();
    let affected = match pool {
        Either::Left(p) => {

            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified, display_name, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'pending', 0, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(&email)
            .bind(&password_hash)
            .bind(&display_name)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| {
        if e.to_string().contains("UNIQUE") || e.to_string().contains("Duplicate") {
            AppError::bad_request(
                "username or email already exists",
                request_id,
                None,
            )
        } else {
            AppError::internal(e.to_string(), request_id)
        }
    })?
    .rows_affected()
        }
        Either::Right(p) => {

            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified, display_name, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'pending', 0, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(&email)
            .bind(&password_hash)
            .bind(&display_name)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| {
        if e.to_string().contains("UNIQUE") || e.to_string().contains("Duplicate") {
            AppError::bad_request(
                "username or email already exists",
                request_id,
                None,
            )
        } else {
            AppError::internal(e.to_string(), request_id)
        }
    })?
    .rows_affected()
        }
    };
    if affected == 0 {
        return Err(AppError::bad_request(
            "username or email already exists",
            request_id,
            None,
        ));
    }
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = crate::audit::AuditEntry::user_action(&user.id, "admin.user.create")
        .with_target("user", &user_id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": user_id, "username": username, "status": "pending" })),
    ))
}

/// GET /api/v1/admin/users/{id} — 单个用户管理视图（user.manage）。
async fn get_admin_user(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "getAdminUser";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "user.manage", request_id).await?;
    let view = match pool {
        Either::Left(p) => {
            let row = sqlx::query(
                "SELECT id, username_normalized, email_normalized, email_verified, status, display_name, trust_level AS level, trust_level, created_at, updated_at, last_login_at, delete_requested_at, deleted_at, version
                 FROM users WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            match row {
                Some(row) => Some(admin_user_json(pool, &row).await),
                None => None,
            }
        }
        Either::Right(p) => {
            let row = sqlx::query(
                "SELECT id, username_normalized, email_normalized, email_verified, status, display_name, trust_level AS level, trust_level, created_at, updated_at, last_login_at, delete_requested_at, deleted_at, version
                 FROM users WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            match row {
                Some(row) => Some(admin_user_json_mysql(pool, &row).await),
                None => None,
            }
        }
    };
    match view {
        Some(v) => Ok(Json(v)),
        None => Err(AppError::not_found("user not found", request_id)),
    }
}

/// PATCH /api/v1/admin/users/{id} — 更新用户（状态/角色 assignment；
/// If-Match version + reason + recent-auth + 审计；**禁止**改余额/流水）。
async fn update_admin_user(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "updateAdminUser";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "user.manage", request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let expected_version: i64 = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse()
        .map_err(|_| AppError::bad_request("If-Match must be an integer", request_id, None))?;

    let status = body
        .get("status")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(status) = &status {
        if !matches!(
            status.as_str(),
            "pending" | "active" | "restricted" | "banned"
        ) {
            return Err(AppError::bad_request("invalid status", request_id, None));
        }
    }
    let display_name = body
        .get("display_name")
        .and_then(Value::as_str)
        .map(str::to_string);
    if let Some(ref dn) = display_name {
        if crate::users::blacklist::is_nickname_blacklisted(pool, dn)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
        {
            return Err(AppError::bad_request(
                "该昵称已被列入黑名单，禁止使用",
                request_id,
                None,
            ));
        }
    }
    let now = crate::outbox::now_millis();
    let new_version = expected_version + 1;
    crate::bootstrap::update_user_guarded(
        pool,
        &id,
        expected_version,
        status.as_deref(),
        display_name.as_deref(),
        now,
    )
    .await
    .map_err(|error| match error {
        crate::bootstrap::GuardedUserUpdateError::NotFound => {
            AppError::not_found("user not found", request_id)
        }
        crate::bootstrap::GuardedUserUpdateError::VersionConflict => {
            AppError::version_conflict("user version conflict", request_id)
        }
        crate::bootstrap::GuardedUserUpdateError::LastAdministrator => {
            AppError::conflict("cannot disable the last active administrator", request_id)
        }
        crate::bootstrap::GuardedUserUpdateError::Database(error) => {
            AppError::internal(error.to_string(), request_id)
        }
    })?;
    // 纵深防御（M02-SESSION-06）：封禁立即撤销全部会话——与 resolve_session
    // 的实时状态检查互为冗余，任一机制失效时另一机制兜底
    if status.as_deref() == Some("banned") {
        let _ = crate::auth::session::revoke_all_sessions(pool, &id, "admin_user_ban").await;
    }
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = crate::audit::AuditEntry::user_action(&user.id, "admin.user.update")
        .with_target("user", &id)
        .with_reason(&reason)
        .with_metadata(json!({ "status": status, "version": new_version }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    get_admin_user(State(state), auth, Path(id)).await
}

/// POST /api/v1/admin/users/{id}/randomize-nickname
/// 管理员一键随机用户昵称：原昵称自动存入黑名单，并更新目标用户昵称为规范随机昵称。
async fn randomize_admin_user_nickname(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Option<axum::Json<Value>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "randomize_admin_user_nickname";
    let admin = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &admin.id, "user.manage", request_id).await?;

    let reason = body
        .as_ref()
        .and_then(|axum::Json(b)| b.get("reason").and_then(Value::as_str))
        .unwrap_or("管理员一键随机重置违规昵称")
        .trim();
    let reason = if reason.is_empty() {
        "管理员一键随机重置违规昵称"
    } else {
        reason
    };

    let now = crate::outbox::now_millis();
    let outcome =
        crate::users::blacklist::randomize_user_nickname(pool, &id, &admin.id, reason, now)
            .await
            .map_err(|e| match e {
                crate::users::blacklist::RandomizeNicknameError::NotFound => {
                    AppError::not_found("user not found", request_id)
                }
                crate::users::blacklist::RandomizeNicknameError::Database(msg) => {
                    AppError::internal(msg, request_id)
                }
            })?;

    // 写入审计日志
    let _ = AuditEntry::user_action(&admin.id, "admin.user.randomize_nickname")
        .with_target("user", &id)
        .with_reason(reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({
            "old_nickname": &outcome.old_nickname,
            "new_nickname": &outcome.new_nickname,
            "version": outcome.new_version,
            "at": now,
        }))
        .record(pool)
        .await;

    Ok(Json(json!({
        "ok": true,
        "user_id": outcome.user_id,
        "old_nickname": outcome.old_nickname,
        "new_nickname": outcome.new_nickname,
        "version": outcome.new_version,
    })))
}

#[derive(serde::Deserialize)]
struct BlacklistQuery {
    limit: Option<i64>,
    offset: Option<i64>,
    q: Option<String>,
}

/// GET /api/v1/admin/nickname-blacklist
async fn list_admin_nickname_blacklist(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<BlacklistQuery>,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_admin_nickname_blacklist";
    let admin = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &admin.id, "user.manage", request_id).await?;

    let limit = query.limit.unwrap_or(50);
    let offset = query.offset.unwrap_or(0);
    let (items, total) =
        crate::users::blacklist::list_nickname_blacklist(pool, limit, offset, query.q.as_deref())
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(Json(json!({
        "items": items,
        "total": total,
    })))
}

/// POST /api/v1/admin/nickname-blacklist
async fn add_admin_nickname_blacklist(
    State(state): State<AppState>,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "add_admin_nickname_blacklist";
    let admin = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &admin.id, "user.manage", request_id).await?;

    let nickname = body
        .get("nickname")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("nickname required", request_id, None))?
        .trim();
    if nickname.is_empty() {
        return Err(AppError::bad_request(
            "nickname cannot be empty",
            request_id,
            None,
        ));
    }
    let reason = body.get("reason").and_then(Value::as_str).map(str::trim);

    let now = crate::outbox::now_millis();
    let inserted = crate::users::blacklist::add_to_nickname_blacklist(
        pool,
        nickname,
        reason,
        Some(&admin.id),
        now,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let _ = AuditEntry::user_action(&admin.id, "admin.nickname_blacklist.add")
        .with_target("nickname_blacklist", nickname)
        .with_reason(reason.unwrap_or("管理员手动加入黑名单"))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;

    Ok((
        StatusCode::CREATED,
        Json(json!({
            "ok": true,
            "nickname": nickname,
            "inserted": inserted,
        })),
    ))
}

/// DELETE /api/v1/admin/nickname-blacklist/{id}
async fn delete_admin_nickname_blacklist(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "delete_admin_nickname_blacklist";
    let admin = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &admin.id, "user.manage", request_id).await?;

    let removed = crate::users::blacklist::remove_from_nickname_blacklist(pool, &id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let _ = AuditEntry::user_action(&admin.id, "admin.nickname_blacklist.delete")
        .with_target("nickname_blacklist", &id)
        .with_reason("管理员移出黑名单")
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;

    Ok(Json(json!({
        "ok": true,
        "removed": removed,
    })))
}

// ────────────────────────── 角色管理（M13-ADMIN-02）───────────────────────

fn role_json(r: &sqlx::sqlite::SqliteRow, permissions: Vec<String>) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "name": r.get::<String,_>("name"),
        "display_name": r.get::<String,_>("display_name"),
        "description": r.get::<Option<String>,_>("description"),
        "is_system": r.get::<i64,_>("is_system") != 0,
        "permissions": permissions,
        "created_at": r.get::<i64,_>("created_at"),
        "updated_at": r.get::<i64,_>("updated_at"),
    })
}

fn role_json_mysql(r: &sqlx::mysql::MySqlRow, permissions: Vec<String>) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "name": r.get::<String,_>("name"),
        "display_name": r.get::<String,_>("display_name"),
        "description": r.get::<Option<String>,_>("description"),
        "is_system": r.get::<i64,_>("is_system") != 0,
        "permissions": permissions,
        "created_at": r.get::<i64,_>("created_at"),
        "updated_at": r.get::<i64,_>("updated_at"),
    })
}

async fn role_permissions(
    pool: &crate::db::DatabasePool,
    role_id: &str,
) -> Result<Vec<String>, AppError> {
    let rows: Vec<(String,)> = match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT p.name FROM role_permissions rp JOIN permissions p ON p.id = rp.permission_id WHERE rp.role_id = ? ORDER BY p.name",
        )
        .bind(role_id)
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), "listAdminRoles"))?,
        Either::Right(p) => sqlx::query_as(
            "SELECT p.name FROM role_permissions rp JOIN permissions p ON p.id = rp.permission_id WHERE rp.role_id = ? ORDER BY p.name",
        )
        .bind(role_id)
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), "listAdminRoles"))?,
    };
    Ok(rows.into_iter().map(|(name,)| name).collect())
}

/// GET /api/v1/admin/roles — 角色列表（role.manage）。
async fn list_admin_roles(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "listAdminRoles";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "role.manage", request_id).await?;
    let mut items = Vec::new();
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(
                "SELECT id, name, display_name, description, is_system, created_at, updated_at FROM roles ORDER BY is_system DESC, name",
            )
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in &rows {
                let role_id: String = row.get("id");
                let permissions = role_permissions(pool, &role_id).await?;
                items.push(role_json(row, permissions));
            }
        }
        Either::Right(p) => {
            let rows = sqlx::query(
                "SELECT id, name, display_name, description, is_system, created_at, updated_at FROM roles ORDER BY is_system DESC, name",
            )
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in &rows {
                let role_id: String = row.get("id");
                let permissions = role_permissions(pool, &role_id).await?;
                items.push(role_json_mysql(row, permissions));
            }
        }
    }
    // 权限目录（视觉对齐 M17-GAPFIX-07）：前端网格渲染全部可勾选权限。
    let mut all_permissions: Vec<String> = crate::authz::PERMISSION_REGISTRY
        .iter()
        .map(|p| p.name.to_string())
        .collect();
    all_permissions.sort();
    Ok(Json(
        json!({ "items": items, "all_permissions": all_permissions }),
    ))
}

/// POST /api/v1/admin/roles — 创建自定义角色（reason + recent-auth + 审计；
/// 权限必须来自注册表；system 角色不可由 API 创建）。
async fn create_admin_role(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "createAdminRole";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "role.manage", request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("name required", request_id, None))?
        .trim()
        .to_string();
    let display_name = body
        .get("display_name")
        .and_then(Value::as_str)
        .unwrap_or(&name)
        .to_string();
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let permissions: Vec<String> = string_array(body.get("permissions")).unwrap_or_default();
    if name.is_empty()
        || name.len() > 64
        || !name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    {
        return Err(AppError::bad_request(
            "invalid role name (lowercase ascii/digits/underscore)",
            request_id,
            None,
        ));
    }
    if permissions.is_empty() {
        return Err(AppError::bad_request(
            "at least one permission required",
            request_id,
            None,
        ));
    }
    // 权限名必须全部存在于注册表（拒绝未知/伪造权限名）。
    let registry: Vec<String> = crate::authz::PERMISSION_REGISTRY
        .iter()
        .map(|p| p.name.to_string())
        .collect();
    for p in &permissions {
        if !registry.contains(p) {
            return Err(AppError::bad_request(
                format!("unknown permission '{p}'"),
                request_id,
                None,
            ));
        }
    }
    let now = crate::outbox::now_millis();
    let role_id = uuid::Uuid::now_v7().to_string();
    let created = match pool {
        Either::Left(p) => {

            sqlx::query(
                "INSERT INTO roles (id, name, display_name, description, is_system, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 0, ?, ?)",
            )
            .bind(&role_id)
            .bind(&name)
            .bind(&display_name)
            .bind(&description)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| {
        if e.to_string().contains("UNIQUE") || e.to_string().contains("Duplicate") {
            AppError::bad_request(
                "role name already exists",
                request_id,
                None,
            )
        } else {
            AppError::internal(e.to_string(), request_id)
        }
    })
            .map(|r| r.rows_affected())?
        }
        Either::Right(p) => {

            sqlx::query(
                "INSERT INTO roles (id, name, display_name, description, is_system, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 0, ?, ?)",
            )
            .bind(&role_id)
            .bind(&name)
            .bind(&display_name)
            .bind(&description)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| {
        if e.to_string().contains("UNIQUE") || e.to_string().contains("Duplicate") {
            AppError::bad_request(
                "role name already exists",
                request_id,
                None,
            )
        } else {
            AppError::internal(e.to_string(), request_id)
        }
    })
            .map(|r| r.rows_affected())?
        }
    };
    if created == 0 {
        return Err(AppError::bad_request(
            "role name already exists",
            request_id,
            None,
        ));
    }
    // 写入 role_permissions
    for perm_name in &permissions {
        let perm_id: Option<String> = match pool {
            Either::Left(p) => sqlx::query_scalar("SELECT id FROM permissions WHERE name = ?")
                .bind(perm_name)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            Either::Right(p) => sqlx::query_scalar("SELECT id FROM permissions WHERE name = ?")
                .bind(perm_name)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        };
        if let Some(perm_id) = perm_id {
            let _ = match pool {
                Either::Left(p) => sqlx::query(
                    "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
                )
                .bind(&role_id)
                .bind(&perm_id)
                .execute(p)
                .await
                .map(|_| ()),
                Either::Right(p) => sqlx::query(
                    "INSERT IGNORE INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
                )
                .bind(&role_id)
                .bind(&perm_id)
                .execute(p)
                .await
                .map(|_| ()),
            };
        }
    }
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = crate::audit::AuditEntry::user_action(&user.id, "admin.role.create")
        .with_target("role", &role_id)
        .with_reason(&reason)
        .with_metadata(json!({ "name": name, "permissions": permissions }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "id": role_id, "name": name })),
    ))
}

/// GET /api/v1/admin/roles/{id} — 单角色（role.manage）。
async fn get_admin_role(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "getAdminRole";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "role.manage", request_id).await?;
    let view = match pool {
        Either::Left(p) => {
            let row = sqlx::query(
                "SELECT id, name, display_name, description, is_system, created_at, updated_at FROM roles WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            match row {
                Some(r) => {
                    let role_id: String = r.get("id");
                    let permissions = role_permissions(pool, &role_id).await?;
                    Some(role_json(&r, permissions))
                }
                None => None,
            }
        }
        Either::Right(p) => {
            let row = sqlx::query(
                "SELECT id, name, display_name, description, is_system, created_at, updated_at FROM roles WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            match row {
                Some(r) => {
                    let role_id: String = r.get("id");
                    let permissions = role_permissions(pool, &role_id).await?;
                    Some(role_json_mysql(&r, permissions))
                }
                None => None,
            }
        }
    };
    let Some(view) = view else {
        return Err(AppError::not_found("role not found", request_id));
    };
    Ok(Json(view))
}

/// PATCH /api/v1/admin/roles/{id} — 更新角色（If-Match updated_at + reason +
/// recent-auth + 审计；system 角色仅可改 display_name/description）。
async fn update_admin_role(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "updateAdminRole";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "role.manage", request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let expected_version: i64 = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse()
        .map_err(|_| AppError::bad_request("If-Match must be an integer", request_id, None))?;

    let row = match pool {
        Either::Left(p) => {
            sqlx::query(
                "SELECT id, name, display_name, description, is_system, created_at, updated_at FROM roles WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .map(|r| (r.get::<i64, _>("is_system"), r.get::<i64, _>("updated_at")))
        }
        Either::Right(p) => {
            sqlx::query(
                "SELECT id, name, display_name, description, is_system, created_at, updated_at FROM roles WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .map(|r| (r.get::<i64, _>("is_system"), r.get::<i64, _>("updated_at")))
        }
    };
    let Some((is_system, updated_at)) = row else {
        return Err(AppError::not_found("role not found", request_id));
    };
    if updated_at != expected_version {
        return Err(AppError::version_conflict(
            "role version conflict",
            request_id,
        ));
    }
    let display_name = body
        .get("display_name")
        .and_then(Value::as_str)
        .map(str::to_string);
    let description = body
        .get("description")
        .and_then(Value::as_str)
        .map(str::to_string);
    let permissions: Option<Vec<String>> = string_array(body.get("permissions"));
    let now = crate::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE roles SET display_name = COALESCE(?, display_name), description = COALESCE(?, description), updated_at = ? WHERE id = ? AND updated_at = ?",
            )
            .bind(&display_name)
            .bind(&description)
            .bind(now)
            .bind(&id)
            .bind(expected_version)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE roles SET display_name = COALESCE(?, display_name), description = COALESCE(?, description), updated_at = ? WHERE id = ? AND updated_at = ?",
            )
            .bind(&display_name)
            .bind(&description)
            .bind(now)
            .bind(&id)
            .bind(expected_version)
            .execute(p)
            .await
            .map(|_| ())
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };
    // 权限更新（非 system 角色）：先清后插，权限名必须来自注册表。
    if let Some(permissions) = permissions {
        if is_system != 0 {
            return Err(AppError::bad_request(
                "system role permissions cannot be changed",
                request_id,
                None,
            ));
        }
        let registry: Vec<String> = crate::authz::PERMISSION_REGISTRY
            .iter()
            .map(|p| p.name.to_string())
            .collect();
        for p in &permissions {
            if !registry.contains(p) {
                return Err(AppError::bad_request(
                    format!("unknown permission '{p}'"),
                    request_id,
                    None,
                ));
            }
        }
        let _ = match pool {
            Either::Left(p) => sqlx::query("DELETE FROM role_permissions WHERE role_id = ?")
                .bind(&id)
                .execute(p)
                .await
                .map(|_| ()),
            Either::Right(p) => sqlx::query("DELETE FROM role_permissions WHERE role_id = ?")
                .bind(&id)
                .execute(p)
                .await
                .map(|_| ()),
        };
        for perm_name in &permissions {
            let perm_id: Option<String> = match pool {
                Either::Left(p) => sqlx::query_scalar("SELECT id FROM permissions WHERE name = ?")
                    .bind(perm_name)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?,
                Either::Right(p) => sqlx::query_scalar("SELECT id FROM permissions WHERE name = ?")
                    .bind(perm_name)
                    .fetch_optional(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            };
            if let Some(perm_id) = perm_id {
                let _ = match pool {
                    Either::Left(p) => sqlx::query(
                        "INSERT OR IGNORE INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
                    )
                    .bind(&id)
                    .bind(&perm_id)
                    .execute(p)
                    .await
                    .map(|_| ()),
                    Either::Right(p) => sqlx::query(
                        "INSERT IGNORE INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
                    )
                    .bind(&id)
                    .bind(&perm_id)
                    .execute(p)
                    .await
                    .map(|_| ()),
                };
            }
        }
    }
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = crate::audit::AuditEntry::user_action(&user.id, "admin.role.update")
        .with_target("role", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    get_admin_role(State(state), auth, Path(id)).await
}
/// GET /api/v1/admin/boards — 全部板块（board.manage；M03-BOARDS-05 管理投影；
/// GAP-FIX 筛选补齐：q=name/slug 模糊）。板块为全站有限集合（数十量级），
/// q 过滤在 SQL 完成（LIKE + ESCAPE '!'，SQLite/MySQL 一致）。
async fn list_admin_boards(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "listAdminBoards";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "board.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "board.manage permission required",
            request_id,
        ));
    }

    let q_pattern = params
        .get("q")
        .map(|raw| raw.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let escaped = s.replace('!', "!!").replace('%', "!%").replace('_', "!_");
            format!("%{escaped}%")
        });

    // post_count（视觉对齐 M17-GAPFIX-07）：published 且未删帖子数（相关子查询）。
    let items: Vec<Value> = match pool {
        Either::Left(p) => {
            let rows = sqlx::query(
                "SELECT id, slug, name, description, icon, sort_order, parent_id, visibility, posting_mode, is_active, created_at, updated_at,
                        (SELECT COUNT(*) FROM posts p WHERE p.board_id = boards.id AND p.status = 'published' AND p.deleted_at IS NULL) AS post_count
                 FROM boards
                 WHERE (? IS NULL OR name LIKE ? ESCAPE '!' OR slug LIKE ? ESCAPE '!')
                 ORDER BY sort_order ASC, name ASC",
            )
            .bind(&q_pattern)
            .bind(&q_pattern)
            .bind(&q_pattern)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            rows.iter().map(board_admin_row_json).collect()
        }
        Either::Right(p) => {
            let rows = sqlx::query(
                "SELECT id, slug, name, description, icon, sort_order, parent_id, visibility, posting_mode, is_active, created_at, updated_at,
                        (SELECT COUNT(*) FROM posts p WHERE p.board_id = boards.id AND p.status = 'published' AND p.deleted_at IS NULL) AS post_count
                 FROM boards
                 WHERE (? IS NULL OR name LIKE ? ESCAPE '!' OR slug LIKE ? ESCAPE '!')
                 ORDER BY sort_order ASC, name ASC",
            )
            .bind(&q_pattern)
            .bind(&q_pattern)
            .bind(&q_pattern)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            rows.iter().map(board_admin_row_json_mysql).collect()
        }
    };

    // 版主（视觉对齐 M17-GAPFIX-07）：board_role_assignments × users，按板块聚合。
    let moderators_sql = "SELECT bra.board_id AS board_id, u.username_normalized AS username
             FROM board_role_assignments bra
             JOIN users u ON u.id = bra.user_id
             ORDER BY u.username_normalized";
    let mod_rows: Vec<(String, String)> = match pool {
        Either::Left(p) => sqlx::query_as(moderators_sql)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(moderators_sql)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let mut mods_by_board: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for (board_id, username) in mod_rows {
        mods_by_board.entry(board_id).or_default().push(username);
    }
    let items: Vec<Value> = items
        .into_iter()
        .map(|mut item| {
            if let Some(obj) = item.as_object_mut() {
                let id = obj
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                let mods = mods_by_board.remove(&id).unwrap_or_default();
                obj.insert("moderators".to_string(), json!(mods));
            }
            item
        })
        .collect();

    Ok(Json(json!({ "items": items })))
}
/// POST /api/v1/admin/boards — 创建板块（权限门 + 校验 + 审计，M03-BOARDS-05）
async fn create_admin_board(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "createAdminBoard";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "board.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "board.manage permission required",
            request_id,
        ));
    }

    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin board create",
            request_id,
            None,
        ));
    }

    let input = BoardCreateInput {
        slug: body
            .get("slug")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        name: body
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        icon: body.get("icon").and_then(Value::as_str).map(str::to_string),
        sort_order: body.get("sort_order").and_then(Value::as_i64).unwrap_or(0),
        parent_id: body
            .get("parent_id")
            .and_then(Value::as_str)
            .map(str::to_string),
        visibility: body
            .get("visibility")
            .and_then(Value::as_str)
            .unwrap_or("public")
            .to_string(),
        posting_mode: body
            .get("posting_mode")
            .and_then(Value::as_str)
            .unwrap_or("normal")
            .to_string(),
    };

    let created = create_board(pool, &user.id, input, &reason, request_id).await?;
    Ok(Json(created))
}

/// PATCH /api/v1/admin/boards/{id} — 更新板块（If-Match 版本 + reason + 审计，
/// M03-BOARDS-05）
async fn update_admin_board(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    auth: AuthSession,
    body: Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "updateAdminBoard";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "board.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "board.manage permission required",
            request_id,
        ));
    }

    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current version integer",
                request_id,
                None,
            )
        })?;

    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin board update",
            request_id,
            None,
        ));
    }

    let input = BoardUpdateInput {
        slug: body.get("slug").and_then(Value::as_str).map(str::to_string),
        name: body.get("name").and_then(Value::as_str).map(str::to_string),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        icon: body.get("icon").and_then(Value::as_str).map(str::to_string),
        sort_order: body.get("sort_order").and_then(Value::as_i64),
        parent_id: body
            .get("parent_id")
            .map(|v| v.as_str().map(str::to_string)),
        is_active: body.get("is_active").and_then(Value::as_bool),
        visibility: body
            .get("visibility")
            .and_then(Value::as_str)
            .map(str::to_string),
        posting_mode: body
            .get("posting_mode")
            .and_then(Value::as_str)
            .map(str::to_string),
    };

    let updated = update_board(pool, &user.id, &id, input, if_match, &reason, request_id).await?;
    Ok(Json(updated))
}
/// GET /api/v1/admin/boards/{id} — 单板块（board.manage；M13-ADMIN-03）。
async fn get_admin_board(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "getAdminBoard";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "board.manage", request_id).await?;
    let row = match pool {
        Either::Left(p) => {
            sqlx::query(
                "SELECT id, slug, name, description, icon, sort_order, parent_id, visibility, posting_mode, is_active, created_at, updated_at,
                        (SELECT COUNT(*) FROM posts p WHERE p.board_id = boards.id AND p.status = 'published' AND p.deleted_at IS NULL) AS post_count
                 FROM boards WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .map(|r| board_admin_row_json(&r))
        }
        Either::Right(p) => {
            sqlx::query(
                "SELECT id, slug, name, description, icon, sort_order, parent_id, visibility, posting_mode, is_active, created_at, updated_at,
                        (SELECT COUNT(*) FROM posts p WHERE p.board_id = boards.id AND p.status = 'published' AND p.deleted_at IS NULL) AS post_count
                 FROM boards WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .map(|r| board_admin_row_json_mysql(&r))
        }
    };
    let Some(view) = row else {
        return Err(AppError::not_found("board not found", request_id));
    };
    Ok(Json(view))
}

// ───────────── 板块角色 assignment（P1 整改，0022 board_role_assignments）────

/// GET /api/v1/admin/boards/{id}/roles — 板块启用角色 + assignment 列表
/// （role.manage）。
async fn list_board_role_assignments(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "list_admin_board_roles";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "role.manage", request_id).await?;

    let exists: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT 1 FROM boards WHERE id = ?")
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT 1 FROM boards WHERE id = ?")
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    if exists.is_none() {
        return Err(AppError::not_found("board not found", request_id));
    }

    let sql =
        "SELECT a.id, a.board_id, a.user_id, a.role_id, a.granted_by, a.granted_at, a.expires_at,
                      r.name AS role_name, u.username_normalized AS username
               FROM board_role_assignments a
               JOIN roles r ON r.id = a.role_id
               JOIN users u ON u.id = a.user_id
               WHERE a.board_id = ?
               ORDER BY a.granted_at DESC, a.id";
    /// 查询行：(id, board_id, user_id, role_id, granted_by, granted_at,
    /// expires_at, role_name, username)。
    type AssignmentRow = (
        String,
        String,
        String,
        String,
        Option<String>,
        i64,
        Option<i64>,
        String,
        String,
    );
    let rows: Vec<AssignmentRow> = match pool {
        Either::Left(p) => sqlx::query_as(sql).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as(sql).fetch_all(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let assignments: Vec<Value> = rows
        .into_iter()
        .map(
            |(
                aid,
                board_id,
                user_id,
                role_id,
                granted_by,
                granted_at,
                expires_at,
                role_name,
                username,
            )| {
                json!({
                    "id": aid,
                    "board_id": board_id,
                    "user_id": user_id,
                    "username": username,
                    "role_id": role_id,
                    "role_name": role_name,
                    "granted_by": granted_by,
                    "granted_at": granted_at,
                    "expires_at": expires_at,
                })
            },
        )
        .collect();

    Ok(Json(json!({
        "board_id": id,
        "assignments": assignments,
    })))
}

/// POST /api/v1/admin/boards/{id}/roles — 授予板块角色（role.manage）。
///
/// body {user_id, role_name, expires_at?, reason}：
/// - 仅允许板块范围角色（`board_` 前缀），全局角色拒绝（防越权提升）；
/// - expires_at 可选（Unix 毫秒），必须晚于当前时间；
/// - 幂等：UNIQUE(board_id, user_id, role_id) 冲突返回 409；
/// - reason 必填 + 审计 `admin.board.role.grant`。
async fn assign_board_role(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "assign_board_role";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "role.manage", request_id).await?;

    let reason = required_reason(&body, request_id)?;
    let target_user_id = body
        .get("user_id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let role_name = body
        .get("role_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    let expires_at = body.get("expires_at").and_then(Value::as_i64);
    if target_user_id.is_empty() || role_name.is_empty() {
        return Err(AppError::bad_request(
            "user_id and role_name are required",
            request_id,
            None,
        ));
    }
    if !role_name.starts_with("board_") {
        return Err(AppError::bad_request(
            "only board-scoped roles (board_*) can be assigned to a board",
            request_id,
            None,
        ));
    }

    // 板块与用户存在性校验。
    let board_exists: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT 1 FROM boards WHERE id = ?")
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT 1 FROM boards WHERE id = ?")
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    if board_exists.is_none() {
        return Err(AppError::not_found("board not found", request_id));
    }
    let user_exists: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT 1 FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(&target_user_id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT 1 FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(&target_user_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if user_exists.is_none() {
        return Err(AppError::not_found("user not found", request_id));
    }

    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(&role_name)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(&role_name)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    }
    .ok_or_else(|| AppError::not_found("role not found", request_id))?;

    if let Some(expires) = expires_at {
        if expires <= crate::outbox::now_millis() {
            return Err(AppError::bad_request(
                "expires_at must be in the future",
                request_id,
                None,
            ));
        }
    }

    let now = crate::outbox::now_millis();
    let assignment_id = uuid::Uuid::now_v7().to_string();
    let sql = "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_by, granted_at, expires_at)
               VALUES (?, ?, ?, ?, ?, ?, ?)";
    let insert_result = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&assignment_id)
            .bind(&id)
            .bind(&target_user_id)
            .bind(&role_id)
            .bind(&user.id)
            .bind(now)
            .bind(expires_at)
            .execute(p)
            .await
            .map(|r| r.rows_affected()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&assignment_id)
            .bind(&id)
            .bind(&target_user_id)
            .bind(&role_id)
            .bind(&user.id)
            .bind(now)
            .bind(expires_at)
            .execute(p)
            .await
            .map(|r| r.rows_affected()),
    };
    if let Err(e) = insert_result {
        // 唯一约束冲突 → 已持有该板块角色（幂等语义返回 409 提示）。
        if e.as_database_error()
            .map(|d| d.is_unique_violation())
            .unwrap_or(false)
        {
            return Err(AppError::conflict(
                "assignment already exists (board, user, role)",
                request_id,
            ));
        }
        return Err(AppError::internal(e.to_string(), request_id));
    }

    AuditEntry::user_action(&user.id, "admin.board.role.grant")
        .with_target("board", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({
            "user_id": target_user_id,
            "role_name": role_name,
            "expires_at": expires_at,
        }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(private_no_store(
        (
            StatusCode::OK,
            Json(json!({
                "id": assignment_id,
                "board_id": id,
                "user_id": target_user_id,
                "role_name": role_name,
                "expires_at": expires_at,
                "status": "granted",
            })),
        )
            .into_response(),
    ))
}

/// DELETE /api/v1/admin/boards/{id}/roles/{user_id}/{role_name} — 撤销板块
/// 角色（role.manage；body {reason} 必填，审计 `admin.board.role.revoke`）。
async fn revoke_board_role(
    State(state): State<AppState>,
    auth: AuthSession,
    Path((id, target_user_id, role_name)): Path<(String, String, String)>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "revoke_board_role";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "role.manage", request_id).await?;

    let req: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?
    };
    let reason = required_reason(&req, request_id)?;

    let role_id: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(&role_name)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(&role_name)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let Some(role_id) = role_id else {
        return Err(AppError::not_found("role not found", request_id));
    };

    let deleted = match pool {
        Either::Left(p) => sqlx::query(
            "DELETE FROM board_role_assignments WHERE board_id = ? AND user_id = ? AND role_id = ?",
        )
        .bind(&id)
        .bind(&target_user_id)
        .bind(&role_id)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "DELETE FROM board_role_assignments WHERE board_id = ? AND user_id = ? AND role_id = ?",
        )
        .bind(&id)
        .bind(&target_user_id)
        .bind(&role_id)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
    };

    if deleted > 0 {
        AuditEntry::user_action(&user.id, "admin.board.role.revoke")
            .with_target("board", &id)
            .with_reason(&reason)
            .with_policy_version(AUTHZ_POLICY_VERSION)
            .with_metadata(json!({
                "user_id": target_user_id,
                "role_name": role_name,
            }))
            .record(pool)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    }

    Ok(private_no_store(
        (
            StatusCode::OK,
            Json(json!({
                "board_id": id,
                "user_id": target_user_id,
                "role_name": role_name,
                "revoked": deleted > 0,
            })),
        )
            .into_response(),
    ))
}

fn board_admin_row_json(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "slug": r.get::<String,_>("slug"),
        "name": r.get::<String,_>("name"),
        "description": r.get::<Option<String>,_>("description"),
        "icon": r.get::<Option<String>,_>("icon"),
        "sort_order": r.get::<i64,_>("sort_order"),
        "parent_id": r.get::<Option<String>,_>("parent_id"),
        "visibility": r.get::<String,_>("visibility"),
        "posting_mode": r.get::<String,_>("posting_mode"),
        "is_active": r.get::<i64,_>("is_active") != 0,
        "post_count": r.get::<i64,_>("post_count"),
        "version": r.get::<i64,_>("updated_at"),
        "created_at": r.get::<i64,_>("created_at"),
        "updated_at": r.get::<i64,_>("updated_at"),
    })
}

fn board_admin_row_json_mysql(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "slug": r.get::<String,_>("slug"),
        "name": r.get::<String,_>("name"),
        "description": r.get::<Option<String>,_>("description"),
        "icon": r.get::<Option<String>,_>("icon"),
        "sort_order": r.get::<i64,_>("sort_order"),
        "parent_id": r.get::<Option<String>,_>("parent_id"),
        "visibility": r.get::<String,_>("visibility"),
        "posting_mode": r.get::<String,_>("posting_mode"),
        "is_active": r.get::<i64,_>("is_active") != 0,
        "post_count": r.get::<i64,_>("post_count"),
        "version": r.get::<i64,_>("updated_at"),
        "created_at": r.get::<i64,_>("created_at"),
        "updated_at": r.get::<i64,_>("updated_at"),
    })
}
/// GET /api/v1/admin/tags — 标签列表（含禁用状态，M03-BOARDS-06；GAP-FIX
/// 筛选补齐：q=name/slug 模糊、status=active|disabled|merged）。
///
/// status 映射（0061 新增 tags.status 列，语义与标签合并工作流对齐）：
/// - active：is_active=1 且 status IS NULL（正常启用）；
/// - disabled：is_active=0（既有停用开关）；
/// - merged：status='merged'（已被合并进其他标签）。
///
/// 直接 SQL 读取（不再走 load_all_tags 全量缓存投影）：需要读取 0061 新增
/// status 列且数据量级为全站标签（有限规模，LIMIT 上限 200）。
async fn list_admin_tags(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "listAdminTags";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "tag.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "tag.manage permission required",
            request_id,
        ));
    }

    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(200)
        .clamp(1, 200);
    let q_pattern = params
        .get("q")
        .map(|raw| raw.trim())
        .filter(|s| !s.is_empty())
        .map(|s| {
            let escaped = s.replace('!', "!!").replace('%', "!%").replace('_', "!_");
            format!("%{escaped}%")
        });
    let status_sql: &str = match params
        .get("status")
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        Some("active") => "AND t.is_active = 1 AND t.status IS NULL",
        Some("disabled") => "AND t.is_active = 0",
        Some("merged") => "AND t.status = 'merged'",
        Some(other) => {
            return Err(AppError::bad_request(
                format!("status must be one of active|disabled|merged, got: {other}"),
                request_id,
                None,
            ))
        }
        None => "",
    };

    let sql = format!(
        "SELECT t.id, t.slug, t.name, t.description, t.color, t.group_id, t.usage_count, t.is_active, t.status
         FROM tags t
         WHERE (? IS NULL OR t.name LIKE ? ESCAPE '!' OR t.slug LIKE ? ESCAPE '!')
         {status_sql}
         ORDER BY t.usage_count DESC, t.name ASC
         LIMIT ?"
    );
    let items: Vec<Value> = match pool {
        Either::Left(p) => {
            let rows = sqlx::query(&sql)
                .bind(&q_pattern)
                .bind(&q_pattern)
                .bind(&q_pattern)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            rows.iter()
                .map(|r| {
                    json!({
                        "id": r.get::<String,_>("id"),
                        "slug": r.get::<String,_>("slug"),
                        "name": r.get::<String,_>("name"),
                        "description": r.get::<String,_>("description"),
                        "color": r.get::<String,_>("color"),
                        "group_id": r.get::<Option<String>,_>("group_id"),
                        "usage_count": r.get::<i64,_>("usage_count"),
                        "is_active": r.get::<i64,_>("is_active") != 0,
                        "status": r.get::<Option<String>,_>("status"),
                    })
                })
                .collect()
        }
        Either::Right(p) => {
            let rows = sqlx::query(&sql)
                .bind(&q_pattern)
                .bind(&q_pattern)
                .bind(&q_pattern)
                .bind(limit)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            rows.iter()
                .map(|r| {
                    json!({
                        "id": r.get::<String,_>("id"),
                        "slug": r.get::<String,_>("slug"),
                        "name": r.get::<String,_>("name"),
                        "description": r.get::<String,_>("description"),
                        "color": r.get::<String,_>("color"),
                        "group_id": r.get::<Option<String>,_>("group_id"),
                        "usage_count": r.get::<i64,_>("usage_count"),
                        "is_active": r.get::<i64,_>("is_active") != 0,
                        "status": r.get::<Option<String>,_>("status"),
                    })
                })
                .collect()
        }
    };
    Ok(Json(json!({ "items": items })))
}
/// POST /api/v1/admin/tags — 创建标签（唯一性 + 审计，M03-BOARDS-07）
async fn create_admin_tag(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "createAdminTag";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "tag.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "tag.manage permission required",
            request_id,
        ));
    }

    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin tag create",
            request_id,
            None,
        ));
    }

    let input = crate::tags::TagCreateInput {
        name: body
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        slug: body.get("slug").and_then(Value::as_str).map(str::to_string),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string(),
        color: body
            .get("color")
            .and_then(Value::as_str)
            .map(str::to_string),
        group_id: body
            .get("group_id")
            .and_then(Value::as_str)
            .map(str::to_string),
    };
    let created = create_tag(pool, &user.id, input, &reason, request_id).await?;
    Ok(Json(created))
}

/// PATCH /api/v1/admin/tags/{id} — 更新标签（If-Match 版本 + 审计，M03-BOARDS-07）
async fn update_admin_tag(
    State(state): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
    auth: AuthSession,
    body: Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "updateAdminTag";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(pool, &user.id, "tag.manage", None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "tag.manage permission required",
            request_id,
        ));
    }

    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current version integer",
                request_id,
                None,
            )
        })?;

    let reason = body
        .get("reason")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required for admin tag update",
            request_id,
            None,
        ));
    }

    let input = crate::tags::TagUpdateInput {
        name: body.get("name").and_then(Value::as_str).map(str::to_string),
        slug: body.get("slug").map(|v| v.as_str().map(str::to_string)),
        description: body
            .get("description")
            .and_then(Value::as_str)
            .map(str::to_string),
        color: body.get("color").map(|v| v.as_str().map(str::to_string)),
        group_id: body.get("group_id").map(|v| v.as_str().map(str::to_string)),
        is_active: body.get("is_active").and_then(Value::as_bool),
    };
    let updated = update_tag(pool, &user.id, &id, input, if_match, &reason, request_id).await?;
    Ok(Json(updated))
}
/// GET /api/v1/admin/tags/{id} — 单标签（tag.manage；M13-ADMIN-03）。
async fn get_admin_tag(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "getAdminTag";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_permission(pool, &user.id, "tag.manage", request_id).await?;
    let row = match pool {
        Either::Left(p) => {
            sqlx::query(
                "SELECT id, slug, name, description, color, group_id, usage_count, is_active, created_at, updated_at
                 FROM tags WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .map(|r| tag_admin_row_json(&r))
        }
        Either::Right(p) => {
            sqlx::query(
                "SELECT id, slug, name, description, color, group_id, usage_count, is_active, created_at, updated_at
                 FROM tags WHERE id = ?",
            )
            .bind(&id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .map(|r| tag_admin_row_json_mysql(&r))
        }
    };
    let Some(view) = row else {
        return Err(AppError::not_found("tag not found", request_id));
    };
    Ok(Json(view))
}

fn tag_admin_row_json(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "slug": r.get::<String,_>("slug"),
        "name": r.get::<String,_>("name"),
        "description": r.get::<String,_>("description"),
        "color": r.get::<Option<String>,_>("color"),
        "group_id": r.get::<Option<String>,_>("group_id"),
        "usage_count": r.get::<i64,_>("usage_count"),
        "is_active": r.get::<i64,_>("is_active") != 0,
        "version": r.get::<i64,_>("updated_at"),
    })
}

fn tag_admin_row_json_mysql(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "slug": r.get::<String,_>("slug"),
        "name": r.get::<String,_>("name"),
        "description": r.get::<String,_>("description"),
        "color": r.get::<Option<String>,_>("color"),
        "group_id": r.get::<Option<String>,_>("group_id"),
        "usage_count": r.get::<i64,_>("usage_count"),
        "is_active": r.get::<i64,_>("is_active") != 0,
        "version": r.get::<i64,_>("updated_at"),
    })
}
/// GET /api/v1/admin/ai/config — Provider 列表（脱敏）+ 默认策略（M09-UI-06）。
async fn get_admin_ai_config(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_ai_config";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let config = ai_admin_config_json(pool, request_id).await?;
    Ok(Json(config))
}

/// 站点级 AI 配置行（0061 ai_site_config 单行表）。
///
/// 存储方案说明（GAP-FIX AI 契约修复）：0052 的 ai_providers 只有 Provider
/// 行级配置（version 是 Provider 自己的版本），没有站点级聚合。为承载
/// GET/PATCH /api/v1/admin/ai/config 顶层 version 与 enabled/data_mode/
/// flags/budgets，选择 0061 新增的单行表 `ai_site_config`（固定列 + JSON
/// 文本）——三方言结构等价、无方言特定类型，且 version 乐观并发直接复用
/// 行级 UPDATE 守卫（与 site_settings 同一模式）。
#[derive(sqlx::FromRow, Clone)]
struct AiSiteConfigRow {
    enabled: i64,
    data_mode: String,
    flags_json: String,
    budgets_json: String,
    version: i64,
    updated_at: i64,
}

/// 读取站点级 AI 配置单行（迁移种子保证存在）。
async fn load_ai_site_config(
    pool: &crate::db::DatabasePool,
    request_id: &str,
) -> Result<AiSiteConfigRow, AppError> {
    let sql = "SELECT enabled, data_mode, flags_json, budgets_json, version, updated_at
               FROM ai_site_config WHERE id = 'singleton'";
    let row = match pool {
        sqlx::Either::Left(p) => {
            sqlx::query_as::<_, AiSiteConfigRow>(sql)
                .fetch_optional(p)
                .await
        }
        sqlx::Either::Right(p) => {
            sqlx::query_as::<_, AiSiteConfigRow>(sql)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    row.ok_or_else(|| {
        AppError::internal(
            "ai_site_config singleton row missing (run migrations)",
            request_id,
        )
    })
}

/// AI 用途 flags 的已知键与默认值（关）。
const AI_FLAG_KEYS: [&str; 4] = ["formatting", "seo", "tagging", "moderation"];
/// AI 预算的已知键（默认 null = 未配置）。
const AI_BUDGET_KEYS: [&str; 4] = [
    "per_user_daily_tokens",
    "per_user_daily_usd",
    "site_daily_tokens",
    "site_daily_usd",
];

/// 站点级 AI 配置 JSON（已知键补全默认值；未知键原样保留）。
fn ai_site_config_section(row: &AiSiteConfigRow) -> Value {
    let mut flags = json!({});
    for key in AI_FLAG_KEYS {
        flags[key] = json!(false);
    }
    if let Ok(stored) = serde_json::from_str::<Value>(&row.flags_json) {
        if let Some(map) = stored.as_object() {
            for (k, v) in map {
                if let Some(b) = v.as_bool() {
                    flags[k] = json!(b);
                }
            }
        }
    }
    let mut budgets = json!({});
    for key in AI_BUDGET_KEYS {
        budgets[key] = Value::Null;
    }
    if let Ok(stored) = serde_json::from_str::<Value>(&row.budgets_json) {
        if let Some(map) = stored.as_object() {
            for (k, v) in map {
                if v.is_null() || v.is_number() {
                    budgets[k] = v.clone();
                }
            }
        }
    }
    json!({
        "enabled": row.enabled != 0,
        "data_mode": row.data_mode,
        "flags": flags,
        "budgets": budgets,
    })
}

/// GET/PATCH /api/v1/admin/ai/config 的完整响应（站点级配置 + Provider 列表）。
async fn ai_admin_config_json(
    pool: &crate::db::DatabasePool,
    request_id: &str,
) -> Result<Value, AppError> {
    let site = load_ai_site_config(pool, request_id).await?;
    let items: Vec<Value> = match pool {
        sqlx::Either::Left(p) => sqlx::query(
            "SELECT id, name, adapter_type, base_url, default_model, status, secret_configured, data_mode, timeout_ms, max_input_tokens, max_output_tokens, max_concurrency, version, created_at, updated_at
             FROM ai_providers ORDER BY name",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(ai_provider_config_row)
        .collect(),
        sqlx::Either::Right(p) => sqlx::query(
            "SELECT id, name, adapter_type, base_url, default_model, status, secret_configured, data_mode, timeout_ms, max_input_tokens, max_output_tokens, max_concurrency, version, created_at, updated_at
             FROM ai_providers ORDER BY name",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(ai_provider_config_row_mysql)
        .collect(),
    };
    let mut section = ai_site_config_section(&site);
    Ok(json!({
        "version": site.version,
        "updated_at": site.updated_at,
        "purposes": ["formatting", "seo", "tagging", "moderation"],
        "providers": items,
        // 站点级字段平铺到顶层（前端 pickConfig 读取顶层键）。
        "enabled": section["enabled"].clone(),
        "data_mode": section["data_mode"].clone(),
        "flags": section["flags"].take(),
        "budgets": section["budgets"].take(),
    }))
}

fn ai_provider_config_row(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "name": r.get::<String,_>("name"),
        "adapter_type": r.get::<String,_>("adapter_type"),
        "api_type": r.get::<String,_>("adapter_type"),
        "base_url": r.get::<String,_>("base_url"),
        "default_model": r.get::<String,_>("default_model"),
        "model": r.get::<String,_>("default_model"),
        "status": r.get::<String,_>("status"),
        "secret_configured": r.get::<i64,_>("secret_configured") != 0,
        "data_mode": r.get::<String,_>("data_mode"),
        "timeout_ms": r.get::<i64,_>("timeout_ms"),
        "max_input_tokens": r.get::<i64,_>("max_input_tokens"),
        "max_output_tokens": r.get::<i64,_>("max_output_tokens"),
        "max_concurrency": r.get::<i64,_>("max_concurrency"),
        "version": r.get::<i64,_>("version"),
        "created_at": r.get::<i64,_>("created_at"),
        "updated_at": r.get::<i64,_>("updated_at"),
    })
}

fn ai_provider_config_row_mysql(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "name": r.get::<String,_>("name"),
        "adapter_type": r.get::<String,_>("adapter_type"),
        "api_type": r.get::<String,_>("adapter_type"),
        "base_url": r.get::<String,_>("base_url"),
        "default_model": r.get::<String,_>("default_model"),
        "model": r.get::<String,_>("default_model"),
        "status": r.get::<String,_>("status"),
        "secret_configured": r.get::<i64,_>("secret_configured") != 0,
        "data_mode": r.get::<String,_>("data_mode"),
        "timeout_ms": r.get::<i64,_>("timeout_ms"),
        "max_input_tokens": r.get::<i64,_>("max_input_tokens"),
        "max_output_tokens": r.get::<i64,_>("max_output_tokens"),
        "max_concurrency": r.get::<i64,_>("max_concurrency"),
        "version": r.get::<i64,_>("version"),
        "created_at": r.get::<i64,_>("created_at"),
        "updated_at": r.get::<i64,_>("updated_at"),
    })
}

/// PATCH /api/v1/admin/ai/config — 站点级配置 + Provider upsert（admin.manage
/// + reason + 审计；GAP-FIX AI 契约修复）。
///
/// 与前端 admin/ai 页面 save action 对齐：body 携带 enabled/data_mode/
/// flags/budgets/expected_version/reason，If-Match 头为当前顶层 version
/// （优先头，缺省回落 body.expected_version；与 ai_site_config.version 比较，
/// 不符 409）。Provider upsert 保持兼容：body 同时携带 name + base_url 时
/// 执行原有 upsert（SQLite ON CONFLICT / MySQL ON DUPLICATE KEY）。
async fn update_ai_config(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    headers: HeaderMap,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "patch_admin_ai_config";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let reason = required_reason(&body, request_id)?;

    // 版本门：If-Match 头优先，回落 body.expected_version（前端两者都发）。
    let expected_version = match headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.trim().trim_matches('"').parse::<i64>())
    {
        Some(Ok(v)) => Some(v),
        Some(Err(_)) => {
            return Err(AppError::bad_request(
                "If-Match must be the current version integer",
                request_id,
                None,
            ))
        }
        None => body.get("expected_version").and_then(Value::as_i64),
    };
    let Some(expected_version) = expected_version else {
        return Err(AppError::bad_request(
            "If-Match header or expected_version is required",
            request_id,
            None,
        ));
    };
    if expected_version < 1 {
        return Err(AppError::bad_request(
            "expected_version must be >= 1",
            request_id,
            None,
        ));
    }

    let current = load_ai_site_config(pool, request_id).await?;
    if expected_version != current.version {
        return Err(AppError::conflict(
            "ai config version mismatch (reload and retry)",
            request_id,
        ));
    }

    // ── 站点级部分更新（缺省字段保持原值）──
    let mut enabled = current.enabled;
    let mut data_mode = current.data_mode.clone();
    let mut flags_json = current.flags_json.clone();
    let mut budgets_json = current.budgets_json.clone();
    let mut changed: Vec<String> = Vec::new();

    if let Some(v) = body.get("enabled").and_then(Value::as_bool) {
        let v = v as i64;
        if enabled != v {
            enabled = v;
            changed.push("enabled".to_string());
        }
    }
    if let Some(v) = body.get("data_mode").and_then(Value::as_str) {
        let v = v.trim().to_string();
        if !["disabled", "metadata_only", "redacted", "full_with_consent"].contains(&v.as_str()) {
            return Err(AppError::bad_request(
                "data_mode must be one of disabled|metadata_only|redacted|full_with_consent",
                request_id,
                None,
            ));
        }
        if data_mode != v {
            data_mode = v;
            changed.push("data_mode".to_string());
        }
    }
    // flags：对象，值必须全为布尔（已知 4 键，未知键原样透传存储）。
    if let Some(v) = body.get("flags") {
        let Some(map) = v.as_object() else {
            return Err(AppError::bad_request(
                "flags must be an object of booleans",
                request_id,
                None,
            ));
        };
        if map.values().any(|x| !x.is_boolean()) {
            return Err(AppError::bad_request(
                "flags must be an object of booleans",
                request_id,
                None,
            ));
        }
        let encoded = serde_json::to_string(&Value::Object(map.clone()))
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        if flags_json != encoded {
            flags_json = encoded;
            changed.push("flags".to_string());
        }
    }
    // budgets：对象，值必须为数字或 null（已知 4 键）。
    if let Some(v) = body.get("budgets") {
        let Some(map) = v.as_object() else {
            return Err(AppError::bad_request(
                "budgets must be an object of numbers or null",
                request_id,
                None,
            ));
        };
        if map.values().any(|x| !x.is_null() && !x.is_number()) {
            return Err(AppError::bad_request(
                "budgets must be an object of numbers or null",
                request_id,
                None,
            ));
        }
        let encoded = serde_json::to_string(&Value::Object(map.clone()))
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        if budgets_json != encoded {
            budgets_json = encoded;
            changed.push("budgets".to_string());
        }
    }

    // 全列 UPDATE + version 乐观锁（0 行受影响 = 并发冲突 → 409）。
    let now = crate::ai::consent::now();
    let update_site = "UPDATE ai_site_config
        SET enabled = ?, data_mode = ?, flags_json = ?, budgets_json = ?,
            version = version + 1, updated_at = ?
        WHERE id = 'singleton' AND version = ?";
    let affected = match pool {
        sqlx::Either::Left(p) => sqlx::query(update_site)
            .bind(enabled)
            .bind(&data_mode)
            .bind(&flags_json)
            .bind(&budgets_json)
            .bind(now)
            .bind(expected_version)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
        sqlx::Either::Right(p) => sqlx::query(update_site)
            .bind(enabled)
            .bind(&data_mode)
            .bind(&flags_json)
            .bind(&budgets_json)
            .bind(now)
            .bind(expected_version)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
    };
    if affected == 0 {
        return Err(AppError::conflict(
            "ai config version mismatch (reload and retry)",
            request_id,
        ));
    }

    // ── Provider delete（支持按 id 或 name 删除渠道）──
    let mut provider_deleted: Option<Value> = None;
    let delete_id = body
        .get("delete_provider_id")
        .or_else(|| body.get("delete_provider"))
        .and_then(Value::as_str);
    if let Some(del_id) = delete_id {
        let del_id = del_id.trim();
        if !del_id.is_empty() {
            let task_count: i64 = match pool {
                sqlx::Either::Left(p) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM ai_tasks WHERE provider_id = ? OR provider_id IN (SELECT id FROM ai_providers WHERE name = ?)",
                    )
                    .bind(del_id)
                    .bind(del_id)
                    .fetch_one(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
                }
                sqlx::Either::Right(p) => {
                    sqlx::query_scalar(
                        "SELECT COUNT(*) FROM ai_tasks WHERE provider_id = ? OR provider_id IN (SELECT id FROM ai_providers WHERE name = ?)",
                    )
                    .bind(del_id)
                    .bind(del_id)
                    .fetch_one(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
                }
            };
            if task_count > 0 {
                return Err(AppError::bad_request(
                    "该渠道存在关联的任务记录，无法直接删除，请将其状态设为停用",
                    request_id,
                    None,
                ));
            }
            // 清理可能存在的 consents 关联
            match pool {
                sqlx::Either::Left(p) => {
                    let _ = sqlx::query("DELETE FROM ai_consents WHERE provider_id = ? OR provider_id IN (SELECT id FROM ai_providers WHERE name = ?)")
                        .bind(del_id)
                        .bind(del_id)
                        .execute(p)
                        .await;
                }
                sqlx::Either::Right(p) => {
                    let _ = sqlx::query("DELETE FROM ai_consents WHERE provider_id = ? OR provider_id IN (SELECT id FROM ai_providers WHERE name = ?)")
                        .bind(del_id)
                        .bind(del_id)
                        .execute(p)
                        .await;
                }
            };
            let affected = match pool {
                sqlx::Either::Left(p) => {
                    sqlx::query("DELETE FROM ai_providers WHERE id = ? OR name = ?")
                        .bind(del_id)
                        .bind(del_id)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?
                        .rows_affected()
                }
                sqlx::Either::Right(p) => {
                    sqlx::query("DELETE FROM ai_providers WHERE id = ? OR name = ?")
                        .bind(del_id)
                        .bind(del_id)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?
                        .rows_affected()
                }
            };
            provider_deleted = Some(json!({ "id": del_id, "deleted": affected }));
            changed.push("provider_delete".to_string());
        }
    }

    // ── Provider upsert / update（兼容既有调用：body 带 name+base_url 时执行）──
    let mut provider_upserted: Option<Value> = None;
    if let (Some(name), Some(base_url)) = (
        body.get("name").and_then(Value::as_str),
        body.get("base_url").and_then(Value::as_str),
    ) {
        let name = name.trim().to_string();
        let base_url = base_url.trim().to_string();
        if !name.is_empty() && name.len() <= 120 && !base_url.is_empty() {
            let default_model = body
                .get("default_model")
                .and_then(Value::as_str)
                .unwrap_or("gpt-4o-mini")
                .trim()
                .to_string();
            let raw_adapter = body
                .get("adapter_type")
                .and_then(Value::as_str)
                .unwrap_or("openai_compatible");
            let adapter_type = match raw_adapter {
                "anthropic" => "anthropic",
                "custom" => "custom",
                _ => "openai_compatible",
            }
            .to_string();
            let provider_data_mode = body
                .get("data_mode")
                .and_then(Value::as_str)
                .unwrap_or("redacted")
                .to_string();
            let status = body
                .get("status")
                .and_then(Value::as_str)
                .map(|s| match s {
                    "enabled" => "enabled",
                    _ => "disabled",
                })
                .unwrap_or("enabled")
                .to_string();
            let has_secret = body
                .get("api_key")
                .or_else(|| body.get("secret"))
                .and_then(Value::as_str)
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);
            let secret_val: i64 = if has_secret { 1 } else { 0 };

            let target_id = body
                .get("id")
                .or_else(|| body.get("provider_id"))
                .and_then(Value::as_str)
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let affected = match target_id {
                Some(ref pid) => {
                    let update_sql = "UPDATE ai_providers
                         SET name = ?, adapter_type = ?, base_url = ?, default_model = ?, status = ?,
                             secret_configured = CASE WHEN ? = 1 THEN 1 ELSE secret_configured END,
                             data_mode = ?, updated_at = ?
                         WHERE id = ?";
                    match pool {
                        sqlx::Either::Left(p) => sqlx::query(update_sql)
                            .bind(&name)
                            .bind(&adapter_type)
                            .bind(&base_url)
                            .bind(&default_model)
                            .bind(&status)
                            .bind(secret_val)
                            .bind(&provider_data_mode)
                            .bind(now)
                            .bind(pid)
                            .execute(p)
                            .await
                            .map_err(|e| AppError::internal(e.to_string(), request_id))?
                            .rows_affected(),
                        sqlx::Either::Right(p) => sqlx::query(update_sql)
                            .bind(&name)
                            .bind(&adapter_type)
                            .bind(&base_url)
                            .bind(&default_model)
                            .bind(&status)
                            .bind(secret_val)
                            .bind(&provider_data_mode)
                            .bind(now)
                            .bind(pid)
                            .execute(p)
                            .await
                            .map_err(|e| AppError::internal(e.to_string(), request_id))?
                            .rows_affected(),
                    }
                }
                None => {
                    let provider_id = uuid::Uuid::now_v7().to_string();
                    match pool {
                        sqlx::Either::Left(p) => sqlx::query(
                            "INSERT INTO ai_providers
                                 (id, name, adapter_type, base_url, api_type, default_model, status, secret_configured, data_mode, created_at, updated_at)
                             VALUES (?, ?, ?, ?, 'chat', ?, ?, ?, ?, ?, ?)
                             ON CONFLICT(name) DO UPDATE SET base_url = excluded.base_url, default_model = excluded.default_model,
                                 status = excluded.status, secret_configured = CASE WHEN excluded.secret_configured = 1 THEN 1 ELSE ai_providers.secret_configured END,
                                 data_mode = excluded.data_mode, updated_at = excluded.updated_at",
                        )
                        .bind(&provider_id)
                        .bind(&name)
                        .bind(&adapter_type)
                        .bind(&base_url)
                        .bind(&default_model)
                        .bind(&status)
                        .bind(secret_val)
                        .bind(&provider_data_mode)
                        .bind(now)
                        .bind(now)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?
                        .rows_affected(),
                        sqlx::Either::Right(p) => sqlx::query(
                            "INSERT INTO ai_providers
                                 (id, name, adapter_type, base_url, api_type, default_model, status, secret_configured, data_mode, created_at, updated_at)
                             VALUES (?, ?, ?, ?, 'chat', ?, ?, ?, ?, ?, ?)
                             ON DUPLICATE KEY UPDATE base_url = VALUES(base_url), default_model = VALUES(default_model),
                                 status = VALUES(status), secret_configured = CASE WHEN VALUES(secret_configured) = 1 THEN 1 ELSE ai_providers.secret_configured END,
                                 data_mode = VALUES(data_mode), updated_at = VALUES(updated_at)",
                        )
                        .bind(&provider_id)
                        .bind(&name)
                        .bind(&adapter_type)
                        .bind(&base_url)
                        .bind(&default_model)
                        .bind(&status)
                        .bind(secret_val)
                        .bind(&provider_data_mode)
                        .bind(now)
                        .bind(now)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?
                        .rows_affected(),
                    }
                }
            };
            provider_upserted =
                Some(json!({ "provider": name, "upserted": affected, "status": status }));
            changed.push("provider".to_string());
        }
    }

    AuditEntry::user_action(&user.id, "ai.config.update")
        .with_target("ai_site_config", "singleton")
        .with_reason(&reason)
        .with_policy_version(crate::authz::decision::AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "changed": changed }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    // 响应与 GET 同形（前端 PATCH 后直接用返回的配置刷新视图）。
    let mut config = ai_admin_config_json(pool, request_id).await?;
    if let Some(provider) = provider_upserted {
        config["provider_upsert"] = provider;
    }
    if let Some(deleted) = provider_deleted {
        config["provider_deleted"] = deleted;
    }
    Ok(Json(config))
}

/// POST /api/v1/admin/ai/providers/test — 固定脱敏探针（不接收用户正文）。
async fn test_ai_provider(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_ai_providers_test";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let url = body
        .get("base_url")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("base_url required", request_id, None))?;
    let policy = crate::ai::gateway::EgressPolicy::default();
    match policy.validate_endpoint(url) {
        Ok(_) => Ok(Json(
            json!({ "ok": true, "backend": "ai_provider", "detail": "egress policy ok" }),
        )),
        Err(e) => Ok(Json(
            json!({ "ok": false, "backend": "ai_provider", "error_class": e.code() }),
        )),
    }
}

/// GET /api/v1/admin/ai/tasks — 管理端任务列表（安全投影，不扩大可见性）。
async fn list_ai_tasks(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_ai_tasks";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let items: Vec<Value> = match pool {
        sqlx::Either::Left(p) => sqlx::query(
            "SELECT id, task_type, target_type, target_id, user_id, status, attempt, error_class, created_at
             FROM ai_tasks ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(ai_task_row)
        .collect(),
        sqlx::Either::Right(p) => sqlx::query(
            "SELECT id, task_type, target_type, target_id, user_id, status, attempt, error_class, created_at
             FROM ai_tasks ORDER BY created_at DESC LIMIT 100",
        )
        .fetch_all(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .iter()
        .map(ai_task_row_mysql)
        .collect(),
    };
    Ok(Json(json!({ "items": items })))
}

fn ai_task_row(r: &sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "task_type": r.get::<String,_>("task_type"),
        "target_type": r.get::<String,_>("target_type"),
        "target_id": r.get::<String,_>("target_id"),
        "user_id": r.get::<String,_>("user_id"),
        "status": r.get::<String,_>("status"),
        "attempt": r.get::<i64,_>("attempt"),
        "error_class": r.get::<Option<String>,_>("error_class"),
        "created_at": r.get::<i64,_>("created_at"),
    })
}

fn ai_task_row_mysql(r: &sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": r.get::<String,_>("id"),
        "task_type": r.get::<String,_>("task_type"),
        "target_type": r.get::<String,_>("target_type"),
        "target_id": r.get::<String,_>("target_id"),
        "user_id": r.get::<String,_>("user_id"),
        "status": r.get::<String,_>("status"),
        "attempt": r.get::<i64,_>("attempt"),
        "error_class": r.get::<Option<String>,_>("error_class"),
        "created_at": r.get::<i64,_>("created_at"),
    })
}

/// POST /api/v1/admin/ai/tasks/{id}/cancel — 管理端取消任意任务。
async fn cancel_ai_task_admin(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_ai_tasks_id_cancel";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let now = crate::ai::tasks::now();
    let affected = match pool {
        sqlx::Either::Left(p) => sqlx::query(
            "UPDATE ai_tasks SET status = 'cancelled', finished_at = ?, updated_at = ?
                 WHERE id = ? AND status IN ('queued','running','retry_wait')",
        )
        .bind(now)
        .bind(now)
        .bind(&id)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
        sqlx::Either::Right(p) => sqlx::query(
            "UPDATE ai_tasks SET status = 'cancelled', finished_at = ?, updated_at = ?
                 WHERE id = ? AND status IN ('queued','running','retry_wait')",
        )
        .bind(now)
        .bind(now)
        .bind(&id)
        .execute(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
    };
    Ok(Json(json!({ "id": id, "cancelled": affected })))
}

/// POST /api/v1/admin/ai/tasks/{id}/retry — 重置 retry_wait/dead 到 queued。
async fn retry_ai_task(
    State(state): State<AppState>,
    auth: crate::auth::session::AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_ai_tasks_id_retry";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let now = crate::ai::tasks::now();
    let affected = match pool {
        sqlx::Either::Left(p) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'queued', attempt = 0, error_class = NULL, error_message_safe = NULL, finished_at = NULL, updated_at = ?
                 WHERE id = ? AND status IN ('retry_wait','dead')",
            )
            .bind(now)
            .bind(&id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected()
        }
        sqlx::Either::Right(p) => {
            sqlx::query(
                "UPDATE ai_tasks SET status = 'queued', attempt = 0, error_class = NULL, error_message_safe = NULL, finished_at = NULL, updated_at = ?
                 WHERE id = ? AND status IN ('retry_wait','dead')",
            )
            .bind(now)
            .bind(&id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected()
        }
    };
    Ok(Json(json!({ "id": id, "retried": affected })))
}
/// GET /api/v1/admin/video/policies — 全部 Provider 策略（含缺省关闭态）。
async fn list_video_policies(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_video_policies";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let mut items = Vec::new();
    for provider in Provider::ALL {
        let policy = load_policy(pool, provider)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        items.push(video_policy_json(&policy));
    }
    Ok(Json(json!({ "policies": items })))
}

/// POST /api/v1/admin/video/policies/test — 策略自检（离线分类 + 策略门）。
async fn test_video_policy(
    State(state): State<AppState>,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_video_policies_test";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let provider = body
        .get("provider")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("provider required", request_id, None))?;
    let provider = Provider::parse(provider)
        .ok_or_else(|| AppError::bad_request("unknown provider", request_id, None))?;
    let policy = load_policy(pool, provider)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let mut result = json!({
        "ok": false,
        "backend": "video_provider",
        "provider": provider.as_str(),
        "policy_enabled": policy.enabled,
    });
    match body.get("source_url").and_then(Value::as_str) {
        Some(url) => match crate::video::classify(url) {
            Ok(c) => {
                if !policy.enabled {
                    result["error_class"] = json!("video_provider_disabled");
                } else if !crate::video::is_allowed_host(&c.host, &policy.allow_hosts) {
                    result["error_class"] = json!("video_provider_host_not_allowed");
                } else {
                    result["ok"] = json!(true);
                    result["classified"] = json!({
                        "provider": c.provider.as_str(),
                        "host": c.host,
                        "media_type": c.media_type,
                    });
                }
            }
            Err(e) => {
                result["error_class"] = json!(e.code());
            }
        },
        None => {
            result["ok"] = json!(policy.enabled);
        }
    }
    Ok(Json(result))
}

/// GET /api/v1/admin/video/policies/{provider} — 单 Provider 策略。
async fn get_video_policy(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_video_policies_provider_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let provider = Provider::parse(&provider)
        .ok_or_else(|| AppError::bad_request("unknown provider", request_id, None))?;
    let policy = load_policy(pool, provider)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(Json(json!({ "policy": video_policy_json(&policy) })))
}

/// PATCH /api/v1/admin/video/policies/{provider} — 更新策略（If-Match + reason
/// + 审计；写入后触发历史引用重检）。
async fn update_video_policy(
    State(state): State<AppState>,
    Path(provider): Path<String>,
    headers: HeaderMap,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "patch_admin_video_policies_provider_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let provider = Provider::parse(&provider)
        .ok_or_else(|| AppError::bad_request("unknown provider", request_id, None))?;
    let if_match = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .parse::<i64>()
        .map_err(|_| AppError::bad_request("If-Match must be an integer", request_id, None))?;

    let (policy, downgraded) =
        update_provider_policy(pool, provider, &body, if_match, now_millis())
            .await
            .map_err(|e| video_policy_error_to_app(e, request_id))?;

    let audit = AuditEntry::user_action(&user.id, "video_policy.update")
        .with_target("video_provider", provider.as_str())
        .with_reason(&reason)
        .with_metadata(json!({
            "enabled": policy.enabled,
            "version": policy.version,
            "rechecked_references": downgraded,
        }));
    let _ = audit.record(pool).await;

    Ok(Json(json!({
        "policy": video_policy_json(&policy),
        "rechecked_references": downgraded,
    })))
}

fn video_policy_json(p: &crate::video::VideoPolicy) -> Value {
    json!({
        "provider": p.provider.as_str(),
        "enabled": p.enabled,
        "allow_hosts": p.allow_hosts,
        "max_redirects": p.max_redirects,
        "max_response_bytes": p.max_response_bytes,
        "max_playlist_depth": p.max_playlist_depth,
        "max_segments": p.max_segments,
        "max_duration_ms": p.max_duration_ms,
        "config": p.config,
        "version": p.version,
        "updated_at": p.updated_at,
    })
}

fn video_policy_error_to_app(e: VideoError, request_id: &str) -> AppError {
    match e {
        VideoError::VersionConflict { .. } => {
            AppError::version_conflict("video policy version conflict", request_id)
        }
        VideoError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        VideoError::Classify(_) => {
            AppError::bad_request("invalid policy host list", request_id, None)
        }
        _ => AppError::internal(e.code(), request_id),
    }
}

/// 高风险管理操作前置：会话必须处于近期认证窗口（M02-MFA-07 step-up，
/// M11-CONSENT-05 recent-auth）。会话缺失/过期/超出窗口 → 拒绝（fail closed）。
/// `pub(crate)` 供 admin_plugins/admin 各域路由复用。
pub(crate) async fn require_recent_auth(
    state: &AppState,
    jar: &CookieJar,
    request_id: &str,
) -> Result<String, AppError> {
    let Some(pool) = state.db.as_deref() else {
        return Err(AppError::internal("database not configured", request_id));
    };
    let Some(token) = jar
        .get(crate::auth::session::SESSION_COOKIE_NAME)
        .map(|c| c.value().to_string())
    else {
        return Err(AppError::unauthorized(
            "authentication required",
            request_id,
        ));
    };
    let required = crate::auth::session::is_step_up_required_for_session(
        pool,
        &token,
        state.config.step_up_window_secs,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if required {
        return Err(AppError::step_up_required(request_id));
    }
    Ok(token)
}

/// OIDC 服务层错误 → 业务 Problem 响应（admin 域走业务格式）。
fn oidc_admin_error(e: crate::oidc::OidcError, request_id: &str) -> AppError {
    match e {
        crate::oidc::OidcError::InvalidRequest(d) => AppError::bad_request(d, request_id, None),
        crate::oidc::OidcError::NotFound(d) => AppError::not_found(d, request_id),
        crate::oidc::OidcError::AccessDenied(d) => AppError::forbidden(d, request_id),
        other => AppError::internal(other.to_string(), request_id),
    }
}

fn string_array(value: Option<&Value>) -> Option<Vec<String>> {
    value.and_then(Value::as_array).map(|items| {
        items
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    })
}

/// GET /api/v1/admin/oauth-clients — 分页列出 Client（admin.manage）。
async fn list_oauth_clients(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "listAdminOAuthClients";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let after = params.get("after").map(String::as_str);
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30);
    let (clients, next_cursor) = crate::oidc::clients::list_clients(pool, after, limit)
        .await
        .map_err(|e| oidc_admin_error(e, request_id))?;
    Ok(Json(json!({
        "clients": clients.iter().map(crate::oidc::clients::client_admin_view).collect::<Vec<_>>(),
        "next_cursor": next_cursor,
    })))
}

/// POST /api/v1/admin/oauth-clients — 创建 Client（admin.manage + reason +
/// recent-auth + 精确 URI 校验 + 审计；confidential secret 只显示一次）。
async fn create_oauth_client(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "createAdminOAuthClient";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;

    let name = body
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let client_type = body
        .get("client_type")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let redirect_uris = string_array(body.get("redirect_uris")).unwrap_or_default();
    let post_logout_uris = string_array(body.get("post_logout_uris")).unwrap_or_default();
    let scopes = string_array(body.get("scopes")).unwrap_or_default();

    let input = crate::oidc::clients::ClientCreateInput {
        name,
        client_type,
        redirect_uris,
        post_logout_uris,
        scopes,
    };
    let (client, secret) =
        crate::oidc::clients::create_client(pool, &input, &user.id, now_millis())
            .await
            .map_err(|e| oidc_admin_error(e, request_id))?;

    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    AuditEntry::user_action(&user.id, "oauth_client.create")
        .with_target("oauth_client", &client.id)
        .with_reason(&reason)
        .with_policy_version(crate::authz::decision::AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let mut view = crate::oidc::clients::client_admin_view(&client);
    if let Some(secret) = secret {
        // 明文 secret 仅在创建时返回一次。
        view["secret"] = json!(secret);
    }
    Ok((StatusCode::CREATED, Json(json!({ "client": view }))))
}

/// GET /api/v1/admin/oauth-clients/{id} — 单个 Client（admin.manage）。
async fn get_oauth_client(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "getAdminOAuthClient";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let client = crate::oidc::clients::fetch_client_by_internal_id(pool, &id)
        .await
        .map_err(|e| oidc_admin_error(e, request_id))?
        .ok_or_else(|| AppError::not_found("oauth client not found", request_id))?;
    Ok(Json(json!({
        "client": crate::oidc::clients::client_admin_view(&client),
    })))
}

/// PATCH /api/v1/admin/oauth-clients/{id} — 更新/停用 Client（admin.manage +
/// reason + recent-auth + If-Match 乐观锁 + 精确 URI 校验 + 审计）。
async fn update_oauth_client(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "updateAdminOAuthClient";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;

    let client = crate::oidc::clients::fetch_client_by_internal_id(pool, &id)
        .await
        .map_err(|e| oidc_admin_error(e, request_id))?
        .ok_or_else(|| AppError::not_found("oauth client not found", request_id))?;

    // If-Match 版本守卫（M11-CONSENT-05 版本化更新）。
    if let Some(if_match) = headers.get("if-match").and_then(|v| v.to_str().ok()) {
        let expected = if_match
            .trim()
            .parse::<i64>()
            .map_err(|_| AppError::bad_request("If-Match must be an integer", request_id, None))?;
        if expected != client.version {
            return Err(AppError::version_conflict(
                "oauth client version conflict",
                request_id,
            ));
        }
    }

    let input = crate::oidc::clients::ClientUpdateInput {
        name: body.get("name").and_then(Value::as_str).map(str::to_string),
        client_type: body
            .get("client_type")
            .and_then(Value::as_str)
            .map(str::to_string),
        redirect_uris: string_array(body.get("redirect_uris")),
        post_logout_uris: string_array(body.get("post_logout_uris")),
        scopes: string_array(body.get("scopes")),
        status: body
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_string),
        reset_secret: body.get("reset_secret").and_then(Value::as_bool),
    };

    crate::oidc::clients::update_client(pool, &client, &input, &user.id, now_millis())
        .await
        .map_err(|e| oidc_admin_error(e, request_id))?;

    // secret 重置：仅 Confidential，仅回传一次。
    let mut view = {
        let updated = crate::oidc::clients::fetch_client_by_internal_id(pool, &id)
            .await
            .map_err(|e| oidc_admin_error(e, request_id))?
            .ok_or_else(|| AppError::not_found("oauth client not found", request_id))?;
        crate::oidc::clients::client_admin_view(&updated)
    };
    if input.reset_secret == Some(true) && client.client_type == "confidential" {
        let secret = crate::auth::token::generate_token();
        crate::oidc::clients::update_client_secret(
            pool,
            &client.id,
            &hash_token(&secret),
            &user.id,
            now_millis(),
        )
        .await
        .map_err(|e| oidc_admin_error(e, request_id))?;
        view["secret"] = json!(secret);
        AuditEntry::user_action(&user.id, "oauth_client.secret_reset")
            .with_target("oauth_client", &client.id)
            .with_reason(&reason)
            .record(pool)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    }

    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    AuditEntry::user_action(&user.id, "oauth_client.update")
        .with_target("oauth_client", &client.id)
        .with_reason(&reason)
        .with_policy_version(crate::authz::decision::AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(Json(json!({ "client": view })))
}
fn marketplace_err(e: crate::marketplace::MarketplaceError, request_id: &str) -> AppError {
    crate::marketplace::marketplace_error_to_app(e, request_id)
}

/// GET /api/v1/admin/marketplace/clients — 列出 Marketplace Client
/// （admin.manage；含 scope 与商户余额摘要）。
async fn list_marketplace_clients(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_marketplace_clients";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let after = params.get("after").map(String::as_str);
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30);
    let (clients, next_cursor) = crate::marketplace::clients::list_clients(pool, after, limit)
        .await
        .map_err(|e| marketplace_err(e, request_id))?;
    let mut items = Vec::new();
    for view in clients {
        let client_id = view["id"].as_str().unwrap_or("").to_string();
        let scopes = crate::marketplace::clients::list_scopes(pool, &client_id)
            .await
            .map_err(|e| marketplace_err(e, request_id))?;
        let balance = crate::marketplace::balance::balance_view(pool, &client_id).await;
        let mut v = view;
        v["scopes"] = json!(scopes);
        v["balance"] = balance.unwrap_or(json!({"error": "no_merchant_account"}));
        items.push(v);
    }
    Ok(Json(
        json!({ "clients": items, "next_cursor": next_cursor }),
    ))
}

/// GET /api/v1/admin/marketplace/clients/{id} — 单个 Client（admin.manage）。
async fn get_marketplace_client(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_marketplace_clients_id_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let client = match crate::marketplace::clients::fetch_client_by_client_id(pool, &id)
        .await
        .map_err(|e| marketplace_err(e, request_id))?
    {
        Some(c) => c,
        None => crate::marketplace::clients::fetch_client_by_internal_id(pool, &id)
            .await
            .map_err(|e| marketplace_err(e, request_id))?
            .ok_or_else(|| AppError::not_found("marketplace client not found", request_id))?,
    };
    let mut view = crate::marketplace::clients::client_view_json(&client);
    view["scopes"] = json!(crate::marketplace::clients::list_scopes(pool, &client.id)
        .await
        .map_err(|e| marketplace_err(e, request_id))?);
    view["balance"] = crate::marketplace::balance::balance_view(pool, &client.id)
        .await
        .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(view))
}

/// PATCH /api/v1/admin/marketplace/clients/{id} — 注册/更新 Client、
/// 逐 scope 审批、状态切换（admin.manage + reason + recent-auth + If-Match +
/// 审计）。
async fn update_marketplace_client(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "patch_admin_marketplace_clients_id_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let expected_version = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1);
    let client = crate::marketplace::clients::upsert_client(
        pool,
        &id,
        &body,
        expected_version,
        &user.id,
        &user.username,
        now_millis(),
    )
    .await
    .map_err(|e| marketplace_err(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    AuditEntry::user_action(&user.id, "marketplace.client.update")
        .with_target("client", &client.client_id)
        .with_reason(&reason)
        .with_metadata(json!({ "status": client.status, "version": client.version }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let mut view = crate::marketplace::clients::client_view_json(&client);
    view["scopes"] = json!(crate::marketplace::clients::list_scopes(pool, &client.id)
        .await
        .map_err(|e| marketplace_err(e, request_id))?);
    Ok(Json(view))
}

/// POST /api/v1/admin/marketplace/clients/{id}/rotate-webhook-secret —
/// 轮换 Webhook Secret（明文只返回一次）。
async fn rotate_webhook_secret(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_marketplace_clients_id_rotate_webhook_secret";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let (client, secret) = crate::marketplace::clients::rotate_webhook_secret(
        pool,
        &id,
        &user.id,
        &reason,
        &state.config.marketplace_webhook_encryption_key,
        now_millis(),
    )
    .await
    .map_err(|e| marketplace_err(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let mut view = crate::marketplace::clients::client_view_json(&client);
    view["webhook_secret"] = json!(secret);
    Ok(Json(view))
}

/// POST /api/v1/admin/marketplace/clients/{id}/emergency-disable —
/// 紧急停用（立即阻止新 Intent/confirm/refund；历史保留）。
async fn emergency_disable_client(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_marketplace_clients_id_emergency_disable";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let expected_version = headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(1);
    let client = crate::marketplace::clients::emergency_disable(
        pool,
        &id,
        &reason,
        &user.id,
        &user.username,
        expected_version,
        now_millis(),
    )
    .await
    .map_err(|e| marketplace_err(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    Ok(Json(crate::marketplace::clients::client_view_json(&client)))
}

/// GET /api/v1/admin/marketplace/offers — 列出报价（可传 client_id 过滤）。
async fn list_marketplace_offers(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_marketplace_offers";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let items = match params.get("client_id") {
        Some(client_id) => {
            let c = crate::marketplace::clients::fetch_client_by_client_id(pool, client_id)
                .await
                .map_err(|e| marketplace_err(e, request_id))?;
            match c {
                Some(c) => crate::marketplace::offers::list_offers_for_client(pool, &c.id, true)
                    .await
                    .map_err(|e| marketplace_err(e, request_id))?,
                None => Vec::new(),
            }
        }
        None => {
            // 全部 Client 的 Offer（管理端跨 Client 视图）。
            let (clients, _) = crate::marketplace::clients::list_clients(pool, None, 500)
                .await
                .map_err(|e| marketplace_err(e, request_id))?;
            let mut all = Vec::new();
            for c in clients {
                let id = c["id"].as_str().unwrap_or("").to_string();
                let mut offers =
                    crate::marketplace::offers::list_offers_for_client(pool, &id, true)
                        .await
                        .map_err(|e| marketplace_err(e, request_id))?;
                all.append(&mut offers);
            }
            all
        }
    };
    Ok(Json(json!({ "offers": items })))
}

/// GET /api/v1/admin/marketplace/transactions — 交易视图（Purchase + Refund
/// + 商户余额 + 对账记录）。
async fn list_marketplace_transactions(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_marketplace_transactions";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let client_id = params.get("client_id").map(String::as_str);
    let after = params.get("after").map(String::as_str);
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30);
    let purchases = if let Some(c) = client_id {
        let client = crate::marketplace::clients::fetch_client_by_client_id(pool, c)
            .await
            .map_err(|e| marketplace_err(e, request_id))?;
        match client {
            Some(client) => crate::marketplace::checkout::list_purchases(
                pool,
                None,
                Some(&client.id),
                after,
                limit,
            )
            .await
            .map_err(|e| marketplace_err(e, request_id))?,
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let refunds = if let Some(c) = client_id {
        let client = crate::marketplace::clients::fetch_client_by_client_id(pool, c)
            .await
            .map_err(|e| marketplace_err(e, request_id))?;
        match client {
            Some(client) => crate::marketplace::refunds::list_refunds(
                pool,
                None,
                Some(&client.id),
                after,
                limit,
            )
            .await
            .map_err(|e| marketplace_err(e, request_id))?,
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    let balances = if let Some(c) = client_id {
        let client = crate::marketplace::clients::fetch_client_by_client_id(pool, c)
            .await
            .map_err(|e| marketplace_err(e, request_id))?;
        match client {
            Some(client) => vec![crate::marketplace::balance::balance_view(pool, &client.id)
                .await
                .map_err(|e| marketplace_err(e, request_id))?],
            None => Vec::new(),
        }
    } else {
        Vec::new()
    };
    Ok(Json(json!({
        "purchases": purchases,
        "refunds": refunds,
        "balances": balances,
    })))
}

/// GET /api/v1/admin/marketplace/webhook-deliveries — 投递记录列表。
async fn list_webhook_deliveries(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_marketplace_webhook_deliveries";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let client_id = params.get("client_id").map(String::as_str);
    let status = params.get("status").map(String::as_str);
    let after = params.get("after").map(String::as_str);
    let limit = params
        .get("limit")
        .and_then(|v| v.parse::<i64>().ok())
        .unwrap_or(30);
    let rows = crate::marketplace::webhooks::list_deliveries(pool, client_id, status, after, limit)
        .await
        .map_err(|e| marketplace_err(e, request_id))?;
    Ok(Json(json!({
        "deliveries": rows.iter().map(crate::marketplace::webhooks::delivery_json).collect::<Vec<_>>()
    })))
}

/// POST /api/v1/admin/marketplace/webhook-deliveries/{id}/replay — 手动重放
/// （保持原 event_id）。
async fn replay_webhook_delivery(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_marketplace_webhook_deliveries_id_replay";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let _token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let view = crate::marketplace::webhooks::replay_delivery(
        pool,
        &id,
        None,
        &state.config.marketplace_webhook_encryption_key,
        &crate::marketplace::webhooks::UnavailableWebhookClient,
        now_millis(),
    )
    .await
    .map_err(|e| marketplace_err(e, request_id))?;
    let _ = AuditEntry::user_action(&user.id, "marketplace.webhook.replay")
        .with_target("delivery", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(Json(view))
}

/// POST /api/v1/admin/marketplace/reconciliation/run — 增量对账。
async fn run_reconciliation(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_marketplace_reconciliation_run";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let _token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let client_key = body
        .get("client_id")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("client_id required", request_id, None))?;
    let after_cursor = body
        .get("after_cursor")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let client = crate::marketplace::clients::fetch_client_by_client_id(pool, client_key)
        .await
        .map_err(|e| marketplace_err(e, request_id))?
        .ok_or_else(|| AppError::not_found("marketplace client not found", request_id))?;
    let view = crate::marketplace::reconcile::run_reconciliation(
        pool,
        &client.id,
        after_cursor,
        now_millis(),
    )
    .await
    .map_err(|e| marketplace_err(e, request_id))?;
    let _ = AuditEntry::user_action(&user.id, "marketplace.reconciliation.run")
        .with_target("client", &client.client_id)
        .with_reason(&reason)
        .with_metadata(json!({ "status": view["status"] }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(Json(view))
}

/// POST /api/v1/admin/marketplace/refunds/{id}/retry — 处理 requested 退款
/// （管理员补偿/冲正后重试）。
async fn retry_requested_refund(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    Path(id): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "post_admin_marketplace_refunds_id_retry";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let _token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let view =
        crate::marketplace::refunds::retry_requested_refund(pool, &id, &user.id, now_millis())
            .await
            .map_err(|e| marketplace_err(e, request_id))?;
    let _ = AuditEntry::user_action(&user.id, "marketplace.refund.retry")
        .with_target("refund", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(Json(view))
}
// ────────────────────────── 主题管理（M13-THEME）──────────────────────────

/// GET /api/v1/admin/themes — 全部主题（含禁用/隔离态；admin.manage）。
async fn list_themes(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_admin_themes";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let themes = crate::theme::list_themes(pool)
        .await
        .map_err(|e| crate::routes::themes::theme_error_to_app(e, request_id))?;
    Ok(Json(json!({
        "themes": themes.iter().map(|t| t.json()).collect::<Vec<_>>(),
        "default": crate::theme::DEFAULT_THEME_NAME,
        "schema_version": crate::theme::THEME_SCHEMA_VERSION,
    })))
}

/// POST /api/v1/admin/themes/data-packages — 上传数据型主题（admin.manage +
/// reason + recent-auth + 审计；M13-THEME-06 走附件安全处理语义：大小限制、
/// 解压/内容扫描、版本校验、隔离状态 disabled）。
async fn upload_theme_package(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<(StatusCode, Json<Value>), AppError> {
    let request_id = "post_admin_themes_data_packages";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;

    let raw = serde_json::to_vec(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    if raw.len() > crate::theme::MAX_PACKAGE_BYTES {
        return Err(AppError::bad_request(
            "theme package exceeds size limit",
            request_id,
            None,
        ));
    }
    let installed = crate::theme::upload_theme_package(pool, &body, &user.id)
        .await
        .map_err(|e| crate::routes::themes::theme_error_to_app(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = AuditEntry::user_action(&user.id, "theme.upload")
        .with_target("theme", &installed.name)
        .with_reason(&reason)
        .with_metadata(json!({ "status": "disabled", "revision": 1 }))
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok((
        StatusCode::CREATED,
        Json(json!({ "theme": installed.json() })),
    ))
}

/// PUT /api/v1/admin/themes/default — 设置站点默认主题（激活；reason + 审计）。
async fn set_default_theme(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "put_admin_themes_default";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| AppError::bad_request("name required", request_id, None))?
        .trim()
        .to_string();
    let updated = crate::theme::set_default_theme(pool, &name, &user.id, &reason)
        .await
        .map_err(|e| crate::routes::themes::theme_error_to_app(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = AuditEntry::user_action(&user.id, "theme.default.update")
        .with_target("theme", &name)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(Json(json!({ "theme": updated.json() })))
}

/// DELETE /api/v1/admin/themes/{name} — 删除主题（内置 default 与当前站点
/// 默认不可删除；reason + 审计）。
async fn delete_theme(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    Path(name): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    let request_id = "delete_admin_themes_name_";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_admin(pool, &user.id, request_id).await?;
    let token = require_recent_auth(&state, &jar, request_id).await?;
    let reason = required_reason(&body, request_id)?;
    crate::theme::delete_theme(pool, &name, &user.id, &reason)
        .await
        .map_err(|e| crate::routes::themes::theme_error_to_app(e, request_id))?;
    let _ = crate::auth::session::mark_step_up(pool, &token).await;
    let _ = AuditEntry::user_action(&user.id, "theme.delete")
        .with_target("theme", &name)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await;
    Ok(Json(json!({ "deleted": name })))
}

/// PATCH /api/v1/admin/themes/{name}/settings — 更新 Token 设置（closed schema
/// 校验 + 新修订 + If-Match revision + reason + recent-auth + 审计；
/// 变更立即提升 theme_revision，SSR/浏览器/缓存/偏好同步）。
async fn update_theme_settings(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    headers: HeaderMap,
    Path(name): Path<String>,
    axum::Json(body): axum::Json<Value>,
) -> Result<Json<Value>, AppError> {
    #[allow(clippy::result_large_err)] // AppError 为统一错误类型
    {
        let request_id = "patch_admin_themes_name_settings";
        let user = auth.require_auth(request_id)?;
        let pool = state
            .db
            .as_deref()
            .ok_or_else(|| AppError::internal("database not configured", request_id))?;
        require_admin(pool, &user.id, request_id).await?;
        let token = require_recent_auth(&state, &jar, request_id).await?;
        let reason = required_reason(&body, request_id)?;
        let expected = headers
            .get("if-match")
            .and_then(|v| v.to_str().ok())
            .map(|v| {
                v.trim().parse::<i64>().map_err(|_| {
                    AppError::bad_request(
                        "If-Match must be the current revision integer",
                        request_id,
                        None,
                    )
                })
            })
            .transpose()?;
        let tokens = body
            .get("tokens")
            .ok_or_else(|| AppError::bad_request("tokens required", request_id, None))?;
        let updated =
            crate::theme::update_theme_settings(pool, &name, tokens, &user.id, &reason, expected)
                .await
                .map_err(|e| crate::routes::themes::theme_error_to_app(e, request_id))?;
        let _ = crate::auth::session::mark_step_up(pool, &token).await;
        let _ = AuditEntry::user_action(&user.id, "theme.settings.update")
            .with_target("theme", &name)
            .with_reason(&reason)
            .with_metadata(json!({ "revision": updated.revision }))
            .with_policy_version(AUTHZ_POLICY_VERSION)
            .record(pool)
            .await;
        Ok(Json(json!({ "theme": updated.json() })))
    }
}
