use std::sync::Arc;
use std::time::Duration;

use axum::{middleware, routing::get, Router};
use tower_http::{limit::RequestBodyLimitLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::{
    config::AppConfig,
    db::pool::DatabasePool,
    middleware::{
        csrf::csrf_protection, request_id::request_id, security_headers::security_headers,
    },
    routes::{
        admin, ai, auth, boards, comments, economy, feeds, health::healthz, marketplace,
        moderation, oidc, openapi::openapi, posts, ready, search, storage, themes, users, video,
    },
};

/// 应用共享状态
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: Option<Arc<DatabasePool>>,
}

impl AppState {
    /// 检查数据库是否可用
    pub fn has_db(&self) -> bool {
        self.db.is_some()
    }
}

/// 构建完整路由
pub fn build_router(config: AppConfig, db: Option<DatabasePool>) -> Router {
    let state = AppState {
        config: Arc::new(config),
        db: db.map(Arc::new),
    };

    // 基础端点（不需要 AppState）
    let base_routes = Router::new()
        .route("/healthz", get(healthz))
        .route("/readyz", get(ready::readyz))
        .with_state(state.clone());

    // OpenAPI 端点
    let openapi_routes = Router::new()
        .route("/api/v1/openapi.json", get(openapi))
        .with_state(state.clone());

    // API v1 路由（按领域模块组织）
    let api_routes = Router::new()
        .merge(auth::router())
        .merge(users::router())
        .merge(boards::router())
        .merge(posts::router())
        .merge(comments::router())
        .merge(moderation::router())
        .merge(storage::router())
        .merge(economy::router())
        .merge(ai::router())
        .merge(video::router())
        .merge(oidc::router())
        .merge(marketplace::router())
        .merge(admin::router())
        .merge(feeds::router())
        .merge(search::router())
        .merge(themes::router())
        // CSRF 防护：状态变更请求 + 会话 Cookie 必须携带合法 X-CSRF-Token
        .layer(middleware::from_fn_with_state(
            state.clone(),
            csrf_protection,
        ))
        .with_state(state);

    Router::new()
        .merge(base_routes)
        .merge(openapi_routes)
        .merge(api_routes)
        // 中间件层（从外到内顺序）
        .layer(middleware::from_fn(security_headers))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .layer(RequestBodyLimitLayer::new(10 * 1024 * 1024)) // 10MB 上限
        .layer(TraceLayer::new_for_http())
        .layer(middleware::from_fn(request_id))
}
