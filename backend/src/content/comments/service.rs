//! M04-COMMENTS 服务层：楼层分配、分页游标、限时编辑、修订快照、软删除。
//!
//! 设计约定（全部为服务层单一事实来源，路由层只做鉴权/校验/错误映射）：
//!
//! - **楼层分配**（M04-COMMENTS-03）必须在**写事务内**完成：`MAX(floor)+1`
//!   计算与 INSERT 同一事务；`UNIQUE(post_id, floor)`（0038）唯一约束兜底
//!   并发，冲突映射为稳定的 [`CreateCommentError::FloorContended`]（路由 409）。
//!   楼层保持连续（软删评论保留楼层，不重编号）。
//! - **列表分页**（M04-COMMENTS-04）：稳定排序 `floor ASC, id ASC`；`after`
//!   为不透明游标 `base64url("floor:id")`（避免客户端猜测内部字段，与
//!   [`crate::audit::AuditCursor`] 编码约定一致）。
//! - **限时编辑**（M04-COMMENTS-05）：作者可在 [`COMMENT_EDIT_WINDOW_MS`]
//!   （created_at 起 30 分钟）内编辑；版本守卫（If-Match/body version），
//!   每次写入追加一条不可变 `comment_revisions` 快照（`UNIQUE(comment_id,
//!   version)`，0039）。
//! - **正文列**：`comments.content` 仅存 Markdown 原文；`body_html` 读取时经
//!   [`render_and_sanitize`] 计算（comments 无 html 列，与 post_contents
//!   写时渲染约定不同——理由见 agent-A report M04-COMMENTS-01）。修订快照
//!   仍写时渲染（`comment_revisions.body_html` 落库，0039 列定义）。

use serde_json::{json, Value};
use sqlx::Either;

use crate::content::markdown::{policy::policy_version, render_and_sanitize};
use crate::content::model::{Comment, CommentStatus};
use crate::db::busy::{retry_on_busy, BusyCounter, BusyPolicy};
use crate::db::DatabasePool;

/// 作者限时编辑窗口（M04-COMMENTS-05）：`created_at` 起 30 分钟内可编辑。
pub const COMMENT_EDIT_WINDOW_MS: i64 = 30 * 60 * 1000;

/// 评论创建错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreateCommentError {
    /// `UNIQUE(post_id, floor)` 兜底触发：并发下楼层分配竞争；调用方按 409
    /// 稳定返回（M04-COMMENTS-03）。
    FloorContended,
    Db(String),
}

impl From<sqlx::Error> for CreateCommentError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for CreateCommentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::FloorContended => write!(f, "floor allocation raced with a concurrent reply"),
            Self::Db(msg) => write!(f, "comment create db error: {msg}"),
        }
    }
}

/// 评论编辑错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditCommentError {
    NotFound,
    VersionMismatch { expected: i64, actual: i64 },
    Db(String),
}

impl From<sqlx::Error> for EditCommentError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for EditCommentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "comment not found"),
            Self::VersionMismatch { expected, actual } => {
                write!(
                    f,
                    "comment version mismatch: expected {expected}, current {actual}"
                )
            }
            Self::Db(msg) => write!(f, "comment edit db error: {msg}"),
        }
    }
}

/// 列表 keyset 游标（M04-COMMENTS-04）：`floor ASC, id ASC`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommentCursor {
    pub floor: i64,
    pub id: String,
}

impl CommentCursor {
    pub fn new(floor: i64, id: impl Into<String>) -> Self {
        Self {
            floor,
            id: id.into(),
        }
    }

    /// 编码为 `base64url("floor:id")`，避免调用方猜测内部字段。
    pub fn encode(&self) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(format!("{}:{}", self.floor, self.id))
    }

    /// 解码游标；格式非法返回 [`CommentCursorError`]。
    pub fn decode(encoded: &str) -> Result<Self, CommentCursorError> {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        let bytes = URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| CommentCursorError::Malformed)?;
        let text = String::from_utf8(bytes).map_err(|_| CommentCursorError::Malformed)?;
        let (floor, id) = text.split_once(':').ok_or(CommentCursorError::Malformed)?;
        let floor = floor
            .parse::<i64>()
            .map_err(|_| CommentCursorError::Malformed)?;
        if id.is_empty() {
            return Err(CommentCursorError::Malformed);
        }
        Ok(Self {
            floor,
            id: id.to_owned(),
        })
    }
}

/// 游标解码错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommentCursorError {
    Malformed,
}

impl std::fmt::Display for CommentCursorError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "malformed comment cursor")
    }
}

impl std::error::Error for CommentCursorError {}

/// 校验 parent 属于同一主题（M04-COMMENTS-02，防跨主题引用泄漏）。
///
/// 复用 [`Comment::validate_quote_scope`]（parent 与 quoted 同主题规则）：
/// 由调用方先完成 parent 存在性与可见性重检（status published 且
/// `deleted_at IS NULL`），本函数只做同主题断言。
pub fn validate_parent_scope(post_id: &str, parent_post_id: &str) -> Result<(), &'static str> {
    Comment {
        id: String::new(),
        post_id: post_id.to_string(),
        author_id: String::new(),
        parent_id: None,
        quoted_comment_id: None,
        floor: 0,
        status: CommentStatus::Published,
        version: 0,
        created_at: 0,
        updated_at: 0,
        deleted_at: None,
    }
    .validate_quote_scope(Some(parent_post_id), None)
}

/// 评论投影行（列表/详情/编辑响应共用；LEFT JOIN users 取作者卡字段）。
#[derive(sqlx::FromRow, Debug, Clone)]
pub struct CommentProjection {
    pub id: String,
    pub post_id: String,
    pub author_id: String,
    pub parent_id: Option<String>,
    pub floor: i64,
    pub version: i64,
    pub status: String,
    /// Markdown 原文（仅 published 评论参与 body_html 计算）。
    pub content: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub author_name: Option<String>,
    pub author_display_name: Option<String>,
    pub author_level: Option<i64>,
}

/// 投影查询公共列（三库一致）。
const PROJECTION_COLUMNS: &str =
    "c.id, c.post_id, c.author_id, c.parent_id, c.floor, c.version, c.status, c.content, \
     c.created_at, c.updated_at, \
     u.username_normalized AS author_name, u.display_name AS author_display_name, \
     u.level AS author_level";

/// 评论 → Comment schema JSON（`body_html` 读取时渲染；非 published → null）。
///
/// OpenAPI Comment = ResourceMeta(id/version/created_at/updated_at) + Author(
/// username/display_name/level/profile_url) + status + body_html + parent_id +
/// floor + post_id。
pub fn comment_json(c: &CommentProjection) -> Value {
    let username = c.author_name.clone().unwrap_or_default();
    json!({
        "id": c.id,
        "post_id": c.post_id,
        "author": {
            "username": username,
            "display_name": c.author_display_name,
            "level": c.author_level.unwrap_or(1),
            "profile_url": format!("/users/{username}"),
        },
        "parent_id": c.parent_id,
        "floor": c.floor,
        "version": c.version,
        "status": c.status,
        "body_html": if c.status == "published" {
            Value::String(render_and_sanitize(&c.content))
        } else {
            Value::Null
        },
        "created_at": c.created_at,
        "updated_at": c.updated_at,
    })
}

/// 创建评论输入（路由层已完成 auth/权限/内容校验/主题/板块/锁帖/parent 重检）。
pub struct CreateCommentInput<'a> {
    pub comment_id: String,
    pub post_id: &'a str,
    pub author_id: &'a str,
    pub parent_id: Option<&'a str>,
    pub markdown: &'a str,
    pub now: i64,
}

/// 创建成功结果（路由组装响应用）。
#[derive(Debug, Clone)]
pub struct CreatedComment {
    pub floor: i64,
    pub body_html: String,
}

/// 事务内原子楼层分配 + 插入 + 帖子计数（M04-COMMENTS-03）。
///
/// 楼层 = `MAX(floor)+1`（软删评论保留楼层，不重编号）；`UNIQUE(post_id,
/// floor)` 唯一约束兜底并发，触发时返回 [`CreateCommentError::FloorContended`]。
/// SQLite 写事务可能因并发 busy/SNAPSHOT 失败：复用 M01-JOBS-09 指数退避重试
/// （整体重跑事务，保证楼层分配原子性）。
pub async fn create_comment(
    pool: &DatabasePool,
    input: &CreateCommentInput<'_>,
) -> Result<CreatedComment, CreateCommentError> {
    let body_html = render_and_sanitize(input.markdown);
    match pool {
        Either::Left(p) => {
            let floor = retry_on_busy(&BusyPolicy::default(), &BusyCounter::default(), || {
                create_comment_sqlite_tx(p, input)
            })
            .await
            .map_err(map_create_error)?;
            Ok(CreatedComment { floor, body_html })
        }
        Either::Right(p) => {
            let floor = create_comment_mysql_tx(p, input)
                .await
                .map_err(map_create_error)?;
            Ok(CreatedComment { floor, body_html })
        }
    }
}

/// 创建失败 → 稳定错误（`UNIQUE(post_id, floor)` 兜底 → FloorContended）。
fn map_create_error(e: sqlx::Error) -> CreateCommentError {
    if is_unique_violation(&e) {
        CreateCommentError::FloorContended
    } else {
        CreateCommentError::from(e)
    }
}

/// SQLite 创建事务（重试闭包体内；返回分配到的楼层）。
async fn create_comment_sqlite_tx(
    p: &sqlx::SqlitePool,
    input: &CreateCommentInput<'_>,
) -> Result<i64, sqlx::Error> {
    let mut tx = p.begin().await?;
    let floor: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(floor), 0) + 1 FROM comments WHERE post_id = ?")
            .bind(input.post_id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query(
        "INSERT INTO comments (id, post_id, author_id, parent_id, quoted_comment_id, content, content_format, status, floor, version, created_at, updated_at, deleted_at)
         VALUES (?, ?, ?, ?, NULL, ?, 'markdown', 'published', ?, 1, ?, ?, NULL)",
    )
    .bind(&input.comment_id)
    .bind(input.post_id)
    .bind(input.author_id)
    .bind(input.parent_id)
    .bind(input.markdown)
    .bind(floor)
    .bind(input.now)
    .bind(input.now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE posts SET reply_count = reply_count + 1, last_reply_id = ?, last_reply_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&input.comment_id)
    .bind(input.now)
    .bind(input.now)
    .bind(input.post_id)
    .execute(&mut *tx)
    .await?;
    // after_reply 策略：回复落库后为回复者写入 reply grant（M04-VISIBILITY-05）。
    grant_reply_if_after_reply_sqlite(&mut tx, input.post_id, input.author_id, input.now).await?;
    tx.commit().await?;
    Ok(floor)
}

/// SQLite：after_reply 策略下写入 reply grant（M04-VISIBILITY-05）。
///
/// 冻结规则：用户在本主题发布**有效且可见**的回复后获得访问授权；grant 键为
/// `post:{post_id}`（migration 0040 归一化约定），`UNIQUE(user_id,
/// grant_target_key)` 使同一用户的多条回复只持有一条 grant（重复插入忽略）。
/// 回复删除/处罚后的撤销语义见 [`revoke_reply_grant_if_not_persistent`]。
async fn grant_reply_if_after_reply_sqlite(
    tx: &mut sqlx::SqliteConnection,
    post_id: &str,
    user_id: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    let policy: Option<(String, String)> = sqlx::query_as(
        "SELECT pol.kind, pol.id
         FROM posts p LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
         WHERE p.id = ?",
    )
    .bind(post_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((kind, policy_id)) = policy else {
        return Ok(());
    };
    if kind != "after_reply" {
        return Ok(());
    }
    let grant_id = uuid::Uuid::now_v7().to_string();
    let source_id = format!("reply:{post_id}:{user_id}:{now}");
    sqlx::query(
        "INSERT OR IGNORE INTO content_access_grants
         (id, user_id, post_id, comment_id, policy_id, source_kind, source_id, point_operation_id, grant_target_key, granted_at, revoked_at)
         VALUES (?, ?, ?, NULL, ?, 'reply', ?, NULL, ?, ?, NULL)",
    )
    .bind(grant_id)
    .bind(user_id)
    .bind(post_id)
    .bind(policy_id)
    .bind(source_id)
    .bind(format!("post:{post_id}"))
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// MySQL/MariaDB 创建事务（InnoDB REPEATABLE READ + `UNIQUE(post_id, floor)`
/// 兜底；行锁死锁/超时由业务层按 409 稳定返回，与 M01-DB 指导一致）。
async fn create_comment_mysql_tx(
    p: &sqlx::MySqlPool,
    input: &CreateCommentInput<'_>,
) -> Result<i64, sqlx::Error> {
    let mut tx = p.begin().await?;
    let floor: i64 =
        sqlx::query_scalar("SELECT COALESCE(MAX(floor), 0) + 1 FROM comments WHERE post_id = ?")
            .bind(input.post_id)
            .fetch_one(&mut *tx)
            .await?;
    sqlx::query(
        "INSERT INTO comments (id, post_id, author_id, parent_id, quoted_comment_id, content, content_format, status, floor, version, created_at, updated_at, deleted_at)
         VALUES (?, ?, ?, ?, NULL, ?, 'markdown', 'published', ?, 1, ?, ?, NULL)",
    )
    .bind(&input.comment_id)
    .bind(input.post_id)
    .bind(input.author_id)
    .bind(input.parent_id)
    .bind(input.markdown)
    .bind(floor)
    .bind(input.now)
    .bind(input.now)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE posts SET reply_count = reply_count + 1, last_reply_id = ?, last_reply_at = ?, updated_at = ? WHERE id = ?",
    )
    .bind(&input.comment_id)
    .bind(input.now)
    .bind(input.now)
    .bind(input.post_id)
    .execute(&mut *tx)
    .await?;
    // after_reply 策略：回复落库后为回复者写入 reply grant（M04-VISIBILITY-05）。
    grant_reply_if_after_reply_mysql(&mut tx, input.post_id, input.author_id, input.now).await?;
    tx.commit().await?;
    Ok(floor)
}

/// MySQL/MariaDB：after_reply 策略下写入 reply grant（M04-VISIBILITY-05）。
/// 与 SQLite 分支同语义；`INSERT IGNORE` 使同一用户重复回复只持有一条 grant。
async fn grant_reply_if_after_reply_mysql(
    tx: &mut sqlx::MySqlConnection,
    post_id: &str,
    user_id: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    let policy: Option<(String, String)> = sqlx::query_as(
        "SELECT pol.kind, pol.id
         FROM posts p LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
         WHERE p.id = ?",
    )
    .bind(post_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((kind, policy_id)) = policy else {
        return Ok(());
    };
    if kind != "after_reply" {
        return Ok(());
    }
    let grant_id = uuid::Uuid::now_v7().to_string();
    let source_id = format!("reply:{post_id}:{user_id}:{now}");
    sqlx::query(
        "INSERT IGNORE INTO content_access_grants
         (id, user_id, post_id, comment_id, policy_id, source_kind, source_id, point_operation_id, grant_target_key, granted_at, revoked_at)
         VALUES (?, ?, ?, NULL, ?, 'reply', ?, NULL, ?, ?, NULL)",
    )
    .bind(grant_id)
    .bind(user_id)
    .bind(post_id)
    .bind(policy_id)
    .bind(source_id)
    .bind(format!("post:{post_id}"))
    .bind(now)
    .execute(&mut *tx)
    .await?;
    Ok(())
}

/// 按 id 读取评论投影（含软删行；调用方负责可见性过滤）。
pub async fn load_comment_projection(
    pool: &DatabasePool,
    id: &str,
) -> Result<Option<CommentProjection>, sqlx::Error> {
    let sql = format!(
        "SELECT {PROJECTION_COLUMNS}
         FROM comments c
         LEFT JOIN users u ON u.id = c.author_id
         WHERE c.id = ?"
    );
    match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, CommentProjection>(&sql)
                .bind(id)
                .fetch_optional(p)
                .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, CommentProjection>(&sql)
                .bind(id)
                .fetch_optional(p)
                .await
        }
    }
}

/// keyset 分页查询主题评论（M04-COMMENTS-04）。
///
/// 稳定排序 `floor ASC, id ASC`；`after` 为上一页最后一条游标；返回
/// `(items, has_more)`，内部 fetch limit+1 判定 `has_more`。
pub async fn list_comments_page(
    pool: &DatabasePool,
    post_id: &str,
    after: Option<&CommentCursor>,
    limit: i64,
) -> Result<(Vec<CommentProjection>, bool), sqlx::Error> {
    let sql = format!(
        "SELECT {PROJECTION_COLUMNS}
         FROM comments c
         LEFT JOIN users u ON u.id = c.author_id
         WHERE c.post_id = ?
           AND (? IS NULL OR (c.floor > ? OR (c.floor = ? AND c.id > ?)))
         ORDER BY c.floor ASC, c.id ASC LIMIT ?"
    );
    let fetch = limit + 1;
    let after_floor = after.map(|c| c.floor);
    let after_id = after.map(|c| c.id.as_str());
    let rows: Vec<CommentProjection> = match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, CommentProjection>(&sql)
                .bind(post_id)
                .bind(after_floor)
                .bind(after_floor)
                .bind(after_floor)
                .bind(after_id)
                .bind(fetch)
                .fetch_all(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as::<_, CommentProjection>(&sql)
                .bind(post_id)
                .bind(after_floor)
                .bind(after_floor)
                .bind(after_floor)
                .bind(after_id)
                .bind(fetch)
                .fetch_all(p)
                .await?
        }
    };
    let has_more = rows.len() > limit as usize;
    let items = rows.into_iter().take(limit as usize).collect();
    Ok((items, has_more))
}

/// 更新评论输入（路由层已完成 author/状态/窗口/版本校验；本函数内仍以
/// 版本守卫的 UPDATE 兜底并发，并在同一事务内写入不可变修订快照）。
pub struct EditCommentInput<'a> {
    pub comment_id: &'a str,
    pub editor_id: &'a str,
    pub new_markdown: &'a str,
    pub expected_version: i64,
    pub change_reason: Option<&'a str>,
    pub now: i64,
}

/// 作者编辑：版本守卫 UPDATE + 不可变 `comment_revisions` 快照（M04-COMMENTS-05）。
///
/// 同一事务内：`UPDATE comments SET content=?, version=version+1, updated_at=?`
/// （`WHERE version = expected_version`，rows=0 时区分 NotFound/VersionMismatch）
/// → INSERT `comment_revisions`（version = expected_version+1，UNIQUE
/// (comment_id, version) 兜底）。SQLite 分支复用 M01-JOBS-09 指数退避重试。
pub async fn update_comment(
    pool: &DatabasePool,
    input: &EditCommentInput<'_>,
) -> Result<(), EditCommentError> {
    let body_html = render_and_sanitize(input.new_markdown);
    let new_version = input.expected_version + 1;
    let revision_id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            let outcome = retry_on_busy(&BusyPolicy::default(), &BusyCounter::default(), || {
                update_comment_sqlite_tx(p, input, &body_html, new_version, &revision_id)
            })
            .await;
            map_edit_outcome(outcome, input, new_version)
        }
        Either::Right(p) => {
            let outcome =
                update_comment_mysql_tx(p, input, &body_html, new_version, &revision_id).await;
            map_edit_outcome(outcome, input, new_version)
        }
    }
}

/// 编辑事务结果（rows=0 分支把 NotFound/VersionMismatch 编码进 Ok 变体，
/// 使 busy 重试闭包保持 `Result<_, sqlx::Error>`）。
enum EditTxOutcome {
    Success,
    NotFound,
    VersionMismatch { actual: i64 },
}

/// 编辑事务结果 → 服务层错误（`UNIQUE(comment_id, version)` 兜底 → 版本冲突）。
fn map_edit_outcome(
    outcome: Result<EditTxOutcome, sqlx::Error>,
    input: &EditCommentInput<'_>,
    new_version: i64,
) -> Result<(), EditCommentError> {
    match outcome {
        Ok(EditTxOutcome::Success) => Ok(()),
        Ok(EditTxOutcome::NotFound) => Err(EditCommentError::NotFound),
        Ok(EditTxOutcome::VersionMismatch { actual }) => Err(EditCommentError::VersionMismatch {
            expected: input.expected_version,
            actual,
        }),
        Err(e) if is_unique_violation(&e) => Err(EditCommentError::VersionMismatch {
            expected: input.expected_version,
            actual: new_version,
        }),
        Err(e) => Err(EditCommentError::from(e)),
    }
}

/// SQLite 编辑事务（重试闭包体内）。
async fn update_comment_sqlite_tx(
    p: &sqlx::SqlitePool,
    input: &EditCommentInput<'_>,
    body_html: &str,
    new_version: i64,
    revision_id: &str,
) -> Result<EditTxOutcome, sqlx::Error> {
    let mut tx = p.begin().await?;
    let updated = sqlx::query(
        "UPDATE comments SET content = ?, version = version + 1, updated_at = ?
         WHERE id = ? AND deleted_at IS NULL AND status = 'published' AND version = ?",
    )
    .bind(input.new_markdown)
    .bind(input.now)
    .bind(input.comment_id)
    .bind(input.expected_version)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        let current: Option<(i64,)> =
            sqlx::query_as("SELECT version FROM comments WHERE id = ? AND deleted_at IS NULL")
                .bind(input.comment_id)
                .fetch_optional(&mut *tx)
                .await?;
        return Ok(match current {
            Some((actual,)) if actual != input.expected_version => {
                EditTxOutcome::VersionMismatch { actual }
            }
            _ => EditTxOutcome::NotFound,
        });
    }
    sqlx::query(
        "INSERT INTO comment_revisions (id, comment_id, editor_id, body_markdown, body_html, renderer_version, change_reason, version, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(revision_id)
    .bind(input.comment_id)
    .bind(input.editor_id)
    .bind(input.new_markdown)
    .bind(body_html)
    .bind(policy_version())
    .bind(input.change_reason)
    .bind(new_version)
    .bind(input.now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(EditTxOutcome::Success)
}

/// MySQL/MariaDB 编辑事务。
async fn update_comment_mysql_tx(
    p: &sqlx::MySqlPool,
    input: &EditCommentInput<'_>,
    body_html: &str,
    new_version: i64,
    revision_id: &str,
) -> Result<EditTxOutcome, sqlx::Error> {
    let mut tx = p.begin().await?;
    let updated = sqlx::query(
        "UPDATE comments SET content = ?, version = version + 1, updated_at = ?
         WHERE id = ? AND deleted_at IS NULL AND status = 'published' AND version = ?",
    )
    .bind(input.new_markdown)
    .bind(input.now)
    .bind(input.comment_id)
    .bind(input.expected_version)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        let current: Option<(i64,)> =
            sqlx::query_as("SELECT version FROM comments WHERE id = ? AND deleted_at IS NULL")
                .bind(input.comment_id)
                .fetch_optional(&mut *tx)
                .await?;
        return Ok(match current {
            Some((actual,)) if actual != input.expected_version => {
                EditTxOutcome::VersionMismatch { actual }
            }
            _ => EditTxOutcome::NotFound,
        });
    }
    sqlx::query(
        "INSERT INTO comment_revisions (id, comment_id, editor_id, body_markdown, body_html, renderer_version, change_reason, version, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(revision_id)
    .bind(input.comment_id)
    .bind(input.editor_id)
    .bind(input.new_markdown)
    .bind(body_html)
    .bind(policy_version())
    .bind(input.change_reason)
    .bind(new_version)
    .bind(input.now)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(EditTxOutcome::Success)
}

/// 软删除评论（M04-COMMENTS-06）：`status='deleted'` + `deleted_at` 置位，
/// 行保留（占位投影/审计）。返回 `true` 表示删除成功（未删除行不存在）。
///
/// 语义约定：软删**不递减** `posts.reply_count`——软删评论以占位投影继续
/// 占用楼层，`reply_count` 表示"已发布的回复总数"（含占位），删除后数量
/// 不变（与 SCHEMA.md 楼层不重编号语义一致）。
///
/// after_reply 撤销（M04-VISIBILITY-05）：回复删除后按冻结规则
/// `reply_grant_persists` 决定是否保留授权——`0` 时撤销该用户对主题的 reply
/// grant（`revoked_at` 置位），`1` 时保留。
pub async fn soft_delete_comment(
    pool: &DatabasePool,
    id: &str,
    now: i64,
) -> Result<bool, sqlx::Error> {
    // 先读主题与作者（after_reply grant 撤销判定；删除的目标行不存在 → 直接 404）
    let meta: Option<(String, String)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT post_id, author_id FROM comments WHERE id = ?")
                .bind(id)
                .fetch_optional(p)
                .await?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT post_id, author_id FROM comments WHERE id = ?")
                .bind(id)
                .fetch_optional(p)
                .await?
        }
    };
    let Some((post_id, author_id)) = meta else {
        return Ok(false);
    };

    let sql = "UPDATE comments SET status = 'deleted', deleted_at = ?, updated_at = ?
               WHERE id = ? AND deleted_at IS NULL";
    let deleted = match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(now)
                .bind(now)
                .bind(id)
                .execute(p)
                .await?
                .rows_affected()
                == 1
        }
        Either::Right(p) => {
            sqlx::query(sql)
                .bind(now)
                .bind(now)
                .bind(id)
                .execute(p)
                .await?
                .rows_affected()
                == 1
        }
    };
    if deleted {
        revoke_reply_grant_if_not_persistent(pool, &post_id, &author_id, now).await?;
    }
    Ok(deleted)
}

/// after_reply 回复删除后按冻结规则撤销/保留 reply grant（M04-VISIBILITY-05）。
///
/// 仅当主题策略为 `after_reply` 且 `reply_grant_persists = 0` 时撤销（置
/// `revoked_at`）；`reply_grant_persists = 1` 或非 after_reply 策略不动作。
async fn revoke_reply_grant_if_not_persistent(
    pool: &DatabasePool,
    post_id: &str,
    user_id: &str,
    now: i64,
) -> Result<(), sqlx::Error> {
    let persists: Option<i64> = match pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT pol.reply_grant_persists
             FROM posts p
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             WHERE p.id = ? AND pol.kind = 'after_reply'",
            )
            .bind(post_id)
            .fetch_optional(p)
            .await?
        }
        Either::Right(p) => {
            sqlx::query_scalar(
                "SELECT pol.reply_grant_persists
             FROM posts p
             LEFT JOIN content_access_policies pol ON pol.id = p.access_policy_id
             WHERE p.id = ? AND pol.kind = 'after_reply'",
            )
            .bind(post_id)
            .fetch_optional(p)
            .await?
        }
    };
    let Some(persists) = persists else {
        return Ok(());
    };
    if persists != 0 {
        return Ok(());
    }
    let grant_key = format!("post:{post_id}");
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE content_access_grants SET revoked_at = ?
                 WHERE user_id = ? AND grant_target_key = ? AND source_kind = 'reply'
                   AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(user_id)
            .bind(grant_key)
            .execute(p)
            .await?;
        }
        Either::Right(p) => {
            sqlx::query(
                "UPDATE content_access_grants SET revoked_at = ?
                 WHERE user_id = ? AND grant_target_key = ? AND source_kind = 'reply'
                   AND revoked_at IS NULL",
            )
            .bind(now)
            .bind(user_id)
            .bind(grant_key)
            .execute(p)
            .await?;
        }
    }
    Ok(())
}

/// 数据库唯一约束违规判定（与 `idempotency` 模块内部实现一致）。
fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db) if db.is_unique_violation()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comment_cursor_round_trips() {
        let cursor = CommentCursor::new(7, "comment-abc");
        assert_eq!(CommentCursor::decode(&cursor.encode()), Ok(cursor));
        let cursor = CommentCursor::new(0, "id-with-dashes");
        assert_eq!(CommentCursor::decode(&cursor.encode()), Ok(cursor));
    }

    #[test]
    fn comment_cursor_rejects_malformed_input() {
        assert_eq!(
            CommentCursor::decode("not!base64"),
            Err(CommentCursorError::Malformed)
        );
        assert_eq!(
            CommentCursor::decode(""),
            Err(CommentCursorError::Malformed)
        );
        // base64 合法但内容无 "floor:id" 结构
        assert_eq!(
            CommentCursor::decode("YWJj"), // "abc"
            Err(CommentCursorError::Malformed)
        );
        assert_eq!(
            CommentCursor::decode(&base64_url("7:")), // 空 id
            Err(CommentCursorError::Malformed)
        );
    }

    #[test]
    fn parent_scope_rejects_cross_post() {
        assert!(validate_parent_scope("p1", "p1").is_ok());
        assert_eq!(
            validate_parent_scope("p1", "p2"),
            Err("parent comment must belong to the same post")
        );
    }

    #[test]
    fn edit_window_constant_is_30_minutes() {
        assert_eq!(COMMENT_EDIT_WINDOW_MS, 30 * 60 * 1000);
    }

    fn base64_url(text: &str) -> String {
        use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
        URL_SAFE_NO_PAD.encode(text)
    }
}
