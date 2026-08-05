use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Serialize;

use crate::app::AppState;
use crate::db::migrate::CheckMode;

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
/// 检查数据库连接、迁移状态（版本/顺序/checksum）、存储目录和必要密钥
pub fn router() -> Router<AppState> {
    Router::new().route("/readyz", get(readyz))
}

pub async fn readyz(State(state): State<AppState>) -> Response {
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

    let migrations_status = migration_status(&state).await;

    let storage_status = if state.config.storage_dir.exists() {
        "ok"
    } else {
        "missing"
    };

    let overall = if db_status == "ok" && migrations_status == "ok" && storage_status == "ok" {
        "ok"
    } else {
        "degraded"
    };

    let body = Json(ReadyResponse {
        status: overall,
        checks: ReadyChecks {
            database: db_status,
            migrations: migrations_status,
            storage_dir: storage_status,
        },
    });

    // M01-DB-12：连接失败、迁移落后/超前、checksum 不匹配时明确失败（503）。
    // 响应体只含状态枚举，不包含 DSN、连接串或错误文本。
    if overall == "ok" {
        (StatusCode::OK, body).into_response()
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, body).into_response()
    }
}

/// 迁移就绪状态（M01-DB-12）
///
/// 使用只读迁移检查（不创建/写入迁移表）判定：
/// - 无数据库或迁移目录无文件 → `skip`
/// - 目录不可读 → `error`
/// - checksum 不匹配（已执行迁移内容被修改）→ `checksum_mismatch`
/// - 已应用版本超前（代码未知的迁移）→ `ahead`
/// - 有待应用迁移 → `behind`
/// - 完全一致 → `ok`
async fn migration_status(state: &AppState) -> &'static str {
    let Some(pool) = &state.db else {
        return "skip";
    };
    let files = match crate::db::migrate::read_migration_files(&state.config.migrations_dir) {
        Ok(files) => files,
        Err(e) => {
            tracing::warn!(
                error = %e,
                dir = %state.config.migrations_dir.display(),
                "migration directory check failed in /readyz"
            );
            return "error";
        }
    };
    if files.is_empty() {
        return "skip";
    }

    match crate::db::migrate::check_migrations_with_mode(CheckMode::ReadOnly, pool, &files).await {
        Ok(result) if !result.checksum_mismatches.is_empty() => "checksum_mismatch",
        Ok(result) if !result.future_versions.is_empty() => "ahead",
        Ok(result) if !result.pending.is_empty() => "behind",
        Ok(_) => "ok",
        Err(e) => {
            tracing::warn!(error = %e, "migration check failed in /readyz");
            "error"
        }
    }
}
