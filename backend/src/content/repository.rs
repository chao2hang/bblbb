//! M04-SCHEMA-01：posts 仓储契约。
//!
//! 提供元数据投影的插入/读取（三库一致）。骨架遗留 NOT NULL 列
//! （content/content_format/visibility/pinned）以空串/默认值写入，随
//! M04-POSTS 替换骨架时收口。

use sqlx::Either;

use crate::db::DatabasePool;

use super::model::{
    Comment, CommentStatus, Draft, Post, PostContent, PostRevision, PostStatus, PostType,
};

/// 插入一篇帖子（含元数据列；骨架遗留列写空值/默认）。
pub async fn insert_post(pool: &DatabasePool, post: &Post) -> Result<(), sqlx::Error> {
    let sql = "INSERT INTO posts (
        id, board_id, author_id, post_type, slug, title, status, version,
        scheduled_at, published_at, pinned_at, featured_at, closed_at,
        canonical_url, seo_title, seo_description, view_count, reply_count,
        last_reply_id, last_reply_at, content, created_at, updated_at, deleted_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '', ?, ?, ?)";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&post.id)
            .bind(&post.board_id)
            .bind(&post.author_id)
            .bind(post.post_type.as_str())
            .bind(&post.slug)
            .bind(&post.title)
            .bind(post.status.as_str())
            .bind(post.version)
            .bind(post.scheduled_at)
            .bind(post.published_at)
            .bind(post.pinned_at)
            .bind(post.featured_at)
            .bind(post.closed_at)
            .bind(&post.canonical_url)
            .bind(&post.seo_title)
            .bind(&post.seo_description)
            .bind(post.view_count)
            .bind(post.reply_count)
            .bind(&post.last_reply_id)
            .bind(post.last_reply_at)
            .bind(post.created_at)
            .bind(post.updated_at)
            .bind(post.deleted_at)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&post.id)
            .bind(&post.board_id)
            .bind(&post.author_id)
            .bind(post.post_type.as_str())
            .bind(&post.slug)
            .bind(&post.title)
            .bind(post.status.as_str())
            .bind(post.version)
            .bind(post.scheduled_at)
            .bind(post.published_at)
            .bind(post.pinned_at)
            .bind(post.featured_at)
            .bind(post.closed_at)
            .bind(&post.canonical_url)
            .bind(&post.seo_title)
            .bind(&post.seo_description)
            .bind(post.view_count)
            .bind(post.reply_count)
            .bind(&post.last_reply_id)
            .bind(post.last_reply_at)
            .bind(post.created_at)
            .bind(post.updated_at)
            .bind(post.deleted_at)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

/// 按 id 读取帖子元数据（不存在 → `None`；含 deleted 行）。
pub async fn get_post(pool: &DatabasePool, id: &str) -> Result<Option<Post>, sqlx::Error> {
    let sql = "SELECT id, board_id, author_id, post_type, slug, title, status, version,
        scheduled_at, published_at, pinned_at, featured_at, closed_at,
        canonical_url, seo_title, seo_description, view_count, reply_count,
        last_reply_id, last_reply_at, created_at, updated_at, deleted_at
        FROM posts WHERE id = ?";
    match pool {
        Either::Left(p) => {
            let row = sqlx::query_as::<_, PostRow>(sql)
                .bind(id)
                .fetch_optional(p)
                .await?;
            Ok(row.map(PostRow::into_model))
        }
        Either::Right(p) => {
            let row = sqlx::query_as::<_, PostRow>(sql)
                .bind(id)
                .fetch_optional(p)
                .await?;
            Ok(row.map(PostRow::into_model))
        }
    }
}

#[derive(sqlx::FromRow)]
struct PostRow {
    id: String,
    board_id: String,
    author_id: String,
    post_type: String,
    slug: Option<String>,
    title: String,
    status: String,
    version: i64,
    scheduled_at: Option<i64>,
    published_at: Option<i64>,
    pinned_at: Option<i64>,
    featured_at: Option<i64>,
    closed_at: Option<i64>,
    canonical_url: Option<String>,
    seo_title: Option<String>,
    seo_description: Option<String>,
    view_count: i64,
    reply_count: i64,
    last_reply_id: Option<String>,
    last_reply_at: Option<i64>,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
}

impl PostRow {
    fn into_model(self) -> Post {
        Post {
            id: self.id,
            board_id: self.board_id,
            author_id: self.author_id,
            post_type: PostType::parse(&self.post_type).expect("post_type 必须为稳定枚举"),
            slug: self.slug,
            title: self.title,
            status: PostStatus::parse(&self.status).expect("status 必须为稳定枚举"),
            version: self.version,
            scheduled_at: self.scheduled_at,
            published_at: self.published_at,
            pinned_at: self.pinned_at,
            featured_at: self.featured_at,
            closed_at: self.closed_at,
            canonical_url: self.canonical_url,
            seo_title: self.seo_title,
            seo_description: self.seo_description,
            view_count: self.view_count,
            reply_count: self.reply_count,
            last_reply_id: self.last_reply_id,
            last_reply_at: self.last_reply_at,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
        }
    }
}

/// 保存/覆盖帖子当前正文（post_contents，1:1 with posts）。
pub async fn save_post_content(
    pool: &DatabasePool,
    content: &PostContent,
) -> Result<(), sqlx::Error> {
    let sql = "INSERT INTO post_contents (
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
        updated_at = excluded.updated_at";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&content.post_id)
            .bind(&content.body_markdown)
            .bind(&content.body_html)
            .bind(&content.restricted_markdown)
            .bind(&content.restricted_html)
            .bind(&content.renderer_version)
            .bind(&content.excerpt)
            .bind(content.updated_at)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&content.post_id)
            .bind(&content.body_markdown)
            .bind(&content.body_html)
            .bind(&content.restricted_markdown)
            .bind(&content.restricted_html)
            .bind(&content.renderer_version)
            .bind(&content.excerpt)
            .bind(content.updated_at)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

/// 读取帖子当前正文（不存在 → `None`）。
pub async fn load_post_content(
    pool: &DatabasePool,
    post_id: &str,
) -> Result<Option<PostContent>, sqlx::Error> {
    let sql = "SELECT post_id, body_markdown, body_html, restricted_markdown, restricted_html,
        renderer_version, excerpt, updated_at FROM post_contents WHERE post_id = ?";
    match pool {
        Either::Left(p) => {
            let row = sqlx::query_as::<_, PostContentRow>(sql)
                .bind(post_id)
                .fetch_optional(p)
                .await?;
            Ok(row.map(PostContentRow::into_model))
        }
        Either::Right(p) => {
            let row = sqlx::query_as::<_, PostContentRow>(sql)
                .bind(post_id)
                .fetch_optional(p)
                .await?;
            Ok(row.map(PostContentRow::into_model))
        }
    }
}

/// 写入一条不可变修订快照（post_revisions；同 (post_id, version) 冲突）。
pub async fn insert_post_revision(
    pool: &DatabasePool,
    revision: &PostRevision,
) -> Result<(), sqlx::Error> {
    let sql = "INSERT INTO post_revisions (
        id, post_id, editor_id, body_markdown, body_html, restricted_markdown,
        restricted_html, renderer_version, change_reason, version, created_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&revision.id)
            .bind(&revision.post_id)
            .bind(&revision.editor_id)
            .bind(&revision.body_markdown)
            .bind(&revision.body_html)
            .bind(&revision.restricted_markdown)
            .bind(&revision.restricted_html)
            .bind(&revision.renderer_version)
            .bind(&revision.change_reason)
            .bind(revision.version)
            .bind(revision.created_at)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&revision.id)
            .bind(&revision.post_id)
            .bind(&revision.editor_id)
            .bind(&revision.body_markdown)
            .bind(&revision.body_html)
            .bind(&revision.restricted_markdown)
            .bind(&revision.restricted_html)
            .bind(&revision.renderer_version)
            .bind(&revision.change_reason)
            .bind(revision.version)
            .bind(revision.created_at)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

/// 读取某帖全部修订，按 version 升序（列表/详情用，M04-POSTS-11）。
pub async fn list_post_revisions(
    pool: &DatabasePool,
    post_id: &str,
) -> Result<Vec<PostRevision>, sqlx::Error> {
    let sql = "SELECT id, post_id, editor_id, body_markdown, body_html,
        restricted_markdown, restricted_html, renderer_version, change_reason,
        version, created_at FROM post_revisions WHERE post_id = ? ORDER BY version ASC";
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query_as::<_, PostRevisionRow>(sql)
                .bind(post_id)
                .fetch_all(p)
                .await?;
            Ok(rows.into_iter().map(PostRevisionRow::into_model).collect())
        }
        Either::Right(p) => {
            let rows = sqlx::query_as::<_, PostRevisionRow>(sql)
                .bind(post_id)
                .fetch_all(p)
                .await?;
            Ok(rows.into_iter().map(PostRevisionRow::into_model).collect())
        }
    }
}

#[derive(sqlx::FromRow)]
struct PostContentRow {
    post_id: String,
    body_markdown: String,
    body_html: String,
    restricted_markdown: Option<String>,
    restricted_html: Option<String>,
    renderer_version: String,
    excerpt: String,
    updated_at: i64,
}

impl PostContentRow {
    fn into_model(self) -> PostContent {
        PostContent {
            post_id: self.post_id,
            body_markdown: self.body_markdown,
            body_html: self.body_html,
            restricted_markdown: self.restricted_markdown,
            restricted_html: self.restricted_html,
            renderer_version: self.renderer_version,
            excerpt: self.excerpt,
            updated_at: self.updated_at,
        }
    }
}

#[derive(sqlx::FromRow)]
struct PostRevisionRow {
    id: String,
    post_id: String,
    editor_id: String,
    body_markdown: String,
    body_html: String,
    restricted_markdown: Option<String>,
    restricted_html: Option<String>,
    renderer_version: String,
    change_reason: Option<String>,
    version: i64,
    created_at: i64,
}

impl PostRevisionRow {
    fn into_model(self) -> PostRevision {
        PostRevision {
            id: self.id,
            post_id: self.post_id,
            editor_id: self.editor_id,
            body_markdown: self.body_markdown,
            body_html: self.body_html,
            restricted_markdown: self.restricted_markdown,
            restricted_html: self.restricted_html,
            renderer_version: self.renderer_version,
            change_reason: self.change_reason,
            version: self.version,
            created_at: self.created_at,
        }
    }
}

// ─────────────────────── 草稿（M04-SCHEMA-03） ───────────────────────

/// 插入一条草稿。
pub async fn insert_draft(pool: &DatabasePool, draft: &Draft) -> Result<(), sqlx::Error> {
    let sql = "INSERT INTO drafts (
        id, owner_id, board_id, post_type, title, markdown, visibility_level,
        access_policy, scheduled_at, version, created_at, updated_at, deleted_at
    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&draft.id)
            .bind(&draft.owner_id)
            .bind(&draft.board_id)
            .bind(draft.post_type.as_str())
            .bind(&draft.title)
            .bind(&draft.markdown)
            .bind(draft.visibility_level)
            .bind(&draft.access_policy)
            .bind(draft.scheduled_at)
            .bind(draft.version)
            .bind(draft.created_at)
            .bind(draft.updated_at)
            .bind(draft.deleted_at)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&draft.id)
            .bind(&draft.owner_id)
            .bind(&draft.board_id)
            .bind(draft.post_type.as_str())
            .bind(&draft.title)
            .bind(&draft.markdown)
            .bind(draft.visibility_level)
            .bind(&draft.access_policy)
            .bind(draft.scheduled_at)
            .bind(draft.version)
            .bind(draft.created_at)
            .bind(draft.updated_at)
            .bind(draft.deleted_at)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

/// 按 id + owner 读取草稿（仅本人；已软删返回 `None`）。
pub async fn get_draft(
    pool: &DatabasePool,
    id: &str,
    owner_id: &str,
) -> Result<Option<Draft>, sqlx::Error> {
    let sql = "SELECT id, owner_id, board_id, post_type, title, markdown,
        visibility_level, access_policy, scheduled_at, version, created_at,
        updated_at, deleted_at FROM drafts WHERE id = ? AND owner_id = ? AND deleted_at IS NULL";
    match pool {
        Either::Left(p) => {
            let row = sqlx::query_as::<_, DraftRow>(sql)
                .bind(id)
                .bind(owner_id)
                .fetch_optional(p)
                .await?;
            Ok(row.map(DraftRow::into_model))
        }
        Either::Right(p) => {
            let row = sqlx::query_as::<_, DraftRow>(sql)
                .bind(id)
                .bind(owner_id)
                .fetch_optional(p)
                .await?;
            Ok(row.map(DraftRow::into_model))
        }
    }
}

/// 更新草稿并递增 version（乐观并发，If-Match 冲突由调用方用 version 判定）。
pub async fn update_draft(pool: &DatabasePool, draft: &Draft) -> Result<(), sqlx::Error> {
    let sql = "UPDATE drafts SET
        board_id = ?, post_type = ?, title = ?, markdown = ?, visibility_level = ?,
        access_policy = ?, scheduled_at = ?, version = version + 1, updated_at = ?
        WHERE id = ? AND owner_id = ? AND deleted_at IS NULL";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&draft.board_id)
            .bind(draft.post_type.as_str())
            .bind(&draft.title)
            .bind(&draft.markdown)
            .bind(draft.visibility_level)
            .bind(&draft.access_policy)
            .bind(draft.scheduled_at)
            .bind(draft.updated_at)
            .bind(&draft.id)
            .bind(&draft.owner_id)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&draft.board_id)
            .bind(draft.post_type.as_str())
            .bind(&draft.title)
            .bind(&draft.markdown)
            .bind(draft.visibility_level)
            .bind(&draft.access_policy)
            .bind(draft.scheduled_at)
            .bind(draft.updated_at)
            .bind(&draft.id)
            .bind(&draft.owner_id)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

/// 软删除草稿（`deleted_at` 置位；行保留供审计/恢复）。
pub async fn delete_draft(
    pool: &DatabasePool,
    id: &str,
    owner_id: &str,
    deleted_at: i64,
) -> Result<(), sqlx::Error> {
    let sql = "UPDATE drafts SET deleted_at = ?, updated_at = ? WHERE id = ? AND owner_id = ? AND deleted_at IS NULL";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(deleted_at)
            .bind(deleted_at)
            .bind(id)
            .bind(owner_id)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(deleted_at)
            .bind(deleted_at)
            .bind(id)
            .bind(owner_id)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

/// owner 维度 cursor 列表（按 updated_at 降序；cursor = 上次最后一条的
/// updated_at，keyset 分页）。`before` 为 `None` 时取最新页。
pub async fn list_drafts_cursor(
    pool: &DatabasePool,
    owner_id: &str,
    before: Option<i64>,
    limit: i64,
) -> Result<Vec<Draft>, sqlx::Error> {
    let sql = "SELECT id, owner_id, board_id, post_type, title, markdown,
        visibility_level, access_policy, scheduled_at, version, created_at,
        updated_at, deleted_at FROM drafts
        WHERE owner_id = ? AND deleted_at IS NULL AND (? IS NULL OR updated_at < ?)
        ORDER BY updated_at DESC, id DESC LIMIT ?";
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query_as::<_, DraftRow>(sql)
                .bind(owner_id)
                .bind(before)
                .bind(before)
                .bind(limit)
                .fetch_all(p)
                .await?;
            Ok(rows.into_iter().map(DraftRow::into_model).collect())
        }
        Either::Right(p) => {
            let rows = sqlx::query_as::<_, DraftRow>(sql)
                .bind(owner_id)
                .bind(before)
                .bind(before)
                .bind(limit)
                .fetch_all(p)
                .await?;
            Ok(rows.into_iter().map(DraftRow::into_model).collect())
        }
    }
}

/// 读取 owner 的定时发布草稿（scheduled_at 非空且未过期、未软删、未发布）。
pub async fn list_scheduled_drafts(
    pool: &DatabasePool,
    due_before: i64,
    limit: i64,
) -> Result<Vec<Draft>, sqlx::Error> {
    let sql = "SELECT id, owner_id, board_id, post_type, title, markdown,
        visibility_level, access_policy, scheduled_at, version, created_at,
        updated_at, deleted_at FROM drafts
        WHERE deleted_at IS NULL AND scheduled_at IS NOT NULL AND scheduled_at <= ?
        ORDER BY scheduled_at ASC LIMIT ?";
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query_as::<_, DraftRow>(sql)
                .bind(due_before)
                .bind(limit)
                .fetch_all(p)
                .await?;
            Ok(rows.into_iter().map(DraftRow::into_model).collect())
        }
        Either::Right(p) => {
            let rows = sqlx::query_as::<_, DraftRow>(sql)
                .bind(due_before)
                .bind(limit)
                .fetch_all(p)
                .await?;
            Ok(rows.into_iter().map(DraftRow::into_model).collect())
        }
    }
}

#[derive(sqlx::FromRow)]
struct DraftRow {
    id: String,
    owner_id: String,
    board_id: Option<String>,
    post_type: String,
    title: String,
    markdown: String,
    visibility_level: Option<i64>,
    access_policy: Option<String>,
    scheduled_at: Option<i64>,
    version: i64,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
}

impl DraftRow {
    fn into_model(self) -> Draft {
        Draft {
            id: self.id,
            owner_id: self.owner_id,
            board_id: self.board_id,
            post_type: PostType::parse(&self.post_type).expect("post_type 必须为稳定枚举"),
            title: self.title,
            markdown: self.markdown,
            visibility_level: self.visibility_level,
            access_policy: self.access_policy,
            scheduled_at: self.scheduled_at,
            version: self.version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
        }
    }
}

// ─────────────────────── 评论（M04-SCHEMA-04） ───────────────────────

/// 插入一条评论（楼层号由调用方在事务内分配，M04-COMMENTS-03）。
pub async fn insert_comment(pool: &DatabasePool, comment: &Comment) -> Result<(), sqlx::Error> {
    let sql = "INSERT INTO comments (
        id, post_id, author_id, parent_id, quoted_comment_id, content,
        content_format, status, floor, version, created_at, updated_at, deleted_at
    ) VALUES (?, ?, ?, ?, ?, '', 'markdown', ?, ?, 1, ?, ?, ?)";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&comment.id)
            .bind(&comment.post_id)
            .bind(&comment.author_id)
            .bind(&comment.parent_id)
            .bind(&comment.quoted_comment_id)
            .bind(comment.status.as_str())
            .bind(comment.floor)
            .bind(comment.created_at)
            .bind(comment.updated_at)
            .bind(comment.deleted_at)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&comment.id)
            .bind(&comment.post_id)
            .bind(&comment.author_id)
            .bind(&comment.parent_id)
            .bind(&comment.quoted_comment_id)
            .bind(comment.status.as_str())
            .bind(comment.floor)
            .bind(comment.created_at)
            .bind(comment.updated_at)
            .bind(comment.deleted_at)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

/// 按 id 读取评论（含 deleted 行；调用方负责投影过滤）。
pub async fn get_comment(pool: &DatabasePool, id: &str) -> Result<Option<Comment>, sqlx::Error> {
    let sql = "SELECT id, post_id, author_id, parent_id, quoted_comment_id, status,
        floor, version, created_at, updated_at, deleted_at FROM comments WHERE id = ?";
    match pool {
        Either::Left(p) => {
            let row = sqlx::query_as::<_, CommentRow>(sql)
                .bind(id)
                .fetch_optional(p)
                .await?;
            Ok(row.map(CommentRow::into_model))
        }
        Either::Right(p) => {
            let row = sqlx::query_as::<_, CommentRow>(sql)
                .bind(id)
                .fetch_optional(p)
                .await?;
            Ok(row.map(CommentRow::into_model))
        }
    }
}

/// 按帖子读取评论（floor 升序；调用方负责可见性过滤）。
pub async fn list_comments_by_post(
    pool: &DatabasePool,
    post_id: &str,
) -> Result<Vec<Comment>, sqlx::Error> {
    let sql = "SELECT id, post_id, author_id, parent_id, quoted_comment_id, status,
        floor, version, created_at, updated_at, deleted_at FROM comments
        WHERE post_id = ? ORDER BY floor ASC";
    match pool {
        Either::Left(p) => {
            let rows = sqlx::query_as::<_, CommentRow>(sql)
                .bind(post_id)
                .fetch_all(p)
                .await?;
            Ok(rows.into_iter().map(CommentRow::into_model).collect())
        }
        Either::Right(p) => {
            let rows = sqlx::query_as::<_, CommentRow>(sql)
                .bind(post_id)
                .fetch_all(p)
                .await?;
            Ok(rows.into_iter().map(CommentRow::into_model).collect())
        }
    }
}

/// 更新评论并递增 version（乐观并发；作者限时编辑由服务层判定）。
pub async fn update_comment(pool: &DatabasePool, comment: &Comment) -> Result<(), sqlx::Error> {
    let sql = "UPDATE comments SET quoted_comment_id = ?, status = ?,
        version = version + 1, updated_at = ? WHERE id = ? AND deleted_at IS NULL";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(&comment.quoted_comment_id)
            .bind(comment.status.as_str())
            .bind(comment.updated_at)
            .bind(&comment.id)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(&comment.quoted_comment_id)
            .bind(comment.status.as_str())
            .bind(comment.updated_at)
            .bind(&comment.id)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

/// 软删除评论（`deleted_at` 置位；行保留供审计/占位投影）。
pub async fn delete_comment(
    pool: &DatabasePool,
    id: &str,
    deleted_at: i64,
) -> Result<(), sqlx::Error> {
    let sql =
        "UPDATE comments SET deleted_at = ?, updated_at = ? WHERE id = ? AND deleted_at IS NULL";
    match pool {
        Either::Left(p) => sqlx::query(sql)
            .bind(deleted_at)
            .bind(deleted_at)
            .bind(id)
            .execute(p)
            .await
            .map(|_| ()),
        Either::Right(p) => sqlx::query(sql)
            .bind(deleted_at)
            .bind(deleted_at)
            .bind(id)
            .execute(p)
            .await
            .map(|_| ()),
    }
}

#[derive(sqlx::FromRow)]
struct CommentRow {
    id: String,
    post_id: String,
    author_id: String,
    parent_id: Option<String>,
    quoted_comment_id: Option<String>,
    status: String,
    floor: i64,
    version: i64,
    created_at: i64,
    updated_at: i64,
    deleted_at: Option<i64>,
}

impl CommentRow {
    fn into_model(self) -> Comment {
        Comment {
            id: self.id,
            post_id: self.post_id,
            author_id: self.author_id,
            parent_id: self.parent_id,
            quoted_comment_id: self.quoted_comment_id,
            floor: self.floor,
            status: CommentStatus::parse(&self.status).expect("status 必须为稳定枚举"),
            version: self.version,
            created_at: self.created_at,
            updated_at: self.updated_at,
            deleted_at: self.deleted_at,
        }
    }
}
