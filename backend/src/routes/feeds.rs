use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use sha2::Digest;

use crate::feeds::{
    compute_cache_revisions, count_sitemap_posts, load_feed_posts, load_sitemap_posts, render_atom,
    render_robots, render_rss, render_sitemap, render_sitemap_index, site_search_index,
    x_robots_tag, FeedCache, FeedCacheEntry, FeedCacheKey,
};
use crate::outbox::now_millis;
use crate::{app::AppState, error::AppError};

/// RSS/Atom/sitemap/robots 路由（M08-FEEDS）。
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/api/v1/rss", get(get_rss_feed))
        .route("/api/v1/atom", get(get_atom_feed))
        .route("/api/v1/sitemap.xml", get(get_sitemap))
        .route("/robots.txt", get(get_robots_txt))
}

#[derive(Deserialize)]
struct FeedQuery {
    #[serde(default = "default_limit")]
    limit: i64,
    /// 稳定 keyset cursor（`base64url("published_at|id")`，M08-FEEDS-01）。
    #[serde(default)]
    after: Option<String>,
}

#[derive(Deserialize)]
struct SitemapQuery {
    /// 分片页码（1-based；缺省时若总量超限返回 `<sitemapindex>`）。
    #[serde(default)]
    page: Option<i64>,
    #[serde(default = "default_sitemap_limit")]
    limit: i64,
}

fn default_limit() -> i64 {
    20
}

fn default_sitemap_limit() -> i64 {
    500
}

/// GET /api/v1/rss — RSS 2.0（稳定 cursor + 缓存/ETag，M08-FEEDS-01）。
async fn get_rss_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let request_id = "rss";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let limit = query.limit.clamp(1, 50);

    // M08-FEEDS-06：缓存按数据源身份 + policy/content revision 与投影维度隔离。
    let cache = FeedCache::global();
    let (policy_rev, content_rev) = compute_cache_revisions(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let identity = crate::feeds::cache_pool_identity(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let params = format!(
        "limit={limit}&after={}",
        query.after.as_deref().unwrap_or("")
    );
    let key = FeedCacheKey {
        endpoint: "rss",
        params,
        identity,
        policy_revision: policy_rev,
        content_revision: content_rev,
        projection_dim: "public|rss",
    };
    let now = now_millis();
    let (body, etag) = match cache.get(&key, now) {
        Some(entry) => (entry.body, entry.etag),
        None => {
            let site_index = site_search_index(pool)
                .await
                .map_err(|e| AppError::internal(e, request_id))?;
            let page = load_feed_posts(pool, limit, query.after.as_deref(), &site_index)
                .await
                .map_err(|e| AppError::internal(e, request_id))?;
            let body = render_rss(&page.items, now);
            let etag = etag_for("rss", &body);
            cache.put(
                key,
                FeedCacheEntry {
                    body: body.clone(),
                    etag: etag.clone(),
                    computed_at: now,
                },
            );
            (body, etag)
        }
    };

    Ok(xml_response(
        "application/rss+xml; charset=utf-8",
        &etag,
        &body,
        &headers,
    ))
}

/// GET /api/v1/atom — Atom 1.0（字段/链接/更新时间/XML escaping，M08-FEEDS-02）。
async fn get_atom_feed(
    State(state): State<AppState>,
    Query(query): Query<FeedQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let request_id = "atom";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let limit = query.limit.clamp(1, 50);

    let cache = FeedCache::global();
    let (policy_rev, content_rev) = compute_cache_revisions(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let identity = crate::feeds::cache_pool_identity(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let params = format!(
        "limit={limit}&after={}",
        query.after.as_deref().unwrap_or("")
    );
    let key = FeedCacheKey {
        endpoint: "atom",
        params,
        identity,
        policy_revision: policy_rev,
        content_revision: content_rev,
        projection_dim: "public|atom",
    };
    let now = now_millis();
    let (body, etag) = match cache.get(&key, now) {
        Some(entry) => (entry.body, entry.etag),
        None => {
            let site_index = site_search_index(pool)
                .await
                .map_err(|e| AppError::internal(e, request_id))?;
            let page = load_feed_posts(pool, limit, query.after.as_deref(), &site_index)
                .await
                .map_err(|e| AppError::internal(e, request_id))?;
            let body = render_atom(&page.items, now);
            let etag = etag_for("atom", &body);
            cache.put(
                key,
                FeedCacheEntry {
                    body: body.clone(),
                    etag: etag.clone(),
                    computed_at: now,
                },
            );
            (body, etag)
        }
    };

    Ok(xml_response(
        "application/atom+xml; charset=utf-8",
        &etag,
        &body,
        &headers,
    ))
}

/// GET /api/v1/sitemap.xml — 只列入允许索引的公开 canonical URL
/// （限量 + 分页/分片，M08-FEEDS-03）。
async fn get_sitemap(
    State(state): State<AppState>,
    Query(query): Query<SitemapQuery>,
    headers: HeaderMap,
) -> Result<Response, AppError> {
    let request_id = "sitemap";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;
    let limit = query.limit.clamp(100, 5000);

    let site_index = site_search_index(pool)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let total = count_sitemap_posts(pool, &site_index)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;
    let total_pages = ((total as f64) / (limit as f64)).ceil() as i64;

    let body = match query.page {
        None if total_pages > 1 => {
            // 超限：返回 <sitemapindex> 分片导航。
            let pages: Vec<usize> = (1..=total_pages).map(|p| p as usize).collect();
            render_sitemap_index(&pages, "/api/v1/sitemap.xml")
        }
        None => {
            let items = load_sitemap_posts(pool, 0, limit, &site_index)
                .await
                .map_err(|e| AppError::internal(e, request_id))?;
            render_sitemap(&items)
        }
        Some(page) => {
            if page < 1 || page > total_pages.max(1) {
                // 越界分片：返回空 urlset（稳定 200，避免枚举总量泄漏分片数）。
                render_sitemap(&[])
            } else {
                let offset = (page - 1) * limit;
                let items = load_sitemap_posts(pool, offset, limit, &site_index)
                    .await
                    .map_err(|e| AppError::internal(e, request_id))?;
                render_sitemap(&items)
            }
        }
    };
    let etag = etag_for("sitemap", &body);
    Ok(xml_response(
        "application/xml; charset=utf-8",
        &etag,
        &body,
        &headers,
    ))
}

/// GET /robots.txt — 动态 robots（AI 爬虫默认拒绝；声明层不替代服务端边界，
/// M08-FEEDS-04）。
async fn get_robots_txt(State(state): State<AppState>) -> Result<Response, AppError> {
    let _ = &state;
    let body = render_robots();
    let mut resp = (StatusCode::OK, body).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=3600"),
    );
    Ok(resp)
}

/// 构造 XML 响应：Content-Type + `X-Robots-Tag: noindex` + `Cache-Control` +
/// ETag + 304 短路（`If-None-Match` 命中）。
fn xml_response(content_type: &str, etag: &str, body: &str, headers: &HeaderMap) -> Response {
    let etag_value =
        HeaderValue::from_str(etag).unwrap_or_else(|_| HeaderValue::from_static("\"feeds\""));
    if let Some(inm) = headers.get(header::IF_NONE_MATCH) {
        if inm.to_str().ok().is_some_and(|v| v == etag) {
            let mut not_modified = Response::new(axum::body::Body::empty());
            *not_modified.status_mut() = StatusCode::NOT_MODIFIED;
            not_modified.headers_mut().insert(header::ETAG, etag_value);
            return not_modified;
        }
    }
    let mut resp = (StatusCode::OK, body.to_owned()).into_response();
    resp.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(content_type)
            .unwrap_or_else(|_| HeaderValue::from_static("text/plain")),
    );
    // Feed/sitemap 是内容分发通道，页面本身不参与搜索索引。
    resp.headers_mut().insert(
        "x-robots-tag",
        HeaderValue::from_static(x_robots_tag(false)),
    );
    resp.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("public, max-age=300"),
    );
    resp.headers_mut().insert(header::ETAG, etag_value);
    resp
}

/// 稳定 ETag（内容派生的弱校验值）。
fn etag_for(kind: &str, body: &str) -> String {
    let mut hasher = <sha2::Sha256 as Digest>::new();
    hasher.update(kind.as_bytes());
    hasher.update(body.as_bytes());
    let out = hasher.finalize();
    let hex = out[..6]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    format!("\"feeds-{hex}\"")
}
