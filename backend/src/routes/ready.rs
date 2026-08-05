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
/// 检查数据库连接、迁移状态（版本比对）、存储目录和必要密钥
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

    Json(ReadyResponse {
        status: overall,
        checks: ReadyChecks {
            database: db_status,
            migrations: migrations_status,
            storage_dir: storage_status,
        },
    })
}

/// 迁移就绪状态（M00-BACKEND-07/08）
///
/// 比较 migrations 目录中最大版本与数据库 `_sqlx_migrations` 已应用的最大版本：
/// - 数据库未配置或迁移目录无文件 → `skip`
/// - 目录不可读 → `error`
/// - 已应用版本落后 → `behind`（尚未执行全部迁移，含全新数据库）
/// - 已应用版本超前 → `ahead`
/// - 已应用版本一致 → `ok`
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
    let max_available = files
        .iter()
        .map(|f| f.version)
        .max()
        .expect("files is non-empty");

    match crate::db::migrate::max_applied_version(pool).await {
        Ok(Some(applied)) if applied as u64 == max_available => "ok",
        Ok(Some(applied)) if applied as u64 > max_available => "ahead",
        Ok(_) => "behind",
        Err(e) => {
            tracing::warn!(error = %e, "migration version check failed in /readyz");
            "error"
        }
    }
}
