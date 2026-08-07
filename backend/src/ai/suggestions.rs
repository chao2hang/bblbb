//! AI 建议（M09-SUGGESTIONS）：模型输出解析/校验、建议落库与人工采纳。
//!
//! - 模型输出是不可信数据：解析为结构化 JSON，拒绝结构不符/超限/混入 HTML/脚本；
//! - formatting/SEO/tagging/moderation 各自版本化 schema（`schema_version`）；
//! - 采纳时重新鉴权 + `base_revision`/If-Match + Markdown 安全 + 幂等。

use serde_json::{json, Value};
use sqlx::{Either, Row};

use crate::db::DatabasePool;
use crate::outbox::now_millis;

use super::SUGGESTION_SCHEMA_VERSION;

/// Suggestion 稳定错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SuggestionError {
    NotFound(String),
    Invalid(String),
    /// base_revision 落后于当前内容（版本冲突）。
    VersionConflict {
        expected: i64,
        actual: i64,
    },
    /// 越权（非作者且非授权审核员）。
    Forbidden(String),
    /// 重复采纳（幂等成功重放）。
    AlreadyAccepted,
    Db(String),
}

impl From<sqlx::Error> for SuggestionError {
    fn from(e: sqlx::Error) -> Self {
        SuggestionError::Db(e.to_string())
    }
}

/// 建议种类（与 ai_suggestions.suggestion_type 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuggestionKind {
    Formatting,
    Seo,
    Tagging,
    Moderation,
}

impl SuggestionKind {
    pub const ALL: [SuggestionKind; 4] = [
        SuggestionKind::Formatting,
        SuggestionKind::Seo,
        SuggestionKind::Tagging,
        SuggestionKind::Moderation,
    ];

    pub fn as_str(&self) -> &'static str {
        match self {
            SuggestionKind::Formatting => "formatting",
            SuggestionKind::Seo => "seo",
            SuggestionKind::Tagging => "tagging",
            SuggestionKind::Moderation => "moderation",
        }
    }

    pub fn parse(value: &str) -> Option<SuggestionKind> {
        Self::ALL.iter().find(|v| v.as_str() == value).copied()
    }
}

/// 模型输出解析（M09-SUGGESTIONS-01）：返回结构化的 suggestion payload。
///
/// 校验规则：
/// - 顶层必须是 JSON 对象；
/// - 不能包含 `<script`、`javascript:`、`data:` 等 HTML/脚本注入形态；
/// - 字段长度/数量上限；
/// - 未知字段忽略（不信任模型 schema）。
pub fn parse_suggestion_payload(raw: &str, kind: SuggestionKind) -> Result<Value, SuggestionError> {
    if raw.len() > 120_000 {
        return Err(SuggestionError::Invalid(
            "suggestion payload too large".into(),
        ));
    }
    let v: Value = serde_json::from_str(raw)
        .map_err(|e| SuggestionError::Invalid(format!("invalid model output: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| SuggestionError::Invalid("suggestion must be a JSON object".into()))?;
    if obj.len() > 32 {
        return Err(SuggestionError::Invalid("too many fields".into()));
    }
    // 禁止注入形态。
    let flat = raw.to_lowercase();
    for needle in [
        "<script",
        "javascript:",
        "data:text/html",
        "<iframe",
        "onerror=",
    ] {
        if flat.contains(needle) {
            return Err(SuggestionError::Invalid("injection marker detected".into()));
        }
    }
    // 按 kind 校验必要字段。
    match kind {
        SuggestionKind::Formatting => {
            require_text(obj, "content", 100_000)?;
        }
        SuggestionKind::Seo => {
            for field in ["title", "summary"] {
                if let Some(v) = obj.get(field) {
                    if !v.is_string() {
                        return Err(SuggestionError::Invalid(format!("{field} must be string")));
                    }
                }
            }
        }
        SuggestionKind::Tagging => {
            if let Some(v) = obj.get("tags") {
                let arr = v
                    .as_array()
                    .ok_or_else(|| SuggestionError::Invalid("tags must be array".into()))?;
                if arr.len() > 20 {
                    return Err(SuggestionError::Invalid("too many tags".into()));
                }
            }
        }
        SuggestionKind::Moderation => {
            for field in ["risk_categories", "score", "recommendation"] {
                require_text(obj, field, 1000)?;
            }
        }
    }
    Ok(v)
}

fn require_text(
    obj: &serde_json::Map<String, Value>,
    field: &str,
    max_len: usize,
) -> Result<(), SuggestionError> {
    match obj.get(field) {
        Some(Value::String(s)) if s.chars().count() <= max_len => Ok(()),
        Some(Value::String(_)) => Err(SuggestionError::Invalid(format!("{field} too long"))),
        _ => Err(SuggestionError::Invalid(format!("missing {field}"))),
    }
}

/// 校验并落库建议（幂等：同 task 已存在则返回既有）。
#[allow(clippy::too_many_arguments)] // 有界建议 API：全部参数均必需且显式
pub async fn create_suggestion(
    pool: &DatabasePool,
    task_id: &str,
    kind: SuggestionKind,
    target_type: &str,
    target_id: &str,
    user_id: &str,
    base_revision: i64,
    payload: &Value,
    now: i64,
) -> Result<Value, SuggestionError> {
    if base_revision < 0 {
        return Err(SuggestionError::Invalid(
            "base_revision must be >= 0".into(),
        ));
    }
    if let Some(existing) = find_by_task(pool, task_id).await? {
        return Ok(existing);
    }
    let id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(db) => {
            let insert = sqlx::query(
                "INSERT INTO ai_suggestions
                     (id, task_id, suggestion_type, target_type, target_id, user_id, schema_version, base_revision, payload_json, decision, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(&id)
            .bind(task_id)
            .bind(kind.as_str())
            .bind(target_type)
            .bind(target_id)
            .bind(user_id)
            .bind(SUGGESTION_SCHEMA_VERSION)
            .bind(base_revision)
            .bind(payload.to_string())
            .bind(now)
            .bind(now)
            .execute(db)
            .await;
            if let Err(e) = insert {
                if !is_unique_violation(&e) {
                    return Err(SuggestionError::Db(e.to_string()));
                }
            }
        }
        Either::Right(db) => {
            let insert = sqlx::query(
                "INSERT INTO ai_suggestions
                     (id, task_id, suggestion_type, target_type, target_id, user_id, schema_version, base_revision, payload_json, decision, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 'pending', ?, ?)",
            )
            .bind(&id)
            .bind(task_id)
            .bind(kind.as_str())
            .bind(target_type)
            .bind(target_id)
            .bind(user_id)
            .bind(SUGGESTION_SCHEMA_VERSION)
            .bind(base_revision)
            .bind(payload.to_string())
            .bind(now)
            .bind(now)
            .execute(db)
            .await;
            if let Err(e) = insert {
                if !is_unique_violation(&e) {
                    return Err(SuggestionError::Db(e.to_string()));
                }
            }
        }
    }
    find_by_task(pool, task_id)
        .await?
        .ok_or_else(|| SuggestionError::Db("suggestion missing after insert".into()))
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.is_unique_violation())
}

async fn find_by_task(
    pool: &DatabasePool,
    task_id: &str,
) -> Result<Option<Value>, SuggestionError> {
    let row = match pool {
        Either::Left(db) => sqlx::query(
            "SELECT id, suggestion_type, target_type, target_id, user_id, schema_version, base_revision, payload_json, decision, accepted_fields_json, accepted_at, created_at
             FROM ai_suggestions WHERE task_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(db)
        .await
        .map_err(SuggestionError::from)?
        .map(suggestion_projection),
        Either::Right(db) => sqlx::query(
            "SELECT id, suggestion_type, target_type, target_id, user_id, schema_version, base_revision, payload_json, decision, accepted_fields_json, accepted_at, created_at
             FROM ai_suggestions WHERE task_id = ? ORDER BY created_at DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(db)
        .await
        .map_err(SuggestionError::from)?
        .map(suggestion_projection_mysql),
    };
    Ok(row)
}

fn suggestion_projection(row: sqlx::sqlite::SqliteRow) -> Value {
    json!({
        "id": row.get::<String,_>("id"),
        "suggestion_type": row.get::<String,_>("suggestion_type"),
        "target_type": row.get::<String,_>("target_type"),
        "target_id": row.get::<String,_>("target_id"),
        "user_id": row.get::<String,_>("user_id"),
        "schema_version": row.get::<i64,_>("schema_version"),
        "base_revision": row.get::<i64,_>("base_revision"),
        "payload": serde_json::from_str::<Value>(&row.get::<String,_>("payload_json")).unwrap_or(Value::Null),
        "decision": row.get::<String,_>("decision"),
        "accepted_fields": row.get::<Option<String>,_>("accepted_fields_json").and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        "accepted_at": row.get::<Option<i64>,_>("accepted_at"),
        "created_at": row.get::<i64,_>("created_at"),
    })
}

fn suggestion_projection_mysql(row: sqlx::mysql::MySqlRow) -> Value {
    json!({
        "id": row.get::<String,_>("id"),
        "suggestion_type": row.get::<String,_>("suggestion_type"),
        "target_type": row.get::<String,_>("target_type"),
        "target_id": row.get::<String,_>("target_id"),
        "user_id": row.get::<String,_>("user_id"),
        "schema_version": row.get::<i64,_>("schema_version"),
        "base_revision": row.get::<i64,_>("base_revision"),
        "payload": serde_json::from_str::<Value>(&row.get::<String,_>("payload_json")).unwrap_or(Value::Null),
        "decision": row.get::<String,_>("decision"),
        "accepted_fields": row.get::<Option<String>,_>("accepted_fields_json").and_then(|s| serde_json::from_str::<Value>(&s).ok()),
        "accepted_at": row.get::<Option<i64>,_>("accepted_at"),
        "created_at": row.get::<i64,_>("created_at"),
    })
}

/// 读取建议（作者或授权审核员）。
pub async fn get_suggestion(
    pool: &DatabasePool,
    user_id: &str,
    suggestion_id: &str,
) -> Result<Value, SuggestionError> {
    let (suggestion_type, owner) = match pool {
        Either::Left(db) => {
            let row = sqlx::query_as::<_, (String, String)>(
                "SELECT suggestion_type, user_id FROM ai_suggestions WHERE id = ?",
            )
            .bind(suggestion_id)
            .fetch_optional(db)
            .await
            .map_err(SuggestionError::from)?;
            row.ok_or_else(|| SuggestionError::NotFound("suggestion not found".into()))?
        }
        Either::Right(db) => {
            let row = sqlx::query_as::<_, (String, String)>(
                "SELECT suggestion_type, user_id FROM ai_suggestions WHERE id = ?",
            )
            .bind(suggestion_id)
            .fetch_optional(db)
            .await
            .map_err(SuggestionError::from)?;
            row.ok_or_else(|| SuggestionError::NotFound("suggestion not found".into()))?
        }
    };
    // moderation 建议只给作者 + 授权审核员；其余仅作者。
    if owner != user_id {
        let is_moderator = crate::authz::roles::aggregate_permissions(pool, user_id, None)
            .await
            .map(|agg| agg.permissions.contains("moderation.review"))
            .unwrap_or(false);
        if suggestion_type != "moderation" || !is_moderator {
            return Err(SuggestionError::Forbidden("not allowed".into()));
        }
    }
    match pool {
        Either::Left(db) => {
            let row = sqlx::query(
                "SELECT id, suggestion_type, target_type, target_id, user_id, schema_version, base_revision, payload_json, decision, accepted_fields_json, accepted_at, created_at
                 FROM ai_suggestions WHERE id = ?",
            )
            .bind(suggestion_id)
            .fetch_one(db)
            .await
            .map_err(SuggestionError::from)?;
            Ok(suggestion_projection(row))
        }
        Either::Right(db) => {
            let row = sqlx::query(
                "SELECT id, suggestion_type, target_type, target_id, user_id, schema_version, base_revision, payload_json, decision, accepted_fields_json, accepted_at, created_at
                 FROM ai_suggestions WHERE id = ?",
            )
            .bind(suggestion_id)
            .fetch_one(db)
            .await
            .map_err(SuggestionError::from)?;
            Ok(suggestion_projection_mysql(row))
        }
    }
}

/// 采纳（M09-SUGGESTIONS-05/06）：重新鉴权 + base_revision 校验 + 幂等。
///
/// `current_revision` 为采纳目标当前内容 revision（调用方从草稿/帖子读取）；
/// `expected_base_version` 为用户提交的 If-Match 值；`selected_fields` 限制
/// 采纳字段（None = 全字段）。
#[allow(clippy::too_many_arguments)]
pub async fn accept_suggestion(
    pool: &DatabasePool,
    user_id: &str,
    suggestion_id: &str,
    expected_base_version: i64,
    current_revision: i64,
    selected_fields: Option<&[String]>,
    now: i64,
) -> Result<Value, SuggestionError> {
    if expected_base_version < 1 {
        return Err(SuggestionError::Invalid(
            "expected_base_version must be >= 1".into(),
        ));
    }
    let (base_revision, owner): (i64, String) = match pool {
        Either::Left(db) => {
            sqlx::query_as("SELECT base_revision, user_id FROM ai_suggestions WHERE id = ?")
                .bind(suggestion_id)
                .fetch_optional(db)
                .await
                .map_err(SuggestionError::from)?
                .ok_or_else(|| SuggestionError::NotFound("suggestion not found".into()))?
        }
        Either::Right(db) => {
            sqlx::query_as("SELECT base_revision, user_id FROM ai_suggestions WHERE id = ?")
                .bind(suggestion_id)
                .fetch_optional(db)
                .await
                .map_err(SuggestionError::from)?
                .ok_or_else(|| SuggestionError::NotFound("suggestion not found".into()))?
        }
    };
    if owner != user_id {
        return Err(SuggestionError::Forbidden("not the author".into()));
    }
    // 版本冲突：当前内容 revision 必须等于 expected（防覆盖新编辑）。
    if current_revision != expected_base_version || expected_base_version < base_revision {
        return Err(SuggestionError::VersionConflict {
            expected: expected_base_version,
            actual: current_revision,
        });
    }
    // 幂等：已采纳 → AlreadyAccepted。
    let decision: String = match pool {
        Either::Left(db) => sqlx::query_scalar("SELECT decision FROM ai_suggestions WHERE id = ?")
            .bind(suggestion_id)
            .fetch_one(db)
            .await
            .map_err(SuggestionError::from)?,
        Either::Right(db) => sqlx::query_scalar("SELECT decision FROM ai_suggestions WHERE id = ?")
            .bind(suggestion_id)
            .fetch_one(db)
            .await
            .map_err(SuggestionError::from)?,
    };
    if decision == "accepted" {
        return Err(SuggestionError::AlreadyAccepted);
    }
    let fields_json = selected_fields.map(|f| serde_json::to_string(f).unwrap_or("[]".into()));
    let affected = match pool {
        Either::Left(db) => {
            sqlx::query(
                "UPDATE ai_suggestions SET decision = 'accepted', accepted_fields_json = ?, accepted_at = ?, accepted_by = ?, updated_at = ?
                 WHERE id = ? AND decision = 'pending'",
            )
            .bind(&fields_json)
            .bind(now)
            .bind(user_id)
            .bind(now)
            .bind(suggestion_id)
            .execute(db)
            .await?
            .rows_affected()
        }
        Either::Right(db) => {
            sqlx::query(
                "UPDATE ai_suggestions SET decision = 'accepted', accepted_fields_json = ?, accepted_at = ?, accepted_by = ?, updated_at = ?
                 WHERE id = ? AND decision = 'pending'",
            )
            .bind(&fields_json)
            .bind(now)
            .bind(user_id)
            .bind(now)
            .bind(suggestion_id)
            .execute(db)
            .await?
            .rows_affected()
        }
    };
    if affected == 0 {
        // 并发采纳 → 幂等成功。
        return Err(SuggestionError::AlreadyAccepted);
    }
    Ok(json!({
        "id": suggestion_id,
        "decision": "accepted",
        "accepted_fields": selected_fields.map(|f| f.to_vec()).unwrap_or_default(),
        "accepted_at": now,
    }))
}

/// 输出安全校验（采纳写入前对目标字段做 Markdown/HTML 安全，M09-SUGGESTIONS-05）。
/// 返回 Err 表示字段含不允许的注入形态。
pub fn validate_suggestion(payload: &Value) -> Result<(), SuggestionError> {
    let s = payload.to_string().to_lowercase();
    for needle in [
        "<script",
        "javascript:",
        "data:text/html",
        "<iframe",
        "onerror=",
    ] {
        if s.contains(needle) {
            return Err(SuggestionError::Invalid("injection marker".into()));
        }
    }
    Ok(())
}

/// 当前时间（毫秒）。
pub fn now() -> i64 {
    now_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_accepts_valid_formatting_payload() {
        let ok = parse_suggestion_payload(
            "{\"content\":\"## 新标题\\n\\n正文\",\"changes\":[\"标题层级\"]}",
            SuggestionKind::Formatting,
        )
        .unwrap();
        assert_eq!(ok["content"], "## 新标题\n\n正文");
    }

    #[test]
    fn parse_rejects_injection_and_bad_structure() {
        assert!(matches!(
            parse_suggestion_payload(
                r#"{"content":"<script>alert(1)</script>"}"#,
                SuggestionKind::Formatting
            ),
            Err(SuggestionError::Invalid(_))
        ));
        assert!(matches!(
            parse_suggestion_payload("not json", SuggestionKind::Formatting),
            Err(SuggestionError::Invalid(_))
        ));
        assert!(matches!(
            parse_suggestion_payload(r#"{"tags":"not-array"}"#, SuggestionKind::Tagging),
            Err(SuggestionError::Invalid(_))
        ));
        assert!(matches!(
            parse_suggestion_payload(r#"{"score":5}"#, SuggestionKind::Moderation),
            Err(SuggestionError::Invalid(_))
        ));
    }

    #[test]
    fn validate_suggestion_blocks_injection() {
        assert!(validate_suggestion(&json!({"content": "hello"})).is_ok());
        assert!(validate_suggestion(&json!({"content": "x <script>y"})).is_err());
        assert!(validate_suggestion(&json!({"content": "x javascript:y"})).is_err());
    }
}
