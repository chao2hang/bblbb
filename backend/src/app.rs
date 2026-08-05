use std::sync::Arc;
use std::time::Duration;

use axum::{
    body::Body,
    extract::State,
    http::{Request as HttpRequest, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use tower_http::{limit::RequestBodyLimitLayer, timeout::TimeoutLayer, trace::TraceLayer};

use crate::{
    config::AppConfig,
    db::pool::DatabasePool,
    middleware::{
        csrf::csrf_protection,
        host_origin::host_origin_guard,
        problem::problem_instance,
        request_id::{request_id, RequestId},
        security_headers::security_headers,
    },
    ratelimit::RateLimiter,
    routes::{
        admin, ai, auth, boards, comments, economy, feeds, health::healthz, marketplace, mfa,
        moderation, oidc, openapi::openapi, posts, ready, search, storage, themes, users, video,
    },
};

/// 请求体大小上限（10MB，M00-BACKEND-06）
pub const BODY_LIMIT: usize = 10 * 1024 * 1024;

/// 请求处理超时（30 秒，M00-BACKEND-06）
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// 应用共享状态
///
/// M0 骨架只注入 `config` 与 `db`。M1 扩展点（M00-BACKEND-02）：
/// - `clock: Arc<dyn Clock>`：可测试时钟
/// - `storage: Arc<dyn Storage>`：对象/附件存储接口
/// - `jobs: Arc<JobDispatcher>` / `outbox`：任务与发件箱
/// - `audit: Arc<dyn AuditSink>`：审计写入接口
/// - `flags: Arc<FeatureFlags>`：功能开关
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: Option<Arc<DatabasePool>>,
    pub flags: crate::config::flags::FeatureFlags,
    /// 进程内限流器（M02-IDENTITY-06；多实例再引入 Redis）。
    pub limiter: Arc<RateLimiter>,
}

impl AppState {
    /// 检查数据库是否可用
    pub fn has_db(&self) -> bool {
        self.db.is_some()
    }
}

/// 构建完整路由（Flag 默认全部关闭）。
pub fn build_router(config: AppConfig, db: Option<DatabasePool>) -> Router {
    let flags = config.feature_flags();
    build_router_with_flags(config, db, flags)
}

/// 构建完整路由（自定义 Flag 快照；测试与未来的管理接口用）。
pub fn build_router_with_flags(
    config: AppConfig,
    db: Option<DatabasePool>,
    flags: crate::config::flags::FeatureFlags,
) -> Router {
    let state = AppState {
        config: Arc::new(config),
        db: db.map(Arc::new),
        flags,
        limiter: Arc::new(RateLimiter::new()),
    };
    let guard_state = state.clone();
    let gate_state = state.clone();

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
        .merge(mfa::router())
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

    // Trace span 携带真实 request_id（由 request_id 中间件最先注入扩展，
    // 此处直接读取），与响应头 x-request-id 保持一致
    let trace_layer = TraceLayer::new_for_http().make_span_with(|request: &HttpRequest<Body>| {
        let request_id = request
            .extensions()
            .get::<RequestId>()
            .map(|rid| rid.0.as_str())
            .unwrap_or("unknown");
        tracing::info_span!(
            "http_request",
            method = %request.method(),
            uri = %request.uri(),
            version = ?request.version(),
            request_id = %request_id,
            status = tracing::field::Empty,
            latency = tracing::field::Empty,
        )
    });

    Router::new()
        .merge(base_routes)
        .merge(openapi_routes)
        .merge(api_routes)
        // 中间件层（后添加者在外层；运行顺序为从外到内：
        // problem → request_id → feature_gate → host_origin → trace → body_limit
        // → timeout → security_headers → router）
        // problem_instance 必须最外层：内层中间件（如 Host/Origin/Feature Gate）
        // 提前返回的 Problem 响应也能被补齐 instance/request_id。
        // feature_gate 位于 request_id 内层，可读取 request_id 扩展。
        .layer(middleware::from_fn(security_headers))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            REQUEST_TIMEOUT,
        ))
        .layer(RequestBodyLimitLayer::new(BODY_LIMIT))
        .layer(trace_layer)
        .layer(middleware::from_fn_with_state(
            guard_state,
            host_origin_guard,
        ))
        .layer(middleware::from_fn_with_state(gate_state, feature_gate))
        .layer(middleware::from_fn(request_id))
        .layer(middleware::from_fn(problem_instance))
}

/// Feature Gate（M01-CONFIG-06）：可选能力默认关闭，命中其路由前缀且 Flag
/// 未启用时返回 `feature_disabled`（409）。放在 request_id 内层，可读取
/// request_id；由 problem_instance 补齐 instance。
async fn feature_gate(
    State(state): State<AppState>,
    request: HttpRequest<Body>,
    next: middleware::Next,
) -> Response {
    let path = request.uri().path();
    let Some(feature) = crate::config::flags::feature_for_path(path) else {
        return next.run(request).await;
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    if state.flags.is_enabled(feature, now) {
        return next.run(request).await;
    }
    let request_id = request
        .extensions()
        .get::<RequestId>()
        .map(|rid| rid.0.clone())
        .unwrap_or_else(|| "unknown".to_owned());
    tracing::info!(feature = %feature, path, "request blocked: feature disabled");
    crate::error::AppError::feature_disabled(
        format!("feature '{}' is currently disabled", feature),
        request_id,
    )
    .into_response()
}
