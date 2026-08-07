//! M04-POSTS-06：发布写路径——即时发布与 scheduled 发布 Job 共用。
//!
//! 所有发布都先在事务外执行 [`publish_preflight`]（再次授权与等级校验），再在
//! **同一事务**内原子写入：
//! - `posts` 行：author/status/version/统计值全部服务端权威赋值（不信任客户端）；
//! - `post_contents`：`render_content` 全管线产物（body_html/受限/摘要/策略版本）；
//! - `post_revisions`：初始不可变修订（version=1，editor=author）；
//! - 即时发布额外：板块 `post_count + 1` + 搜索索引 Job 入队；
//! - scheduled 发布：`posts.status='draft' + scheduled_at`，到期后
//!   [`publish_scheduled_post`] 再执行（再次预检 + 事务切换 published）。

use sqlx::Either;

use crate::content::markdown::rerender::render_content;
use crate::content::model::{Post, PostContent, PostRevision, PostStatus};
use crate::content::posts::command::CreatePostCommand;
use crate::content::posts::publish::{publish_preflight, PublishBlocked, PublishPreflightInput};
use crate::content::repository::{get_post, load_post_content};
use crate::db::DatabasePool;
use crate::search::index_job::enqueue_index_job;

/// 发布错误（预检阻断 / 不存在 / 版本冲突 / 数据库）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    Blocked(PublishBlocked),
    NotFound(String),
    /// If-Match 版本与当前不一致（409）。
    VersionMismatch {
        expected: i64,
        actual: i64,
    },
    Db(String),
}

impl From<PublishBlocked> for PublishError {
    fn from(b: PublishBlocked) -> Self {
        Self::Blocked(b)
    }
}

impl From<sqlx::Error> for PublishError {
    fn from(e: sqlx::Error) -> Self {
        Self::Db(e.to_string())
    }
}

impl std::fmt::Display for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Blocked(b) => write!(f, "publish blocked: {b}"),
            Self::NotFound(msg) => write!(f, "publish target not found: {msg}"),
            Self::VersionMismatch { expected, actual } => {
                write!(f, "version mismatch: expected {expected}, current {actual}")
            }
            Self::Db(msg) => write!(f, "publish db error: {msg}"),
        }
    }
}

/// 发布成功后的完整记录。
#[derive(Debug, Clone)]
pub struct PublishedPost {
    pub post: Post,
    pub content: PostContent,
}

/// 从标题生成板块内唯一 slug（低冲突：base-slug + post_id 前 8 位）。
fn generate_slug(title: &str, post_id: &str) -> String {
    let base: String = title
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let base = base.trim_matches('-').to_string();
    let base = if base.is_empty() {
        "post".to_string()
    } else {
        base
    };
    format!("{base}-{}", &post_id[..post_id.len().min(8)])
}

/// 组装发布预检输入（createPost 路径：策略明细字段随 M04-VISIBILITY 落地，
/// level/paid 在当前契约下无明细 → 预检按结构校验拦截，安全 fail-closed）。
fn preflight_input(author_id: &str, cmd: &CreatePostCommand) -> PublishPreflightInput {
    PublishPreflightInput {
        author_id: author_id.to_string(),
        board_id: cmd.board_id.to_string(),
        visibility_level: cmd.visibility_level,
        access_policy: cmd.access_policy.as_str().to_string(),
        min_level: None,
        currency_id: None,
        amount: None,
        attachment_ids: Vec::new(),
    }
}

/// 发布一篇新帖（createPost）：`scheduled_at` 为 None → 即时发布；
/// 为 Some → 落 `status='draft'` 等待 Job。
pub async fn publish_new_post(
    pool: &DatabasePool,
    cmd: &CreatePostCommand,
    author_id: &str,
    now: i64,
) -> Result<PublishedPost, PublishError> {
    publish_preflight(pool, &preflight_input(author_id, cmd))
        .await
        .map_err(PublishError::from)?;

    let rendered = render_content(cmd.markdown.as_str(), None);
    let is_scheduled = cmd.scheduled_at.is_some();

    let post_id = uuid::Uuid::now_v7().to_string();
    let post = Post {
        id: post_id.clone(),
        board_id: cmd.board_id.to_string(),
        author_id: author_id.to_string(),
        post_type: cmd.post_type,
        slug: Some(generate_slug(cmd.title.as_str(), &post_id)),
        title: cmd.title.to_string(),
        status: if is_scheduled {
            PostStatus::Draft
        } else {
            PostStatus::Published
        },
        version: 1,
        scheduled_at: cmd.scheduled_at,
        published_at: if is_scheduled { None } else { Some(now) },
        pinned_at: None,
        featured_at: None,
        closed_at: None,
        canonical_url: None,
        seo_title: None,
        seo_description: None,
        view_count: 0,
        reply_count: 0,
        last_reply_id: None,
        last_reply_at: None,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    };
    let content = PostContent {
        post_id: post_id.clone(),
        body_markdown: cmd.markdown.to_string(),
        body_html: rendered.body_html,
        restricted_markdown: None,
        restricted_html: None,
        renderer_version: rendered.renderer_version,
        excerpt: rendered.excerpt,
        updated_at: now,
    };
    let revision = PostRevision {
        id: uuid::Uuid::now_v7().to_string(),
        post_id: post_id.clone(),
        editor_id: author_id.to_string(),
        body_markdown: cmd.markdown.to_string(),
        body_html: content.body_html.clone(),
        restricted_markdown: None,
        restricted_html: None,
        renderer_version: content.renderer_version.clone(),
        change_reason: Some("initial".to_string()),
        version: 1,
        created_at: now,
    };

    insert_published_tx(pool, &post, &content, &revision, !is_scheduled, now).await?;

    if !is_scheduled {
        enqueue_index_job(pool, "post", &post_id)
            .await
            .map_err(PublishError::Db)?;
    }
    Ok(PublishedPost { post, content })
}

/// 事务写：posts + post_contents + post_revisions（+ 即时发布时板块计数）。
macro_rules! insert_published_body {
    ($tx:expr, $post:expr, $content:expr, $revision:expr, $bump:expr, $now:expr) => {
        sqlx::query(
            "INSERT INTO posts (
                id, board_id, author_id, post_type, slug, title, status, version,
                scheduled_at, published_at, pinned_at, featured_at, closed_at,
                canonical_url, seo_title, seo_description, view_count, reply_count,
                last_reply_id, last_reply_at, content, created_at, updated_at, deleted_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?, ?)",
        )
        .bind(&$post.id)
        .bind(&$post.board_id)
        .bind(&$post.author_id)
        .bind($post.post_type.as_str())
        .bind(&$post.slug)
        .bind(&$post.title)
        .bind($post.status.as_str())
        .bind($post.version)
        .bind($post.scheduled_at)
        .bind($post.published_at)
        .bind($post.pinned_at)
        .bind($post.featured_at)
        .bind($post.closed_at)
        .bind(&$post.canonical_url)
        .bind(&$post.seo_title)
        .bind(&$post.seo_description)
        .bind($post.view_count)
        .bind($post.reply_count)
        .bind(&$post.last_reply_id)
        .bind($post.last_reply_at)
        .bind($post.created_at)
        .bind($post.updated_at)
        .bind($post.deleted_at)
        .execute(&mut *$tx)
        .await?;
        sqlx::query(
            "INSERT INTO post_contents (
                post_id, body_markdown, body_html, restricted_markdown, restricted_html,
                renderer_version, excerpt, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&$content.post_id)
        .bind(&$content.body_markdown)
        .bind(&$content.body_html)
        .bind(&$content.restricted_markdown)
        .bind(&$content.restricted_html)
        .bind(&$content.renderer_version)
        .bind(&$content.excerpt)
        .bind($content.updated_at)
        .execute(&mut *$tx)
        .await?;
        sqlx::query(
            "INSERT INTO post_revisions (
                id, post_id, editor_id, body_markdown, body_html, restricted_markdown,
                restricted_html, renderer_version, change_reason, version, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&$revision.id)
        .bind(&$revision.post_id)
        .bind(&$revision.editor_id)
        .bind(&$revision.body_markdown)
        .bind(&$revision.body_html)
        .bind(&$revision.restricted_markdown)
        .bind(&$revision.restricted_html)
        .bind(&$revision.renderer_version)
        .bind(&$revision.change_reason)
        .bind($revision.version)
        .bind($revision.created_at)
        .execute(&mut *$tx)
        .await?;
        if $bump {
            sqlx::query(
                "UPDATE boards SET post_count = post_count + 1, updated_at = ? WHERE id = ?",
            )
            .bind($now)
            .bind(&$post.board_id)
            .execute(&mut *$tx)
            .await?;
        }
    };
}

/// 事务写：posts + post_contents + post_revisions（+ 即时发布时板块计数）。
async fn insert_published_tx(
    pool: &DatabasePool,
    post: &Post,
    content: &PostContent,
    revision: &PostRevision,
    bump_board_count: bool,
    now: i64,
) -> Result<(), PublishError> {
    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            insert_published_body!(tx, post, content, revision, bump_board_count, now);
            tx.commit().await?;
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            insert_published_body!(tx, post, content, revision, bump_board_count, now);
            tx.commit().await?;
        }
    }
    Ok(())
}

/// scheduled 发布：到期后由 Job 再次预检并切换 `status='draft' → 'published'`。
///
/// - 再次运行 [`publish_preflight`]（账号/等级/板块/策略在**执行时**重读）；
/// - 事务：置 published_at、status、板块 post_count+1；
/// - 搜索索引 Job 入队；返回 PublishedPost。
pub async fn publish_scheduled_post(
    pool: &DatabasePool,
    post_id: &str,
    now: i64,
) -> Result<PublishedPost, PublishError> {
    let post = get_post(pool, post_id)
        .await?
        .ok_or_else(|| PublishError::NotFound("post not found".to_string()))?;
    if post.status != PostStatus::Draft || post.scheduled_at.is_none() {
        return Err(PublishError::NotFound(
            "post is not a scheduled draft".to_string(),
        ));
    }

    // 执行时再次授权与等级校验
    let preflight = PublishPreflightInput {
        author_id: post.author_id.clone(),
        board_id: post.board_id.clone(),
        visibility_level: None, // 帖子表不含 visibility_level；策略投影随 M04-VISIBILITY
        access_policy: "public".to_string(),
        min_level: None,
        currency_id: None,
        amount: None,
        attachment_ids: Vec::new(),
    };
    publish_preflight(pool, &preflight)
        .await
        .map_err(PublishError::from)?;

    match pool {
        Either::Left(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "UPDATE posts SET status = 'published', published_at = ?, updated_at = ? WHERE id = ? AND status = 'draft'",
            )
            .bind(now)
            .bind(now)
            .bind(post_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE boards SET post_count = post_count + 1, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(&post.board_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
        Either::Right(p) => {
            let mut tx = p.begin().await?;
            sqlx::query(
                "UPDATE posts SET status = 'published', published_at = ?, updated_at = ? WHERE id = ? AND status = 'draft'",
            )
            .bind(now)
            .bind(now)
            .bind(post_id)
            .execute(&mut *tx)
            .await?;
            sqlx::query(
                "UPDATE boards SET post_count = post_count + 1, updated_at = ? WHERE id = ?",
            )
            .bind(now)
            .bind(&post.board_id)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
        }
    }

    enqueue_index_job(pool, "post", post_id)
        .await
        .map_err(PublishError::Db)?;

    let content = load_post_content(pool, post_id)
        .await?
        .ok_or_else(|| PublishError::NotFound("post content not found".to_string()))?;
    let refreshed = get_post(pool, post_id)
        .await?
        .ok_or_else(|| PublishError::NotFound("post not found".to_string()))?;
    Ok(PublishedPost {
        post: refreshed,
        content,
    })
}

/// 事务内 upsert post_contents + insert revision（可复用于编辑）。
macro_rules! save_content_and_revision_body {
    ($tx:expr, $content:expr, $revision:expr) => {
        sqlx::query(
            "INSERT INTO post_contents (
                post_id, body_markdown, body_html, restricted_markdown, restricted_html,
                renderer_version, excerpt, updated_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(post_id) DO UPDATE SET
                body_markdown = excluded.body_markdown,
                body_html = excluded.body_html,
                restricted_markdown = excluded.restricted_markdown,
                restricted_html = excluded.restricted_html,
                renderer_version = excluded.renderer_version,
                excerpt = excluded.excerpt,
                updated_at = excluded.updated_at",
        )
        .bind(&$content.post_id)
        .bind(&$content.body_markdown)
        .bind(&$content.body_html)
        .bind(&$content.restricted_markdown)
        .bind(&$content.restricted_html)
        .bind(&$content.renderer_version)
        .bind(&$content.excerpt)
        .bind($content.updated_at)
        .execute(&mut *$tx)
        .await?;
        sqlx::query(
            "INSERT INTO post_revisions (
                id, post_id, editor_id, body_markdown, body_html, restricted_markdown,
                restricted_html, renderer_version, change_reason, version, created_at
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&$revision.id)
        .bind(&$revision.post_id)
        .bind(&$revision.editor_id)
        .bind(&$revision.body_markdown)
        .bind(&$revision.body_html)
        .bind(&$revision.restricted_markdown)
        .bind(&$revision.restricted_html)
        .bind(&$revision.renderer_version)
        .bind(&$revision.change_reason)
        .bind($revision.version)
        .bind($revision.created_at)
        .execute(&mut *$tx)
        .await?;
    };
}

/// 编辑帖子（M04-POSTS-08）：创建**不可变修订**并更新当前正文。
///
/// - `expected_version`：If-Match 版本号；与当前 `posts.version` 不一致 →
///   [`PublishError::VersionMismatch`]（409）；
/// - 事务：更新 `post_contents`（重新渲染）+ 插入 `post_revisions`
///   （version=旧+1，UNIQUE(post_id,version) 保证每版恰好一条）+ `posts`
///   title/version+1/updated_at；
/// - 修订快照不可变：body_markdown 原文、editor、version、created_at 写入后
///   不再修改（后续重渲染 Job 只覆盖 html/excerpt）。
///
/// 编辑输入（M04-POSTS-08）：PATCH 语义——仅提供的字段更新。
#[derive(Debug, Clone)]
pub struct EditPostInput {
    pub title: Option<String>,
    pub markdown: Option<String>,
    pub expected_version: i64,
    pub change_reason: Option<String>,
}

pub async fn edit_post(
    pool: &DatabasePool,
    post_id: &str,
    editor_id: &str,
    input: &EditPostInput,
    now: i64,
) -> Result<Post, PublishError> {
    let current = get_post(pool, post_id)
        .await?
        .ok_or_else(|| PublishError::NotFound("post not found".to_string()))?;
    if current.status == PostStatus::Deleted || current.deleted_at.is_some() {
        return Err(PublishError::NotFound("post not found".to_string()));
    }
    if current.version != input.expected_version {
        return Err(PublishError::VersionMismatch {
            expected: input.expected_version,
            actual: current.version,
        });
    }

    // M04-VISIBILITY-04：编辑前重读作者等级（fail-closed）。
    //
    // EditPostInput 不携带可见性字段，因此按帖子的**当前有效可见等级**重检：
    // 若作者已被降级到帖子有效可见等级之下（如降级后编辑高隐藏帖），阻断。
    // 有效可见等级 = access policy 为 level 时的 min_level，否则 1。
    recheck_edit_author_level(pool, &current).await?;

    let old_content = load_post_content(pool, post_id)
        .await?
        .ok_or_else(|| PublishError::NotFound("post content not found".to_string()))?;

    // 新正文：markdown 提供则重新渲染；否则保留（仅改标题也创建修订）
    let (new_body_html, new_excerpt, new_renderer_version) = match input.markdown.as_deref() {
        Some(md) => {
            let rendered = render_content(md, None);
            (
                rendered.body_html,
                rendered.excerpt,
                rendered.renderer_version,
            )
        }
        None => (
            old_content.body_html.clone(),
            old_content.excerpt.clone(),
            old_content.renderer_version.clone(),
        ),
    };

    let new_version = current.version + 1;
    let revision = PostRevision {
        id: uuid::Uuid::now_v7().to_string(),
        post_id: post_id.to_string(),
        editor_id: editor_id.to_string(),
        body_markdown: input
            .markdown
            .clone()
            .unwrap_or_else(|| old_content.body_markdown.clone()),
        body_html: new_body_html.clone(),
        restricted_markdown: None,
        restricted_html: None,
        renderer_version: new_renderer_version.clone(),
        change_reason: input.change_reason.clone(),
        version: new_version,
        created_at: now,
    };
    let updated_content = PostContent {
        post_id: post_id.to_string(),
        body_markdown: input
            .markdown
            .clone()
            .unwrap_or_else(|| old_content.body_markdown.clone()),
        body_html: new_body_html,
        restricted_markdown: None,
        restricted_html: None,
        renderer_version: new_renderer_version,
        excerpt: new_excerpt,
        updated_at: now,
    };

    // 事务写；修订 UNIQUE(post_id, version) 并发兜底 → 映射为版本冲突
    let tx_result: Result<(), sqlx::Error> = async {
        match pool {
            Either::Left(p) => {
                let mut tx = p.begin().await?;
                save_content_and_revision_body!(tx, &updated_content, &revision);
                sqlx::query(
                    "UPDATE posts SET title = COALESCE(?, title), version = version + 1, updated_at = ? WHERE id = ? AND version = ?",
                )
                .bind(input.title.as_deref())
                .bind(now)
                .bind(post_id)
                .bind(input.expected_version)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
            }
            Either::Right(p) => {
                let mut tx = p.begin().await?;
                save_content_and_revision_body!(tx, &updated_content, &revision);
                sqlx::query(
                    "UPDATE posts SET title = COALESCE(?, title), version = version + 1, updated_at = ? WHERE id = ? AND version = ?",
                )
                .bind(input.title.as_deref())
                .bind(now)
                .bind(post_id)
                .bind(input.expected_version)
                .execute(&mut *tx)
                .await?;
                tx.commit().await?;
            }
        }
        Ok(())
    }
    .await;

    match tx_result {
        Ok(()) => {}
        Err(e) if is_unique_violation(&e) => {
            return Err(PublishError::VersionMismatch {
                expected: input.expected_version,
                actual: current.version,
            });
        }
        Err(e) => return Err(PublishError::Db(e.to_string())),
    }

    // 重新读取（版本已递增）
    let refreshed = get_post(pool, post_id)
        .await?
        .ok_or_else(|| PublishError::NotFound("post not found".to_string()))?;
    Ok(refreshed)
}

/// 唯一约束冲突（并发兜底：同 (post_id, version) 修订 / 同板块 slug）。
fn is_unique_violation(err: &sqlx::Error) -> bool {
    matches!(
        err,
        sqlx::Error::Database(db) if db.is_unique_violation()
    )
}

// ─────────────────────── M04-VISIBILITY-04：编辑前作者等级重检 ──────────────

/// 编辑前作者等级重检（fail-closed）：重读 `users.level`，若帖子当前有效
/// 可见等级超过作者当前等级 → [`PublishBlocked::VisibilityExceedsLevel`]。
///
/// `EditPostInput` 无可见性字段（编辑不改变可见性），重检锚点是帖子**当前
/// 有效可见等级**——`access_policy_id` 指向 level 策略时取其 `min_level`，
/// 其余（public/logged_in/after_reply/paid/未设置）按 1。
async fn recheck_edit_author_level(pool: &DatabasePool, post: &Post) -> Result<(), PublishBlocked> {
    let author_level = read_author_level(pool, &post.author_id).await?;
    let effective = current_effective_visibility(pool, &post.id).await?;
    let requested = effective.max(1);
    if requested > author_level {
        return Err(PublishBlocked::VisibilityExceedsLevel {
            requested,
            author_level,
        });
    }
    Ok(())
}

/// 重读作者当前等级（服务端权威，不信任客户端缓存）。
async fn read_author_level(pool: &DatabasePool, author_id: &str) -> Result<u32, PublishBlocked> {
    let level: Option<i64> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
            .bind(author_id)
            .fetch_optional(p)
            .await
            .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
        Either::Right(p) => sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
            .bind(author_id)
            .fetch_optional(p)
            .await
            .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
    };
    Ok(level
        .ok_or_else(|| PublishBlocked::AccountUnavailable("author not found".to_string()))?
        .clamp(1, i64::from(u32::MAX)) as u32)
}

/// 帖子当前有效可见等级（见 [`recheck_edit_author_level`] 文档）。
async fn current_effective_visibility(
    pool: &DatabasePool,
    post_id: &str,
) -> Result<u32, PublishBlocked> {
    let policy_id: Option<String> = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT access_policy_id FROM posts WHERE id = ?")
            .bind(post_id)
            .fetch_optional(p)
            .await
            .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
        Either::Right(p) => sqlx::query_scalar("SELECT access_policy_id FROM posts WHERE id = ?")
            .bind(post_id)
            .fetch_optional(p)
            .await
            .map_err(|e| PublishBlocked::Internal(e.to_string()))?,
    };
    let Some(pid) = policy_id else {
        return Ok(1);
    };
    let row: Option<(String, Option<i64>)> = match pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT kind, min_level FROM content_access_policies WHERE id = ?")
                .bind(&pid)
                .fetch_optional(p)
                .await
                .map_err(|e| PublishBlocked::Internal(e.to_string()))?
        }
        Either::Right(p) => {
            sqlx::query_as("SELECT kind, min_level FROM content_access_policies WHERE id = ?")
                .bind(&pid)
                .fetch_optional(p)
                .await
                .map_err(|e| PublishBlocked::Internal(e.to_string()))?
        }
    };
    match row {
        Some((kind, min_level)) if kind == "level" => {
            Ok(min_level.map_or(1, |lv| lv.clamp(1, i64::from(u32::MAX)) as u32))
        }
        _ => Ok(1),
    }
}
