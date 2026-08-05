//! M03-BOARDS-05：管理员创建/更新板块——版本冲突、reason 与审计。
//!
//! - 创建：字段校验（BOARDS-02）+ 可见性校验（BOARDS-03 枚举）+ 父级校验
//!   （BOARDS-01，循环/深度/存在性）+ slug 唯一性（`boards_slug_uq` 兜底）+
//!   `admin.board_create` 审计（事务内，M01-AUDIT-08）；
//! - 更新：部分字段校验 + If-Match 版本冲突（`boards.updated_at` 为版本，
//!   BOARDS-04 投影同源）+ 父级/slug 变化校验 + `admin.board_update` 审计
//!   （before/after 白名单字段）——同一事务提交，回滚时审计同步消失；
//! - reason 必须非空（矩阵：所有 `admin.*` 写操作记录 reason、actor、request_id）。

use serde_json::{json, Value};
use sqlx::Either;

use crate::audit::AuditEntry;
use crate::authz::decision::{BoardVisibility, AUTHZ_POLICY_VERSION};
use crate::boards::hierarchy::BoardRef;
use crate::boards::validation::{
    validate_board_fields, validate_board_update, validation_to_error,
};
use crate::boards::{slug_exists, validate_parent};
use crate::db::DatabasePool;
use crate::error::AppError;
use crate::outbox::now_millis;

/// 创建板块输入（请求体解析；`parent_id` 空串 = 根）。
pub struct BoardCreateInput {
    pub slug: String,
    pub name: String,
    pub description: Option<String>,
    pub sort_order: i64,
    pub parent_id: Option<String>,
    pub visibility: String,
    pub posting_mode: String,
}

/// 更新板块输入（部分字段）。
///
/// `parent_id: None` = 未提供（保持不变）；`Some(None)` / `Some("")` = 置为根；
/// `Some(Some(id))` = 移动到新父级。
pub struct BoardUpdateInput {
    pub slug: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub sort_order: Option<i64>,
    pub parent_id: Option<Option<String>>,
    pub is_active: Option<bool>,
    pub visibility: Option<String>,
    pub posting_mode: Option<String>,
}

/// 管理员板块行（服务层读取，含版本与全部可写字段）。
#[derive(sqlx::FromRow)]
struct AdminBoardRow {
    id: String,
    slug: String,
    name: String,
    description: Option<String>,
    parent_id: Option<String>,
    sort_order: i64,
    visibility: String,
    posting_mode: String,
    is_active: i64,
    created_at: i64,
    updated_at: i64,
}

fn board_projection(row: &AdminBoardRow) -> Value {
    json!({
        "id": row.id,
        "slug": row.slug,
        "name": row.name,
        "description": row.description,
        "parent_id": row.parent_id,
        "sort_order": row.sort_order,
        "visibility": row.visibility,
        "posting_mode": row.posting_mode,
        "is_active": row.is_active != 0,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}

/// 父级候选集：活跃且未软删的板块（SCHEMA.md §6 活跃投影）。
async fn parent_candidates(pool: &DatabasePool) -> Result<Vec<BoardRef>, String> {
    let rows: Vec<(String, Option<String>)> = match pool {
        Either::Left(db) => sqlx::query_as(
            "SELECT id, parent_id FROM boards
                 WHERE is_active = 1 AND deleted_at IS NULL",
        )
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?,
        Either::Right(db) => sqlx::query_as(
            "SELECT id, parent_id FROM boards
                 WHERE is_active = 1 AND deleted_at IS NULL",
        )
        .fetch_all(db)
        .await
        .map_err(|e| e.to_string())?,
    };
    Ok(rows
        .into_iter()
        .map(|(id, parent_id)| BoardRef { id, parent_id })
        .collect())
}

async fn load_board(pool: &DatabasePool, board_id: &str) -> Result<Option<AdminBoardRow>, String> {
    match pool {
        Either::Left(db) => sqlx::query_as::<_, AdminBoardRow>(
            "SELECT id, slug, name, description, parent_id, sort_order, visibility, posting_mode,
                    is_active, created_at, updated_at
             FROM boards WHERE id = ?",
        )
        .bind(board_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string()),
        Either::Right(db) => sqlx::query_as::<_, AdminBoardRow>(
            "SELECT id, slug, name, description, parent_id, sort_order, visibility, posting_mode,
                    is_active, created_at, updated_at
             FROM boards WHERE id = ?",
        )
        .bind(board_id)
        .fetch_optional(db)
        .await
        .map_err(|e| e.to_string()),
    }
}

/// 创建板块：校验 → slug 唯一 → 单事务 INSERT + 审计。
pub async fn create_board(
    pool: &DatabasePool,
    actor_id: &str,
    input: BoardCreateInput,
    reason: &str,
    request_id: &str,
) -> Result<Value, AppError> {
    let parent_id = match input.parent_id.as_deref() {
        Some("") | None => None,
        Some(p) => Some(p.to_string()),
    };

    validate_board_fields(
        &input.slug,
        &input.name,
        input.description.as_deref(),
        input.sort_order,
        true,
        &input.posting_mode,
    )
    .map_err(|e| validation_to_error(&e, request_id))?;
    BoardVisibility::parse(&input.visibility).ok_or_else(|| {
        AppError::bad_request(
            format!(
                "visibility 必须是 public/members/restricted/hidden 之一（当前 {}）",
                input.visibility
            ),
            request_id,
            None,
        )
    })?;

    let board_id = uuid::Uuid::now_v7().to_string();
    if let Some(pid) = &parent_id {
        let candidates = parent_candidates(pool)
            .await
            .map_err(|e| AppError::internal(e, request_id))?;
        validate_parent(&candidates, &board_id, Some(pid))
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    }
    if slug_exists(pool, &input.slug)
        .await
        .map_err(|e| AppError::internal(e, request_id))?
    {
        return Err(AppError::conflict("board slug already exists", request_id));
    }

    let now = now_millis();
    let mut tx = match pool {
        Either::Left(p) => Either::Left(
            p.begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        ),
        Either::Right(p) => Either::Right(
            p.begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        ),
    };

    match &mut tx {
        Either::Left(t) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, parent_id, sort_order, visibility, posting_mode, is_active, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
            )
            .bind(&board_id)
            .bind(&input.slug)
            .bind(&input.name)
            .bind(&input.description)
            .bind(&parent_id)
            .bind(input.sort_order)
            .bind(&input.visibility)
            .bind(&input.posting_mode)
            .bind(now)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(t) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, parent_id, sort_order, visibility, posting_mode, is_active, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?, ?, 1, ?, ?)",
            )
            .bind(&board_id)
            .bind(&input.slug)
            .bind(&input.name)
            .bind(&input.description)
            .bind(&parent_id)
            .bind(input.sort_order)
            .bind(&input.visibility)
            .bind(&input.posting_mode)
            .bind(now)
            .bind(now)
            .execute(&mut **t)
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let after = json!({
        "id": board_id,
        "slug": input.slug,
        "name": input.name,
        "description": input.description,
        "parent_id": parent_id,
        "sort_order": input.sort_order,
        "visibility": input.visibility,
        "posting_mode": input.posting_mode,
        "is_active": true,
        "created_at": now,
        "updated_at": now,
    });
    AuditEntry::board_change(
        actor_id,
        "admin.board_create",
        &board_id,
        None,
        &after,
        reason,
        AUTHZ_POLICY_VERSION,
    )
    .record_in_tx(&mut tx)
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match tx {
        Either::Left(t) => t
            .commit()
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(t) => t
            .commit()
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    }

    Ok(json!({ "id": board_id, "version": now }))
}

/// 更新板块：If-Match 版本冲突 → 部分字段校验 → 父级/slug 变化校验 →
/// 单事务 UPDATE（乐观并发）+ 审计（before/after）。
pub async fn update_board(
    pool: &DatabasePool,
    actor_id: &str,
    board_id: &str,
    input: BoardUpdateInput,
    if_match: i64,
    reason: &str,
    request_id: &str,
) -> Result<Value, AppError> {
    let current = load_board(pool, board_id)
        .await
        .map_err(|e| AppError::internal(e, request_id))?
        .ok_or_else(|| AppError::not_found("board not found", request_id))?;
    if if_match != current.updated_at {
        return Err(AppError::version_conflict(
            "board version conflict",
            request_id,
        ));
    }

    validate_board_update(
        input.slug.as_deref(),
        input.name.as_deref(),
        input.description.as_deref(),
        input.sort_order,
        input.posting_mode.as_deref(),
    )
    .map_err(|e| validation_to_error(&e, request_id))?;
    if let Some(v) = &input.visibility {
        BoardVisibility::parse(v).ok_or_else(|| {
            AppError::bad_request(
                format!("visibility 必须是 public/members/restricted/hidden 之一（当前 {v}）"),
                request_id,
                None,
            )
        })?;
    }

    // 新父级（区分未提供 / 置根 / 移动）
    let new_parent: Option<String> = match &input.parent_id {
        None => current.parent_id.clone(),
        Some(inner) => match inner.as_deref() {
            None | Some("") => None,
            Some(p) => Some(p.to_string()),
        },
    };
    if new_parent != current.parent_id {
        let candidates = parent_candidates(pool)
            .await
            .map_err(|e| AppError::internal(e, request_id))?;
        validate_parent(&candidates, board_id, new_parent.as_deref())
            .map_err(|e| AppError::bad_request(e.to_string(), request_id, None))?;
    }
    if let Some(slug) = &input.slug {
        if slug != &current.slug
            && slug_exists(pool, slug)
                .await
                .map_err(|e| AppError::internal(e, request_id))?
        {
            return Err(AppError::conflict("board slug already exists", request_id));
        }
    }

    // 计算新值（类型化，供 UPDATE 与审计 after 复用）
    let new_updated_at = now_millis();
    let new_slug = input.slug.clone().unwrap_or_else(|| current.slug.clone());
    let new_name = input.name.clone().unwrap_or_else(|| current.name.clone());
    let new_description = input
        .description
        .clone()
        .or_else(|| current.description.clone());
    let new_sort_order = input.sort_order.unwrap_or(current.sort_order);
    let new_visibility = input
        .visibility
        .clone()
        .unwrap_or_else(|| current.visibility.clone());
    let new_posting_mode = input
        .posting_mode
        .clone()
        .unwrap_or_else(|| current.posting_mode.clone());
    let new_is_active = input.is_active.unwrap_or(current.is_active != 0);

    let before = board_projection(&current);
    let after = json!({
        "id": board_id,
        "slug": new_slug.clone(),
        "name": new_name.clone(),
        "description": new_description.clone(),
        "parent_id": new_parent.clone(),
        "sort_order": new_sort_order,
        "visibility": new_visibility.clone(),
        "posting_mode": new_posting_mode.clone(),
        "is_active": new_is_active,
        "created_at": current.created_at,
        "updated_at": new_updated_at,
    });

    let mut tx = match pool {
        Either::Left(p) => Either::Left(
            p.begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        ),
        Either::Right(p) => Either::Right(
            p.begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        ),
    };

    let affected: u64 = match &mut tx {
        Either::Left(t) => sqlx::query(
            "UPDATE boards SET
                    slug = ?, name = ?, description = ?, parent_id = ?, sort_order = ?,
                    visibility = ?, posting_mode = ?, is_active = ?, updated_at = ?
                 WHERE id = ? AND updated_at = ?",
        )
        .bind(&new_slug)
        .bind(&new_name)
        .bind(&new_description)
        .bind(&new_parent)
        .bind(new_sort_order)
        .bind(&new_visibility)
        .bind(&new_posting_mode)
        .bind(new_is_active as i64)
        .bind(new_updated_at)
        .bind(board_id)
        .bind(if_match)
        .execute(&mut **t)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
        Either::Right(t) => sqlx::query(
            "UPDATE boards SET
                    slug = ?, name = ?, description = ?, parent_id = ?, sort_order = ?,
                    visibility = ?, posting_mode = ?, is_active = ?, updated_at = ?
                 WHERE id = ? AND updated_at = ?",
        )
        .bind(&new_slug)
        .bind(&new_name)
        .bind(&new_description)
        .bind(&new_parent)
        .bind(new_sort_order)
        .bind(&new_visibility)
        .bind(&new_posting_mode)
        .bind(new_is_active as i64)
        .bind(new_updated_at)
        .bind(board_id)
        .bind(if_match)
        .execute(&mut **t)
        .await
        .map_err(|e| AppError::internal(e.to_string(), request_id))?
        .rows_affected(),
    };
    if affected == 0 {
        return Err(AppError::version_conflict(
            "board version conflict",
            request_id,
        ));
    }

    AuditEntry::board_change(
        actor_id,
        "admin.board_update",
        board_id,
        Some(&before),
        &after,
        reason,
        AUTHZ_POLICY_VERSION,
    )
    .record_in_tx(&mut tx)
    .await
    .map_err(|e| AppError::internal(e.to_string(), request_id))?;

    match tx {
        Either::Left(t) => t
            .commit()
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
        Either::Right(t) => t
            .commit()
            .await
            .map_err(|e| AppError::internal(e.to_string(), request_id))?,
    }

    Ok(json!({ "id": board_id, "version": new_updated_at }))
}
