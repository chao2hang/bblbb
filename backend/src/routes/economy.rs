//! M07-LEVELS 用户侧路由：活动汇总与自动签到打卡。
//!
//! - `GET /api/v1/activity/summary`（get_activity_summary）：当前用户今日签到
//!   状态、连续天数、等级（服务端裁决权益）与经验/余额投影。
//! - `POST /api/v1/activity/visit`（record_visit / `recordAuthenticatedVisit`）：
//!   每日首次有效业务页面访问自动签到（M07-LEVELS-03）——校验登录用户 +
//!   业务页面 + 非 prefetch + 非爬虫/静态资源/健康检查，然后原子领取（幂等
//!   去重 + 账本），返回今日是否首次/奖励数额（`ActivityVisitResult`）。

use axum::{
    extract::State,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};

use crate::{
    app::AppState,
    auth::session::AuthSession,
    authz::decision::AUTHZ_POLICY_VERSION,
    authz::enforce::authorize_action,
    economy::activity::{
        checkin::{validate_visit, VisitContext, VisitRejection},
        service::{activity_summary, claim_check_in, ActivityError},
    },
    error::AppError,
    outbox::now_millis,
};

/// 签到限流：每用户 240 次/小时（正常浏览远低于此；防批量开页刷连续天数）。
const VISIT_USER_LIMIT: u32 = 240;
const VISIT_USER_WINDOW_MS: i64 = 60 * 60 * 1000;
/// 签到限流：每 IP 2400 次/小时（批量账号/异常设备抑制）。
const VISIT_IP_LIMIT: u32 = 2400;
const VISIT_IP_WINDOW_MS: i64 = 60 * 60 * 1000;

/// 活跃/签到路由（M07-LEVELS）。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/activity/summary", get(get_activity_summary))
        .route("/api/v1/activity/visit", post(record_visit))
}

/// GET /api/v1/activity/summary — 今日签到状态/连续天数/等级/经验投影。
async fn get_activity_summary(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Json<Value>, AppError> {
    let request_id = "get_activity_summary";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let body = activity_summary(pool, &user.id, now_millis())
        .await
        .map_err(|e| map_activity_error(e, request_id))?;
    Ok(Json(body))
}

/// POST /api/v1/activity/visit — 记录有效业务页面访问并自动签到。
///
/// 校验（M07-LEVELS-03）：登录用户 + 业务页面路径 + 非 prefetch/静态资源/
/// 爬虫/健康检查；失败请求（4xx）不进入领取路径。领取幂等（M07-LEVELS-05）：
/// 同日重复访问返回 `checked_in_today=true` 且 `today_earned=[]`，不重复奖励。
async fn record_visit(
    State(state): State<AppState>,
    auth: AuthSession,
    headers: axum::http::HeaderMap,
    Json(body): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "record_visit";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let decision = authorize_action(
        pool,
        &user.id,
        "activity.claim_own",
        None,
        AUTHZ_POLICY_VERSION,
    )
    .await
    .map_err(|e| AppError::internal(e, request_id))?;
    if !decision.is_allowed() {
        return Err(AppError::forbidden(
            "activity.claim_own permission required",
            request_id,
        ));
    }

    // 反刷限流（M07-LEVELS-07）：按用户与 IP 固定窗口。
    let now = now_millis();
    let ip = client_ip(&headers).unwrap_or_else(|| "unknown".to_string());
    let user_status = state.limiter.check(
        &format!("visit:user:{}", user.id),
        VISIT_USER_LIMIT,
        VISIT_USER_WINDOW_MS,
        now,
    );
    if !user_status.allowed {
        return Err(AppError::rate_limited(
            "too many page visits, try again later",
            request_id,
            user_status.retry_after_secs,
            user_status.limit,
            user_status.remaining,
            user_status.reset_at_ms / 1000,
        ));
    }
    let ip_status = state.limiter.check(
        &format!("visit:ip:{ip}"),
        VISIT_IP_LIMIT,
        VISIT_IP_WINDOW_MS,
        now,
    );
    if !ip_status.allowed {
        return Err(AppError::rate_limited(
            "too many page visits, try again later",
            request_id,
            ip_status.retry_after_secs,
            ip_status.limit,
            ip_status.remaining,
            ip_status.reset_at_ms / 1000,
        ));
    }

    // 访问校验（M07-LEVELS-03）。
    let path = body
        .get("path")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let user_agent = headers.get("user-agent").and_then(|v| v.to_str().ok());
    let sec_purpose = headers.get("sec-purpose").and_then(|v| v.to_str().ok());
    let purpose = headers
        .get("purpose")
        .and_then(|v| v.to_str().ok())
        .or_else(|| headers.get("x-purpose").and_then(|v| v.to_str().ok()));
    let sec_fetch_dest = headers.get("sec-fetch-dest").and_then(|v| v.to_str().ok());
    let ctx = VisitContext {
        path: &path,
        user_agent,
        sec_purpose,
        purpose,
        sec_fetch_dest,
    };
    if let Err(rej) = validate_visit(&ctx) {
        return Err(visit_rejection_error(rej, request_id));
    }

    // 签到领取（幂等 + 账本 + 等级重建）。
    let outcome = claim_check_in(pool, &user.id, now)
        .await
        .map_err(|e| map_activity_error(e, request_id))?;

    let body = json!({
        "checked_in_today": outcome.checked_in_today,
        "streak_days": outcome.streak_days,
        "today_earned": outcome.today_earned.iter().map(|r| r.to_value()).collect::<Vec<_>>(),
        "point_operation_id": outcome.point_operation_id,
        "activity_day": outcome.activity_day,
        "timezone": outcome.timezone.to_value(),
    });
    let mut resp = (StatusCode::OK, Json(body)).into_response();
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    Ok(resp)
}

/// 访问拒绝 → 安全错误（不暴露风控细节）。
fn visit_rejection_error(rej: VisitRejection, request_id: &'static str) -> AppError {
    match rej {
        VisitRejection::Malformed => AppError::bad_request(
            "path must be a non-empty absolute URL path",
            request_id,
            None,
        ),
        VisitRejection::NotBusinessPage => AppError::bad_request(
            "visit is not a business page; check-in requires a business page",
            request_id,
            None,
        ),
        VisitRejection::NotEligible => {
            AppError::bad_request("request not eligible for check-in", request_id, None)
        }
    }
}

/// 活动错误 → Problem detail（500/404/400/409；不泄漏内部规则细节）。
fn map_activity_error(e: ActivityError, request_id: &'static str) -> AppError {
    match e {
        ActivityError::Db(msg) | ActivityError::Ledger(msg) => AppError::internal(msg, request_id),
        ActivityError::NotFound(msg) => AppError::not_found(msg, request_id),
        ActivityError::Invalid(msg) => AppError::bad_request(msg, request_id, None),
        // M16-HARNESS-04：稳定 Problem code（docs/ERROR-CODES.md）。
        ActivityError::AlreadyClaimed => AppError::with_code(
            axum::http::StatusCode::CONFLICT,
            "activity_already_claimed",
            "Conflict",
            "today's activity is already claimed",
            request_id,
        ),
        ActivityError::NotEligible(msg) => AppError::with_code(
            axum::http::StatusCode::CONFLICT,
            "activity_not_eligible",
            "Conflict",
            msg,
            request_id,
        ),
    }
}

/// 客户端 IP（可信代理链优先；测试/本地缺省 unknown）。
fn client_ip(headers: &axum::http::HeaderMap) -> Option<String> {
    for name in ["x-forwarded-for", "cf-connecting-ip", "x-real-ip"] {
        if let Some(v) = headers.get(name).and_then(|v| v.to_str().ok()) {
            if let Some(first) = v.split(',').next() {
                let first = first.trim();
                if !first.is_empty() {
                    return Some(first.to_string());
                }
            }
        }
    }
    None
}
