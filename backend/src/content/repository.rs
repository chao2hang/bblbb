//! M04-SCHEMA-01：posts 仓储契约。
//!
//! 提供元数据投影的插入/读取（三库一致）。骨架遗留 NOT NULL 列
//! （content/content_format/visibility/pinned）以空串/默认值写入，随
//! M04-POSTS 替换骨架时收口。

use sqlx::Either;

use crate::db::DatabasePool;

use super::model::{Post, PostStatus, PostType};

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
