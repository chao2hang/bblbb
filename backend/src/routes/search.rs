use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
    routing::get,
    Router,
};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::session::AuthSession;
use crate::search::{
    execute_public_search, search_result_json, SearchQueryError, SearchRequest, DEFAULT_LIMIT,
};
use crate::{app::AppState, error::AppError};

/// 搜索路由（M08-INDEX-06/07）。
pub fn router() -> Router<AppState> {
    Router::new().route("/api/v1/search", get(search_public_content))
}

#[derive(Deserialize)]
struct SearchQuery {
    q: String,
    #[serde(default = "default_limit")]
    limit: i64,
    /// keyset 游标（base64url(`depth|indexed_at|doc_id`)）。
    #[serde(default)]
    after: Option<String>,
}

fn default_limit() -> i64 {
    DEFAULT_LIMIT
}

/// GET /api/v1/search — 公开投影搜索（M08-INDEX-06/07）。
///
/// 限制：查询长度/语法（400）、结果数与分页深度（cursor 内编码）、高亮长度；
/// **匿名频率限制由 antibot 中间件的 Search 独立桶执行**（M08-CRAWL-03，
/// CRAWLER-POLICY §5；429/挑战/封禁状态机在中间件层）。结果返回前逐条实时
/// 重检可见性/处罚/索引退出（索引只是候选集，不是授权裁决）；响应只含 OpenAPI
/// `SearchResult` 字段（id/type/title/url/excerpt）+ 有界 highlight。
async fn search_public_content(
    State(state): State<AppState>,
    _auth: AuthSession,
    Query(query): Query<SearchQuery>,
) -> Result<Response, AppError> {
    let request_id = "search";
    let pool = state
        .db
        .as_deref()
        .ok_or_else(|| AppError::internal("database not configured", request_id))?;

    let now = crate::outbox::now_millis();
    let req = SearchRequest::parse(&query.q, query.limit, query.after.as_deref())
        .map_err(|e: SearchQueryError| AppError::bad_request(e.to_string(), request_id, None))?;

    let page = execute_public_search(pool, &req, now)
        .await
        .map_err(|e| AppError::internal(e, request_id))?;

    let items: Vec<Value> = page
        .items
        .iter()
        .zip(page.highlights.iter())
        .map(|(proj, hl)| search_result_json(proj, hl.as_deref()))
        .collect();

    let body = json!({
        "items": items,
        "page": {
            "next_cursor": page.next_cursor,
            "has_more": page.has_more,
        },
    });
    Ok((StatusCode::OK, Json(body)).into_response())
}
