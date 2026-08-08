//! `/metrics` — 受控 Prometheus 文本指标端点（M15-OBSERVE-04/05）。
//!
//! - 仅 loopback / 受控监控可访问（M15-PACKAGE-07）：非 loopback 来源返回
//!   404（隐藏端点存在），响应不含内部诊断细节；
//! - 连接池、队列深度、Outbox 堆积为抓取时计算的 gauge；HTTP/领域计数器由
//!   [`crate::observability::metrics::registry()`] 维护；
//! - Caddy 模板不代理 `/metrics`（见 `deploy/Caddyfile.template`）。

use std::net::SocketAddr;

use axum::{
    body::Body,
    extract::{ConnectInfo, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
};

use crate::app::AppState;
use crate::db::pool::DatabasePool;
use crate::jobs::dispatch::WORKER_QUEUES;
use crate::observability::metrics::registry;

/// Prometheus 文本格式渲染（`text/plain; version=0.0.4`）。
pub async fn metrics(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
) -> Response {
    // M15-PACKAGE-07：只对 loopback/受控监控开放；其他来源 404 隐藏端点。
    if !addr.ip().is_loopback() {
        return (StatusCode::NOT_FOUND, "not found").into_response();
    }

    update_db_gauges(&state).await;
    update_queue_gauges(&state).await;

    let body = registry().render_prometheus();
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(Body::from(body))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// 连接池 gauge（M15-OBSERVE-04：DB pool size/idle/max）。
async fn update_db_gauges(state: &AppState) {
    let Some(pool) = &state.db else {
        return;
    };
    let pool: &DatabasePool = pool.as_ref();
    match pool {
        DatabasePool::Left(p) => {
            registry().set_gauge("bblbb_db_pool_size", p.size() as i64);
            registry().set_gauge("bblbb_db_pool_idle", p.num_idle() as i64);
            registry().set_gauge(
                "bblbb_db_pool_max",
                p.options().get_max_connections() as i64,
            );
        }
        DatabasePool::Right(p) => {
            registry().set_gauge("bblbb_db_pool_size", p.size() as i64);
            registry().set_gauge("bblbb_db_pool_idle", p.num_idle() as i64);
            registry().set_gauge(
                "bblbb_db_pool_max",
                p.options().get_max_connections() as i64,
            );
        }
    }
}

/// 队列/Outbox gauge（M15-OBSERVE-06：dead Job 与堆积告警的输入）。
async fn update_queue_gauges(state: &AppState) {
    let Some(pool) = &state.db else {
        return;
    };
    let mut queued = 0i64;
    let mut running = 0i64;
    let mut dead = 0i64;
    for queue in WORKER_QUEUES {
        if let Ok(snapshot) = crate::jobs::metrics::snapshot(pool, queue).await {
            queued += snapshot.queued;
            running += snapshot.running;
            dead += snapshot.dead;
        }
    }
    registry().set_gauge("bblbb_jobs_queued", queued);
    registry().set_gauge("bblbb_jobs_running", running);
    registry().set_gauge("bblbb_jobs_dead", dead);

    let pending = crate::outbox::pending_count(pool).await.unwrap_or(-1);
    registry().set_gauge("bblbb_outbox_pending", pending);
    let failed: Result<i64, sqlx::Error> = match pool.as_ref() {
        DatabasePool::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE status = 'failed'")
                .fetch_one(p)
                .await
        }
        DatabasePool::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE status = 'failed'")
                .fetch_one(p)
                .await
        }
    };
    registry().set_gauge("bblbb_outbox_failed", failed.unwrap_or(-1));
}
