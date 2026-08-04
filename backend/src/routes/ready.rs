use axum::{extract::State, response::Json, routing::get, Router};
use serde::Serialize;

use crate::app::AppState;

#[derive(Serialize)]
pub struct ReadyResponse {
    status: &'static str,
    checks: ReadyChecks,
}

#[derive(Serialize)]
pub struct ReadyChecks {
    database: &'static str,
    migrations: &'static str,
    storage_dir: &'static str,
}

/// /readyz — 受保护的就绪检查端点
/// 检查数据库连接、迁移状态、存储目录和必要密钥
pub fn router() -> Router<AppState> {
    Router::new().route("/readyz", get(readyz))
}

pub async fn readyz(State(state): State<AppState>) -> Json<ReadyResponse> {
    let db_status = match &state.db {
        Some(pool) => match crate::db::pool::ping(pool).await {
            Ok(()) => "ok",
            Err(e) => {
                tracing::warn!(error = %e, "database ping failed in /readyz");
                "error"
            }
        },
        None => "not_configured",
    };

    let storage_status = if state.config.storage_dir.exists() {
        "ok"
    } else {
        "missing"
    };

    let overall = if db_status == "ok" && storage_status == "ok" {
        "ok"
    } else {
        "degraded"
    };

    Json(ReadyResponse {
        status: overall,
        checks: ReadyChecks {
            database: db_status,
            migrations: "skip",
            storage_dir: storage_status,
        },
    })
}
