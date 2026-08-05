//! 索引幂等 Job（M03-SEARCH-STORE-06）。
//!
//! 单一 job kind `search.index`，payload `{entity_type, entity_id}`：
//!
//! - **创建/更新/恢复**：源状态通过可见性裁决（[`decide_post_indexability`]
//!   等，M03-SEARCH-STORE-05）→ 组装安全纯文本文档（`to_index_plain_text` +
//!   `vet_index_text`）→ 条件 upsert——仅当 stored.policy_revision ≤
//!   candidate.policy_revision 才应用，旧 revision 不覆盖新（docs/SEARCH.md §5）；
//! - **隐藏/删除/退出索引**：源状态被排除（status/visibility/板块/作者/停用）
//!   或源行不存在 → 从 `search_documents` 删除（FTS 由 0030 触发器同步）。
//!
//! 幂等：同一实体重复入队经 `deduplication_key`
//! （`search:index:{entity_type}:{entity_id}`）合并（待处理 Job 已存在则跳过）；
//! 执行本身幂等——upsert 可重复、delete 对缺失行无害（rows=0 视为成功）。

/// 索引 Job kind。
use serde_json::{json, Value};
use sqlx::Either;

use crate::db::DatabasePool;
use crate::jobs::retry::RetryClass;
use crate::jobs::worker::ClaimedJob;
use crate::jobs::worker_loop::JobOutcome;
use crate::outbox::now_millis;
use crate::search::gate::{
    decide_board_indexability, decide_post_indexability, decide_tag_indexability,
    decide_user_indexability, vet_index_text, IndexDecision,
};
use crate::search::{
    clean_index_text, excerpt_from_clean, policy_revision_for, to_index_plain_text, SearchDocument,
    SearchEntityType, BODY_MAX, EXCERPT_MAX, TITLE_MAX,
};

/// 索引 Job kind。
pub const INDEX_JOB_KIND: &str = "search.index";
const INDEX_QUEUE: &str = "default";

/// 入队索引 Job（幂等合并：同一实体的待处理 Job 已存在则跳过）。
///
/// 普通触发路径（测试/管理命令）使用本函数；M4/M8 写路径在业务事务内入队，
/// 复用同一 payload 契约。
pub async fn enqueue_index_job(
    pool: &DatabasePool,
    entity_type: &str,
    entity_id: &str,
) -> Result<(), String> {
    let id = uuid::Uuid::now_v7().to_string();
    let payload = json!({ "entity_type": entity_type, "entity_id": entity_id });
    let dedup = format!("search:index:{entity_type}:{entity_id}");
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO jobs
                     (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                      available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(INDEX_QUEUE)
            .bind(INDEX_JOB_KIND)
            .bind(payload.to_string())
            .bind(now)
            .bind(&dedup)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO jobs
                     (id, queue, kind, payload, payload_version, status, attempts, max_attempts,
                      available_at, deduplication_key, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 1, 'queued', 0, 5, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(INDEX_QUEUE)
            .bind(INDEX_JOB_KIND)
            .bind(payload.to_string())
            .bind(now)
            .bind(&dedup)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Worker 集成入口：解析 `search.index` Job payload 并映射为 [`JobOutcome`]。
///
/// 无效 payload / 未知实体类型 / 安全文本门拒绝 → 永久死信；数据库错误 → 重试。
pub async fn handle_index_job(pool: &DatabasePool, job: &ClaimedJob) -> JobOutcome {
    let entity_type = match job.payload.get("entity_type").and_then(Value::as_str) {
        Some(t) if !t.is_empty() => t,
        _ => {
            return permanent("search.index: invalid payload: missing entity_type");
        }
    };
    let entity_id = match job.payload.get("entity_id").and_then(Value::as_str) {
        Some(id) if !id.is_empty() => id,
        _ => {
            return permanent("search.index: invalid payload: missing entity_id");
        }
    };
    let outcome = match entity_type {
        "post" => reindex_post(pool, entity_id).await,
        "user" => reindex_user(pool, entity_id).await,
        "board" => reindex_board(pool, entity_id).await,
        "tag" => reindex_tag(pool, entity_id).await,
        other => return permanent(&format!("search.index: unknown entity_type {other}")),
    };
    match outcome {
        Ok(()) => JobOutcome::Succeeded,
        Err(e) => JobOutcome::Failed {
            class: RetryClass::Transient,
            error: format!("search.index: {e}"),
        },
    }
}

fn permanent(error: &str) -> JobOutcome {
    JobOutcome::Failed {
        class: RetryClass::Permanent,
        error: error.to_owned(),
    }
}

// ─────────────────────────── 帖子 ───────────────────────────

#[derive(sqlx::FromRow)]
struct PostSource {
    id: String,
    title: String,
    content: String,
    status: String,
    visibility: String,
    author_id: String,
    updated_at: i64,
    board_active: i64,
    board_visibility: String,
    board_updated_at: i64,
    author_status: String,
    author_deleted_at: Option<i64>,
    author_updated_at: i64,
}

async fn reindex_post(pool: &DatabasePool, post_id: &str) -> Result<(), String> {
    let row: Option<PostSource> = match pool {
        Either::Left(p) => sqlx::query_as::<_, PostSource>(
            "SELECT p.id, p.title, p.content, p.status, p.visibility, p.author_id, p.updated_at,
                    COALESCE(b.is_active, 0) AS board_active,
                    COALESCE(b.visibility, 'hidden') AS board_visibility,
                    COALESCE(b.updated_at, 0) AS board_updated_at,
                    COALESCE(u.status, 'deleted') AS author_status,
                    u.deleted_at AS author_deleted_at,
                    COALESCE(u.updated_at, 0) AS author_updated_at
             FROM posts p
             LEFT JOIN boards b ON b.id = p.board_id
             LEFT JOIN users u ON u.id = p.author_id
             WHERE p.id = ?",
        )
        .bind(post_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, PostSource>(
            "SELECT p.id, p.title, p.content, p.status, p.visibility, p.author_id, p.updated_at,
                    COALESCE(b.is_active, 0) AS board_active,
                    COALESCE(b.visibility, 'hidden') AS board_visibility,
                    COALESCE(b.updated_at, 0) AS board_updated_at,
                    COALESCE(u.status, 'deleted') AS author_status,
                    u.deleted_at AS author_deleted_at,
                    COALESCE(u.updated_at, 0) AS author_updated_at
             FROM posts p
             LEFT JOIN boards b ON b.id = p.board_id
             LEFT JOIN users u ON u.id = p.author_id
             WHERE p.id = ?",
        )
        .bind(post_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    let row = match row {
        Some(row) => row,
        None => {
            // 源行不存在（已删除）：从索引清理，幂等。
            delete_document(pool, post_id).await?;
            return Ok(());
        }
    };

    let decision = decide_post_indexability(
        &row.status,
        &row.visibility,
        row.board_active != 0,
        &row.board_visibility,
        &row.author_status,
        row.author_deleted_at,
    );
    match decision {
        IndexDecision::Excluded(_) => delete_document(pool, post_id).await,
        IndexDecision::Indexable => {
            let tags = load_post_tags(pool, post_id).await?;
            let plain_title = to_index_plain_text(&row.title, TITLE_MAX);
            let plain_content = to_index_plain_text(&row.content, BODY_MAX);
            if plain_title.is_empty() || plain_content.is_empty() {
                // 无可索引文本：从索引清理而非存储空文档。
                return delete_document(pool, post_id).await;
            }
            let body = clean_index_text(&format!("{plain_title} {plain_content}"), BODY_MAX);
            let excerpt = excerpt_from_clean(&plain_content, EXCERPT_MAX);
            // P0 门（M03-SEARCH-STORE-05）：转换结果必须通过 vet，
            // restricted 特征串/残留 HTML 一律永久拒绝，绝不入索引。
            for field in [&plain_title, &body, &excerpt] {
                vet_index_text(field).map_err(|e| format!("index text rejected: {e:?}"))?;
            }
            let source_revision = row.updated_at;
            let policy_revision = policy_revision_for(&[
                row.updated_at,
                row.board_updated_at,
                row.author_updated_at,
                row.author_deleted_at.unwrap_or(0),
            ]);
            let doc = SearchDocument::new(
                row.id.clone(),
                SearchEntityType::Post,
                plain_title,
                body,
                Some(excerpt),
                // M4 增加 post.slug 前以 id 作 URL 段占位。
                row.id.clone(),
                Some(row.author_id.clone()),
                tags,
                source_revision,
                policy_revision,
                now_millis(),
            )
            .map_err(|e| format!("invalid search document: {e:?}"))?;
            upsert_document_guarded(pool, &doc).await
        }
    }
}

async fn load_post_tags(pool: &DatabasePool, post_id: &str) -> Result<Vec<String>, String> {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, String>(
            "SELECT t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id
             WHERE pt.post_id = ? AND t.is_active = 1 ORDER BY t.name",
        )
        .bind(post_id)
        .fetch_all(p)
        .await
        .map_err(|e| e.to_string()),
        Either::Right(p) => sqlx::query_scalar::<_, String>(
            "SELECT t.name FROM post_tags pt JOIN tags t ON t.id = pt.tag_id
             WHERE pt.post_id = ? AND t.is_active = 1 ORDER BY t.name",
        )
        .bind(post_id)
        .fetch_all(p)
        .await
        .map_err(|e| e.to_string()),
    }
}

// ─────────────────────────── user / board / tag ───────────────────────────

#[derive(sqlx::FromRow)]
struct UserSource {
    id: String,
    username_normalized: String,
    display_name: Option<String>,
    bio: Option<String>,
    status: String,
    deleted_at: Option<i64>,
    updated_at: i64,
}

async fn reindex_user(pool: &DatabasePool, user_id: &str) -> Result<(), String> {
    let row: Option<UserSource> = match pool {
        Either::Left(p) => sqlx::query_as::<_, UserSource>(
            "SELECT id, username_normalized, display_name, bio, status, deleted_at, updated_at
             FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, UserSource>(
            "SELECT id, username_normalized, display_name, bio, status, deleted_at, updated_at
             FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    let row = match row {
        Some(row) => row,
        None => return delete_document(pool, user_id).await,
    };
    if !matches!(
        decide_user_indexability(&row.status, row.deleted_at),
        IndexDecision::Indexable
    ) {
        return delete_document(pool, user_id).await;
    }
    let plain_title = to_index_plain_text(
        row.display_name
            .as_deref()
            .unwrap_or(&row.username_normalized),
        TITLE_MAX,
    );
    let plain_bio = to_index_plain_text(row.bio.as_deref().unwrap_or(""), BODY_MAX);
    if plain_title.is_empty() {
        return delete_document(pool, user_id).await;
    }
    let body = clean_index_text(
        &format!("{} {plain_bio}", row.username_normalized),
        BODY_MAX,
    );
    let excerpt = excerpt_from_clean(&plain_bio, EXCERPT_MAX);
    for field in [&plain_title, &body, &excerpt] {
        vet_index_text(field).map_err(|e| format!("index text rejected: {e:?}"))?;
    }
    let policy_revision = policy_revision_for(&[row.updated_at, row.deleted_at.unwrap_or(0)]);
    let doc = SearchDocument::new(
        row.id.clone(),
        SearchEntityType::User,
        plain_title,
        body,
        Some(excerpt),
        row.username_normalized.clone(),
        None,
        vec![],
        row.updated_at,
        policy_revision,
        now_millis(),
    )
    .map_err(|e| format!("invalid search document: {e:?}"))?;
    upsert_document_guarded(pool, &doc).await
}

#[derive(sqlx::FromRow)]
struct BoardSource {
    id: String,
    name: String,
    slug: String,
    description: Option<String>,
    is_active: i64,
    visibility: String,
    updated_at: i64,
}

async fn reindex_board(pool: &DatabasePool, board_id: &str) -> Result<(), String> {
    let row: Option<BoardSource> = match pool {
        Either::Left(p) => sqlx::query_as::<_, BoardSource>(
            "SELECT id, name, slug, description, is_active, visibility, updated_at
             FROM boards WHERE id = ?",
        )
        .bind(board_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, BoardSource>(
            "SELECT id, name, slug, description, is_active, visibility, updated_at
             FROM boards WHERE id = ?",
        )
        .bind(board_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    let row = match row {
        Some(row) => row,
        None => return delete_document(pool, board_id).await,
    };
    if !matches!(
        decide_board_indexability(row.is_active != 0, &row.visibility),
        IndexDecision::Indexable
    ) {
        return delete_document(pool, board_id).await;
    }
    let plain_title = to_index_plain_text(&row.name, TITLE_MAX);
    let plain_description = to_index_plain_text(row.description.as_deref().unwrap_or(""), BODY_MAX);
    if plain_title.is_empty() {
        return delete_document(pool, board_id).await;
    }
    let body = clean_index_text(&format!("{plain_title} {plain_description}"), BODY_MAX);
    let excerpt = excerpt_from_clean(&plain_description, EXCERPT_MAX);
    for field in [&plain_title, &body, &excerpt] {
        vet_index_text(field).map_err(|e| format!("index text rejected: {e:?}"))?;
    }
    let doc = SearchDocument::new(
        row.id.clone(),
        SearchEntityType::Board,
        plain_title,
        body,
        Some(excerpt),
        row.slug.clone(),
        None,
        vec![],
        row.updated_at,
        row.updated_at,
        now_millis(),
    )
    .map_err(|e| format!("invalid search document: {e:?}"))?;
    upsert_document_guarded(pool, &doc).await
}

#[derive(sqlx::FromRow)]
struct TagSource {
    id: String,
    name: String,
    slug: Option<String>,
    description: Option<String>,
    is_active: i64,
    updated_at: i64,
}

async fn reindex_tag(pool: &DatabasePool, tag_id: &str) -> Result<(), String> {
    let row: Option<TagSource> = match pool {
        Either::Left(p) => sqlx::query_as::<_, TagSource>(
            "SELECT id, name, slug, description, is_active, updated_at FROM tags WHERE id = ?",
        )
        .bind(tag_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(p) => sqlx::query_as::<_, TagSource>(
            "SELECT id, name, slug, description, is_active, updated_at FROM tags WHERE id = ?",
        )
        .bind(tag_id)
        .fetch_optional(p)
        .await
        .map_err(|e| e.to_string())?,
    };
    let row = match row {
        Some(row) => row,
        None => return delete_document(pool, tag_id).await,
    };
    if !matches!(
        decide_tag_indexability(row.is_active != 0),
        IndexDecision::Indexable
    ) {
        return delete_document(pool, tag_id).await;
    }
    let plain_title = to_index_plain_text(&row.name, TITLE_MAX);
    let plain_description = to_index_plain_text(row.description.as_deref().unwrap_or(""), BODY_MAX);
    if plain_title.is_empty() {
        return delete_document(pool, tag_id).await;
    }
    let body = clean_index_text(&format!("{plain_title} {plain_description}"), BODY_MAX);
    let excerpt = excerpt_from_clean(&plain_description, EXCERPT_MAX);
    for field in [&plain_title, &body, &excerpt] {
        vet_index_text(field).map_err(|e| format!("index text rejected: {e:?}"))?;
    }
    let slug = row.slug.clone().unwrap_or_else(|| row.name.clone());
    let doc = SearchDocument::new(
        row.id.clone(),
        SearchEntityType::Tag,
        plain_title,
        body,
        Some(excerpt),
        slug,
        None,
        vec![],
        row.updated_at,
        row.updated_at,
        now_millis(),
    )
    .map_err(|e| format!("invalid search document: {e:?}"))?;
    upsert_document_guarded(pool, &doc).await
}

// ─────────────────────────── 写入面 ───────────────────────────

/// 条件 upsert：仅当 `stored.policy_revision <= candidate.policy_revision` 应用；
/// 相等（内容/策略未漂移）允许幂等重写，陈旧写（stored 更大）被拒绝
/// （旧 revision 不覆盖新，docs/SEARCH.md §5）。
///
/// 实现：1) 带守卫的 UPDATE；2) 0 行时 `INSERT OR IGNORE`/`INSERT IGNORE`——
/// 行不存在则插入（新文档），行存在（陈旧写/并发胜出者）则不动。
async fn upsert_document_guarded(pool: &DatabasePool, doc: &SearchDocument) -> Result<(), String> {
    let tags_json = serde_json::to_string(&doc.tags).unwrap_or_else(|_| "[]".to_string());
    let updated = match pool {
        Either::Left(p) => sqlx::query(
            "UPDATE search_documents
             SET title = ?, body = ?, excerpt = ?, slug = ?, author_id = ?, tags_json = ?,
                 source_revision = ?, policy_revision = ?, indexed_at = ?
             WHERE doc_id = ? AND policy_revision <= ?",
        )
        .bind(&doc.title)
        .bind(&doc.body)
        .bind(&doc.excerpt)
        .bind(&doc.slug)
        .bind(&doc.author_id)
        .bind(&tags_json)
        .bind(doc.source_revision)
        .bind(doc.policy_revision)
        .bind(doc.indexed_at)
        .bind(&doc.id)
        .bind(doc.policy_revision)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
        Either::Right(p) => sqlx::query(
            "UPDATE search_documents
             SET title = ?, body = ?, excerpt = ?, slug = ?, author_id = ?, tags_json = ?,
                 source_revision = ?, policy_revision = ?, indexed_at = ?
             WHERE doc_id = ? AND policy_revision <= ?",
        )
        .bind(&doc.title)
        .bind(&doc.body)
        .bind(&doc.excerpt)
        .bind(&doc.slug)
        .bind(&doc.author_id)
        .bind(&tags_json)
        .bind(doc.source_revision)
        .bind(doc.policy_revision)
        .bind(doc.indexed_at)
        .bind(&doc.id)
        .bind(doc.policy_revision)
        .execute(p)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected(),
    };
    if updated > 0 {
        return Ok(());
    }
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO search_documents
                     (rowid, doc_id, entity_type, title, body, excerpt, slug, author_id,
                      tags_json, source_revision, policy_revision, indexed_at)
                 VALUES (NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&doc.id)
            .bind(doc.entity_type.as_str())
            .bind(&doc.title)
            .bind(&doc.body)
            .bind(&doc.excerpt)
            .bind(&doc.slug)
            .bind(&doc.author_id)
            .bind(&tags_json)
            .bind(doc.source_revision)
            .bind(doc.policy_revision)
            .bind(doc.indexed_at)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT IGNORE INTO search_documents
                     (rowid, doc_id, entity_type, title, body, excerpt, slug, author_id,
                      tags_json, source_revision, policy_revision, indexed_at)
                 VALUES (NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&doc.id)
            .bind(doc.entity_type.as_str())
            .bind(&doc.title)
            .bind(&doc.body)
            .bind(&doc.excerpt)
            .bind(&doc.slug)
            .bind(&doc.author_id)
            .bind(&tags_json)
            .bind(doc.source_revision)
            .bind(doc.policy_revision)
            .bind(doc.indexed_at)
            .execute(p)
            .await
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// 从索引删除文档（幂等：缺失行 rows=0 视为成功；FTS 由 0030 触发器同步）。
async fn delete_document(pool: &DatabasePool, doc_id: &str) -> Result<(), String> {
    match pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM search_documents WHERE doc_id = ?")
                .bind(doc_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
        Either::Right(p) => {
            sqlx::query("DELETE FROM search_documents WHERE doc_id = ?")
                .bind(doc_id)
                .execute(p)
                .await
                .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}
