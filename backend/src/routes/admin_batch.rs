//! 管理域批量操作与全量 CSV 导出端点（M18-ADMIN-BATCH-01 / M18-ADMIN-BATCH-02）。
//!
//! 批量操作端点（单事务 + 必填 reason 审计 + 批量幂等支持）：
//! - `POST /api/v1/admin/posts/batch`：批量帖子操作（审核/状态变更/置顶/锁定/删除）；
//! - `POST /api/v1/admin/boards/batch`：批量板块启停/状态切换；
//! - `POST /api/v1/admin/tags/batch`：批量标签启停；
//! - `POST /api/v1/admin/users/batch`：批量用户状态变更（禁止封禁最后一位管理员）。
//!
//! 全量 CSV 导出端点（权限门 + UTF-8 BOM + 导出审计）：
//! - `GET /api/v1/admin/posts/export.csv`；
//! - `GET /api/v1/admin/boards/export.csv`；
//! - `GET /api/v1/admin/tags/export.csv`；
//! - `GET /api/v1/admin/users/export.csv`；
//! - `GET /api/v1/admin/audit/export.csv`。

use axum::{
    extract::{Query, State},
    http::{header, HeaderValue, StatusCode},
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
use crate::outbox::now_millis;

/// 单批次最多处理条目数（防止超大请求造成事务超时）。
const MAX_BATCH_SIZE: usize = 200;

/// 权限检查助手。
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

/// 必填 reason（管理写操作审计）。
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

/// 私有数据响应统一 `Cache-Control: private, no-store`。
fn private_no_store(resp: Response) -> Response {
    let mut resp = resp;
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// CSV 单元格转义。
fn escape_csv(val: &str) -> String {
    if val.contains(',') || val.contains('"') || val.contains('\n') || val.contains('\r') {
        format!("\"{}\"", val.replace('"', "\"\""))
    } else {
        val.to_string()
    }
}

/// 构造 CSV 文件下载响应（带 UTF-8 BOM 确保 Excel 正常识别中文）。
fn csv_response(filename: &str, content: String) -> Response {
    let mut bom_and_content = String::with_capacity(3 + content.len());
    bom_and_content.push('\u{FEFF}');
    bom_and_content.push_str(&content);

    let mut resp = (StatusCode::OK, bom_and_content).into_response();
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    let disposition = format!("attachment; filename=\"{filename}\"");
    if let Ok(val) = HeaderValue::from_str(&disposition) {
        headers.insert(header::CONTENT_DISPOSITION, val);
    }
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    resp
}

/// 挂载批量与导出路由。
pub fn router() -> Router<AppState> {
    Router::new()
        // 批量操作
        .route("/api/v1/admin/posts/batch", post(batch_posts))
        .route("/api/v1/admin/boards/batch", post(batch_boards))
        .route("/api/v1/admin/tags/batch", post(batch_tags))
        .route("/api/v1/admin/users/batch", post(batch_users))
        // CSV 导出
        .route("/api/v1/admin/posts/export.csv", get(export_posts_csv))
        .route("/api/v1/admin/boards/export.csv", get(export_boards_csv))
        .route("/api/v1/admin/tags/export.csv", get(export_tags_csv))
        .route("/api/v1/admin/users/export.csv", get(export_users_csv))
        .route("/api/v1/admin/audit/export.csv", get(export_audit_csv))
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. 批量管理接口实现 (M18-ADMIN-BATCH-01)
// ─────────────────────────────────────────────────────────────────────────────

/// POST /api/v1/admin/posts/batch — 批量帖子管理操作（post.moderate）。
async fn batch_posts(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "admin_posts_batch";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "post.moderate", request_id).await?;

    let reason = required_reason(&req, request_id)?;
    let ids: Vec<String> = req
        .get("ids")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    if ids.is_empty() {
        return Err(AppError::bad_request(
            "ids must not be empty",
            request_id,
            None,
        ));
    }
    if ids.len() > MAX_BATCH_SIZE {
        return Err(AppError::bad_request(
            format!("ids size exceeds limit {MAX_BATCH_SIZE}"),
            request_id,
            None,
        ));
    }

    let action = req
        .get("action")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim()
        .to_string();

    let now = now_millis();
    let mut affected_total: u64 = 0;

    // 单事务内批量执行
    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            for id in &ids {
                let sql = match action.as_str() {
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
                    "unfeature" => {
                        "UPDATE posts SET featured_at = NULL, updated_at = ? WHERE id = ?"
                    }
                    "pin" => {
                        "UPDATE posts SET pinned = 1, pinned_at = ?, updated_at = ? WHERE id = ?"
                    }
                    "unpin" => {
                        "UPDATE posts SET pinned = 0, pinned_at = NULL, updated_at = ? WHERE id = ?"
                    }
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

                let mut q = sqlx::query(sql);
                match action.as_str() {
                    "approve" | "delete" | "feature" | "pin" | "lock" => {
                        q = q.bind(now).bind(now).bind(id)
                    }
                    _ => q = q.bind(now).bind(id),
                }
                let res = q
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                affected_total += res.rows_affected();
            }

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            for id in &ids {
                let sql = match action.as_str() {
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
                    "unfeature" => {
                        "UPDATE posts SET featured_at = NULL, updated_at = ? WHERE id = ?"
                    }
                    "pin" => {
                        "UPDATE posts SET pinned = 1, pinned_at = ?, updated_at = ? WHERE id = ?"
                    }
                    "unpin" => {
                        "UPDATE posts SET pinned = 0, pinned_at = NULL, updated_at = ? WHERE id = ?"
                    }
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

                let mut q = sqlx::query(sql);
                match action.as_str() {
                    "approve" | "delete" | "feature" | "pin" | "lock" => {
                        q = q.bind(now).bind(now).bind(id)
                    }
                    _ => q = q.bind(now).bind(id),
                }
                let res = q
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                affected_total += res.rows_affected();
            }

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    // 记录聚合审计
    let target_desc = format!("{action}:{}", ids.len());
    let _ = AuditEntry::user_action(&user.id, "admin.posts.batch")
        .with_target("post", &target_desc)
        .with_reason(&reason)
        .with_metadata(json!({
            "action": action,
            "ids": ids,
            "affected": affected_total,
        }))
        .record(pool)
        .await;

    Ok(private_no_store(
        Json(json!({
            "ok": true,
            "action": action,
            "affected": affected_total,
            "ids": ids,
        }))
        .into_response(),
    ))
}

/// POST /api/v1/admin/boards/batch — 批量板块启停/状态（board.manage）。
async fn batch_boards(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "admin_boards_batch";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "board.manage", request_id).await?;

    let reason = required_reason(&req, request_id)?;
    let ids: Vec<String> = req
        .get("ids")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    if ids.is_empty() {
        return Err(AppError::bad_request(
            "ids must not be empty",
            request_id,
            None,
        ));
    }
    if ids.len() > MAX_BATCH_SIZE {
        return Err(AppError::bad_request(
            format!("ids size exceeds limit {MAX_BATCH_SIZE}"),
            request_id,
            None,
        ));
    }

    let is_active = req.get("is_active").and_then(Value::as_bool);
    let posting_mode = req
        .get("posting_mode")
        .or_else(|| req.get("status"))
        .and_then(Value::as_str);

    if is_active.is_none() && posting_mode.is_none() {
        return Err(AppError::bad_request(
            "either is_active or posting_mode/status must be provided",
            request_id,
            None,
        ));
    }

    let now = now_millis();
    let mut affected_total: u64 = 0;

    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            for id in &ids {
                let res = if let Some(active) = is_active {
                    sqlx::query("UPDATE boards SET is_active = ?, updated_at = ? WHERE id = ?")
                        .bind(if active { 1 } else { 0 })
                        .bind(now)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                } else if let Some(mode) = posting_mode {
                    sqlx::query("UPDATE boards SET posting_mode = ?, updated_at = ? WHERE id = ?")
                        .bind(mode)
                        .bind(now)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                } else {
                    continue;
                }
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                affected_total += res.rows_affected();
            }

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            for id in &ids {
                let res = if let Some(active) = is_active {
                    sqlx::query("UPDATE boards SET is_active = ?, updated_at = ? WHERE id = ?")
                        .bind(if active { 1 } else { 0 })
                        .bind(now)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                } else if let Some(mode) = posting_mode {
                    sqlx::query("UPDATE boards SET posting_mode = ?, updated_at = ? WHERE id = ?")
                        .bind(mode)
                        .bind(now)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                } else {
                    continue;
                }
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                affected_total += res.rows_affected();
            }

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let target_desc = format!("count:{}", ids.len());
    let _ = AuditEntry::user_action(&user.id, "admin.boards.batch")
        .with_target("board", &target_desc)
        .with_reason(&reason)
        .with_metadata(json!({
            "is_active": is_active,
            "posting_mode": posting_mode,
            "ids": ids,
            "affected": affected_total,
        }))
        .record(pool)
        .await;

    Ok(private_no_store(
        Json(json!({
            "ok": true,
            "affected": affected_total,
            "ids": ids,
        }))
        .into_response(),
    ))
}

/// POST /api/v1/admin/tags/batch — 批量标签启停（tag.manage）。
async fn batch_tags(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "admin_tags_batch";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "tag.manage", request_id).await?;

    let reason = required_reason(&req, request_id)?;
    let ids: Vec<String> = req
        .get("ids")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    if ids.is_empty() {
        return Err(AppError::bad_request(
            "ids must not be empty",
            request_id,
            None,
        ));
    }
    if ids.len() > MAX_BATCH_SIZE {
        return Err(AppError::bad_request(
            format!("ids size exceeds limit {MAX_BATCH_SIZE}"),
            request_id,
            None,
        ));
    }

    let is_active = req
        .get("is_active")
        .and_then(Value::as_bool)
        .or_else(|| {
            req.get("action")
                .and_then(Value::as_str)
                .map(|act| act == "enable" || act == "activate")
        })
        .unwrap_or(true);

    let now = now_millis();
    let mut affected_total: u64 = 0;

    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            for id in &ids {
                let res = sqlx::query("UPDATE tags SET is_active = ?, updated_at = ? WHERE id = ?")
                    .bind(if is_active { 1 } else { 0 })
                    .bind(now)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                affected_total += res.rows_affected();
            }

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            for id in &ids {
                let res = sqlx::query("UPDATE tags SET is_active = ?, updated_at = ? WHERE id = ?")
                    .bind(if is_active { 1 } else { 0 })
                    .bind(now)
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                affected_total += res.rows_affected();
            }

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let target_desc = format!("active:{is_active}:{}", ids.len());
    let _ = AuditEntry::user_action(&user.id, "admin.tags.batch")
        .with_target("tag", &target_desc)
        .with_reason(&reason)
        .with_metadata(json!({
            "is_active": is_active,
            "ids": ids,
            "affected": affected_total,
        }))
        .record(pool)
        .await;

    Ok(private_no_store(
        Json(json!({
            "ok": true,
            "affected": affected_total,
            "ids": ids,
        }))
        .into_response(),
    ))
}

/// POST /api/v1/admin/users/batch — 批量用户状态设置（user.manage）。
async fn batch_users(
    State(state): State<AppState>,
    auth: AuthSession,
    Json(req): Json<Value>,
) -> Result<Response, AppError> {
    let request_id = "admin_users_batch";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "user.manage", request_id).await?;

    let reason = required_reason(&req, request_id)?;
    let ids: Vec<String> = req
        .get("ids")
        .and_then(Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();

    if ids.is_empty() {
        return Err(AppError::bad_request(
            "ids must not be empty",
            request_id,
            None,
        ));
    }
    if ids.len() > MAX_BATCH_SIZE {
        return Err(AppError::bad_request(
            format!("ids size exceeds limit {MAX_BATCH_SIZE}"),
            request_id,
            None,
        ));
    }

    let status = req
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();

    if !matches!(status, "pending" | "active" | "restricted" | "banned") {
        return Err(AppError::bad_request(
            "invalid user status",
            request_id,
            None,
        ));
    }

    // 守卫：禁止批量将最后一位管理员修改为非 active 状态
    if status != "active" {
        for id in &ids {
            let is_last = crate::bootstrap::is_last_active_admin(pool, id)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            if is_last {
                return Err(AppError::conflict(
                    "cannot change status of the last active administrator",
                    request_id,
                ));
            }
        }
    }

    let now = now_millis();
    let mut affected_total: u64 = 0;

    match pool {
        Either::Left(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            for id in &ids {
                let res = sqlx::query(
                    "UPDATE users SET status = ?, version = version + 1, updated_at = ? WHERE id = ?",
                )
                .bind(status)
                .bind(now)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                affected_total += res.rows_affected();
            }

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
        Either::Right(p) => {
            let mut tx = p
                .begin()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;

            for id in &ids {
                let res = sqlx::query(
                    "UPDATE users SET status = ?, version = version + 1, updated_at = ? WHERE id = ?",
                )
                .bind(status)
                .bind(now)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
                affected_total += res.rows_affected();
            }

            tx.commit()
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
        }
    }

    let target_desc = format!("status:{status}:{}", ids.len());
    let _ = AuditEntry::user_action(&user.id, "admin.users.batch")
        .with_target("user", &target_desc)
        .with_reason(&reason)
        .with_metadata(json!({
            "status": status,
            "ids": ids,
            "affected": affected_total,
        }))
        .record(pool)
        .await;

    Ok(private_no_store(
        Json(json!({
            "ok": true,
            "status": status,
            "affected": affected_total,
            "ids": ids,
        }))
        .into_response(),
    ))
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. 全量 CSV 导出接口实现 (M18-ADMIN-BATCH-02)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct PostExportQuery {
    pub status: Option<String>,
    pub board: Option<String>,
    pub q: Option<String>,
}

/// GET /api/v1/admin/posts/export.csv
async fn export_posts_csv(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<PostExportQuery>,
) -> Result<Response, AppError> {
    let request_id = "admin_posts_export";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "post.moderate", request_id).await?;

    let mut sql = String::from(
        "SELECT p.id, p.title, p.author_id, u.username_normalized AS author_username,
                p.board_id, b.slug AS board_slug, p.status,
                CASE WHEN p.featured_at IS NOT NULL THEN 1 ELSE 0 END AS is_featured,
                p.pinned AS is_pinned,
                CASE WHEN p.closed_at IS NOT NULL THEN 1 ELSE 0 END AS is_locked,
                p.view_count, p.reply_count, p.created_at, p.updated_at
         FROM posts p
         LEFT JOIN users u ON u.id = p.author_id
         LEFT JOIN boards b ON b.id = p.board_id
         WHERE 1 = 1",
    );

    if query.status.as_deref().unwrap_or("all") != "all" {
        sql.push_str(" AND p.status = ?");
    }
    if query.board.is_some() {
        sql.push_str(" AND (p.board_id = ? OR b.slug = ?)");
    }
    if query.q.is_some() {
        sql.push_str(" AND p.title LIKE ?");
    }
    sql.push_str(" ORDER BY p.created_at DESC");

    let status_val = query.status.as_deref().unwrap_or("all");
    let board_val = query.board.as_deref();
    let q_pattern = query.q.as_ref().map(|s| format!("%{s}%"));

    let mut csv = String::from(
        "ID,标题,作者ID,作者名,板块ID,板块Slug,状态,是否精华,是否置顶,是否锁定,浏览数,回复数,创建时间,更新时间\n",
    );

    match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(&sql);
            if status_val != "all" {
                q = q.bind(status_val);
            }
            if let Some(b) = board_val {
                q = q.bind(b).bind(b);
            }
            if let Some(ref qp) = q_pattern {
                q = q.bind(qp);
            }
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let title: String = row.try_get("title").unwrap_or_default();
                let author_id: String = row.try_get("author_id").unwrap_or_default();
                let author_username: String = row.try_get("author_username").unwrap_or_default();
                let board_id: String = row.try_get("board_id").unwrap_or_default();
                let board_slug: String = row.try_get("board_slug").unwrap_or_default();
                let status: String = row.try_get("status").unwrap_or_default();
                let is_featured: i64 = row.try_get("is_featured").unwrap_or(0);
                let is_pinned: i64 = row.try_get("is_pinned").unwrap_or(0);
                let is_locked: i64 = row.try_get("is_locked").unwrap_or(0);
                let view_count: i64 = row.try_get("view_count").unwrap_or(0);
                let reply_count: i64 = row.try_get("reply_count").unwrap_or(0);
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);
                let updated_at: i64 = row.try_get("updated_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&title),
                    escape_csv(&author_id),
                    escape_csv(&author_username),
                    escape_csv(&board_id),
                    escape_csv(&board_slug),
                    escape_csv(&status),
                    is_featured,
                    is_pinned,
                    is_locked,
                    view_count,
                    reply_count,
                    created_at,
                    updated_at
                ));
            }
        }
        Either::Right(p) => {
            let mut q = sqlx::query(&sql);
            if status_val != "all" {
                q = q.bind(status_val);
            }
            if let Some(b) = board_val {
                q = q.bind(b).bind(b);
            }
            if let Some(ref qp) = q_pattern {
                q = q.bind(qp);
            }
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let title: String = row.try_get("title").unwrap_or_default();
                let author_id: String = row.try_get("author_id").unwrap_or_default();
                let author_username: String = row.try_get("author_username").unwrap_or_default();
                let board_id: String = row.try_get("board_id").unwrap_or_default();
                let board_slug: String = row.try_get("board_slug").unwrap_or_default();
                let status: String = row.try_get("status").unwrap_or_default();
                let is_featured: i64 = row.try_get("is_featured").unwrap_or(0);
                let is_pinned: i64 = row.try_get("is_pinned").unwrap_or(0);
                let is_locked: i64 = row.try_get("is_locked").unwrap_or(0);
                let view_count: i64 = row.try_get("view_count").unwrap_or(0);
                let reply_count: i64 = row.try_get("reply_count").unwrap_or(0);
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);
                let updated_at: i64 = row.try_get("updated_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&title),
                    escape_csv(&author_id),
                    escape_csv(&author_username),
                    escape_csv(&board_id),
                    escape_csv(&board_slug),
                    escape_csv(&status),
                    is_featured,
                    is_pinned,
                    is_locked,
                    view_count,
                    reply_count,
                    created_at,
                    updated_at
                ));
            }
        }
    }

    // 记录导出审计
    let _ = AuditEntry::user_action(&user.id, "admin.posts.export")
        .with_target("posts", "csv")
        .with_reason("export posts csv")
        .record(pool)
        .await;

    Ok(csv_response("posts-export.csv", csv))
}

/// GET /api/v1/admin/boards/export.csv
async fn export_boards_csv(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "admin_boards_export";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "board.manage", request_id).await?;

    let sql = "SELECT id, slug, name, description, is_active, posting_mode, visibility,
                      post_count, created_at
               FROM boards ORDER BY sort_order ASC, created_at ASC";

    let mut csv = String::from("ID,Slug,名称,描述,是否启用,发帖模式,可见性,帖子数,创建时间\n");

    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(sql)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let slug: String = row.try_get("slug").unwrap_or_default();
                let name: String = row.try_get("name").unwrap_or_default();
                let desc: String = row.try_get("description").unwrap_or_default();
                let is_active: i64 = row.try_get("is_active").unwrap_or(0);
                let mode: String = row.try_get("posting_mode").unwrap_or_default();
                let vis: String = row.try_get("visibility").unwrap_or_default();
                let post_count: i64 = row.try_get("post_count").unwrap_or(0);
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&slug),
                    escape_csv(&name),
                    escape_csv(&desc),
                    is_active,
                    escape_csv(&mode),
                    escape_csv(&vis),
                    post_count,
                    created_at
                ));
            }
        }
        Either::Right(p) => {
            let rows = sqlx::query(sql)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let slug: String = row.try_get("slug").unwrap_or_default();
                let name: String = row.try_get("name").unwrap_or_default();
                let desc: String = row.try_get("description").unwrap_or_default();
                let is_active: i64 = row.try_get("is_active").unwrap_or(0);
                let mode: String = row.try_get("posting_mode").unwrap_or_default();
                let vis: String = row.try_get("visibility").unwrap_or_default();
                let post_count: i64 = row.try_get("post_count").unwrap_or(0);
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&slug),
                    escape_csv(&name),
                    escape_csv(&desc),
                    is_active,
                    escape_csv(&mode),
                    escape_csv(&vis),
                    post_count,
                    created_at
                ));
            }
        }
    }

    let _ = AuditEntry::user_action(&user.id, "admin.boards.export")
        .with_target("boards", "csv")
        .with_reason("export boards csv")
        .record(pool)
        .await;

    Ok(csv_response("boards-export.csv", csv))
}

/// GET /api/v1/admin/tags/export.csv
async fn export_tags_csv(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "admin_tags_export";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "tag.manage", request_id).await?;

    let sql = "SELECT id, slug, name, description, is_active, usage_count AS post_count, created_at
               FROM tags ORDER BY usage_count DESC, created_at ASC";

    let mut csv = String::from("ID,Slug,名称,描述,是否启用,帖子数,创建时间\n");

    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(sql)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let slug: String = row.try_get("slug").unwrap_or_default();
                let name: String = row.try_get("name").unwrap_or_default();
                let desc: String = row.try_get("description").unwrap_or_default();
                let is_active: i64 = row.try_get("is_active").unwrap_or(0);
                let post_count: i64 = row.try_get("post_count").unwrap_or(0);
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&slug),
                    escape_csv(&name),
                    escape_csv(&desc),
                    is_active,
                    post_count,
                    created_at
                ));
            }
        }
        Either::Right(p) => {
            let rows = sqlx::query(sql)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let slug: String = row.try_get("slug").unwrap_or_default();
                let name: String = row.try_get("name").unwrap_or_default();
                let desc: String = row.try_get("description").unwrap_or_default();
                let is_active: i64 = row.try_get("is_active").unwrap_or(0);
                let post_count: i64 = row.try_get("post_count").unwrap_or(0);
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&slug),
                    escape_csv(&name),
                    escape_csv(&desc),
                    is_active,
                    post_count,
                    created_at
                ));
            }
        }
    }

    let _ = AuditEntry::user_action(&user.id, "admin.tags.export")
        .with_target("tags", "csv")
        .with_reason("export tags csv")
        .record(pool)
        .await;

    Ok(csv_response("tags-export.csv", csv))
}

/// GET /api/v1/admin/users/export.csv
async fn export_users_csv(
    State(state): State<AppState>,
    auth: AuthSession,
) -> Result<Response, AppError> {
    let request_id = "admin_users_export";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "user.manage", request_id).await?;

    let sql = "SELECT id, username_normalized AS username, email_normalized AS email, display_name, status, trust_level, created_at, updated_at
               FROM users ORDER BY created_at DESC";

    let mut csv = String::from("ID,用户名,邮箱,显示名,状态,信任等级,注册时间,更新时间\n");

    match pool {
        Either::Left(p) => {
            let rows = sqlx::query(sql)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let username: String = row.try_get("username").unwrap_or_default();
                let email: String = row.try_get("email").unwrap_or_default();
                let display_name: String = row.try_get("display_name").unwrap_or_default();
                let status: String = row.try_get("status").unwrap_or_default();
                let trust_level: i64 = row.try_get("trust_level").unwrap_or(0);
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);
                let updated_at: i64 = row.try_get("updated_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&username),
                    escape_csv(&email),
                    escape_csv(&display_name),
                    escape_csv(&status),
                    trust_level,
                    created_at,
                    updated_at
                ));
            }
        }
        Either::Right(p) => {
            let rows = sqlx::query(sql)
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let username: String = row.try_get("username").unwrap_or_default();
                let email: String = row.try_get("email").unwrap_or_default();
                let display_name: String = row.try_get("display_name").unwrap_or_default();
                let status: String = row.try_get("status").unwrap_or_default();
                let trust_level: i64 = row.try_get("trust_level").unwrap_or(0);
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);
                let updated_at: i64 = row.try_get("updated_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&username),
                    escape_csv(&email),
                    escape_csv(&display_name),
                    escape_csv(&status),
                    trust_level,
                    created_at,
                    updated_at
                ));
            }
        }
    }

    let _ = AuditEntry::user_action(&user.id, "admin.users.export")
        .with_target("users", "csv")
        .with_reason("export users csv")
        .record(pool)
        .await;

    Ok(csv_response("users-export.csv", csv))
}

#[derive(Debug, Deserialize)]
pub struct AuditExportQuery {
    pub actor: Option<String>,
    pub action: Option<String>,
    pub q: Option<String>,
}

/// GET /api/v1/admin/audit/export.csv
async fn export_audit_csv(
    State(state): State<AppState>,
    auth: AuthSession,
    Query(query): Query<AuditExportQuery>,
) -> Result<Response, AppError> {
    let request_id = "admin_audit_export";
    let user = auth.require_auth(request_id)?;
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    require_perm(pool, &user.id, "admin.manage", request_id).await?;

    let mut sql = String::from(
        "SELECT a.id, a.actor_id, u.username_normalized AS actor_username, a.action,
                a.target_type, a.target_id, a.ip_address, a.metadata, a.created_at
         FROM audit_logs a
         LEFT JOIN users u ON u.id = a.actor_id
         WHERE 1 = 1",
    );

    if query.actor.is_some() {
        sql.push_str(" AND (a.actor_id = ? OR u.username_normalized = ?)");
    }
    if query.action.is_some() {
        sql.push_str(" AND a.action = ?");
    }
    if query.q.is_some() {
        sql.push_str(" AND a.metadata LIKE ?");
    }
    sql.push_str(" ORDER BY a.created_at DESC, a.id DESC LIMIT 5000");

    let actor_val = query.actor.as_deref();
    let action_val = query.action.as_deref();
    let q_pattern = query.q.as_ref().map(|s| format!("%{s}%"));

    let mut csv =
        String::from("ID,操作者ID,操作者名,操作动作,目标类型,目标ID,IP,元数据,创建时间\n");

    match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(&sql);
            if let Some(act) = actor_val {
                q = q.bind(act).bind(act);
            }
            if let Some(action) = action_val {
                q = q.bind(action);
            }
            if let Some(ref qp) = q_pattern {
                q = q.bind(qp);
            }
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let actor_id: String = row.try_get("actor_id").unwrap_or_default();
                let actor_username: String = row.try_get("actor_username").unwrap_or_default();
                let action: String = row.try_get("action").unwrap_or_default();
                let target_type: String = row.try_get("target_type").unwrap_or_default();
                let target_id: String = row.try_get("target_id").unwrap_or_default();
                let ip: String = row.try_get("ip_address").unwrap_or_default();
                let metadata: String = row.try_get("metadata").unwrap_or_default();
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&actor_id),
                    escape_csv(&actor_username),
                    escape_csv(&action),
                    escape_csv(&target_type),
                    escape_csv(&target_id),
                    escape_csv(&ip),
                    escape_csv(&metadata),
                    created_at
                ));
            }
        }
        Either::Right(p) => {
            let mut q = sqlx::query(&sql);
            if let Some(act) = actor_val {
                q = q.bind(act).bind(act);
            }
            if let Some(action) = action_val {
                q = q.bind(action);
            }
            if let Some(ref qp) = q_pattern {
                q = q.bind(qp);
            }
            let rows = q
                .fetch_all(p)
                .await
                .map_err(|e| AppError::internal(e.to_string(), request_id))?;
            for row in rows {
                let id: String = row.try_get("id").unwrap_or_default();
                let actor_id: String = row.try_get("actor_id").unwrap_or_default();
                let actor_username: String = row.try_get("actor_username").unwrap_or_default();
                let action: String = row.try_get("action").unwrap_or_default();
                let target_type: String = row.try_get("target_type").unwrap_or_default();
                let target_id: String = row.try_get("target_id").unwrap_or_default();
                let ip: String = row.try_get("ip_address").unwrap_or_default();
                let metadata: String = row.try_get("metadata").unwrap_or_default();
                let created_at: i64 = row.try_get("created_at").unwrap_or(0);

                csv.push_str(&format!(
                    "{},{},{},{},{},{},{},{},{}\n",
                    escape_csv(&id),
                    escape_csv(&actor_id),
                    escape_csv(&actor_username),
                    escape_csv(&action),
                    escape_csv(&target_type),
                    escape_csv(&target_id),
                    escape_csv(&ip),
                    escape_csv(&metadata),
                    created_at
                ));
            }
        }
    }

    let _ = AuditEntry::user_action(&user.id, "admin.audit.export")
        .with_target("audit", "csv")
        .with_reason("export audit logs csv")
        .record(pool)
        .await;

    Ok(csv_response("audit-export.csv", csv))
}
