//! AI 逐次同意（M09-SCHEMA-02 / M09-GATEWAY-08/09）。
//!
//! `ai_consents` 表：(user_id, provider_id, purpose) 唯一；`full_with_consent`
//! 才记录（低风险 metadata 不需要 consent）。同意撤回后禁止新任务。

use sqlx::Either;

use crate::db::DatabasePool;
use crate::outbox::now_millis;

use super::TaskKind;

/// Consent 稳定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConsentError {
    NotFound(String),
    Invalid(String),
    /// 用户已存在同 (provider, purpose) 未撤销的同意。
    AlreadyGranted,
    Db(String),
}

impl From<sqlx::Error> for ConsentError {
    fn from(e: sqlx::Error) -> Self {
        ConsentError::Db(e.to_string())
    }
}

/// 授予同意（幂等 upsert：已存在则更新 disclosure 信息；未撤销时返回 AlreadyGranted
/// 由调用方决定是否视为成功重放）。
#[allow(clippy::too_many_arguments)] // 有界同意 API：全部参数均必需且显式
pub async fn grant_consent(
    pool: &DatabasePool,
    user_id: &str,
    provider_id: &str,
    purpose: TaskKind,
    disclosure_version: i64,
    disclosure_hash: &str,
    disclosure_text: &str,
    scope: &str,
    now: i64,
) -> Result<String, ConsentError> {
    if disclosure_version < 1 {
        return Err(ConsentError::Invalid(
            "disclosure_version must be >= 1".into(),
        ));
    }
    if disclosure_hash.len() > 64 {
        return Err(ConsentError::Invalid("disclosure_hash too long".into()));
    }
    // 已存在未撤销 → AlreadyGranted（调用方可做幂等重放处理）。
    if let Some(id) = active_consent_id(pool, user_id, provider_id, purpose, now).await? {
        return Ok(id);
    }
    let id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(db) => {
            // 幂等 upsert：已存在行（含已撤销）→ 复用并解除撤销、刷新披露信息。
            sqlx::query(
                "INSERT INTO ai_consents
                     (id, user_id, provider_id, purpose, data_mode, disclosure_version, disclosure_hash, disclosure_text, scope, granted_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'full_with_consent', ?, ?, ?, ?, ?, ?, ?)
                 ON CONFLICT(user_id, provider_id, purpose) DO UPDATE SET
                     disclosure_version = excluded.disclosure_version,
                     disclosure_hash = excluded.disclosure_hash,
                     disclosure_text = excluded.disclosure_text,
                     scope = excluded.scope,
                     granted_at = excluded.granted_at,
                     revoked_at = NULL,
                     revoke_reason = NULL,
                     updated_at = excluded.updated_at",
            )
            .bind(&id)
            .bind(user_id)
            .bind(provider_id)
            .bind(purpose.as_str())
            .bind(disclosure_version)
            .bind(disclosure_hash)
            .bind(disclosure_text)
            .bind(scope)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(db)
            .await?;
        }
        Either::Right(db) => {
            sqlx::query(
                "INSERT INTO ai_consents
                     (id, user_id, provider_id, purpose, data_mode, disclosure_version, disclosure_hash, disclosure_text, scope, granted_at, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'full_with_consent', ?, ?, ?, ?, ?, ?, ?)
                 ON DUPLICATE KEY UPDATE
                     disclosure_version = VALUES(disclosure_version),
                     disclosure_hash = VALUES(disclosure_hash),
                     disclosure_text = VALUES(disclosure_text),
                     scope = VALUES(scope),
                     granted_at = VALUES(granted_at),
                     revoked_at = NULL,
                     revoke_reason = NULL,
                     updated_at = VALUES(updated_at)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(provider_id)
            .bind(purpose.as_str())
            .bind(disclosure_version)
            .bind(disclosure_hash)
            .bind(disclosure_text)
            .bind(scope)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(db)
            .await?;
        }
    }
    // 返回既有行 id（upsert 可能复用已撤销的行，而非本次生成的 id）。
    find_consent_id(pool, user_id, provider_id, purpose)
        .await?
        .ok_or_else(|| ConsentError::Db("consent row missing after upsert".into()))
}

/// 按 (user, provider, purpose) 查任意行 id（含已撤销；供 upsert 后回读）。
async fn find_consent_id(
    pool: &DatabasePool,
    user_id: &str,
    provider_id: &str,
    purpose: TaskKind,
) -> Result<Option<String>, ConsentError> {
    match pool {
        Either::Left(db) => sqlx::query_scalar(
            "SELECT id FROM ai_consents WHERE user_id = ? AND provider_id = ? AND purpose = ?
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(provider_id)
        .bind(purpose.as_str())
        .fetch_optional(db)
        .await
        .map_err(ConsentError::from),
        Either::Right(db) => sqlx::query_scalar(
            "SELECT id FROM ai_consents WHERE user_id = ? AND provider_id = ? AND purpose = ?
             ORDER BY created_at DESC LIMIT 1",
        )
        .bind(user_id)
        .bind(provider_id)
        .bind(purpose.as_str())
        .fetch_optional(db)
        .await
        .map_err(ConsentError::from),
    }
}

async fn active_consent_id(
    pool: &DatabasePool,
    user_id: &str,
    provider_id: &str,
    purpose: TaskKind,
    _now: i64,
) -> Result<Option<String>, ConsentError> {
    match pool {
        Either::Left(db) => sqlx::query_scalar(
            "SELECT id FROM ai_consents WHERE user_id = ? AND provider_id = ? AND purpose = ? AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(provider_id)
        .bind(purpose.as_str())
        .fetch_optional(db)
        .await
        .map_err(ConsentError::from),
        Either::Right(db) => sqlx::query_scalar(
            "SELECT id FROM ai_consents WHERE user_id = ? AND provider_id = ? AND purpose = ? AND revoked_at IS NULL",
        )
        .bind(user_id)
        .bind(provider_id)
        .bind(purpose.as_str())
        .fetch_optional(db)
        .await
        .map_err(ConsentError::from),
    }
}

/// 用户对 (provider, purpose) 是否有未撤销同意（外发前裁决）。
pub async fn has_active_consent(
    pool: &DatabasePool,
    user_id: &str,
    provider_id: &str,
    purpose: TaskKind,
    now: i64,
) -> Result<bool, ConsentError> {
    Ok(active_consent_id(pool, user_id, provider_id, purpose, now)
        .await?
        .is_some())
}

/// 读取同意投影（用户侧）。
pub async fn consent_for(
    pool: &DatabasePool,
    user_id: &str,
    provider_id: &str,
    purpose: TaskKind,
) -> Result<Option<serde_json::Value>, ConsentError> {
    match pool {
        Either::Left(db) => {
            let row = sqlx::query(
                "SELECT id, disclosure_version, disclosure_hash, disclosure_text, scope, granted_at, revoked_at, revoke_reason
                 FROM ai_consents WHERE user_id = ? AND provider_id = ? AND purpose = ? ORDER BY created_at DESC LIMIT 1",
            )
            .bind(user_id)
            .bind(provider_id)
            .bind(purpose.as_str())
            .fetch_optional(db)
            .await
            .map_err(ConsentError::from)?;
            let Some(row) = row else {
                return Ok(None);
            };
            use sqlx::Row;
            Ok(Some(serde_json::json!({
                "id": row.get::<String,_>("id"),
                "disclosure_version": row.get::<i64,_>("disclosure_version"),
                "disclosure_hash": row.get::<String,_>("disclosure_hash"),
                "disclosure_text": row.get::<String,_>("disclosure_text"),
                "scope": row.get::<String,_>("scope"),
                "granted_at": row.get::<i64,_>("granted_at"),
                "revoked_at": row.get::<Option<i64>,_>("revoked_at"),
                "revoke_reason": row.get::<Option<String>,_>("revoke_reason"),
            })))
        }
        Either::Right(db) => {
            let row = sqlx::query(
                "SELECT id, disclosure_version, disclosure_hash, disclosure_text, scope, granted_at, revoked_at, revoke_reason
                 FROM ai_consents WHERE user_id = ? AND provider_id = ? AND purpose = ? ORDER BY created_at DESC LIMIT 1",
            )
            .bind(user_id)
            .bind(provider_id)
            .bind(purpose.as_str())
            .fetch_optional(db)
            .await
            .map_err(ConsentError::from)?;
            let Some(row) = row else {
                return Ok(None);
            };
            use sqlx::Row;
            Ok(Some(serde_json::json!({
                "id": row.get::<String,_>("id"),
                "disclosure_version": row.get::<i64,_>("disclosure_version"),
                "disclosure_hash": row.get::<String,_>("disclosure_hash"),
                "disclosure_text": row.get::<String,_>("disclosure_text"),
                "scope": row.get::<String,_>("scope"),
                "granted_at": row.get::<i64,_>("granted_at"),
                "revoked_at": row.get::<Option<i64>,_>("revoked_at"),
                "revoke_reason": row.get::<Option<String>,_>("revoke_reason"),
            })))
        }
    }
}

/// 撤回同意（幂等；已撤回视为成功）。返回受影响行数。
pub async fn revoke_consent(
    pool: &DatabasePool,
    user_id: &str,
    provider_id: &str,
    purpose: TaskKind,
    reason: &str,
    now: i64,
) -> Result<i64, ConsentError> {
    match pool {
        Either::Left(db) => {
            let r = sqlx::query(
                "UPDATE ai_consents SET revoked_at = ?, revoke_reason = ?, updated_at = ?
                 WHERE user_id = ? AND provider_id = ? AND purpose = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(reason)
            .bind(now)
            .bind(user_id)
            .bind(provider_id)
            .bind(purpose.as_str())
            .execute(db)
            .await?;
            Ok(r.rows_affected() as i64)
        }
        Either::Right(db) => {
            let r = sqlx::query(
                "UPDATE ai_consents SET revoked_at = ?, revoke_reason = ?, updated_at = ?
                 WHERE user_id = ? AND provider_id = ? AND purpose = ? AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(reason)
            .bind(now)
            .bind(user_id)
            .bind(provider_id)
            .bind(purpose.as_str())
            .execute(db)
            .await?;
            Ok(r.rows_affected() as i64)
        }
    }
}

/// 当前时间（毫秒）。
pub fn now() -> i64 {
    now_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn grant_consent_is_idempotent_and_revocable() {
        let dir = std::env::temp_dir().join(format!("bblbb-ai-consent-{}", uuid::Uuid::now_v7()));
        let url = format!("sqlite://{}", dir.display());
        let pool = crate::db::pool::create_pool(&url).await.unwrap();
        let files = crate::db::migrate::read_migration_files(
            &std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap())
                .join("../migrations/sqlite"),
        )
        .unwrap();
        crate::db::migrate::run_migrations(&pool, &files)
            .await
            .unwrap();
        crate::authz::roles::seed_builtin_roles(&pool)
            .await
            .unwrap();

        let now = now();
        let uid = "u-1";
        let pid = "p-1";
        // 外键前置：user + provider 必须先存在。
        use sqlx::Either;
        let sqlite = match &pool {
            Either::Left(p) => p,
            Either::Right(_) => panic!("SQLite only"),
        };
        let _ = sqlx::query(
            "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level,
                email_verified, email_verified_at, created_at, updated_at)
             VALUES ('u-1', 'u1', 'u1@example.com', 'dummy', 'active', 5, 1, ?, ?, ?)",
        )
        .bind(now - 30 * 24 * 3600 * 1000)
        .bind(now)
        .bind(now)
        .execute(sqlite)
        .await
        .unwrap();
        let _ = sqlx::query(
            "INSERT INTO ai_providers
                (id, name, adapter_type, base_url, api_type, default_model, status, secret_configured, data_mode,
                 timeout_ms, max_input_tokens, max_output_tokens, max_concurrency, version, created_at, updated_at)
             VALUES ('p-1', 'mock', 'openai_compatible', 'https://api.mock.example/v1', 'chat', 'mock-model', 'enabled', 0,
                 'redacted', 15000, 8000, 2000, 4, 1, ?, ?)",
        )
        .bind(now)
        .bind(now)
        .execute(sqlite)
        .await
        .unwrap();

        let a = grant_consent(
            &pool,
            uid,
            pid,
            TaskKind::Formatting,
            1,
            "hash1",
            "text",
            "per_task",
            now,
        )
        .await
        .unwrap();
        // 幂等重放 → 同一 id。
        let b = grant_consent(
            &pool,
            uid,
            pid,
            TaskKind::Formatting,
            1,
            "hash1",
            "text",
            "per_task",
            now,
        )
        .await
        .unwrap();
        assert_eq!(a, b);
        assert!(
            has_active_consent(&pool, uid, pid, TaskKind::Formatting, now)
                .await
                .unwrap()
        );
        // 撤回后无有效同意；再次授予是幂等 upsert——同一行解除撤销并刷新披露。
        let affected = revoke_consent(&pool, uid, pid, TaskKind::Formatting, "test", now)
            .await
            .unwrap();
        assert_eq!(affected, 1);
        assert!(
            !has_active_consent(&pool, uid, pid, TaskKind::Formatting, now)
                .await
                .unwrap()
        );
        let c = grant_consent(
            &pool,
            uid,
            pid,
            TaskKind::Formatting,
            2,
            "hash2",
            "text2",
            "per_task",
            now,
        )
        .await
        .unwrap();
        // 同一 (user, provider, purpose) 唯一约束 → upsert 复用同一行（重新生效）。
        assert_eq!(a, c);
        let v = consent_for(&pool, uid, pid, TaskKind::Formatting)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(v["disclosure_version"], 2);
        let _ = std::fs::remove_file(&dir);
        let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
        let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
    }
}
