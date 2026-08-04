use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;

/// OIDC / OAuth 路由
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/.well-known/openid-configuration", get(well_known_config))
        .route("/oauth/authorize", get(oauth_authorize))
        .route("/oauth/token", post(oauth_token))
        .route("/oauth/userinfo", get(oauth_userinfo))
        .route(
            "/oauth/logout",
            get(oauth_logout_get).post(oauth_logout_post),
        )
        .route("/oauth/revoke", post(oauth_revoke))
        .route("/oauth/jwks.json", get(oauth_jwks))
        .route("/api/v1/oauth/interactions/{id}", get(get_interaction))
        .route(
            "/api/v1/oauth/interactions/{id}/decision",
            post(decide_interaction),
        )
}

async fn well_known_config(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_well_known_openid_configuration")
}

async fn oauth_authorize(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_oauth_authorize")
}

async fn oauth_token(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_oauth_token")
}

async fn oauth_userinfo(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_oauth_userinfo")
}

async fn oauth_logout_get(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_oauth_logout")
}

async fn oauth_logout_post(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_oauth_logout")
}

async fn oauth_revoke(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("post_oauth_revoke")
}

async fn oauth_jwks(State(_state): State<AppState>) -> (StatusCode, Json<Value>) {
    not_implemented("get_oauth_jwks_json")
}

async fn get_interaction(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("get_oauth_interactions_id_")
}

async fn decide_interaction(
    State(_state): State<AppState>,
    Path(_id): Path<String>,
) -> (StatusCode, Json<Value>) {
    not_implemented("post_oauth_interactions_id_decision")
}

fn not_implemented(operation: &str) -> (StatusCode, Json<Value>) {
    (
        StatusCode::NOT_IMPLEMENTED,
        Json(json!({
            "type": "about:blank",
            "title": "Not Implemented",
            "status": 501,
            "code": "not_implemented",
            "detail": format!("Operation '{}' is not yet implemented", operation),
        })),
    )
}
