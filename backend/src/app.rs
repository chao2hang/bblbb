use std::sync::Arc;

use axum::{middleware, routing::get, Router};

use crate::{
    config::AppConfig,
    middleware::request_id::request_id,
    routes::{health::healthz, openapi::openapi},
};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
}

pub fn build_router(config: AppConfig) -> Router {
    let state = AppState {
        config: Arc::new(config),
    };

    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/v1/openapi.json", get(openapi))
        .with_state(state)
        .layer(middleware::from_fn(request_id))
}
