use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use sqlx::Either;

use crate::{app::AppState, error::AppError};

/// RSS/Atom/sitemap 路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/rss", get(get_rss_feed))
        .route("/api/v1/atom", get(get_atom_feed))
}

#[derive(Deserialize)]
struct FeedQuery {
    #[serde(default = "default_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    20
}

/// GET /api/v1/rss — RSS 订阅源
async fn get_rss_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let request_id = "rss";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);
    let posts = fetch_recent_posts(pool, limit, request_id).await?;

    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<rss version=\"2.0\">\n<channel>\n",
    );
    xml.push_str("<title>BBLBB</title>\n<link>/</link>\n<description>最新帖子</description>\n");

    for p in &posts {
        xml.push_str(&format!(
            "<item>\n<title>{}</title>\n<link>/posts/{}</link>\n<pubDate>{}</pubDate>\n</item>\n",
            xml_escape(&p.title),
            p.id,
            p.created_at,
        ));
    }

    xml.push_str("</channel>\n</rss>");

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "application/rss+xml; charset=utf-8",
        )],
        xml,
    )
        .into_response())
}

/// GET /api/v1/atom — Atom 订阅源
async fn get_atom_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
) -> Result<Response, AppError> {
    let request_id = "atom";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let limit = query.limit.clamp(1, 50);
    let posts = fetch_recent_posts(pool, limit, request_id).await?;

    let mut xml = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<feed xmlns=\"http://www.w3.org/2005/Atom\">\n");
    xml.push_str("<title>BBLBB</title>\n<link href=\"/\"/>\n<id>/</id>\n");

    for p in &posts {
        xml.push_str(&format!(
            "<entry>\n<title>{}</title>\n<link href=\"/posts/{}\"/>\n<id>/posts/{}</id>\n<updated>{}</updated>\n</entry>\n",
            xml_escape(&p.title),
            p.id,
            p.id,
            p.created_at,
        ));
    }

    xml.push_str("</feed>");

    Ok((
        [(
            axum::http::header::CONTENT_TYPE,
            "application/atom+xml; charset=utf-8",
        )],
        xml,
    )
        .into_response())
}

async fn fetch_recent_posts(
    pool: &sqlx::Either<sqlx::SqlitePool, sqlx::MySqlPool>,
    limit: i64,
    request_id: &str,
) -> Result<Vec<RecentPost>, AppError> {
    match pool {
        Either::Left(p) => {
            sqlx::query_as::<_, RecentRow>(
                "SELECT p.id, p.title, p.created_at, u.username_normalized as author_name
                 FROM posts p LEFT JOIN users u ON u.id = p.author_id
                 WHERE p.status = 'published' AND p.visibility = 'public'
                 ORDER BY p.created_at DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(p)
            .await
        }
        Either::Right(p) => {
            sqlx::query_as::<_, RecentRow>(
                "SELECT p.id, p.title, p.created_at, u.username_normalized as author_name
                 FROM posts p LEFT JOIN users u ON u.id = p.author_id
                 WHERE p.status = 'published' AND p.visibility = 'public'
                 ORDER BY p.created_at DESC LIMIT ?",
            )
            .bind(limit)
            .fetch_all(p)
            .await
        }
    }
    .map_err(|e| AppError::internal(e.to_string(), request_id))
    .map(|rows| {
        rows.into_iter()
            .map(|r| RecentPost {
                id: r.id,
                title: r.title,
                created_at: r.created_at,
                author_name: r.author_name,
            })
            .collect()
    })
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

struct RecentPost {
    id: String,
    title: String,
    created_at: i64,
    /// 作者名（当前订阅源输出未包含，保留供后续扩展）
    #[allow(dead_code)]
    author_name: Option<String>,
}

#[derive(sqlx::FromRow)]
struct RecentRow {
    id: String,
    title: String,
    created_at: i64,
    author_name: Option<String>,
}
