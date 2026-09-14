//! 经济与个人域扩展路由（GAP-FIX 管理域 Part B）。
//!
//! 覆盖端点（均属 DOCUMENTED_NON_CONTRACT 豁免清单，模式同 admin_ext）：
//! - 积分流水 `GET /api/v1/admin/points/ledger?username=&asset=&kind=&from=&to=`
//!   （points.adjust；point_transactions × users/point_operations/currencies
//!   联查，created_at DESC keyset 分页）；
//! - 积分调整 `POST /api/v1/admin/points/adjust`（points.adjust；**复用
//!   economy ledger service 的 adjust 操作**（`apply_operation`，kind=
//!   Adjust）+ 幂等 begin_or_replay，不裸写 UPDATE；写审计 + 通知被调整用户）；
//! - 本人流水 `GET /api/v1/me/point-transactions`（登录）；
//! - 等级规则存档 CRUD `GET/PATCH /api/v1/admin/levels*` 与经验方案投影
//!   `GET /api/v1/admin/levels/scheme` 于 2026-09 移除（等级体系合并为
//!   LinuxDo 信任等级单轨：/admin/levels 页面改读 /admin/trust-levels 数据；
//!   0062 `level_rules` 与 0050 经验等级表仅作历史迁移兼容，运行时已退役；
//!   冻结的 PublicUser.level/visibility_level/商城 required_level 字段继续保留，
//!   其用户当前等级来源统一为 users.trust_level；等级附件配额
//!   GET/PATCH /api/v1/admin/levels/{id}/attachment-quota 保留于 admin_storage，
//!   档位键为 users.trust_level 0–4）；
//! - 我的处罚 `GET /api/v1/me/sanctions`（登录；读 sanctions 表——
//!   moderation_actions 是案件维度的审核动作日志（0042），0043 的 sanctions
//!   才是「用户被处罚」记录表（user_id/kind/reason/starts_at/ends_at），
//!   故按 sanctions 实现，expires_at 映射 ends_at）；
//! - 修改密码 `POST /api/v1/me/password`（校验当前密码 401
//!   code=invalid_current_password；新密码复用注册强度策略；成功后撤销
//!   除当前会话外的全部 session + security 通知，复用
//!   auth::security_notify::notify_password_changed）；
//! - OAuth 授权管理 `GET/DELETE /api/v1/me/oauth-grants[/{client_id}]`
//!   （登录；数据来源为 0055_oidc 的 oauth_consents（用户逐 Client × scope
//!   授权记录）+ oauth_clients（client_name）+ oauth_tokens（last_used_at
//!   聚合）。真实 OAuth 授权流的写入不在本批范围——端点提供查询/撤销；
//!   撤销同时作废该 Client 名下本人的活跃 token）；
//! - 附件管理 `GET /api/v1/admin/attachments`（storage.manage）与
//!   `DELETE /api/v1/admin/attachments/{id}`（软删 deleted_at + status
//!   ='deleted'，0048 attachments 实际列）；
//! - 下载交易 `GET /api/v1/admin/download-billing/transactions`
//!   （download_billing.manage；download_authorizations × users × attachments
//!   联查——本库无独立 download_transactions 表，扣费流水即
//!   download_authorizations（charged_amount 持久化），filename 取
//!   attachments.original_name，取不到为空串）；
//! - 标签合并 `POST /api/v1/admin/tags/{id}/merge`（tag.manage；post_tags
//!   关联行转移（冲突跳过）+ tags.usage_count 同步 + 源标签 status='merged'/
//!   停用。注意 search_documents.tags_json 由索引 Job 维护，合并后未触发
//!   全量重建——GET /posts 的 tag 筛选走 post_tags 实时联查不受影响，
//!   /search 的 tag 过滤在下次重建前为旧值）；
//! - 付费解锁 `POST /api/v1/posts/{id}/unlock`（登录；paid 策略帖子：
//!   已解锁幂等 200；余额不足 409 code=insufficient_funds；充足则事务内
//!   ledger consume（source_type='post_unlock'）+ content_access_grants
//!   purchase grant + 通知作者）。
//!
//! 写操作约定与 admin_ext 一致：管理写必填 reason（审计）、PATCH 用
//! If-Match、POST 幂等用 client_request_id（idempotency_records）；私有
//! 数据响应 `Cache-Control: private, no-store`。

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use axum_extra::extract::CookieJar;
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Either;

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::authz::decision::{DenyReason, AUTHZ_POLICY_VERSION};
use crate::authz::enforce::{authorize_action, denied_reason, deny_to_error};
use crate::economy::ledger::service::{
    apply_operation, apply_operation_in_mysql_tx, apply_operation_in_sqlite_tx, LedgerCommand,
    LedgerError, LedgerKind, CURRENCY_COIN,
};
use crate::error::AppError;
use crate::idempotency::{
    begin_or_replay, complete, mark_failed, request_hash, FailureCachePolicy, IdempotencyKey,
    IdempotencyOutcome,
};
use crate::outbox::now_millis;

/// 列表 keyset 游标上限。
const MAX_LIST_LIMIT: i64 = 100;
/// 写端点幂等记录保留窗口（24h，与其他写端点一致）。
const IDEMPOTENCY_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// 积分调整绝对值上限（GAP-FIX 契约 |amount| ≤ 1000000）。
const MAX_ADJUST_AMOUNT: i64 = 1_000_000;
/// 付费解锁定价上限（与 posts.price_coin 契约一致）。
const MAX_PRICE_COIN: i64 = 1000;

/// 经济与个人域扩展路由。
pub fn router() -> Router<AppState> {
    Router::new()
        // 积分（points.adjust）
        .route("/api/v1/admin/points/ledger", get(list_admin_points_ledger))
        .route("/api/v1/admin/points/adjust", post(admin_points_adjust))
        // 本人流水（登录）
        .route(
            "/api/v1/me/point-transactions",
            get(list_my_point_transactions),
        )
        // 我的处罚（登录；sanctions 表）
        .route("/api/v1/me/sanctions", get(list_my_sanctions))
        // 账号（登录）
        .route("/api/v1/me/password", post(change_my_password))
        .route("/api/v1/me/oauth-grants", get(list_my_oauth_grants))
        .route(
            "/api/v1/me/oauth-grants/{client_id}",
            axum::routing::delete(revoke_my_oauth_grant),
        )
        // 附件 / 下载计费（storage.manage / download_billing.manage）
        .route("/api/v1/admin/attachments", get(list_admin_attachments))
        .route(
            "/api/v1/admin/attachments/{id}",
            axum::routing::delete(delete_admin_attachment),
        )
        .route(
            "/api/v1/admin/download-billing/transactions",
            get(list_download_billing_transactions),
        )
        // 标签合并（tag.manage）
        .route("/api/v1/admin/tags/{id}/merge", post(merge_tag))
        // 付费解锁（登录）
        .route("/api/v1/posts/{id}/unlock", post(unlock_post))
}

// ─── 共享助手（模式同 admin_ext.rs） ────────────────────────────────────────

/// 领域权限门。
async fn require_perm(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    permission: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let decision = authorize_action(pool, user_id, permission, None, AUTHZ_POLICY_VERSION)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(deny_to_error(
            denied_reason(&decision).unwrap_or(DenyReason::DefaultDeny),
            request_id,
        ));
    }
    Ok(())
}

/// 私有数据响应统一 `Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 解析 keyset 游标（整数毫秒；空串/缺省 = 首页）。
#[allow(clippy::result_large_err)]
fn parse_after_cursor(raw: &Option<String>, request_id: &str) -> Result<Option<i64>, AppError> {
    match raw.as_deref() {
        None | Some("") => Ok(None),
        Some(text) => text.parse::<i64>().map(Some).map_err(|_| {
            AppError::bad_request("after must be an integer cursor", request_id, None)
        }),
    }
}

/// 解析时间范围参数（from/to，Unix 毫秒）。
#[allow(clippy::result_large_err)]
fn parse_time_bound(raw: &Option<String>, request_id: &str) -> Result<Option<i64>, AppError> {
    match raw.as_deref() {
        None | Some("") => Ok(None),
        Some(text) => text.parse::<i64>().map(Some).map_err(|_| {
            AppError::bad_request("from/to must be integer milliseconds", request_id, None)
        }),
    }
}

/// LIKE 通配符转义（`!` 转义符，三方言一致）。
fn like_escape(input: &str) -> String {
    input
        .replace('!', "!!")
        .replace('%', "!%")
        .replace('_', "!_")
}

/// 管理写操作必填 reason。
#[allow(clippy::result_large_err)]
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

/// 插入一条站内通知（type='system'；category 可指定）。
async fn insert_system_notification(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    title: &str,
    body: &str,
    link: Option<&str>,
    category: &str,
    request_id: &str,
) -> Result<(), AppError> {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let sql = "INSERT INTO notifications (id, user_id, type, title, body, link, is_read, created_at, category)
         VALUES (?, ?, 'system', ?, ?, ?, 0, ?, ?)";
    let result = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&id)
            .bind(user_id)
            .bind(title)
            .bind(body)
            .bind(link)
            .bind(now)
            .bind(category)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&id)
            .bind(user_id)
            .bind(title)
            .bind(body)
            .bind(link)
            .bind(now)
            .bind(category)
            .execute(p)
            .await
            .map(|_| ()),
    };
    result.map_err(|e| AppError::internal(e.to_string(), request_id))
}

/// B 币代码（coin/b_coin）→ currencies.id（0047 种子固定 UUID）。
#[allow(clippy::result_large_err)]
fn currency_id(code: &str, request_id: &str) -> Result<String, AppError> {
    match code {
        "coin" => Ok(CURRENCY_COIN.to_string()),
        other => Err(AppError::bad_request(
            format!("currency must be 'coin', got: {other}"),
            request_id,
            None,
        )),
    }
}

/// 账本错误 → AppError（adjust/unlock 共用）。
fn map_ledger_error(err: LedgerError, request_id: &str) -> AppError {
    match err {
        LedgerError::NotFound(msg) => AppError::not_found(msg, request_id),
        LedgerError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        LedgerError::InsufficientBalance => AppError::with_code(
            StatusCode::CONFLICT,
            "insufficient_funds",
            "Insufficient Funds",
            "coin balance is insufficient",
            request_id,
        ),
        LedgerError::IdempotencyConflict => {
            AppError::conflict("idempotency key reused with different request", request_id)
        }
        LedgerError::Forbidden(msg) => AppError::forbidden(msg, request_id),
        other => AppError::internal(other.to_string(), request_id),
    }
}

// ─── 积分流水（admin） ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PointsLedgerQuery {
    /// 用户名精确过滤（username_normalized）。
    #[serde(default)]
    username: Option<String>,
    /// 资产过滤：coin（currencies.code）。
    #[serde(default)]
    asset: Option<String>,
    /// 操作类型过滤（point_operations.kind，值域与账本 CHECK 一致）。
    #[serde(default)]
    kind: Option<String>,
    /// created_at 范围（Unix 毫秒，含端点）。
    #[serde(default)]
    from: Option<String>,
    #[serde(default)]
    to: Option<String>,
    /// keyset 游标：上一页最后一条 created_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_ledger_limit")]
    limit: i64,
}

fn default_ledger_limit() -> i64 {
    50
}

/// 管理流水行（point_transactions 联查投影）。
#[derive(sqlx::FromRow)]
struct AdminLedgerRow {
    id: String,
    username: String,
    kind: String,
    currency: String,
    amount: i64,
    balance_after: i64,
    source_type: Option<String>,
    memo: String,
    created_at: i64,
}

impl AdminLedgerRow {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "username": self.username,
            "kind": self.kind,
            "currency": self.currency,
            "amount": self.amount,
            "balance_after": self.balance_after,
            "source_type": self.source_type,
            "memo": self.memo,
            "created_at": self.created_at,
        })
    }
}

/// GET /api/v1/admin/points/ledger — 积分流水（points.adjust）。
async fn list_admin_points_ledger(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<PointsLedgerQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_admin_points_ledger";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "points.adjust", request_id).await?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let after = parse_after_cursor(&query.after, request_id)?;
    let from = parse_time_bound(&query.from, request_id)?;
    let to = parse_time_bound(&query.to, request_id)?;
    let username = query.username.filter(|s| !s.is_empty());
    let asset = query.asset.filter(|s| !s.is_empty());
    let kind = query.kind.filter(|s| !s.is_empty());

    if let Some(a) = &asset {
        if a != "coin" {
            return Err(AppError::bad_request(
                "asset must be 'coin'",
                request_id,
                None,
            ));
        }
    }
    if let Some(k) = &kind {
        if LedgerKind::parse(k).is_none() {
            return Err(AppError::bad_request(
                format!("kind must be a ledger operation kind, got: {k}"),
                request_id,
                None,
            ));
        }
    }

    // 动态 WHERE（条件与绑定顺序一致）。
    let mut sql = String::from(
        "SELECT t.id, u.username_normalized AS username, op.kind AS kind, c.code AS currency,
                t.delta_balance AS amount, t.balance_after AS balance_after,
                op.source_type AS source_type, op.memo AS memo, t.created_at AS created_at
         FROM point_transactions t
         JOIN users u ON u.id = t.user_id
         JOIN point_operations op ON op.id = t.operation_id
         JOIN currencies c ON c.id = t.currency_id
         WHERE c.code = 'coin'",
    );
    if username.is_some() {
        sql.push_str(" AND u.username_normalized = ?");
    }
    if asset.is_some() {
        sql.push_str(" AND c.code = ?");
    }
    if kind.is_some() {
        sql.push_str(" AND op.kind = ?");
    }
    if from.is_some() {
        sql.push_str(" AND t.created_at >= ?");
    }
    if to.is_some() {
        sql.push_str(" AND t.created_at <= ?");
    }
    if after.is_some() {
        sql.push_str(" AND t.created_at < ?");
    }
    sql.push_str(" ORDER BY t.created_at DESC, t.id DESC LIMIT ?");

    let fetch_limit = limit + 1;
    let rows: Vec<AdminLedgerRow> = match pool {
        Either::Left(p) => {
            let mut q = sqlx::query_as::<_, AdminLedgerRow>(&sql);
            if let Some(v) = &username {
                q = q.bind(v);
            }
            if let Some(v) = &asset {
                q = q.bind(v);
            }
            if let Some(v) = &kind {
                q = q.bind(v);
            }
            if let Some(v) = from {
                q = q.bind(v);
            }
            if let Some(v) = to {
                q = q.bind(v);
            }
            if let Some(v) = after {
                q = q.bind(v);
            }
            q.bind(fetch_limit).fetch_all(p).await
        }
        Either::Right(p) => {
            let mut q = sqlx::query_as::<_, AdminLedgerRow>(&sql);
            if let Some(v) = &username {
                q = q.bind(v);
            }
            if let Some(v) = &asset {
                q = q.bind(v);
            }
            if let Some(v) = &kind {
                q = q.bind(v);
            }
            if let Some(v) = from {
                q = q.bind(v);
            }
            if let Some(v) = to {
                q = q.bind(v);
            }
            if let Some(v) = after {
                q = q.bind(v);
            }
            q.bind(fetch_limit).fetch_all(p).await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = rows.len() as i64 > limit;
    let page: Vec<_> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| r.created_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let items: Vec<Value> = page.iter().map(|r| r.to_json()).collect();
    let resp = (
        StatusCode::OK,
        Json(json!({ "items": items, "next_cursor": next_cursor })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 积分调整（admin） ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct PointsAdjustRequest {
    username: String,
    currency: String,
    amount: i64,
    reason: String,
    client_request_id: String,
}

/// POST /api/v1/admin/points/adjust — 积分调整（points.adjust）。
///
/// 复用 economy ledger service 的 adjust 操作（`apply_operation`，kind=
/// Adjust）：幂等（ledger point_operations 唯一键 + 路由级
/// idempotency_records begin_or_replay 双保险）、余额快照、不可变流水；
/// **不裸写 UPDATE**。成功后写审计并给被调整用户插 type='system' 通知
/// （title「积分调整」）。
async fn admin_points_adjust(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "admin_points_adjust";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "points.adjust", request_id).await?;
    crate::routes::admin::require_recent_auth(&state, &jar, request_id).await?;

    let req: PointsAdjustRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    if req.amount == 0 {
        return Err(AppError::bad_request(
            "amount must not be zero",
            request_id,
            None,
        ));
    }
    if req.amount.unsigned_abs() > MAX_ADJUST_AMOUNT as u64 {
        return Err(AppError::bad_request(
            format!("|amount| must be <= {MAX_ADJUST_AMOUNT}"),
            request_id,
            None,
        ));
    }
    let reason = req.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required",
            request_id,
            None,
        ));
    }
    let crl_len = req.client_request_id.chars().count();
    if !(1..=200).contains(&crl_len) {
        return Err(AppError::bad_request(
            "client_request_id must be 1-200 characters",
            request_id,
            None,
        ));
    }
    let currency_id = currency_id(&req.currency, request_id)?;
    let username = req.username.trim().to_string();
    if username.is_empty() {
        return Err(AppError::bad_request(
            "username is required",
            request_id,
            None,
        ));
    }

    // 目标用户（不存在 → 404）。
    let target_id: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM users WHERE username_normalized = ?")
            .bind(&username)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => {
            sqlx::query_scalar("SELECT id FROM users WHERE username_normalized = ?")
                .bind(&username)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };
    let Some(target_id) = target_id else {
        return Err(AppError::not_found("user not found", request_id));
    };

    // 幂等门（request_hash 覆盖原始请求体；失败不缓存——余额不足等业务
    // 失败允许同 key 重试）。
    let hash = request_hash(&body);
    let idem_key = IdempotencyKey::new("admin.points.adjust", &req.client_request_id)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let outcome = begin_or_replay(
        pool,
        &idem_key,
        &hash,
        IDEMPOTENCY_TTL_MS,
        FailureCachePolicy::Retry,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match outcome {
        IdempotencyOutcome::Created { record_id } => {
            // 账本 adjust 操作（幂等 + 余额快照 + 不可变流水）。
            let cmd = LedgerCommand {
                idempotency_scope: "points.adjust".to_string(),
                idempotency_key: req.client_request_id.clone(),
                kind: LedgerKind::Adjust,
                actor_id: Some(user.id.clone()),
                user_id: target_id.clone(),
                currency_id: currency_id.clone(),
                delta_balance: req.amount,
                delta_frozen: 0,
                source_type: Some("admin_adjust".to_string()),
                source_id: None,
                memo: reason.clone(),
                reverses_operation_id: None,
            };
            let result = apply_operation(pool, cmd, now_millis())
                .await
                .map_err(|e| map_ledger_error(e, request_id))?;
            let balance = result
                .transactions
                .first()
                .map(|t| t.balance_after)
                .unwrap_or(0);

            AuditEntry::user_action(&user.id, "admin.points.adjust")
                .with_target("user", &target_id)
                .with_reason(&reason)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .with_metadata(json!({
                    "username": username,
                    "currency": req.currency,
                    "amount": req.amount,
                    "operation_id": result.operation_id,
                }))
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            // 通知被调整用户（best-effort：通知失败不回滚账本——资金变动
            // 已入账且可从流水追溯，通知属于提醒性质）。
            let currency_label = "金币";
            let sign = if req.amount > 0 { "+" } else { "" };
            let _ = insert_system_notification(
                pool,
                &target_id,
                "积分调整",
                &format!(
                    "管理员对你的{currency_label}进行了调整：{sign}{}。原因：{reason}",
                    req.amount
                ),
                Some("/me"),
                "system",
                request_id,
            )
            .await;

            complete(pool, &record_id, &result.operation_id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            let resp = (
                StatusCode::CREATED,
                Json(json!({
                    "username": username,
                    "currency": req.currency,
                    "amount": req.amount,
                    "balance": balance,
                })),
            )
                .into_response();
            Ok(private_no_store(resp))
        }
        IdempotencyOutcome::Replay { response_reference } => {
            // 同 key+摘要重放：按 operation_id 回读原调整结果。
            if let Some(op_id) = response_reference {
                if let Some(v) =
                    adjust_replay_response(pool, &op_id, &username, &req.currency, request_id)
                        .await?
                {
                    let resp = (StatusCode::CREATED, Json(v)).into_response();
                    return Ok(private_no_store(resp));
                }
            }
            Err(AppError::conflict(
                "idempotent replay but original adjustment not found",
                request_id,
            ))
        }
        IdempotencyOutcome::InProgress => Err(AppError::conflict(
            "request already in progress",
            request_id,
        )),
        IdempotencyOutcome::Conflict => Err(AppError::conflict(
            "idempotency key reused with different request",
            request_id,
        )),
        IdempotencyOutcome::Failed { .. } => Err(AppError::conflict(
            "previous attempt failed; retry with the same or a new idempotency key",
            request_id,
        )),
    }
}

/// 重放响应：按 operation_id 回读原流水的金额与余额快照。
async fn adjust_replay_response(
    pool: &crate::db::DatabasePool,
    operation_id: &str,
    username: &str,
    currency: &str,
    request_id: &str,
) -> Result<Option<Value>, AppError> {
    let sql = "SELECT t.delta_balance, t.balance_after
         FROM point_transactions t WHERE t.operation_id = ? LIMIT 1";
    let row: Option<(i64, i64)> = match pool {
        Either::Left(p) => sqlx::query_as(sql)
            .bind(operation_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(sql)
            .bind(operation_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    Ok(row.map(|(amount, balance)| {
        json!({
            "username": username,
            "currency": currency,
            "amount": amount,
            "balance": balance,
        })
    }))
}

// ─── 本人积分流水 ────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct MyTransactionsQuery {
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_ledger_limit")]
    limit: i64,
}

/// 本人流水行。
#[derive(sqlx::FromRow)]
struct MyTxRow {
    id: String,
    kind: String,
    currency: String,
    amount: i64,
    balance_after: i64,
    memo: String,
    created_at: i64,
}

impl MyTxRow {
    fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "kind": self.kind,
            "currency": self.currency,
            "amount": self.amount,
            "balance_after": self.balance_after,
            "memo": self.memo,
            "created_at": self.created_at,
        })
    }
}

/// GET /api/v1/me/point-transactions — 本人积分流水（登录）。
async fn list_my_point_transactions(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<MyTransactionsQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_my_point_transactions";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let after = parse_after_cursor(&query.after, request_id)?;

    let sql = "SELECT t.id, op.kind AS kind, c.code AS currency, t.delta_balance AS amount,
         t.balance_after AS balance_after, op.memo AS memo, t.created_at AS created_at
         FROM point_transactions t
         JOIN point_operations op ON op.id = t.operation_id
         JOIN currencies c ON c.id = t.currency_id
         WHERE t.user_id = ? AND c.code = 'coin' AND (? IS NULL OR t.created_at < ?)
         ORDER BY t.created_at DESC, t.id DESC LIMIT ?";
    let fetch_limit = limit + 1;
    let rows: Vec<MyTxRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, MyTxRow>(sql)
            .bind(&user.id)
            .bind(after)
            .bind(after)
            .bind(fetch_limit)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, MyTxRow>(sql)
            .bind(&user.id)
            .bind(after)
            .bind(after)
            .bind(fetch_limit)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let has_more = rows.len() as i64 > limit;
    let page: Vec<_> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| r.created_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let items: Vec<Value> = page.iter().map(|r| r.to_json()).collect();
    let resp = (
        StatusCode::OK,
        Json(json!({ "items": items, "next_cursor": next_cursor })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 我的处罚 ────────────────────────────────────────────────────────────────

/// GET /api/v1/me/sanctions — 本人被处罚记录（登录）。
///
/// 数据模型说明：moderation_actions（0042）是案件维度的审核动作日志
/// （actor 是审核员），并非「用户被处罚」表；0043 的 sanctions 才是
/// 处罚记录（user_id + kind + reason + starts_at/ends_at + status）。
/// 此处按 sanctions 实现：expires_at = ends_at（NULL=永久）；
/// revoked 状态的处罚已被撤销，不再列入。
async fn list_my_sanctions(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "list_my_sanctions";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let sql = "SELECT s.id, s.kind, s.reason, s.created_at, s.ends_at, ma.case_id
         FROM sanctions s
         LEFT JOIN moderation_actions ma ON ma.action = 'issue_sanction' AND ma.target_id = s.id
         WHERE s.user_id = ? AND s.status != 'revoked'
         ORDER BY s.created_at DESC, s.id DESC";
    /// (id, kind, reason, created_at, ends_at, case_id)
    type SanctionRow = (
        String,
        String,
        Option<String>,
        i64,
        Option<i64>,
        Option<String>,
    );
    let rows: Vec<SanctionRow> = match pool {
        Either::Left(p) => sqlx::query_as::<_, SanctionRow>(sql)
            .bind(&user.id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, SanctionRow>(sql)
            .bind(&user.id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "id": r.0,
                "kind": r.1,
                "reason": r.2,
                "created_at": r.3,
                "expires_at": r.4,
                "case_id": r.5,
            })
        })
        .collect();

    let resp = (StatusCode::OK, Json(json!({ "items": items }))).into_response();
    Ok(private_no_store(resp))
}

// ─── 修改密码 ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ChangePasswordRequest {
    current_password: String,
    new_password: String,
}

/// 新密码强度校验——与注册策略一致（domain::registration：8..=128 字符 +
/// 必须同时包含字母与数字；M02-IDENTITY 校验规则，此处按同一策略复述，
/// 保持错误信息稳定）。
#[allow(clippy::result_large_err)]
fn validate_new_password(password: &str, request_id: &str) -> Result<(), AppError> {
    let len = password.chars().count();
    if !(8..=128).contains(&len) {
        return Err(AppError::bad_request(
            "new_password must be 8..=128 characters",
            request_id,
            None,
        ));
    }
    let has_letter = password.chars().any(|c| c.is_alphabetic());
    let has_digit = password.chars().any(|c| c.is_ascii_digit());
    if !(has_letter && has_digit) {
        return Err(AppError::bad_request(
            "new_password must contain both letters and digits",
            request_id,
            None,
        ));
    }
    Ok(())
}

/// POST /api/v1/me/password — 修改密码（登录）。
///
/// - 当前密码错误 → 401 code=invalid_current_password（复用
///   auth::password::verify_password，Argon2id 常量时间比较）；
/// - 新密码复用注册强度策略（8..=128 + 字母+数字）；
/// - 成功后：更新 hash、撤销除当前会话外的全部 session
///   （revoke_reason='password_changed'）、审计 + security 通知（复用
///   auth::security_notify::notify_password_changed）→ 204。
async fn change_my_password(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "change_my_password";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let req: ChangePasswordRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    validate_new_password(&req.new_password, request_id)?;

    // 当前密码校验（hash 损坏与密码错误统一 401，不泄漏内部状态）。
    let hash: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT password_hash FROM users WHERE id = ?")
            .bind(&user.id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let verified = hash
        .as_deref()
        .map(|h| {
            crate::auth::password::verify_password(&req.current_password, h)
                == crate::auth::password::VerifyResult::Ok
        })
        .unwrap_or(false);
    if !verified {
        return Err(AppError::with_code(
            StatusCode::UNAUTHORIZED,
            "invalid_current_password",
            "Unauthorized",
            "current password is incorrect",
            request_id,
        ));
    }

    // 更新密码 hash（复用注册的 Argon2id 参数）。
    let new_hash = crate::auth::password::hash_password(&req.new_password)
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    let now = now_millis();
    let updated = match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                .bind(&new_hash)
                .bind(now)
                .bind(&user.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
                .bind(&new_hash)
                .bind(now)
                .bind(&user.id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
    };
    if updated != 1 {
        return Err(AppError::not_found("user not found", request_id));
    }

    // 撤销除当前会话外的全部 session（当前会话保持登录；
    // session_id 缺失（理论上登录后必有）时保守撤销全部）。
    let now = now_millis();
    let revoke_sql = if auth.session_id.is_some() {
        "UPDATE user_sessions SET revoked_at = ?, revoke_reason = 'password_changed'
         WHERE user_id = ? AND revoked_at IS NULL AND id <> ?"
    } else {
        "UPDATE user_sessions SET revoked_at = ?, revoke_reason = 'password_changed'
         WHERE user_id = ? AND revoked_at IS NULL"
    };
    let revoked = match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(revoke_sql).bind(now).bind(&user.id);
            if let Some(sid) = &auth.session_id {
                q = q.bind(sid);
            }
            q.execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
        Either::Right(p) => {
            let mut q = sqlx::query(revoke_sql).bind(now).bind(&user.id);
            if let Some(sid) = &auth.session_id {
                q = q.bind(sid);
            }
            q.execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
    };

    // 审计 + security 通知（复用 auth/security_notify 模式：notifications +
    // 审计 + outbox 事件，独立事务）。
    AuditEntry::user_action(&user.id, "me.password.change")
        .with_target("user", &user.id)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "other_sessions_revoked": revoked }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    crate::auth::security_notify::notify_password_changed(pool, &user.id, request_id)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(private_no_store(StatusCode::NO_CONTENT.into_response()))
}

// ─── OAuth 授权管理 ─────────────────────────────────────────────────────────

/// 一条用户授权（oauth_consents 按 client 聚合）。
///
/// 注意：oauth_consents.client_id 存的是 oauth_clients 的**行 id**（UUID
/// 主键，见 0055 FK `REFERENCES oauth_clients (id)`，授权流 tokens.rs 也按
/// `client.id` 写入），不是对外公开的 client_id 字符串——对外投影时再
/// 联查换回公开标识。
#[derive(Debug, Clone)]
struct UserGrant {
    client_row_id: String,
    scopes: Vec<String>,
    granted_at: i64,
}

/// GET /api/v1/me/oauth-grants — 本人 OAuth 授权列表（登录）。
///
/// 数据来源（0055_oidc，真实授权流写入这些行，本端点只读聚合）：
/// - oauth_consents：用户逐 Client × scope 的授权记录（未撤销）；
/// - oauth_clients.name：client_name；
/// - oauth_tokens：MAX(COALESCE(last_used_at, issued_at)) 作为 last_used_at
///   （consent 行本身无使用时间戳，取该 Client 名下 token 的最近使用）。
async fn list_my_oauth_grants(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "list_my_oauth_grants";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // 1) consents 聚合（Rust 侧分组，避免方言 GROUP_CONCAT 差异）。
    let sql = "SELECT client_id, scope, granted_at FROM oauth_consents
         WHERE user_id = ? AND revoked_at IS NULL ORDER BY granted_at, client_id";
    #[allow(clippy::type_complexity)]
    let rows: Vec<(String, String, i64)> = match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, String, i64)>(sql)
            .bind(&user.id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, (String, String, i64)>(sql)
            .bind(&user.id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let mut grants: Vec<UserGrant> = Vec::new();
    for (client_row_id, scope, granted_at) in rows {
        if let Some(g) = grants.iter_mut().find(|g| g.client_row_id == client_row_id) {
            g.scopes.push(scope);
            if granted_at > g.granted_at {
                g.granted_at = granted_at;
            }
        } else {
            grants.push(UserGrant {
                client_row_id,
                scopes: vec![scope],
                granted_at,
            });
        }
    }
    if grants.is_empty() {
        let resp = (StatusCode::OK, Json(json!({ "items": [] }))).into_response();
        return Ok(private_no_store(resp));
    }

    // 2) 公开 client_id 与名称（oauth_consents 存行 id → 按 id 联查回公开
    //    标识；Client 行已删除的授权保留展示，公开标识退化为行 id、名称为空）。
    let placeholders = grants.iter().map(|_| "?").collect::<Vec<_>>().join(", ");
    let name_sql =
        format!("SELECT id, client_id, name FROM oauth_clients WHERE id IN ({placeholders})");
    #[allow(clippy::type_complexity)]
    let names: Vec<(String, String, String)> = match pool {
        Either::Left(p) => {
            let mut q = sqlx::query_as::<_, (String, String, String)>(&name_sql);
            for g in &grants {
                q = q.bind(&g.client_row_id);
            }
            q.fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
        Either::Right(p) => {
            let mut q = sqlx::query_as::<_, (String, String, String)>(&name_sql);
            for g in &grants {
                q = q.bind(&g.client_row_id);
            }
            q.fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
        }
    };

    // 3) last_used_at（oauth_tokens.client_id 同样存行 id；取该用户在该
    //    Client 名下 token 的最近使用/签发）。
    let usage_sql = "SELECT client_id, MAX(COALESCE(last_used_at, issued_at)) FROM oauth_tokens
         WHERE user_id = ? AND revoked_at IS NULL GROUP BY client_id";
    let usage: Vec<(String, Option<i64>)> = match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, Option<i64>)>(usage_sql)
            .bind(&user.id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, (String, Option<i64>)>(usage_sql)
            .bind(&user.id)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    grants.sort_by_key(|g| std::cmp::Reverse(g.granted_at));
    let items: Vec<Value> = grants
        .iter()
        .map(|g| {
            let (public_client_id, client_name) = names
                .iter()
                .find(|(row_id, _, _)| *row_id == g.client_row_id)
                .map(|(_, cid, name)| (cid.clone(), name.clone()))
                .unwrap_or_else(|| (g.client_row_id.clone(), String::new()));
            let last_used_at = usage
                .iter()
                .find(|(cid, _)| *cid == g.client_row_id)
                .and_then(|(_, t)| *t);
            json!({
                "client_id": public_client_id,
                "client_name": client_name,
                "scopes": g.scopes,
                "granted_at": g.granted_at,
                "last_used_at": last_used_at,
            })
        })
        .collect();

    let resp = (StatusCode::OK, Json(json!({ "items": items }))).into_response();
    Ok(private_no_store(resp))
}

/// DELETE /api/v1/me/oauth-grants/{client_id} — 撤销本人对某 Client 的授权。
///
/// 路径参数是对外公开的 client_id 字符串（OAuth 语义）；表内
/// oauth_consents/oauth_tokens 的 client_id 列存的是 oauth_clients 行 id
/// （0055 FK），因此先解析公开标识 → 行 id 再撤销。Client 不存在 → 404；
/// 已撤销后再删仍 204（幂等）。
async fn revoke_my_oauth_grant(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(client_id): Path<String>,
) -> Result<Response, AppError> {
    let request_id = "revoke_my_oauth_grant";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    // 公开 client_id → 行 id（FK 指向 oauth_clients.id）。
    let client_row_id: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM oauth_clients WHERE client_id = ?")
            .bind(&client_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT id FROM oauth_clients WHERE client_id = ?")
            .bind(&client_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let Some(client_row_id) = client_row_id else {
        return Err(AppError::not_found("oauth grant not found", request_id));
    };

    let now = now_millis();
    let consent_sql = "UPDATE oauth_consents SET revoked_at = ?, revoke_reason = 'user_revoked'
         WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL";
    let token_sql = "UPDATE oauth_tokens SET revoked_at = ?, revoke_reason = 'user_revoked'
         WHERE user_id = ? AND client_id = ? AND revoked_at IS NULL";
    for sql in [consent_sql, token_sql] {
        match pool {
            Either::Left(p) => {
                sqlx::query(sql)
                    .bind(now)
                    .bind(&user.id)
                    .bind(&client_row_id)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            Either::Right(p) => {
                sqlx::query(sql)
                    .bind(now)
                    .bind(&user.id)
                    .bind(&client_row_id)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
        }
    }

    AuditEntry::user_action(&user.id, "me.oauth_grant.revoke")
        .with_target("oauth_client", &client_id)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(private_no_store(StatusCode::NO_CONTENT.into_response()))
}

// ─── 附件管理（admin） ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AdminAttachmentsQuery {
    /// 文件名模糊搜索（original_name）。
    #[serde(default)]
    q: Option<String>,
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_attachments_limit")]
    limit: i64,
}

fn default_attachments_limit() -> i64 {
    30
}

/// GET /api/v1/admin/attachments — 附件列表（storage.manage）。
///
/// attachments（0048）：filename = original_name（可空——历史上传未记录
/// 原名时为 NULL）；uploader = owner 联 users；已软删行（deleted_at
/// 非空）不再列出。
async fn list_admin_attachments(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<AdminAttachmentsQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_admin_attachments";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "storage.manage", request_id).await?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let after = parse_after_cursor(&query.after, request_id)?;
    let q = query.q.filter(|s| !s.is_empty());

    let mut sql = String::from(
        "SELECT a.id, a.original_name AS filename, u.username_normalized AS uploader_username,
                a.size_bytes, a.created_at
         FROM attachments a
         LEFT JOIN users u ON u.id = a.owner_id
         WHERE a.deleted_at IS NULL",
    );
    if q.is_some() {
        sql.push_str(" AND a.original_name LIKE ? ESCAPE '!'");
    }
    if after.is_some() {
        sql.push_str(" AND a.created_at < ?");
    }
    sql.push_str(" ORDER BY a.created_at DESC, a.id DESC LIMIT ?");

    let q_pattern = q.as_ref().map(|s| format!("%{}%", like_escape(s)));
    let fetch_limit = limit + 1;
    /// (id, storage_key, original_name, size_bytes, created_at)
    type AttachmentRow = (String, Option<String>, Option<String>, i64, i64);
    let rows: Vec<AttachmentRow> = match pool {
        Either::Left(p) => {
            let mut query = sqlx::query_as::<_, AttachmentRow>(&sql);
            if let Some(pat) = &q_pattern {
                query = query.bind(pat);
            }
            if let Some(v) = after {
                query = query.bind(v);
            }
            query.bind(fetch_limit).fetch_all(p).await
        }
        Either::Right(p) => {
            let mut query =
                sqlx::query_as::<_, (String, Option<String>, Option<String>, i64, i64)>(&sql);
            if let Some(pat) = &q_pattern {
                query = query.bind(pat);
            }
            if let Some(v) = after {
                query = query.bind(v);
            }
            query.bind(fetch_limit).fetch_all(p).await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = rows.len() as i64 > limit;
    let page: Vec<_> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last().map(|r| r.4.to_string()).unwrap_or_default()
    } else {
        String::new()
    };

    let items: Vec<Value> = page
        .iter()
        .map(|r| {
            json!({
                "id": r.0,
                "filename": r.1,
                "uploader_username": r.2,
                "size_bytes": r.3,
                "created_at": r.4,
            })
        })
        .collect();

    let resp = (
        StatusCode::OK,
        Json(json!({ "items": items, "next_cursor": next_cursor })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// DELETE /api/v1/admin/attachments/{id} — 软删除附件（storage.manage）。
///
/// 软删语义（0048 attachments 实际列）：deleted_at 置位 + status 切
/// 'deleted'（值域内）；存储对象回收由既有清理 Job 负责，此处只改元数据。
async fn delete_admin_attachment(
    State(state): State<AppState>,
    jar: CookieJar,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "delete_admin_attachment";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "storage.manage", request_id).await?;
    crate::routes::admin::require_recent_auth(&state, &jar, request_id).await?;

    let body_value: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&body_value, request_id)?;

    let shop_ref_count: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM shop_products WHERE asset_attachment_id = ? AND status <> 'retired'",
        )
        .bind(&id)
        .fetch_one(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM shop_products WHERE asset_attachment_id = ? AND status <> 'retired'",
        )
        .bind(&id)
        .fetch_one(p)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    if shop_ref_count > 0 {
        return Err(AppError::conflict(
            "attachment is still used by a shop product",
            request_id,
        ));
    }

    let now = now_millis();
    let sql = "UPDATE attachments SET deleted_at = ?, status = 'deleted'
         WHERE id = ? AND deleted_at IS NULL";
    let affected = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(now)
            .bind(&id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
        Either::Right(p) => sqlx::query(sql)
            .bind(now)
            .bind(&id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
    };
    if affected == 0 {
        return Err(AppError::not_found("attachment not found", request_id));
    }

    AuditEntry::user_action(&user.id, "admin.attachment.delete")
        .with_target("attachment", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    Ok(private_no_store(StatusCode::NO_CONTENT.into_response()))
}

// ─── 下载计费交易（admin） ───────────────────────────────────────────────────

#[derive(Deserialize)]
struct DownloadBillingQuery {
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_ledger_limit")]
    limit: i64,
}

/// GET /api/v1/admin/download-billing/transactions — 下载交易列表
/// （download_billing.manage）。
///
/// 数据模型核实：本库无独立 download_transactions 表——下载计费交易即
/// download_authorizations（0048；charged_amount 持久化扣费金额，其
/// point_operation_id 关联账本流水）。filename 取 attachments.original_name，
/// 附件已被清理时 LEFT JOIN 为空 → 空串。
async fn list_download_billing_transactions(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<DownloadBillingQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_download_billing_transactions";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "download_billing.manage", request_id).await?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let after = parse_after_cursor(&query.after, request_id)?;

    let sql = "SELECT d.id, u.username_normalized AS username,
            COALESCE(a.original_name, '') AS filename, d.charged_amount AS amount, d.created_at
         FROM download_authorizations d
         JOIN users u ON u.id = d.user_id
         LEFT JOIN attachments a ON a.id = d.attachment_id
         WHERE (? IS NULL OR d.created_at < ?)
         ORDER BY d.created_at DESC, d.id DESC LIMIT ?";
    let fetch_limit = limit + 1;
    let rows: Vec<(String, String, String, i64, i64)> = match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, String, String, i64, i64)>(sql)
            .bind(after)
            .bind(after)
            .bind(fetch_limit)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, (String, String, String, i64, i64)>(sql)
            .bind(after)
            .bind(after)
            .bind(fetch_limit)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let has_more = rows.len() as i64 > limit;
    let page: Vec<_> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last().map(|r| r.4.to_string()).unwrap_or_default()
    } else {
        String::new()
    };

    let items: Vec<Value> = page
        .iter()
        .map(|r| {
            json!({
                "id": r.0,
                "username": r.1,
                "filename": r.2,
                "amount": r.3,
                "created_at": r.4,
            })
        })
        .collect();

    let resp = (
        StatusCode::OK,
        Json(json!({ "items": items, "next_cursor": next_cursor })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 标签合并（admin） ───────────────────────────────────────────────────────

#[derive(Deserialize)]
struct MergeTagRequest {
    target_id: String,
    reason: String,
}

/// POST /api/v1/admin/tags/{id}/merge — 源标签并入目标标签（tag.manage）。
///
/// 数据模型核实：post_tags 关联表存在（0003 建表、0036 补 created_at），
/// 因此按「关联转移」实现（而非仅在 tags 上做标记）：
/// 1) 源标签的全部 post_tags 关联转移到目标标签（目标已有同一帖子的关联
///    时 INSERT IGNORE 跳过，不破坏复合主键）；
/// 2) 转移后删除源关联，tags.usage_count 同步（目标 += n、源归零）；
/// 3) 源标签标记 status='merged'（0061 列）并停用（is_active=0，防止
///    新帖继续挂到已合并标签）；
/// 4) 返回 {merged, into, moved_usage:n}。
///
/// 已知限制：search_documents.tags_json 由索引 Job 维护，本端点不触发受
/// 影响帖子的全量重建——GET /posts 的 tag= 筛选走 post_tags 实时联查
/// （BE-2a）不受影响；GET /search?tag= 在下次索引重建前返回旧值。
async fn merge_tag(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "merge_tag";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "tag.manage", request_id).await?;

    let req: MergeTagRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = req.reason.trim().to_string();
    if reason.is_empty() {
        return Err(AppError::bad_request(
            "reason is required",
            request_id,
            None,
        ));
    }
    if req.target_id == id {
        return Err(AppError::bad_request(
            "cannot merge a tag into itself",
            request_id,
            None,
        ));
    }

    // 两个标签都必须存在（404）。
    for tag_id in [&id, &req.target_id] {
        let found: Option<i64> = match pool {
            Either::Left(p) => sqlx::query_scalar("SELECT 1 FROM tags WHERE id = ?")
                .bind(tag_id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
            Either::Right(p) => sqlx::query_scalar("SELECT 1 FROM tags WHERE id = ?")
                .bind(tag_id)
                .fetch_optional(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        };
        if found.is_none() {
            return Err(AppError::not_found("tag not found", request_id));
        }
    }

    let now = now_millis();
    // 1) 转移关联（SQLite: INSERT OR IGNORE ... SELECT；MySQL: INSERT IGNORE
    //    ... SELECT——方言差异仅在 IGNORE 关键字）。
    let move_sqlite = "INSERT OR IGNORE INTO post_tags (post_id, tag_id, created_at)
         SELECT post_id, ?, created_at FROM post_tags WHERE tag_id = ?";
    let move_mysql = "INSERT IGNORE INTO post_tags (post_id, tag_id, created_at)
         SELECT post_id, ?, created_at FROM post_tags WHERE tag_id = ?";
    // 2) 删除源关联 + usage 同步 + 源标记合并。
    let delete_src_sql = "DELETE FROM post_tags WHERE tag_id = ?";
    let bump_target_sql =
        "UPDATE tags SET usage_count = usage_count + ?, updated_at = ? WHERE id = ?";
    let mark_source_sql = "UPDATE tags SET status = 'merged', usage_count = 0, is_active = 0, updated_at = ? WHERE id = ?";

    // moved_usage = 转移前源标签的关联数。
    let moved_usage: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM post_tags WHERE tag_id = ?")
            .bind(&id)
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT COUNT(*) FROM post_tags WHERE tag_id = ?")
            .bind(&id)
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    match pool {
        Either::Left(p) => {
            sqlx::query(move_sqlite)
                .bind(&req.target_id)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(delete_src_sql)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(bump_target_sql)
                .bind(moved_usage)
                .bind(now)
                .bind(&req.target_id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(mark_source_sql)
                .bind(now)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            sqlx::query(move_mysql)
                .bind(&req.target_id)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(delete_src_sql)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(bump_target_sql)
                .bind(moved_usage)
                .bind(now)
                .bind(&req.target_id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            sqlx::query(mark_source_sql)
                .bind(now)
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    AuditEntry::user_action(&user.id, "admin.tag.merge")
        .with_target("tag", &id)
        .with_target("tag", &req.target_id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "moved_usage": moved_usage }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = (
        StatusCode::OK,
        Json(json!({
            "merged": id,
            "into": req.target_id,
            "moved_usage": moved_usage,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 付费解锁 ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct UnlockPostRequest {
    client_request_id: String,
}

/// 付费帖子投影（posts + content_access_policies）。
#[derive(sqlx::FromRow)]
struct PaidPostRow {
    id: String,
    author_id: String,
    status: String,
    deleted_at: Option<i64>,
    policy_id: Option<String>,
    policy_kind: Option<String>,
    /// 0061 列：付费定价（coin；NULL=未定价）。
    price_coin: Option<i64>,
    /// 策略行明细（遗留 paid 策略的回退定价来源）。
    policy_amount: Option<i64>,
}

/// 读取付费帖子（不存在/已删 → None）。
async fn load_paid_post(
    pool: &crate::db::DatabasePool,
    id: &str,
    request_id: &str,
) -> Result<Option<PaidPostRow>, AppError> {
    let sql = "SELECT p.id, p.author_id, p.status, p.deleted_at,
            p.access_policy_id AS policy_id, pol.kind AS policy_kind,
            p.price_coin AS price_coin, pol.amount AS policy_amount
         FROM posts p
         LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
         WHERE p.id = ?";
    let row = match pool {
        Either::Left(p) => sqlx::query_as::<_, PaidPostRow>(sql)
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, PaidPostRow>(sql)
            .bind(id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    Ok(row)
}

/// 是否已有有效 purchase grant（grant_target_key = post:{id}）。
async fn has_purchase_grant(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    post_id: &str,
    request_id: &str,
) -> Result<bool, AppError> {
    let sql = "SELECT 1 FROM content_access_grants
         WHERE user_id = ? AND post_id = ? AND source_kind = 'purchase' AND revoked_at IS NULL
         LIMIT 1";
    let found: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .bind(user_id)
            .bind(post_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar(sql)
            .bind(user_id)
            .bind(post_id)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    Ok(found.is_some())
}

/// 当前 coin 余额（账户未建 = 0）。
async fn coin_balance(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<i64, AppError> {
    let sql = "SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?";
    let balance: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .bind(user_id)
            .bind(CURRENCY_COIN)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar(sql)
            .bind(user_id)
            .bind(CURRENCY_COIN)
            .fetch_optional(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    Ok(balance.unwrap_or(0))
}

/// 事务内写 purchase grant（SQLite：INSERT OR IGNORE；幂等并发兜底）。
async fn insert_grant_sqlite(
    conn: &mut sqlx::SqliteConnection,
    user_id: &str,
    post: &PaidPostRow,
    operation_id: Option<&str>,
    now: i64,
) -> Result<(), sqlx::Error> {
    let grant_id = uuid::Uuid::now_v7().to_string();
    let source_id = format!("post_unlock:{}:{}", post.id, user_id);
    sqlx::query(
        "INSERT OR IGNORE INTO content_access_grants
         (id, user_id, post_id, comment_id, policy_id, source_kind, source_id, point_operation_id, grant_target_key, granted_at, revoked_at)
         VALUES (?, ?, ?, NULL, ?, 'purchase', ?, ?, ?, ?, NULL)",
    )
    .bind(&grant_id)
    .bind(user_id)
    .bind(&post.id)
    .bind(&post.policy_id)
    .bind(&source_id)
    .bind(operation_id)
    .bind(format!("post:{}", post.id))
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// 事务内写 purchase grant（MySQL/MariaDB：INSERT IGNORE）。
async fn insert_grant_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    user_id: &str,
    post: &PaidPostRow,
    operation_id: Option<&str>,
    now: i64,
) -> Result<(), sqlx::Error> {
    let grant_id = uuid::Uuid::now_v7().to_string();
    let source_id = format!("post_unlock:{}:{}", post.id, user_id);
    sqlx::query(
        "INSERT IGNORE INTO content_access_grants
         (id, user_id, post_id, comment_id, policy_id, source_kind, source_id, point_operation_id, grant_target_key, granted_at, revoked_at)
         VALUES (?, ?, ?, NULL, ?, 'purchase', ?, ?, ?, ?, NULL)",
    )
    .bind(&grant_id)
    .bind(user_id)
    .bind(&post.id)
    .bind(&post.policy_id)
    .bind(&source_id)
    .bind(operation_id)
    .bind(format!("post:{}", post.id))
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// 事务内给作者插「内容被解锁」通知（SQLite）。
async fn notify_author_sqlite(
    conn: &mut sqlx::SqliteConnection,
    author_id: &str,
    post_id: &str,
    price: i64,
    now: i64,
) -> Result<(), sqlx::Error> {
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, body, link, is_read, created_at, category)
         VALUES (?, ?, 'system', ?, ?, ?, 0, ?, 'system')",
    )
    .bind(&id)
    .bind(author_id)
    .bind("你的付费内容被解锁")
    .bind(format!("你的付费帖子（{price} 金币）被解锁，收益已入账。"))
    .bind(format!("/posts/{post_id}"))
    .bind(now)
    .execute(&mut *conn)
    .await?;
    Ok(())
}

/// 事务内给作者插「内容被解锁」通知（MySQL/MariaDB）。
async fn notify_author_mysql(
    tx: &mut sqlx::Transaction<'_, sqlx::MySql>,
    author_id: &str,
    post_id: &str,
    price: i64,
    now: i64,
) -> Result<(), sqlx::Error> {
    let id = uuid::Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO notifications (id, user_id, type, title, body, link, is_read, created_at, category)
         VALUES (?, ?, 'system', ?, ?, ?, 0, ?, 'system')",
    )
    .bind(&id)
    .bind(author_id)
    .bind("你的付费内容被解锁")
    .bind(format!("你的付费帖子（{price} 金币）被解锁，收益已入账。"))
    .bind(format!("/posts/{post_id}"))
    .bind(now)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// POST /api/v1/posts/{id}/unlock — 付费内容解锁（登录）。
///
/// - 非 paid 策略 → 422 post_not_paid；帖子不存在/未发布 → 404；
/// - 已解锁（content_access_grants 有 purchase 记录）→ 幂等 200
///   {unlocked:true, coin_balance:当前余额}，不重复扣款；
/// - 作者本人 → 免费解锁（写 grant 不扣款——自己买自己的内容没有意义；
///   评估链路 evaluate 对 paid 不放行作者，见 content/visibility/evaluate）；
/// - 余额不足 → 409 code=insufficient_funds；
/// - 充足 → 事务内：ledger consume（source_type='post_unlock'，source_id
///   =post_id，幂等 scope post.unlock）+ purchase grant + 通知作者；
/// - 定价权威来源 posts.price_coin（0061；金币计价），缺失时回退
///   content_access_policies.amount（遗留 paid 策略行）。
async fn unlock_post(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Bytes,
) -> Result<Response, AppError> {
    let request_id = "unlock_post";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let req: UnlockPostRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let crl_len = req.client_request_id.chars().count();
    if !(1..=200).contains(&crl_len) {
        return Err(AppError::bad_request(
            "client_request_id must be 1-200 characters",
            request_id,
            None,
        ));
    }

    let Some(post) = load_paid_post(pool, &id, request_id).await? else {
        return Err(AppError::not_found("post not found", request_id));
    };
    if post.deleted_at.is_some() || post.status != "published" {
        return Err(AppError::not_found("post not found", request_id));
    }
    if post.policy_kind.as_deref() != Some("paid") {
        return Err(AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "post_not_paid",
            "Unprocessable Entity",
            "post is not paid content",
            request_id,
        ));
    }
    // 定价：price_coin 优先，回退策略行 amount；两者皆缺 → 数据不一致。
    let price = post.price_coin.or(post.policy_amount).ok_or_else(|| {
        AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "price_not_configured",
            "Unprocessable Entity",
            "paid post has no price configured",
            request_id,
        )
    })?;
    if !(1..=MAX_PRICE_COIN).contains(&price) {
        return Err(AppError::with_code(
            StatusCode::UNPROCESSABLE_ENTITY,
            "price_not_configured",
            "Unprocessable Entity",
            format!("paid post price must be 1-{MAX_PRICE_COIN} coins"),
            request_id,
        ));
    }
    if post.policy_id.is_none() {
        return Err(AppError::conflict(
            "paid post has no access policy row (grant cannot be recorded)",
            request_id,
        ));
    }

    // 已解锁 → 幂等 200（不重复扣款）。
    if has_purchase_grant(pool, &user.id, &post.id, request_id).await? {
        let balance = coin_balance(pool, &user.id, request_id).await?;
        let resp = (
            StatusCode::OK,
            Json(json!({ "unlocked": true, "coin_balance": balance })),
        )
            .into_response();
        return Ok(private_no_store(resp));
    }

    // 作者本人：免费解锁（写 grant，不扣款、不通知自己）。
    if post.author_id == user.id {
        let now = now_millis();
        match pool {
            Either::Left(p) => {
                let mut conn = p
                    .acquire()
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                insert_grant_sqlite(&mut conn, &user.id, &post, None, now)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            Either::Right(p) => {
                let mut tx = p
                    .begin()
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                insert_grant_mysql(&mut tx, &user.id, &post, None, now)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                tx.commit()
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
        }
        let balance = coin_balance(pool, &user.id, request_id).await?;
        let resp = (
            StatusCode::OK,
            Json(json!({ "unlocked": true, "coin_balance": balance })),
        )
            .into_response();
        return Ok(private_no_store(resp));
    }

    // 幂等门（失败不缓存：余额不足后充值可同 key 重试）。
    let hash = request_hash(&body);
    let idem_key = IdempotencyKey::new("post.unlock", &req.client_request_id)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let outcome = begin_or_replay(
        pool,
        &idem_key,
        &hash,
        IDEMPOTENCY_TTL_MS,
        FailureCachePolicy::Retry,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match outcome {
        IdempotencyOutcome::Created { record_id } => {
            let now = now_millis();
            let result: Result<i64, AppError> = match pool {
                Either::Left(p) => {
                    let mut conn = p
                        .acquire()
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    sqlx::query("BEGIN IMMEDIATE")
                        .execute(&mut *conn)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    let outcome: Result<i64, AppError> = async {
                        // 竞态复查：并发同帖解锁时 grant 可能已写入。
                        if has_purchase_grant(pool, &user.id, &post.id, request_id).await? {
                            return coin_balance(pool, &user.id, request_id).await;
                        }
                        // 扣款（ledger consume，in-tx；余额不足 → 409
                        // insufficient_funds，整个事务回滚）。
                        let cmd = LedgerCommand {
                            idempotency_scope: "post.unlock".to_string(),
                            idempotency_key: req.client_request_id.clone(),
                            kind: LedgerKind::Consume,
                            actor_id: Some(user.id.clone()),
                            user_id: user.id.clone(),
                            currency_id: CURRENCY_COIN.to_string(),
                            delta_balance: -price,
                            delta_frozen: 0,
                            source_type: Some("post_unlock".to_string()),
                            source_id: Some(post.id.clone()),
                            memo: format!("unlock paid post {}", post.id),
                            reverses_operation_id: None,
                        };
                        let op = apply_operation_in_sqlite_tx(&mut conn, cmd, now)
                            .await
                            .map_err(|e| map_ledger_error(e, request_id))?;
                        let balance_after = op
                            .transactions
                            .first()
                            .map(|t| t.balance_after)
                            .unwrap_or(0);
                        // 写 grant（同事务；幂等并发兜底 INSERT OR IGNORE）。
                        insert_grant_sqlite(
                            &mut conn,
                            &user.id,
                            &post,
                            Some(&op.operation_id),
                            now,
                        )
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                        // 通知作者（同事务）。
                        notify_author_sqlite(&mut conn, &post.author_id, &post.id, price, now)
                            .await
                            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                        Ok(balance_after)
                    }
                    .await;
                    match outcome {
                        Ok(v) => {
                            sqlx::query("COMMIT")
                                .execute(&mut *conn)
                                .await
                                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                            Ok(v)
                        }
                        Err(e) => {
                            let _ = sqlx::query("ROLLBACK").execute(&mut *conn).await;
                            Err(e)
                        }
                    }
                }
                Either::Right(p) => {
                    let mut tx = p
                        .begin()
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    let outcome: Result<i64, AppError> = async {
                        if has_purchase_grant(pool, &user.id, &post.id, request_id).await? {
                            return coin_balance(pool, &user.id, request_id).await;
                        }
                        let cmd = LedgerCommand {
                            idempotency_scope: "post.unlock".to_string(),
                            idempotency_key: req.client_request_id.clone(),
                            kind: LedgerKind::Consume,
                            actor_id: Some(user.id.clone()),
                            user_id: user.id.clone(),
                            currency_id: CURRENCY_COIN.to_string(),
                            delta_balance: -price,
                            delta_frozen: 0,
                            source_type: Some("post_unlock".to_string()),
                            source_id: Some(post.id.clone()),
                            memo: format!("unlock paid post {}", post.id),
                            reverses_operation_id: None,
                        };
                        let op = apply_operation_in_mysql_tx(&mut tx, cmd, now)
                            .await
                            .map_err(|e| map_ledger_error(e, request_id))?;
                        let balance_after = op
                            .transactions
                            .first()
                            .map(|t| t.balance_after)
                            .unwrap_or(0);
                        insert_grant_mysql(&mut tx, &user.id, &post, Some(&op.operation_id), now)
                            .await
                            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                        notify_author_mysql(&mut tx, &post.author_id, &post.id, price, now)
                            .await
                            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                        Ok(balance_after)
                    }
                    .await;
                    match outcome {
                        Ok(v) => {
                            tx.commit()
                                .await
                                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                            Ok(v)
                        }
                        Err(e) => {
                            let _ = tx.rollback().await;
                            Err(e)
                        }
                    }
                }
            };

            match result {
                Ok(balance) => {
                    complete(pool, &record_id, &post.id)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                    let resp = (
                        StatusCode::OK,
                        Json(json!({ "unlocked": true, "coin_balance": balance })),
                    )
                        .into_response();
                    Ok(private_no_store(resp))
                }
                Err(e) => {
                    // 释放幂等键（Retry 策略下同 key 可重试）。
                    let _ = mark_failed(pool, &record_id).await;
                    Err(e)
                }
            }
        }
        IdempotencyOutcome::Replay { .. } => {
            // 同 key+摘要重放：grant 已写入（Created 分支成功后才 complete），
            // 返回当前态。
            if !has_purchase_grant(pool, &user.id, &post.id, request_id).await? {
                return Err(AppError::conflict(
                    "idempotent replay but unlock grant not found",
                    request_id,
                ));
            }
            let balance = coin_balance(pool, &user.id, request_id).await?;
            let resp = (
                StatusCode::OK,
                Json(json!({ "unlocked": true, "coin_balance": balance })),
            )
                .into_response();
            Ok(private_no_store(resp))
        }
        IdempotencyOutcome::InProgress => Err(AppError::conflict(
            "request already in progress",
            request_id,
        )),
        IdempotencyOutcome::Conflict => Err(AppError::conflict(
            "idempotency key reused with different request",
            request_id,
        )),
        IdempotencyOutcome::Failed { .. } => Err(AppError::conflict(
            "previous attempt failed; retry with the same or a new idempotency key",
            request_id,
        )),
    }
}
