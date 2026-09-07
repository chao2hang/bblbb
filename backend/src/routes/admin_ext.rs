//! 管理域扩展路由（GAP-FIX 管理域 Part A）。
//!
//! 覆盖端点（均属 DOCUMENTED_NON_CONTRACT 豁免清单，模式同 marketplace/plugins）：
//! - 仪表盘统计 `GET /api/v1/admin/stats`（admin.manage）；
//! - BI 指标 `GET /api/v1/admin/bi/metrics?period=day|week|month|year`（admin.manage）；
//! - 审计读取 `GET /api/v1/admin/audit-logs?q=&actor=&after=&limit=`（admin.manage，
//!   created_at DESC keyset 分页，users 左联 actor_username）；
//! - 系统设置 `GET/PATCH /api/v1/admin/settings`（admin.manage；PATCH 用
//!   If-Match version 乐观锁 + reason 审计，冲突 409）；
//! - 帖子管理 `GET /api/v1/admin/posts?status=&board=&q=`（post.moderate）与
//!   `POST /api/v1/admin/posts/{id}/action`（approve/reject/hide/restore/feature/
//!   unfeature/pin/unpin/lock/unlock/delete，全部写审计）；
//! - 通知广播 `GET /api/v1/admin/notifications/outbox`、`POST .../broadcast`
//!   （幂等 begin_or_replay，对 active 用户分批 500 插入 type='system' 通知）、
//!   `POST .../outbox/{id}/recall`（删除该广播未读通知行，已读保留）；
//! - 角色分配 `POST /api/v1/admin/users/{id}/roles`（role.manage，user_roles 表，
//!   0021_rbac 既有结构）与 `DELETE /api/v1/admin/users/{id}/roles/{role_name}`；
//! - 公开统计 `GET /api/v1/stats`（无需登录，Cache-Control public 60s）。
//!
//! 帖子布尔态**复用既有同义列**（见 0061 迁移注释，避免双源漂移）：
//! `is_featured = posts.featured_at IS NOT NULL`、`is_pinned = posts.pinned`、
//! `is_locked = posts.closed_at IS NOT NULL`（STATE-MACHINES §Post：closed_at
//! 非空即锁帖）。posts.price_coin 为 0061 新增列（付费解锁定价，NULL=免费）。
//!
//! 写操作约定：管理写操作必填 reason（审计；广播契约 body 未带 reason，提供
//! 则记录）；PATCH 用 If-Match version；广播 POST 用 client_request_id 幂等
//! （idempotency_records）。

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Json, Response},
    routing::{get, post},
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::app::AppState;
use crate::audit::AuditEntry;
use crate::auth::session::AuthSession;
use crate::authz::decision::{DenyReason, AUTHZ_POLICY_VERSION};
use crate::authz::enforce::{authorize_action, denied_reason, deny_to_error};
use crate::error::AppError;
use crate::idempotency::{
    begin_or_replay, complete, request_hash, FailureCachePolicy, IdempotencyKey, IdempotencyOutcome,
};
use crate::outbox::now_millis;

/// 列表 keyset 游标上限。
const MAX_LIST_LIMIT: i64 = 100;
/// 广播幂等记录保留窗口（与其他写端点一致：24h）。
const IDEMPOTENCY_TTL_MS: i64 = 24 * 60 * 60 * 1000;
/// 广播通知分批大小（每批多行 VALUES；500×6 绑定参数远低于 SQLite
/// 32766 与 MySQL 占位符上限）。
const BROADCAST_BATCH: usize = 500;

/// 管理域扩展路由。
pub fn router() -> Router<AppState> {
    Router::new()
        // 仪表盘 / BI / 审计读取
        .route("/api/v1/admin/stats", get(get_admin_stats))
        .route("/api/v1/admin/stats/trend", get(get_admin_stats_trend))
        .route("/api/v1/admin/bi/metrics", get(get_bi_metrics))
        .route("/api/v1/admin/audit-logs", get(list_audit_logs))
        // 系统设置（单行，If-Match 乐观锁）
        .route(
            "/api/v1/admin/settings",
            get(get_admin_settings).patch(update_admin_settings),
        )
        // 帖子管理（post.moderate）
        .route("/api/v1/admin/posts", get(list_admin_posts))
        .route("/api/v1/admin/posts/{id}/action", post(admin_post_action))
        // 通知广播（admin.manage）
        .route(
            "/api/v1/admin/notifications/outbox",
            get(list_broadcast_outbox),
        )
        .route(
            "/api/v1/admin/notifications/templates",
            get(list_notification_templates),
        )
        .route(
            "/api/v1/admin/notifications/broadcast",
            post(broadcast_notification),
        )
        .route(
            "/api/v1/admin/notifications/outbox/{id}/recall",
            post(recall_broadcast),
        )
        // 角色分配（role.manage；user_roles 0021 既有表）
        .route("/api/v1/admin/users/{id}/roles", post(assign_user_role))
        .route(
            "/api/v1/admin/users/{id}/roles/{role_name}",
            axum::routing::delete(revoke_user_role),
        )
        // 公开统计（无需登录）
        .route("/api/v1/stats", get(get_public_stats))
}

// ─── 共享助手 ───────────────────────────────────────────────────────────────

/// 领域权限门（模式同 admin.rs `require_permission`）。
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

/// 管理写操作必填 reason（模式同 admin.rs `required_reason`）。
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

/// 解析 If-Match 头为整数版本（与 tags/boards 的乐观锁约定一致：裸整数）。
#[allow(clippy::result_large_err)]
fn parse_if_match(headers: &HeaderMap, request_id: &str) -> Result<i64, AppError> {
    headers
        .get("if-match")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| AppError::bad_request("If-Match header is required", request_id, None))?
        .trim()
        .trim_matches('"')
        .parse::<i64>()
        .map_err(|_| {
            AppError::bad_request(
                "If-Match must be the current version integer",
                request_id,
                None,
            )
        })
}

/// UTC 今日 00:00 的 Unix 毫秒（纯算术取整，无时区库依赖）。
fn utc_day_start_ms(now_ms: i64) -> i64 {
    let day_secs = now_ms.div_euclid(1000).div_euclid(86_400) * 86_400;
    day_secs * 1000
}

/// LIKE 通配符转义（`!` 为转义符：SQLite 与 MySQL 的 `ESCAPE '!'` 语法一致，
/// 且 `!` 不是任一方的字符串字面量转义字符，无需双写）。
fn like_escape(input: &str) -> String {
    input
        .replace('!', "!!")
        .replace('%', "!%")
        .replace('_', "!_")
}

/// 变化百分比（整数、向下取整；上一窗口为 0 时无定义 → 0）。
fn delta_pct(current: i64, previous: i64) -> i64 {
    if previous > 0 {
        (current - previous) * 100 / previous
    } else {
        0
    }
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

// ─── 仪表盘统计 ─────────────────────────────────────────────────────────────

/// GET /api/v1/admin/stats — 仪表盘统计（admin.manage）。
///
/// 口径说明（现有表结构的最佳近似，均有注释）：
/// - members：status='active' 且未删除的用户数；
/// - members_delta_7d：近 7 天注册数（含非 active 状态，deleted_at IS NULL）；
/// - posts_today：UTC 今日发布的帖子（发布时间 = COALESCE(published_at,
///   created_at)，定时发布帖按实际 published_at 计）；
/// - posts_today_delta：今日对比昨日全天（UTC）的差值（带符号）；
/// - reports_pending：待处理举报（reports.status ∈ open/triaged/
///   investigating/reopened，见 0041_moderation_cases 状态机）；
/// - active_today：今日活跃 = users.last_login_at ≥ UTC 今日 0 点（无独立
///   活跃事件表，登录时间即最近活跃信号的最佳近似）。
async fn get_admin_stats(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "get_admin_stats";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let now = now_millis();
    let today_start = utc_day_start_ms(now);
    let yesterday_start = today_start - 86_400_000;
    let week_ago = now - 7 * 86_400_000;

    // 单条聚合（标量子查询）一次取齐全部计数，避免多次往返；
    // 行类型按方言不同，两个分支内直接取值。
    let sql = "SELECT
        (SELECT COUNT(*) FROM users WHERE status = 'active' AND deleted_at IS NULL) AS members,
        (SELECT COUNT(*) FROM users WHERE created_at >= ? AND deleted_at IS NULL) AS members_delta_7d,
        (SELECT COUNT(*) FROM posts WHERE status = 'published' AND deleted_at IS NULL
            AND COALESCE(published_at, created_at) >= ?) AS posts_today,
        (SELECT COUNT(*) FROM posts WHERE status = 'published' AND deleted_at IS NULL
            AND COALESCE(published_at, created_at) >= ? AND COALESCE(published_at, created_at) < ?) AS posts_yesterday,
        (SELECT COUNT(*) FROM reports WHERE status IN ('open', 'triaged', 'investigating', 'reopened')) AS reports_pending,
        (SELECT COUNT(*) FROM users WHERE status = 'active' AND deleted_at IS NULL AND last_login_at >= ?) AS active_today";

    let stats = match pool {
        Either::Left(p) => {
            let r = sqlx::query(sql)
                .bind(week_ago)
                .bind(today_start)
                .bind(yesterday_start)
                .bind(today_start)
                .bind(today_start)
                .fetch_one(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            (
                r.get::<i64, _>("members"),
                r.get::<i64, _>("members_delta_7d"),
                r.get::<i64, _>("posts_today"),
                r.get::<i64, _>("posts_yesterday"),
                r.get::<i64, _>("reports_pending"),
                r.get::<i64, _>("active_today"),
            )
        }
        Either::Right(p) => {
            let r = sqlx::query(sql)
                .bind(week_ago)
                .bind(today_start)
                .bind(yesterday_start)
                .bind(today_start)
                .bind(today_start)
                .fetch_one(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            (
                r.get::<i64, _>("members"),
                r.get::<i64, _>("members_delta_7d"),
                r.get::<i64, _>("posts_today"),
                r.get::<i64, _>("posts_yesterday"),
                r.get::<i64, _>("reports_pending"),
                r.get::<i64, _>("active_today"),
            )
        }
    };
    let (members, members_delta_7d, posts_today, posts_yesterday, reports_pending, active_today) =
        stats;

    // 最近管理动作（audit_logs 最新 8 条，users 左联 actor_username）。
    let actions_sql = "SELECT a.action, a.created_at, u.username_normalized AS actor_username
             FROM audit_logs a
             LEFT JOIN users u ON u.id = a.actor_id
             ORDER BY a.created_at DESC, a.id DESC
             LIMIT 8";
    let actions: Vec<(String, i64, Option<String>)> = match pool {
        Either::Left(p) => sqlx::query_as(actions_sql)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(actions_sql)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let resp = (
        StatusCode::OK,
        Json(json!({
            "members": members,
            "members_delta_7d": members_delta_7d,
            "posts_today": posts_today,
            "posts_yesterday": posts_yesterday,
            "posts_today_delta": posts_today - posts_yesterday,
            "reports_pending": reports_pending,
            "active_today": active_today,
            "recent_admin_actions": actions
                .iter()
                .map(|(action, created_at, actor_username)| {
                    json!({
                        "action": action,
                        "actor_username": actor_username,
                        "created_at": created_at,
                    })
                })
                .collect::<Vec<_>>(),
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 运营趋势（仪表盘图表，视觉对齐 M17-GAPFIX-07） ─────────────────────────

#[derive(Deserialize)]
struct StatsTrendQuery {
    #[serde(default)]
    period: Option<String>,
}

/// GET /api/v1/admin/stats/trend?period=day|week|month|year — 运营趋势
/// 8 桶时间序列（admin.manage）。
///
/// 桶宽：day=3h×8（近 24h）/ week=1d×8 / month=4d×8 / year=45d×8（末桶 =
/// 进行中周期）。口径：posts=published 且 COALESCE(published_at, created_at)
/// 落桶；comments=published；reports=新增举报；active_users=桶内 posts+
/// comments 作者去重数。分桶在 Rust 内完成（避免方言日期函数差异）；
/// 响应只带 start/end 毫秒，标签由前端按本地时区渲染。
#[allow(clippy::too_many_lines)]
async fn get_admin_stats_trend(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<StatsTrendQuery>,
) -> Result<Response, AppError> {
    let request_id = "get_admin_stats_trend";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let period = query.period.unwrap_or_else(|| "day".to_string());
    let (bucket_ms, align_to_day) = match period.as_str() {
        "day" => (3 * 3_600_000i64, false),
        "week" => (86_400_000, true),
        "month" => (4 * 86_400_000, true),
        "year" => (45 * 86_400_000, true),
        _ => {
            return Err(AppError::bad_request(
                "period must be one of day|week|month|year",
                request_id,
                None,
            ))
        }
    };
    const BUCKETS: i64 = 8;
    let now = now_millis();
    // 末桶右边界对齐（时桶对齐下一整点；日桶对齐明日 UTC 起点）。
    let last_end = if align_to_day {
        utc_day_start_ms(now) + 86_400_000
    } else {
        (now / 3_600_000 + 1) * 3_600_000
    };
    let window_start = last_end - BUCKETS * bucket_ms;

    // 内容：published 帖子（ts, author）。
    let posts_sql = "SELECT COALESCE(published_at, created_at) AS ts, author_id
             FROM posts
             WHERE status = 'published' AND deleted_at IS NULL
               AND COALESCE(published_at, created_at) >= ?";
    let posts_rows: Vec<(i64, String)> = match pool {
        Either::Left(p) => sqlx::query_as(posts_sql)
            .bind(window_start)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(posts_sql)
            .bind(window_start)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    // 内容：published 评论（ts, author）。
    let comments_sql = "SELECT created_at AS ts, author_id
             FROM comments
             WHERE status = 'published' AND created_at >= ?";
    let comments_rows: Vec<(i64, String)> = match pool {
        Either::Left(p) => sqlx::query_as(comments_sql)
            .bind(window_start)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(comments_sql)
            .bind(window_start)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    // 审核：新增举报（ts）。
    let reports_sql = "SELECT created_at AS ts FROM reports WHERE created_at >= ?";
    let reports_rows: Vec<(i64,)> = match pool {
        Either::Left(p) => sqlx::query_as(reports_sql)
            .bind(window_start)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as(reports_sql)
            .bind(window_start)
            .fetch_all(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let n = BUCKETS as usize;
    let mut posts = vec![0i64; n];
    let mut comments = vec![0i64; n];
    let mut reports = vec![0i64; n];
    let mut authors: Vec<std::collections::HashSet<String>> =
        vec![std::collections::HashSet::new(); n];
    for (ts, author) in posts_rows {
        if ts >= window_start && ts < last_end {
            let idx = ((ts - window_start) / bucket_ms) as usize;
            posts[idx] += 1;
            authors[idx].insert(author);
        }
    }
    for (ts, author) in comments_rows {
        if ts >= window_start && ts < last_end {
            let idx = ((ts - window_start) / bucket_ms) as usize;
            comments[idx] += 1;
            authors[idx].insert(author);
        }
    }
    for (ts,) in reports_rows {
        if ts >= window_start && ts < last_end {
            let idx = ((ts - window_start) / bucket_ms) as usize;
            reports[idx] += 1;
        }
    }

    let buckets: Vec<Value> = (0..n)
        .map(|i| {
            let start = window_start + i as i64 * bucket_ms;
            json!({
                "start": start,
                "end": start + bucket_ms,
                "posts": posts[i],
                "comments": comments[i],
                "active_users": authors[i].len() as i64,
                "reports": reports[i],
            })
        })
        .collect();

    let resp = (
        StatusCode::OK,
        Json(json!({
            "period": period,
            "bucket_ms": bucket_ms,
            "buckets": buckets,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── BI 指标 ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct BiMetricsQuery {
    #[serde(default)]
    period: Option<String>,
}

/// GET /api/v1/admin/bi/metrics — 四指标按 period 窗口聚合（admin.manage）。
///
/// - 窗口：day=24h / week=7d / month=30d / year=365d（滚动窗口 [now-L, now)）；
/// - delta_pct 与上一等长窗口 [now-2L, now-L) 比较；
/// - target 为固定运营基线估算（无历史目标数据；示意性基线，非承诺值）；
/// - active_members：窗口内登录过的 active 用户（last_login_at 近似活跃）；
/// - new_posts：窗口内发布的帖子（发布时间口径同 stats）；
/// - point_flow：窗口内积分流水总量 = SUM(|delta_balance|)（0047
///   point_transactions）；
/// - moderation_pass_rate：窗口内关闭的举报（updated_at 落在窗口且 status ∈
///   resolved/rejected）中 resolved 占比（0-100 整数；无关闭数据 → 0）。
async fn get_bi_metrics(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<BiMetricsQuery>,
) -> Result<Response, AppError> {
    let request_id = "get_bi_metrics";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let period = query.period.unwrap_or_else(|| "day".to_string());
    let window_ms: i64 = match period.as_str() {
        "day" => 24 * 3_600_000,
        "week" => 7 * 86_400_000,
        "month" => 30 * 86_400_000,
        "year" => 365 * 86_400_000,
        _ => {
            return Err(AppError::bad_request(
                "period must be one of day|week|month|year",
                request_id,
                None,
            ))
        }
    };

    let now = now_millis();
    let cur_start = now - window_ms;
    let prev_start = now - 2 * window_ms;

    let active_cur = count_active_since(pool, cur_start, request_id).await?;
    let active_prev = count_active_since(pool, prev_start, request_id).await?;
    let posts_cur = count_posts_between(pool, cur_start, i64::MAX, request_id).await?;
    let posts_prev = count_posts_between(pool, prev_start, cur_start, request_id).await?;
    // SUM 聚合的整数口径与 storage/quota.rs 的 SUM 用法一致（COALESCE 兜底空集）。
    let flow_cur = sum_point_flow_between(pool, cur_start, i64::MAX, request_id).await?;
    let flow_prev = sum_point_flow_between(pool, prev_start, cur_start, request_id).await?;
    // COUNT(CASE WHEN ...) 聚合在三库都返回整数，避免 SUM 的 DECIMAL 类型差异。
    let (resolved_cur, closed_cur) =
        moderation_outcome_between(pool, cur_start, i64::MAX, request_id).await?;
    let (resolved_prev, closed_prev) =
        moderation_outcome_between(pool, prev_start, cur_start, request_id).await?;
    let rate_cur = if closed_cur > 0 {
        resolved_cur * 100 / closed_cur
    } else {
        0
    };
    let rate_prev = if closed_prev > 0 {
        resolved_prev * 100 / closed_prev
    } else {
        0
    };

    let metrics = json!([
        {
            "key": "active_members", "label": "活跃成员",
            "value": active_cur, "target": 100, "delta_pct": delta_pct(active_cur, active_prev),
        },
        {
            "key": "new_posts", "label": "新增帖子",
            "value": posts_cur, "target": 50, "delta_pct": delta_pct(posts_cur, posts_prev),
        },
        {
            "key": "point_flow", "label": "积分流水",
            "value": flow_cur, "target": 10000, "delta_pct": delta_pct(flow_cur, flow_prev),
        },
        {
            "key": "moderation_pass_rate", "label": "审核通过率",
            "value": rate_cur, "target": 90, "delta_pct": delta_pct(rate_cur, rate_prev),
        },
    ]);

    let resp = (
        StatusCode::OK,
        Json(json!({
            "period": period,
            "generated_at": now,
            "metrics": metrics,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// 窗口起点后登录过的 active 用户数。
async fn count_active_since(
    pool: &crate::db::DatabasePool,
    since: i64,
    request_id: &str,
) -> Result<i64, AppError> {
    let sql = "SELECT COUNT(*) FROM users WHERE status = 'active' AND deleted_at IS NULL AND last_login_at >= ?";
    let n = match pool {
        Either::Left(p) => sqlx::query_scalar(sql).bind(since).fetch_one(p).await,
        Either::Right(p) => sqlx::query_scalar(sql).bind(since).fetch_one(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(n)
}

/// [from, to) 窗口内发布的帖子数。
async fn count_posts_between(
    pool: &crate::db::DatabasePool,
    from: i64,
    to: i64,
    request_id: &str,
) -> Result<i64, AppError> {
    let sql = "SELECT COUNT(*) FROM posts WHERE status = 'published' AND deleted_at IS NULL
        AND COALESCE(published_at, created_at) >= ? AND COALESCE(published_at, created_at) < ?";
    let n = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(sql)
                .bind(from)
                .bind(to)
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(sql)
                .bind(from)
                .bind(to)
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(n)
}

/// [from, to) 窗口内积分流水总量（|delta_balance| 求和）。
async fn sum_point_flow_between(
    pool: &crate::db::DatabasePool,
    from: i64,
    to: i64,
    request_id: &str,
) -> Result<i64, AppError> {
    let sql = "SELECT COALESCE(SUM(ABS(delta_balance)), 0) FROM point_transactions
        WHERE created_at >= ? AND created_at < ?";
    let n = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(sql)
                .bind(from)
                .bind(to)
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar(sql)
                .bind(from)
                .bind(to)
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(n)
}

/// [from, to) 窗口内关闭举报的 (resolved 数, resolved+rejected 数)。
async fn moderation_outcome_between(
    pool: &crate::db::DatabasePool,
    from: i64,
    to: i64,
    request_id: &str,
) -> Result<(i64, i64), AppError> {
    let sql = "SELECT
        COUNT(CASE WHEN status = 'resolved' THEN 1 END) AS resolved_n,
        COUNT(CASE WHEN status IN ('resolved', 'rejected') THEN 1 END) AS closed_n
     FROM reports WHERE updated_at >= ? AND updated_at < ?";
    let row = match pool {
        Either::Left(p) => {
            let r = sqlx::query(sql)
                .bind(from)
                .bind(to)
                .fetch_one(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            (r.get::<i64, _>("resolved_n"), r.get::<i64, _>("closed_n"))
        }
        Either::Right(p) => {
            let r = sqlx::query(sql)
                .bind(from)
                .bind(to)
                .fetch_one(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            (r.get::<i64, _>("resolved_n"), r.get::<i64, _>("closed_n"))
        }
    };
    Ok(row)
}

// ─── 审计读取 ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AuditLogsQuery {
    /// actor 过滤：用户 id 或用户名（精确匹配）。
    #[serde(default)]
    actor: Option<String>,
    /// 关键字过滤（action/target/metadata 模糊匹配）。
    #[serde(default)]
    q: Option<String>,
    /// keyset 游标：上一页最后一条 created_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_audit_limit")]
    limit: i64,
}

fn default_audit_limit() -> i64 {
    50
}

/// 审计日志行（users 左联 actor_username）。
#[derive(sqlx::FromRow)]
struct AuditLogRow {
    id: String,
    actor_id: Option<String>,
    actor_username: Option<String>,
    action: String,
    target_type: Option<String>,
    target_id: Option<String>,
    metadata: Option<String>,
    created_at: i64,
}

/// GET /api/v1/admin/audit-logs — 审计日志读取（admin.manage）。
///
/// created_at DESC keyset 分页；`detail` 为 audit_logs.metadata 的 JSON 投影
/// （存储即 JSON 字符串，解析失败原样返回字符串）。
async fn list_audit_logs(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<AuditLogsQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_audit_logs";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let after = parse_after_cursor(&query.after, request_id)?;
    let actor = query.actor.filter(|s| !s.is_empty());
    let q = query.q.filter(|s| !s.is_empty());

    // 动态 WHERE（条件与绑定顺序一致；LIKE 统一 ESCAPE '!'，见 like_escape）。
    let mut sql = String::from(
        "SELECT a.id, a.actor_id, u.username_normalized AS actor_username, a.action,
                a.target_type, a.target_id, a.metadata, a.created_at
         FROM audit_logs a
         LEFT JOIN users u ON u.id = a.actor_id
         WHERE 1 = 1",
    );
    if after.is_some() {
        sql.push_str(" AND a.created_at < ?");
    }
    if actor.is_some() {
        sql.push_str(" AND (a.actor_id = ? OR u.username_normalized = ?)");
    }
    if q.is_some() {
        sql.push_str(
            " AND (a.action LIKE ? ESCAPE '!' OR a.target_type LIKE ? ESCAPE '!'
                  OR a.target_id LIKE ? ESCAPE '!' OR a.metadata LIKE ? ESCAPE '!')",
        );
    }
    sql.push_str(" ORDER BY a.created_at DESC, a.id DESC LIMIT ?");

    let q_pattern = q.as_ref().map(|s| format!("%{}%", like_escape(s)));
    // 总数（视觉对齐 M17-GAPFIX-07「共 N 条」）：含 q/actor 过滤、忽略游标。
    let mut count_sql = String::from(
        "SELECT COUNT(*) FROM audit_logs a
         LEFT JOIN users u ON u.id = a.actor_id
         WHERE 1 = 1",
    );
    if actor.is_some() {
        count_sql.push_str(" AND (a.actor_id = ? OR u.username_normalized = ?)");
    }
    if q.is_some() {
        count_sql.push_str(
            " AND (a.action LIKE ? ESCAPE '!' OR a.target_type LIKE ? ESCAPE '!'
                  OR a.target_id LIKE ? ESCAPE '!' OR a.metadata LIKE ? ESCAPE '!')",
        );
    }
    let total: i64 = match pool {
        Either::Left(p) => {
            let mut query = sqlx::query_scalar::<_, i64>(&count_sql);
            if let Some(a) = &actor {
                query = query.bind(a).bind(a);
            }
            if let Some(pat) = &q_pattern {
                query = query.bind(pat).bind(pat).bind(pat).bind(pat);
            }
            query.fetch_one(p).await
        }
        Either::Right(p) => {
            let mut query = sqlx::query_scalar::<_, i64>(&count_sql);
            if let Some(a) = &actor {
                query = query.bind(a).bind(a);
            }
            if let Some(pat) = &q_pattern {
                query = query.bind(pat).bind(pat).bind(pat).bind(pat);
            }
            query.fetch_one(p).await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let fetch_limit = limit + 1;
    let rows: Vec<AuditLogRow> = match pool {
        Either::Left(p) => {
            let mut query = sqlx::query_as::<_, AuditLogRow>(&sql);
            if let Some(v) = after {
                query = query.bind(v);
            }
            if let Some(a) = &actor {
                query = query.bind(a).bind(a);
            }
            if let Some(pat) = &q_pattern {
                query = query.bind(pat).bind(pat).bind(pat).bind(pat);
            }
            query.bind(fetch_limit).fetch_all(p).await
        }
        Either::Right(p) => {
            let mut query = sqlx::query_as::<_, AuditLogRow>(&sql);
            if let Some(v) = after {
                query = query.bind(v);
            }
            if let Some(a) = &actor {
                query = query.bind(a).bind(a);
            }
            if let Some(pat) = &q_pattern {
                query = query.bind(pat).bind(pat).bind(pat).bind(pat);
            }
            query.bind(fetch_limit).fetch_all(p).await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = rows.len() as i64 > limit;
    let page: Vec<AuditLogRow> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| r.created_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let items: Vec<Value> = page
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "actor_id": r.actor_id,
                "actor_username": r.actor_username,
                "action": r.action,
                "object_type": r.target_type,
                "object_id": r.target_id,
                "detail": r.metadata.as_deref().map(|m| {
                    serde_json::from_str::<Value>(m).unwrap_or(Value::String(m.to_string()))
                }),
                "created_at": r.created_at,
            })
        })
        .collect();

    let resp = (
        StatusCode::OK,
        Json(json!({ "items": items,
                "next_cursor": next_cursor,
                "total": total, })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 系统设置 ────────────────────────────────────────────────────────────────

/// site_settings 单行投影（0061 迁移结构；public_source 由 0063 添加；SMTP 由 0064 添加）。
#[derive(sqlx::FromRow, Clone)]
struct SiteSettingsRow {
    open_registration: i64,
    email_verification: i64,
    anonymous_replies: i64,
    public_rss: i64,
    maintenance_mode: i64,
    site_name: String,
    default_lang: String,
    public_source: String,
    api_rate_limit: Option<i64>,
    version: i64,
    updated_at: i64,
    smtp_enabled: i64,
    smtp_host: String,
    smtp_port: i64,
    smtp_user: String,
    smtp_pass: String,
    smtp_from_email: String,
    smtp_from_name: String,
    smtp_encryption: String,
}

/// 设置 JSON 投影（settings 字段集）。
fn settings_json(r: &SiteSettingsRow) -> Value {
    json!({
        "open_registration": r.open_registration != 0,
        "email_verification": r.email_verification != 0,
        "anonymous_replies": r.anonymous_replies != 0,
        "public_rss": r.public_rss != 0,
        "maintenance_mode": r.maintenance_mode != 0,
        "site_name": r.site_name,
        "default_lang": r.default_lang,
        "public_source": r.public_source,
        "api_rate_limit": r.api_rate_limit,
        "smtp_enabled": r.smtp_enabled != 0,
        "smtp_host": r.smtp_host,
        "smtp_port": r.smtp_port,
        "smtp_user": r.smtp_user,
        "smtp_pass_configured": !r.smtp_pass.is_empty(),
        "smtp_from_email": r.smtp_from_email,
        "smtp_from_name": r.smtp_from_name,
        "smtp_encryption": r.smtp_encryption,
    })
}

/// 读取 site_settings 单行（迁移种子保证存在；缺失视为内部错误）。
async fn load_site_settings(
    pool: &crate::db::DatabasePool,
    request_id: &str,
) -> Result<SiteSettingsRow, AppError> {
    let sql = "SELECT open_registration, email_verification, anonymous_replies, public_rss,
                      maintenance_mode, site_name, default_lang, public_source,
                      api_rate_limit, version, updated_at,
                      smtp_enabled, smtp_host, smtp_port, smtp_user, smtp_pass,
                      smtp_from_email, smtp_from_name, smtp_encryption
               FROM site_settings WHERE id = 'singleton'";
    let row = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, SiteSettingsRow>(sql)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, SiteSettingsRow>(sql)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    row.ok_or_else(|| {
        AppError::internal(
            "site_settings singleton row missing (run migrations)",
            request_id,
        )
    })
}

/// GET /api/v1/admin/settings — 读取系统设置（admin.manage）。
async fn get_admin_settings(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "get_admin_settings";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let row = load_site_settings(pool, request_id).await?;
    let resp = (
        StatusCode::OK,
        Json(json!({
            "settings": settings_json(&row),
            "version": row.version,
            "updated_at": row.updated_at,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// PATCH /api/v1/admin/settings — 部分更新系统设置（admin.manage）。
///
/// If-Match = 当前 version（乐观锁）：先读当前行、在 Rust 合并变更、再以
/// `WHERE version = ?` 全列 UPDATE——0 行受影响即版本冲突（409）。
/// 变更字段以审计 metadata 记录（仅设置键名，无敏感值）。
async fn update_admin_settings(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: HeaderMap,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "patch_admin_settings";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let if_match = parse_if_match(&headers, request_id)?;
    let patch: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&patch, request_id)?;

    let current = load_site_settings(pool, request_id).await?;
    if if_match != current.version {
        return Err(AppError::conflict(
            "settings version mismatch (reload and retry)",
            request_id,
        ));
    }

    // 兼容两种请求形状：顶层平铺（与 AI 配置 PATCH 一致）或 `settings`
    // 嵌套对象（管理台常用形状）；reason 恒在顶层读取。
    let flat = match patch.get("settings") {
        Some(Value::Object(_)) => patch["settings"].clone(),
        _ => patch.clone(),
    };

    // 合并变更（缺省字段保持原值；显式 null 的语义按字段类型区分）。
    let mut next = current.clone();
    let mut changed: Vec<String> = Vec::new();
    for (key, old) in [
        ("open_registration", &mut next.open_registration),
        ("email_verification", &mut next.email_verification),
        ("anonymous_replies", &mut next.anonymous_replies),
        ("public_rss", &mut next.public_rss),
        ("maintenance_mode", &mut next.maintenance_mode),
    ] {
        if let Some(v) = flat.get(key).and_then(Value::as_bool) {
            let v = v as i64;
            if *old != v {
                *old = v;
                changed.push(key.to_string());
            }
        }
    }
    if let Some(v) = flat.get("site_name") {
        let name = v
            .as_str()
            .map(str::trim)
            .ok_or_else(|| AppError::bad_request("site_name must be a string", request_id, None))?;
        let len = name.chars().count();
        if !(1..=100).contains(&len) {
            return Err(AppError::bad_request(
                "site_name must be 1-100 characters",
                request_id,
                None,
            ));
        }
        if next.site_name != name {
            next.site_name = name.to_string();
            changed.push("site_name".to_string());
        }
    }
    if let Some(v) = flat.get("default_lang") {
        let lang = v.as_str().map(str::trim).ok_or_else(|| {
            AppError::bad_request("default_lang must be a string", request_id, None)
        })?;
        let len = lang.chars().count();
        if !(1..=16).contains(&len) {
            return Err(AppError::bad_request(
                "default_lang must be 1-16 characters",
                request_id,
                None,
            ));
        }
        if next.default_lang != lang {
            next.default_lang = lang.to_string();
            changed.push("default_lang".to_string());
        }
    }
    // public_source：公开源（RSS / API）地址。必填，校验语义与原型一致
    // （/^https?:\/\S+\.\S+/）：http(s) 前缀 + 主机非空且含点 + 无空白，
    // 1-200 字符；缺 key = 保持原值。
    if let Some(v) = flat.get("public_source") {
        let src = v.as_str().map(str::trim).ok_or_else(|| {
            AppError::bad_request("public_source must be a string", request_id, None)
        })?;
        let rest = src
            .strip_prefix("https://")
            .or_else(|| src.strip_prefix("http://"))
            .unwrap_or_default();
        let len = src.chars().count();
        let valid = !rest.is_empty()
            && !src.contains(char::is_whitespace)
            && rest.contains('.')
            && (1..=200).contains(&len);
        if !valid {
            return Err(AppError::bad_request(
                "public_source must be a valid http(s) URL (1-200 characters)",
                request_id,
                None,
            ));
        }
        if next.public_source != src {
            next.public_source = src.to_string();
            changed.push("public_source".to_string());
        }
    }
    // api_rate_limit：显式 null = 清空（不限速），数值 = 新限制。
    if let Some(v) = flat.get("api_rate_limit") {
        let parsed = if v.is_null() {
            None
        } else {
            let n = v.as_i64().ok_or_else(|| {
                AppError::bad_request(
                    "api_rate_limit must be an integer or null",
                    request_id,
                    None,
                )
            })?;
            if !(1..=100_000).contains(&n) {
                return Err(AppError::bad_request(
                    "api_rate_limit must be between 1 and 100000",
                    request_id,
                    None,
                ));
            }
            Some(n)
        };
        if next.api_rate_limit != parsed {
            next.api_rate_limit = parsed;
            changed.push("api_rate_limit".to_string());
        }
    }

    // SMTP 设置（0064 新增）
    if let Some(v) = flat.get("smtp_enabled").and_then(Value::as_bool) {
        let v = v as i64;
        if next.smtp_enabled != v {
            next.smtp_enabled = v;
            changed.push("smtp_enabled".to_string());
        }
    }
    if let Some(v) = flat.get("smtp_host") {
        let host = v
            .as_str()
            .map(str::trim)
            .ok_or_else(|| AppError::bad_request("smtp_host must be a string", request_id, None))?;
        if host.chars().count() > 255 {
            return Err(AppError::bad_request(
                "smtp_host must be at most 255 characters",
                request_id,
                None,
            ));
        }
        if next.smtp_host != host {
            next.smtp_host = host.to_string();
            changed.push("smtp_host".to_string());
        }
    }
    if let Some(v) = flat.get("smtp_port") {
        let port = v.as_i64().ok_or_else(|| {
            AppError::bad_request("smtp_port must be an integer", request_id, None)
        })?;
        if !(1..=65535).contains(&port) {
            return Err(AppError::bad_request(
                "smtp_port must be between 1 and 65535",
                request_id,
                None,
            ));
        }
        if next.smtp_port != port {
            next.smtp_port = port;
            changed.push("smtp_port".to_string());
        }
    }
    if let Some(v) = flat.get("smtp_user") {
        let user = v
            .as_str()
            .map(str::trim)
            .ok_or_else(|| AppError::bad_request("smtp_user must be a string", request_id, None))?;
        if user.chars().count() > 255 {
            return Err(AppError::bad_request(
                "smtp_user must be at most 255 characters",
                request_id,
                None,
            ));
        }
        if next.smtp_user != user {
            next.smtp_user = user.to_string();
            changed.push("smtp_user".to_string());
        }
    }
    // smtp_pass: 敏感凭据。若传非 null 字符串则更新密码（空字符串允许清空密码）；缺字段或 null 保持原值。
    if let Some(v) = flat.get("smtp_pass") {
        if !v.is_null() {
            let pass = v.as_str().ok_or_else(|| {
                AppError::bad_request("smtp_pass must be a string or null", request_id, None)
            })?;
            if next.smtp_pass != pass {
                next.smtp_pass = pass.to_string();
                changed.push("smtp_pass".to_string());
            }
        }
    }
    if let Some(v) = flat.get("smtp_from_email") {
        let email = v.as_str().map(str::trim).ok_or_else(|| {
            AppError::bad_request("smtp_from_email must be a string", request_id, None)
        })?;
        if email.chars().count() > 255 {
            return Err(AppError::bad_request(
                "smtp_from_email must be at most 255 characters",
                request_id,
                None,
            ));
        }
        if !email.is_empty() && (!email.contains('@') || email.contains(char::is_whitespace)) {
            return Err(AppError::bad_request(
                "smtp_from_email must be a valid email address",
                request_id,
                None,
            ));
        }
        if next.smtp_from_email != email {
            next.smtp_from_email = email.to_string();
            changed.push("smtp_from_email".to_string());
        }
    }
    if let Some(v) = flat.get("smtp_from_name") {
        let name = v.as_str().map(str::trim).ok_or_else(|| {
            AppError::bad_request("smtp_from_name must be a string", request_id, None)
        })?;
        if name.chars().count() > 255 {
            return Err(AppError::bad_request(
                "smtp_from_name must be at most 255 characters",
                request_id,
                None,
            ));
        }
        if next.smtp_from_name != name {
            next.smtp_from_name = name.to_string();
            changed.push("smtp_from_name".to_string());
        }
    }
    if let Some(v) = flat.get("smtp_encryption") {
        let enc = v.as_str().map(str::trim).ok_or_else(|| {
            AppError::bad_request("smtp_encryption must be a string", request_id, None)
        })?;
        let lower = enc.to_ascii_lowercase();
        if !["none", "starttls", "tls"].contains(&lower.as_str()) {
            return Err(AppError::bad_request(
                "smtp_encryption must be 'none', 'starttls', or 'tls'",
                request_id,
                None,
            ));
        }
        if next.smtp_encryption != lower {
            next.smtp_encryption = lower;
            changed.push("smtp_encryption".to_string());
        }
    }

    // 全列 UPDATE + version 乐观锁（0 行受影响 = 并发冲突 → 409）。
    let sql = "UPDATE site_settings
        SET open_registration = ?, email_verification = ?, anonymous_replies = ?, public_rss = ?,
            maintenance_mode = ?, site_name = ?, default_lang = ?, public_source = ?,
            api_rate_limit = ?, smtp_enabled = ?, smtp_host = ?, smtp_port = ?,
            smtp_user = ?, smtp_pass = ?, smtp_from_email = ?, smtp_from_name = ?,
            smtp_encryption = ?, version = version + 1, updated_at = ?
        WHERE id = 'singleton' AND version = ?";
    let now = now_millis();
    let affected = match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(next.open_registration)
            .bind(next.email_verification)
            .bind(next.anonymous_replies)
            .bind(next.public_rss)
            .bind(next.maintenance_mode)
            .bind(&next.site_name)
            .bind(&next.default_lang)
            .bind(&next.public_source)
            .bind(next.api_rate_limit)
            .bind(next.smtp_enabled)
            .bind(&next.smtp_host)
            .bind(next.smtp_port)
            .bind(&next.smtp_user)
            .bind(&next.smtp_pass)
            .bind(&next.smtp_from_email)
            .bind(&next.smtp_from_name)
            .bind(&next.smtp_encryption)
            .bind(now)
            .bind(if_match)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
        Either::Right(p) => sqlx::query(sql)
            .bind(next.open_registration)
            .bind(next.email_verification)
            .bind(next.anonymous_replies)
            .bind(next.public_rss)
            .bind(next.maintenance_mode)
            .bind(&next.site_name)
            .bind(&next.default_lang)
            .bind(&next.public_source)
            .bind(next.api_rate_limit)
            .bind(next.smtp_enabled)
            .bind(&next.smtp_host)
            .bind(next.smtp_port)
            .bind(&next.smtp_user)
            .bind(&next.smtp_pass)
            .bind(&next.smtp_from_email)
            .bind(&next.smtp_from_name)
            .bind(&next.smtp_encryption)
            .bind(now)
            .bind(if_match)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
    };

    if affected == 0 {
        return Err(AppError::conflict(
            "settings version mismatch (reload and retry)",
            request_id,
        ));
    }

    AuditEntry::user_action(&user.id, "admin.settings.update")
        .with_target("site_settings", "singleton")
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "changed": changed }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let row = load_site_settings(pool, request_id).await?;
    let resp = (
        StatusCode::OK,
        Json(json!({
            "settings": settings_json(&row),
            "version": row.version,
            "updated_at": row.updated_at,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 帖子管理 ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct AdminPostsQuery {
    /// 状态筛选：all|published|hidden|draft|pending_review|deleted
    /// （pending_review → status='draft' AND review_status='pending_review'，
    /// 见 0046 风险审核；deleted → deleted_at 非空）。
    #[serde(default)]
    status: Option<String>,
    /// 板块 slug 过滤。
    #[serde(default)]
    board: Option<String>,
    /// 标题模糊搜索。
    #[serde(default)]
    q: Option<String>,
    /// keyset 游标：上一页最后一条 created_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_posts_limit")]
    limit: i64,
}

fn default_posts_limit() -> i64 {
    30
}

/// 管理帖子列表行（布尔态按同义列投影，见模块注释）。
#[derive(sqlx::FromRow)]
struct AdminPostRow {
    id: String,
    title: String,
    author_username: Option<String>,
    board_slug: Option<String>,
    board_name: Option<String>,
    status: String,
    review_status: String,
    view_count: i64,
    created_at: i64,
    /// featured_at IS NOT NULL（同义列投影，SQLite/MySQL 均返回 0/1）。
    is_featured: i64,
    /// posts.pinned（0003 既有布尔列）。
    is_pinned: i64,
    /// closed_at IS NOT NULL（STATE-MACHINES §Post：closed_at 非空即锁帖）。
    is_locked: i64,
}

/// GET /api/v1/admin/posts — 管理帖子列表（post.moderate）。
///
/// 默认返回未删除帖子（deleted_at IS NULL）；status=deleted 查看已删帖。
async fn list_admin_posts(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<AdminPostsQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_admin_posts";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "post.moderate", request_id).await?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let after = parse_after_cursor(&query.after, request_id)?;
    let board = query.board.filter(|s| !s.is_empty());
    let q = query.q.filter(|s| !s.is_empty());
    let status = query.status.filter(|s| !s.is_empty());
    // status 直接映射（published/hidden/draft/locked → posts.status 列）；
    // pending_review 是 review_status 上的复合条件（见 AdminPostsQuery 注释）。
    let status_literal = status.as_deref().and_then(|s| match s {
        "published" | "hidden" | "draft" | "locked" => Some(s),
        _ => None,
    });

    // 动态 WHERE（条件与绑定顺序一致）。
    let mut sql = String::from(
        "SELECT p.id, p.title, u.username_normalized AS author_username, b.slug AS board_slug,
                b.name AS board_name,
                p.status, p.review_status, p.view_count, p.created_at,
                (p.featured_at IS NOT NULL) AS is_featured,
                p.pinned AS is_pinned,
                (p.closed_at IS NOT NULL) AS is_locked
         FROM posts p
         LEFT JOIN users u ON u.id = p.author_id
         LEFT JOIN boards b ON b.id = p.board_id
         WHERE 1 = 1",
    );
    match status.as_deref() {
        None | Some("all") => sql.push_str(" AND p.deleted_at IS NULL"),
        Some("deleted") => sql.push_str(" AND p.deleted_at IS NOT NULL"),
        Some("pending_review") => {
            sql.push_str(" AND p.deleted_at IS NULL AND p.status = 'draft' AND p.review_status = 'pending_review'")
        }
        Some(_) => {
            if status_literal.is_some() {
                sql.push_str(" AND p.deleted_at IS NULL AND p.status = ?")
            }
        }
    }
    if board.is_some() {
        sql.push_str(" AND p.board_id = (SELECT id FROM boards WHERE slug = ?)");
    }
    if q.is_some() {
        sql.push_str(" AND p.title LIKE ? ESCAPE '!'");
    }
    if after.is_some() {
        sql.push_str(" AND p.created_at < ?");
    }
    sql.push_str(" ORDER BY p.created_at DESC, p.id DESC LIMIT ?");

    let q_pattern = q.as_ref().map(|s| format!("%{}%", like_escape(s)));
    let fetch_limit = limit + 1;
    let rows: Vec<AdminPostRow> = match pool {
        Either::Left(p) => {
            let mut query = sqlx::query_as::<_, AdminPostRow>(&sql);
            if let Some(s) = status_literal {
                query = query.bind(s);
            }
            if let Some(b) = &board {
                query = query.bind(b);
            }
            if let Some(pat) = &q_pattern {
                query = query.bind(pat);
            }
            if let Some(v) = after {
                query = query.bind(v);
            }
            query.bind(fetch_limit).fetch_all(p).await
        }
        Either::Right(p) => {
            let mut query = sqlx::query_as::<_, AdminPostRow>(&sql);
            if let Some(s) = status_literal {
                query = query.bind(s);
            }
            if let Some(b) = &board {
                query = query.bind(b);
            }
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
    let page: Vec<AdminPostRow> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| r.created_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };

    let items: Vec<Value> = page
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "title": r.title,
                "author_username": r.author_username,
                "board_slug": r.board_slug,
                "board_name": r.board_name,
                "status": r.status,
                "review_status": r.review_status,
                "is_featured": r.is_featured != 0,
                "is_pinned": r.is_pinned != 0,
                "is_locked": r.is_locked != 0,
                "view_count": r.view_count,
                "created_at": r.created_at,
            })
        })
        .collect();

    // Tab 计数（视觉对齐 M17-GAPFIX-07）：全局按状态聚合（忽略 board/q/after），
    // 口径与各 status 分支一致。
    let counts_sql = "SELECT
            COUNT(*) AS all_count,
            COALESCE(SUM(CASE WHEN p.status = 'draft' AND p.review_status = 'pending_review' THEN 1 ELSE 0 END), 0) AS pending_review,
            COALESCE(SUM(CASE WHEN p.status = 'published' THEN 1 ELSE 0 END), 0) AS published,
            COALESCE(SUM(CASE WHEN p.featured_at IS NOT NULL AND p.status = 'published' THEN 1 ELSE 0 END), 0) AS featured,
            COALESCE(SUM(CASE WHEN p.status = 'hidden' THEN 1 ELSE 0 END), 0) AS hidden,
            COALESCE(SUM(CASE WHEN p.deleted_at IS NOT NULL THEN 1 ELSE 0 END), 0) AS deleted
         FROM posts p";
    let counts = match pool {
        Either::Left(p) => sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64)>(counts_sql)
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64)>(counts_sql)
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };
    let counts_json = json!({
        "all": counts.0,
        "pending_review": counts.1,
        "published": counts.2,
        "featured": counts.3,
        "hidden": counts.4,
        "deleted": counts.5,
    });

    let resp = (
        StatusCode::OK,
        Json(json!({ "items": items, "next_cursor": next_cursor, "counts": counts_json })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

/// 帖子当前状态（action 前置读取 + 响应投影）。
#[derive(sqlx::FromRow)]
struct AdminPostStateRow {
    id: String,
    status: String,
    review_status: String,
    is_featured: i64,
    is_pinned: i64,
    is_locked: i64,
}

/// 读取帖子当前管理状态（含布尔态同义列投影）。
async fn load_post_state(
    pool: &crate::db::DatabasePool,
    post_id: &str,
    request_id: &str,
) -> Result<Option<AdminPostStateRow>, AppError> {
    let sql = "SELECT id, status, review_status,
                      (featured_at IS NOT NULL) AS is_featured,
                      pinned AS is_pinned,
                      (closed_at IS NOT NULL) AS is_locked
               FROM posts WHERE id = ?";
    let row = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, AdminPostStateRow>(sql)
                .bind(post_id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, AdminPostStateRow>(sql)
                .bind(post_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(row)
}

/// POST /api/v1/admin/posts/{id}/action — 帖子管理动作（post.moderate）。
///
/// - approve/reject 操作 review_status（pending_review 队列的放行/退回：
///   approve → published + published_at（若空）；reject → 退回普通草稿，
///   作者可修改后重新提交；下架请用 hide）；
/// - hide/restore/delete 操作 status（restore 仅作用于 hidden/deleted，
///   避免把未过审草稿直接发布；与 moderation_cases 流程的差异：本端点是
///   管理台轻量动作，不重跑风险策略——需要案件档案/修订的走 moderation）；
/// - feature/unfeature、pin/unpin、lock/unlock 操作同义列
///   （featured_at / pinned+pinned_at / closed_at）。
///
/// 全部动作写审计（action = admin.post.<action>，带 from 状态与 reason）。
async fn admin_post_action(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "admin_post_action";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "post.moderate", request_id).await?;

    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&req, request_id)?;
    let action = req
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    let Some(current) = load_post_state(pool, &id, request_id).await? else {
        return Err(AppError::not_found("post not found", request_id));
    };

    let now = now_millis();
    // 动作 → SQL（restore 的 WHERE 守卫见注释；其余按目标态覆写）。
    let sql: &str = match action.as_str() {
        "approve" => {
            "UPDATE posts SET status = 'published', review_status = 'none',
                 published_at = COALESCE(published_at, ?), updated_at = ?
             WHERE id = ?"
        }
        "reject" => {
            "UPDATE posts SET status = 'draft', review_status = 'none', updated_at = ?
             WHERE id = ?"
        }
        "hide" => "UPDATE posts SET status = 'hidden', updated_at = ? WHERE id = ?",
        "restore" => {
            "UPDATE posts SET status = 'published', deleted_at = NULL, updated_at = ?
             WHERE id = ? AND status IN ('hidden', 'deleted')"
        }
        "delete" => {
            "UPDATE posts SET status = 'deleted', deleted_at = ?, updated_at = ?
             WHERE id = ?"
        }
        "feature" => "UPDATE posts SET featured_at = ?, updated_at = ? WHERE id = ?",
        "unfeature" => "UPDATE posts SET featured_at = NULL, updated_at = ? WHERE id = ?",
        "pin" => "UPDATE posts SET pinned = 1, pinned_at = ?, updated_at = ? WHERE id = ?",
        "unpin" => "UPDATE posts SET pinned = 0, pinned_at = NULL, updated_at = ? WHERE id = ?",
        "lock" => "UPDATE posts SET closed_at = ?, updated_at = ? WHERE id = ?",
        "unlock" => "UPDATE posts SET closed_at = NULL, updated_at = ? WHERE id = ?",
        other => {
            return Err(AppError::bad_request(
                format!("unknown action: {other}"),
                request_id,
                None,
            ))
        }
    };

    // 绑定序列按 SQL 占位符顺序（三参数动作为 (now, now, id)）。
    let affected = match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(sql);
            match action.as_str() {
                "approve" | "delete" | "feature" | "pin" | "lock" => {
                    q = q.bind(now).bind(now).bind(&id)
                }
                _ => q = q.bind(now).bind(&id),
            }
            q.execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
        Either::Right(p) => {
            let mut q = sqlx::query(sql);
            match action.as_str() {
                "approve" | "delete" | "feature" | "pin" | "lock" => {
                    q = q.bind(now).bind(now).bind(&id)
                }
                _ => q = q.bind(now).bind(&id),
            }
            q.execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
    };

    if affected == 0 {
        // restore 的状态守卫未命中（帖子不是 hidden/deleted）。
        if action == "restore" {
            return Err(AppError::bad_request(
                "post is not hidden or deleted; nothing to restore",
                request_id,
                None,
            ));
        }
        return Err(AppError::not_found("post not found", request_id));
    }

    AuditEntry::user_action(&user.id, format!("admin.post.{action}").as_str())
        .with_target("post", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "action": action, "from_status": current.status }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let row = load_post_state(pool, &id, request_id)
        .await?
        .ok_or_else(|| AppError::internal("post state disappeared after action", request_id))?;
    let resp = (
        StatusCode::OK,
        Json(json!({
            "id": row.id,
            "status": row.status,
            "review_status": row.review_status,
            "is_featured": row.is_featured != 0,
            "is_pinned": row.is_pinned != 0,
            "is_locked": row.is_locked != 0,
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 通知广播 ───────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct OutboxQuery {
    /// keyset 游标：上一页最后一条 created_at（毫秒）。
    #[serde(default)]
    after: Option<String>,
    #[serde(default = "default_outbox_limit")]
    limit: i64,
}

fn default_outbox_limit() -> i64 {
    30
}

/// 广播发件箱行。
#[derive(sqlx::FromRow)]
struct BroadcastRow {
    id: String,
    title: String,
    body: String,
    target_count: i64,
    created_at: i64,
}

/// GET /api/v1/admin/notifications/outbox — 广播发件箱（admin.manage）。
async fn list_broadcast_outbox(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<OutboxQuery>,
) -> Result<Response, AppError> {
    let request_id = "list_broadcast_outbox";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let limit = query.limit.clamp(1, MAX_LIST_LIMIT);
    let after = parse_after_cursor(&query.after, request_id)?;
    let sql = "SELECT id, title, body, target_count, created_at
               FROM notification_broadcasts
               WHERE (? IS NULL OR created_at < ?)
               ORDER BY created_at DESC, id DESC
               LIMIT ?";
    let fetch_limit = limit + 1;
    let rows: Vec<BroadcastRow> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, BroadcastRow>(sql)
                .bind(after)
                .bind(after)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, BroadcastRow>(sql)
                .bind(after)
                .bind(after)
                .bind(fetch_limit)
                .fetch_all(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let has_more = rows.len() as i64 > limit;
    let page: Vec<BroadcastRow> = rows.into_iter().take(limit as usize).collect();
    let next_cursor = if has_more {
        page.last()
            .map(|r| r.created_at.to_string())
            .unwrap_or_default()
    } else {
        String::new()
    };
    let items: Vec<Value> = page
        .iter()
        .map(|r| {
            json!({
                "id": r.id,
                "title": r.title,
                "body": r.body,
                "target_count": r.target_count,
                "created_at": r.created_at,
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

/// 广播请求体。
#[derive(Deserialize)]
struct BroadcastRequest {
    title: String,
    body: String,
    client_request_id: String,
    /// 目标受众（M17-GAPFIX-07）：all=全体 active（默认）| admins=管理员角色。
    #[serde(default)]
    target: Option<String>,
    /// 契约 body 未强制 reason（GAP-FIX 管理域）；提供则写入审计。
    #[serde(default)]
    reason: Option<String>,
}

/// POST /api/v1/admin/notifications/broadcast — 全站广播（admin.manage）。
///
/// 流程：幂等门（scope `admin.broadcast`，key=client_request_id）→ 先写
/// notification_broadcasts 行（target_count=0）→ 对 status='active' 且未删除
/// 用户分批（500/批）插 notifications（type/category='system'，携带
/// broadcast_id）→ 回填 target_count=实际插入数 → 审计 → 201。
async fn broadcast_notification(
    State(state): State<AppState>,
    auth: AuthSession,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "broadcast_notification";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let req: BroadcastRequest = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let title_len = req.title.chars().count();
    if !(1..=100).contains(&title_len) {
        return Err(AppError::bad_request(
            "title must be 1-100 characters",
            request_id,
            None,
        ));
    }
    let body_len = req.body.chars().count();
    if !(1..=2000).contains(&body_len) {
        return Err(AppError::bad_request(
            "body must be 1-2000 characters",
            request_id,
            None,
        ));
    }
    let target = req.target.as_deref().unwrap_or("all");
    if !matches!(target, "all" | "admins") {
        return Err(AppError::bad_request(
            "target must be 'all' or 'admins'",
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

    // 幂等门（request_hash 覆盖原始请求体）。
    let hash = request_hash(&body);
    let idem_key = IdempotencyKey::new("admin.broadcast", &req.client_request_id)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let outcome = begin_or_replay(
        pool,
        &idem_key,
        &hash,
        IDEMPOTENCY_TTL_MS,
        FailureCachePolicy::Cache,
    )
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match outcome {
        IdempotencyOutcome::Created { record_id } => {
            let now = now_millis();
            let broadcast_id = uuid::Uuid::now_v7().to_string();

            // 1) 广播行（target_count 稍后回填）。
            let insert_broadcast =
                "INSERT INTO notification_broadcasts (id, title, body, target_count, created_at)
                                    VALUES (?, ?, ?, 0, ?)";
            match pool {
                Either::Left(p) => {
                    sqlx::query(insert_broadcast)
                        .bind(&broadcast_id)
                        .bind(&req.title)
                        .bind(&req.body)
                        .bind(now)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                }
                Either::Right(p) => {
                    sqlx::query(insert_broadcast)
                        .bind(&broadcast_id)
                        .bind(&req.title)
                        .bind(&req.body)
                        .bind(now)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                }
            }

            // 2) active 用户分批插入通知。
            let target_count =
                insert_broadcast_notifications(pool, &broadcast_id, &req, now, request_id).await?;

            // 3) 回填实际插入数。
            let update_count = "UPDATE notification_broadcasts SET target_count = ? WHERE id = ?";
            match pool {
                Either::Left(p) => {
                    sqlx::query(update_count)
                        .bind(target_count)
                        .bind(&broadcast_id)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                }
                Either::Right(p) => {
                    sqlx::query(update_count)
                        .bind(target_count)
                        .bind(&broadcast_id)
                        .execute(p)
                        .await
                        .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                }
            }

            // 4) 幂等完成（response_reference = broadcast_id，重放回读用）。
            complete(pool, &record_id, &broadcast_id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            // 5) 审计（reason 可选：契约 body 未强制，提供则记录）。
            let mut entry = AuditEntry::user_action(&user.id, "admin.notification.broadcast")
                .with_target("notification_broadcast", &broadcast_id)
                .with_policy_version(AUTHZ_POLICY_VERSION)
                .with_metadata(json!({
                    "title": req.title,
                    "target_count": target_count,
                }));
            if let Some(reason) = req.reason.as_deref().filter(|r| !r.trim().is_empty()) {
                entry = entry.with_reason(reason);
            }
            entry
                .record(pool)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            let resp = (
                StatusCode::CREATED,
                Json(json!({ "id": broadcast_id, "target_count": target_count })),
            )
                .into_response();
            Ok(private_no_store(resp))
        }
        IdempotencyOutcome::Replay { response_reference } => {
            // 同 key+摘要重放：返回原广播的 {id, target_count}。
            if let Some(broadcast_id) = response_reference {
                let sql = "SELECT id, target_count FROM notification_broadcasts WHERE id = ?";
                let row = match pool {
                    Either::Left(p) => {
                        sqlx::query_as::<_, (String, i64)>(sql)
                            .bind(&broadcast_id)
                            .fetch_optional(p)
                            .await
                    }
                    Either::Right(p) => {
                        sqlx::query_as::<_, (String, i64)>(sql)
                            .bind(&broadcast_id)
                            .fetch_optional(p)
                            .await
                    }
                }
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                if let Some((id, target_count)) = row {
                    let resp = (
                        StatusCode::CREATED,
                        Json(json!({ "id": id, "target_count": target_count })),
                    )
                        .into_response();
                    return Ok(private_no_store(resp));
                }
            }
            Err(AppError::conflict(
                "idempotent replay but original broadcast not found",
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
            "previous attempt failed; retry with a new idempotency key",
            request_id,
        )),
    }
}

/// 分批插入广播通知（每批 BROADCAST_BATCH 行多行 VALUES）；返回实际插入数。
async fn insert_broadcast_notifications(
    pool: &crate::db::DatabasePool,
    broadcast_id: &str,
    req: &BroadcastRequest,
    now: i64,
    request_id: &str,
) -> Result<i64, AppError> {
    // 广播目标：active 且未删除用户（target=admins 时限管理员角色）。
    let target = req.target.as_deref().unwrap_or("all");
    let users_sql = if target == "admins" {
        "SELECT id FROM users WHERE status = 'active' AND deleted_at IS NULL
             AND EXISTS (SELECT 1 FROM user_roles ur JOIN roles r ON r.id = ur.role_id
                 WHERE ur.user_id = users.id AND r.name = 'administrator')"
    } else {
        "SELECT id FROM users WHERE status = 'active' AND deleted_at IS NULL"
    };
    let user_ids: Vec<String> = match pool {
        Either::Left(p) => sqlx::query_scalar(users_sql).fetch_all(p).await,
        Either::Right(p) => sqlx::query_scalar(users_sql).fetch_all(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let mut inserted: i64 = 0;
    for chunk in user_ids.chunks(BROADCAST_BATCH) {
        // 每行绑定 6 个参数（id/user_id/title/body/created_at/broadcast_id）；
        // type/category 为常量 'system'（notifications CHECK 值域内）。
        let mut sql = String::from(
            "INSERT INTO notifications (id, user_id, type, title, body, link, is_read, created_at, broadcast_id, category) VALUES ",
        );
        for i in 0..chunk.len() {
            if i > 0 {
                sql.push(',');
            }
            sql.push_str("(?, ?, 'system', ?, ?, NULL, 0, ?, ?, 'system')");
        }
        let rows = match pool {
            Either::Left(p) => {
                let mut q = sqlx::query(&sql);
                for uid in chunk {
                    q = q
                        .bind(uuid::Uuid::now_v7().to_string())
                        .bind(uid)
                        .bind(&req.title)
                        .bind(&req.body)
                        .bind(now)
                        .bind(broadcast_id);
                }
                q.execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
                    .rows_affected()
            }
            Either::Right(p) => {
                let mut q = sqlx::query(&sql);
                for uid in chunk {
                    q = q
                        .bind(uuid::Uuid::now_v7().to_string())
                        .bind(uid)
                        .bind(&req.title)
                        .bind(&req.body)
                        .bind(now)
                        .bind(broadcast_id);
                }
                q.execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?
                    .rows_affected()
            }
        };
        inserted += rows as i64;
    }
    Ok(inserted)
}

/// POST /api/v1/admin/notifications/outbox/{id}/recall — 撤回广播（admin.manage）。
///
/// 删除该广播产生且未读（is_read=0）的通知行（已读保留——用户已消费的
/// 记录不追溯删除）；按 broadcast_id 精确关联（0061 加列），不做 title 匹配。
async fn recall_broadcast(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "recall_broadcast";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&req, request_id)?;

    // 广播必须存在（404 而非静默成功，避免误拼 id 误报成功）。
    let exists: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT 1 FROM notification_broadcasts WHERE id = ?")
                .bind(&id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT 1 FROM notification_broadcasts WHERE id = ?")
                .bind(&id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    if exists.is_none() {
        return Err(AppError::not_found("broadcast not found", request_id));
    }

    let deleted = match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM notifications WHERE broadcast_id = ? AND is_read = 0")
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM notifications WHERE broadcast_id = ? AND is_read = 0")
                .bind(&id)
                .execute(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?
                .rows_affected()
        }
    };

    AuditEntry::user_action(&user.id, "admin.notification.recall")
        .with_target("notification_broadcast", &id)
        .with_reason(&reason)
        .with_policy_version(AUTHZ_POLICY_VERSION)
        .with_metadata(json!({ "deleted_unread": deleted }))
        .record(pool)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    let resp = StatusCode::NO_CONTENT.into_response();
    Ok(private_no_store(resp))
}

// ─── 通知模板注册表（视觉对齐 M17-GAPFIX-07） ───────────────────────────────

/// 内置通知模板（代码内常量，如实反映现有触发点；模板内容在各写入点内联，
/// 尚无模板表——在线编辑需 0064 模板表 + 渲染管线，属后续扩展）。
const NOTIFICATION_TEMPLATES: &[(&str, &str, &str)] = &[
    (
        "security.new_device",
        "新设备登录提醒",
        "账号安全：新设备登录时写入登录账号的收件箱",
    ),
    (
        "security.password_changed",
        "密码已更改提醒",
        "账号安全：密码重置/修改后通知本人",
    ),
    (
        "security.mfa_changed",
        "MFA 设置变更提醒",
        "账号安全：TOTP 启用/取消后通知本人",
    ),
    (
        "security.session_revoked",
        "会话撤销提醒",
        "账号安全：设备被逐出后通知本人",
    ),
    (
        "security.recovery_code_used",
        "恢复码使用告警",
        "账号安全：恢复码被使用（疑似接管）时告警",
    ),
    (
        "achievements.unlocked",
        "成就解锁通知",
        "激励：成就解锁时通知本人（含奖励明细）",
    ),
    (
        "admin.broadcast",
        "全站广播",
        "运营：管理员全员广播（outbox 发件箱，可召回未读）",
    ),
    (
        "economy.system_notice",
        "积分/经济系统通知",
        "激励：积分调整等经济事件按 category 通知本人",
    ),
];

/// GET /api/v1/admin/notifications/templates — 通知模板注册表 + 队列概况
/// （admin.manage）。只读：模板内容内联于各触发点（无模板表）；站内通知
/// 直达收件箱（无外发投递队列，失败恒 0；SMTP 接入后扩展投递表）。
async fn list_notification_templates(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "listNotificationTemplates";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let templates: Vec<Value> = NOTIFICATION_TEMPLATES
        .iter()
        .map(|(id, name, trigger)| {
            json!({
                "id": id,
                "name": name,
                "trigger": trigger,
                "channel": "站内信",
                "queue": "直达收件箱",
            })
        })
        .collect();

    // 队列概况：广播发件箱行数（notification_broadcasts）。
    let outbox_count: i64 = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM notification_broadcasts")
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(p) => sqlx::query_scalar("SELECT COUNT(*) FROM notification_broadcasts")
            .fetch_one(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    };

    let resp = (
        StatusCode::OK,
        Json(json!({
            "templates": templates,
            "queue": {
                "mode": "in_app_direct",
                "failed": 0,
                "outbox_count": outbox_count,
            },
        })),
    )
        .into_response();
    Ok(private_no_store(resp))
}

// ─── 角色分配 ───────────────────────────────────────────────────────────────

/// 用户当前全局角色名列表（user_roles × roles，0021_rbac 既有读取路径——
/// admin 用户投影 roles 字段同源，授予/撤销后立即可见）。
async fn roles_for_user(
    pool: &crate::db::DatabasePool,
    user_id: &str,
) -> Result<Vec<String>, AppError> {
    let sql = "SELECT r.name FROM user_roles ur JOIN roles r ON r.id = ur.role_id
               WHERE ur.user_id = ? ORDER BY r.name";
    let rows: Vec<(String,)> = match pool {
        Either::Left(p) => sqlx::query_as(sql).bind(user_id).fetch_all(p).await,
        Either::Right(p) => sqlx::query_as(sql).bind(user_id).fetch_all(p).await,
    }
    .map_err(|e| AppError::internal(e.to_string(), "listUserRoles"))?;
    Ok(rows.into_iter().map(|(name,)| name).collect())
}

/// 目标用户存在且未删除（404 语义）。
async fn user_exists(
    pool: &crate::db::DatabasePool,
    user_id: &str,
    request_id: &str,
) -> Result<bool, AppError> {
    let hit: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT 1 FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(user_id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT 1 FROM users WHERE id = ? AND deleted_at IS NULL")
                .bind(user_id)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(hit.is_some())
}

/// 按名称查角色 id（roles.name 唯一，0021_rbac）。
async fn role_id_by_name(
    pool: &crate::db::DatabasePool,
    role_name: &str,
    request_id: &str,
) -> Result<Option<String>, AppError> {
    let hit: Option<String> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
                .bind(role_name)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
                .bind(role_name)
                .fetch_optional(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    Ok(hit)
}

/// POST /api/v1/admin/users/{id}/roles — 授予全局角色（role.manage）。
///
/// 写 user_roles（0021 既有表：user_id/role_id/granted_by/granted_at）；
/// 幂等（已有分配直接返回当前角色列表）；禁止给自己分配（防止
/// role.manage 持有者自我提权，模式同 moderation 的 own-content 守卫）。
async fn assign_user_role(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "assign_user_role";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "role.manage", request_id).await?;

    let req: Value = serde_json::from_slice(&body)
        .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    let reason = required_reason(&req, request_id)?;
    let role_name = req
        .get("role_name")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();
    if role_name.is_empty() || role_name.len() > 64 {
        return Err(AppError::bad_request(
            "role_name required",
            request_id,
            None,
        ));
    }

    if !user_exists(pool, &id, request_id).await? {
        return Err(AppError::not_found("user not found", request_id));
    }
    if id == user.id {
        return Err(AppError::forbidden(
            "cannot assign roles to yourself",
            request_id,
        ));
    }
    let Some(role_id) = role_id_by_name(pool, &role_name, request_id).await? else {
        return Err(AppError::not_found("role not found", request_id));
    };

    // 幂等：已有分配直接返回（不重复插入、不重复审计）。
    let already: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_roles WHERE user_id = ? AND role_id = ?")
                .bind(&id)
                .bind(&role_id)
                .fetch_one(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM user_roles WHERE user_id = ? AND role_id = ?")
                .bind(&id)
                .bind(&role_id)
                .fetch_one(p)
                .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    if already == 0 {
        let now = now_millis();
        let insert = "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                      VALUES (?, ?, ?, ?, NULL)";
        match pool {
            Either::Left(p) => {
                sqlx::query(insert)
                    .bind(&id)
                    .bind(&role_id)
                    .bind(&user.id)
                    .bind(now)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
            Either::Right(p) => {
                sqlx::query(insert)
                    .bind(&id)
                    .bind(&role_id)
                    .bind(&user.id)
                    .bind(now)
                    .execute(p)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            }
        }

        AuditEntry::user_action(&user.id, "admin.user.role.grant")
            .with_target("user", &id)
            .with_reason(&reason)
            .with_policy_version(AUTHZ_POLICY_VERSION)
            .with_metadata(json!({ "role_name": role_name }))
            .record(pool)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    }

    let roles = roles_for_user(pool, &id).await?;
    let resp = (StatusCode::OK, Json(json!({ "roles": roles }))).into_response();
    Ok(private_no_store(resp))
}

/// DELETE /api/v1/admin/users/{id}/roles/{role_name} — 撤销全局角色（role.manage）。
///
/// body {reason}（审计必填）；幂等（未持有该角色时同样 200 返回当前角色）。
async fn revoke_user_role(
    State(state): State<AppState>,
    auth: AuthSession,
    Path((id, role_name)): Path<(String, String)>,
    body: axum::body::Bytes,
) -> Result<Response, AppError> {
    let request_id = "revoke_user_role";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "role.manage", request_id).await?;

    let req: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body)
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?
    };
    let reason = required_reason(&req, request_id)?;

    if !user_exists(pool, &id, request_id).await? {
        return Err(AppError::not_found("user not found", request_id));
    }
    let Some(role_id) = role_id_by_name(pool, &role_name, request_id).await? else {
        return Err(AppError::not_found("role not found", request_id));
    };

    let deleted = match pool {
        Either::Left(p) => sqlx::query("DELETE FROM user_roles WHERE user_id = ? AND role_id = ?")
            .bind(&id)
            .bind(&role_id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
        Either::Right(p) => sqlx::query("DELETE FROM user_roles WHERE user_id = ? AND role_id = ?")
            .bind(&id)
            .bind(&role_id)
            .execute(p)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?
            .rows_affected(),
    };

    if deleted > 0 {
        AuditEntry::user_action(&user.id, "admin.user.role.revoke")
            .with_target("user", &id)
            .with_reason(&reason)
            .with_policy_version(AUTHZ_POLICY_VERSION)
            .with_metadata(json!({ "role_name": role_name }))
            .record(pool)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
    }

    let roles = roles_for_user(pool, &id).await?;
    let resp = (StatusCode::OK, Json(json!({ "roles": roles }))).into_response();
    Ok(private_no_store(resp))
}

// ─── 公开统计 ───────────────────────────────────────────────────────────────

/// GET /api/v1/stats — 公开站点统计（无需登录）。
///
/// 公开可见口径：members=active 未删除用户、posts=published 未删除帖子、
/// comments=published 未删除评论、boards=启用未删除板块、tags=启用标签。
/// 计数为全量 COUNT（无 WHERE 参数，不受查询注入面影响）。
async fn get_public_stats(State(state): State<AppState>) -> Result<Response, AppError> {
    let request_id = "get_public_stats";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let sql = "SELECT
        (SELECT COUNT(*) FROM users WHERE status = 'active' AND deleted_at IS NULL) AS members,
        (SELECT COUNT(*) FROM posts WHERE status = 'published' AND deleted_at IS NULL) AS posts,
        (SELECT COUNT(*) FROM comments WHERE status = 'published' AND deleted_at IS NULL) AS comments,
        (SELECT COUNT(*) FROM boards WHERE is_active = 1 AND deleted_at IS NULL) AS boards,
        (SELECT COUNT(*) FROM tags WHERE is_active = 1) AS tags";
    let row = match pool {
        Either::Left(p) => {
            let r = sqlx::query(sql)
                .fetch_one(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            (
                r.get::<i64, _>("members"),
                r.get::<i64, _>("posts"),
                r.get::<i64, _>("comments"),
                r.get::<i64, _>("boards"),
                r.get::<i64, _>("tags"),
            )
        }
        Either::Right(p) => {
            let r = sqlx::query(sql)
                .fetch_one(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            (
                r.get::<i64, _>("members"),
                r.get::<i64, _>("posts"),
                r.get::<i64, _>("comments"),
                r.get::<i64, _>("boards"),
                r.get::<i64, _>("tags"),
            )
        }
    };
    let (members, posts, comments, boards, tags) = row;

    // 公开计数可缓存 60s（GAP-FIX 公开统计契约）。
    let mut resp = (
        StatusCode::OK,
        Json(json!({
            "members": members,
            "posts": posts,
            "comments": comments,
            "boards": boards,
            "tags": tags,
        })),
    )
        .into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=60"),
    );
    Ok(resp)
}
