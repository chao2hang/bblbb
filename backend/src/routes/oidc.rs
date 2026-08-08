//! OIDC / OAuth 路由（M11-PROTOCOL / M11-CONSENT）。
//!
//! 边界：
//! - 协议端点（`.well-known`、`/oauth/*`）返回标准 OAuth/OIDC 错误 JSON，
//!   不套业务 Problem 格式（docs/AUTH-OIDC.md §14）；
//! - `/api/v1/oauth/interactions/*` 走 Session + CSRF（全局 CSRF 中间件）；
//! - Feature Flag `Oidc` 默认关闭：路由前缀命中 `feature_for_path`，
//!   关闭时中间件直接 409 `feature_disabled`，不影响本地登录与核心论坛；
//! - issuer/回调 URL 全部来自固定 `AppConfig.public_origin`，不信任 Host 头。
#![allow(clippy::result_large_err)] // 协议错误以 axum Response 返回（体积固定可接受）

use std::collections::HashMap;

use axum::{
    body::Bytes,
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode, Uri},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use serde_json::{json, Value};

use crate::app::AppState;
use crate::auth::session::AuthSession;
use crate::oidc::protocol::{self, AuthorizeRequest};
use crate::oidc::OidcError;

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

// ─────────────────────────── 工具 ───────────────────────────

/// 标准 OAuth/OIDC 错误响应（JSON + Cache-Control: no-store）。
fn oauth_error(status: StatusCode, error: &str, description: &str) -> Response {
    (
        status,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        axum::Json(crate::oidc::oauth_error_body(error, description)),
    )
        .into_response()
}

/// 从 OidcError 映射为标准 OAuth 错误响应。
fn oauth_error_response(err: &OidcError) -> Response {
    // M15-OBSERVE-05：OIDC Token/授权码错误指标
    crate::observability::metrics::registry().counter_inc("bblbb_oidc_token_errors_total", 1);
    match err {
        OidcError::InvalidRequest(d) => oauth_error(StatusCode::BAD_REQUEST, "invalid_request", d),
        OidcError::InvalidClient(d) => oauth_error(StatusCode::UNAUTHORIZED, "invalid_client", d),
        OidcError::InvalidGrant(d) => oauth_error(StatusCode::BAD_REQUEST, "invalid_grant", d),
        OidcError::AccessDenied(d) => oauth_error(StatusCode::FORBIDDEN, "access_denied", d),
        OidcError::NotFound(d) => oauth_error(StatusCode::NOT_FOUND, "invalid_request", d),
        OidcError::ServerError(d) => {
            oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", d)
        }
        OidcError::Db(d) => oauth_error(StatusCode::INTERNAL_SERVER_ERROR, "server_error", d),
    }
}

/// 固定 issuer（public_origin）；未配置 → 无法服务。
fn issuer_of(state: &AppState) -> Result<String, Response> {
    let origin = state.config.public_origin.trim();
    if origin.is_empty() {
        return Err(oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "OIDC issuer is not configured",
        ));
    }
    Ok(origin.trim_end_matches('/').to_string())
}

fn parse_basic_auth(headers: &HeaderMap) -> Option<(String, String)> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let (scheme, rest) = value.split_once(' ')?;
    if !scheme.eq_ignore_ascii_case("basic") {
        return None;
    }
    let decoded = protocol::base64_decode_standard(rest.trim())?;
    let decoded = String::from_utf8(decoded).ok()?;
    let (user, pass) = decoded.split_once(':')?;
    Some((user.to_string(), pass.to_string()))
}

/// Client 认证（token / revoke 端点）。
///
/// - Confidential：`client_secret_basic` 或 `client_secret_post`；
/// - Public：仅 `client_id`（表单），不得使用 Basic（强制 PKCE 纵深防御）；
/// - 未知 client / secret 不匹配 → `invalid_client`。
async fn authenticate_client(
    state: &AppState,
    headers: &HeaderMap,
    form: &HashMap<String, String>,
) -> Result<crate::oidc::clients::OAuthClient, Response> {
    let pool = state.db.as_deref().ok_or_else(|| {
        oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "server error",
        )
    })?;
    let basic = parse_basic_auth(headers);
    let used_basic = basic.is_some();
    let form_client_id = form.get("client_id").cloned();
    let form_secret = form.get("client_secret").cloned();

    // 歧义：Basic 与表单 client_id 同时提供 → 拒绝。
    let (client_id, secret) = match (basic, form_client_id) {
        (Some(_), Some(_)) => {
            return Err(oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_client",
                "client authentication is ambiguous",
            ))
        }
        (Some((cid, secret)), None) => (cid, Some(secret)),
        (None, Some(cid)) => (cid, form_secret),
        (None, None) => {
            return Err(oauth_error(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "client authentication required",
            ))
        }
    };

    let client = crate::oidc::clients::fetch_client_by_client_id(pool, &client_id)
        .await
        .map_err(|e| oauth_error_response(&e))?
        .ok_or_else(|| oauth_error(StatusCode::UNAUTHORIZED, "invalid_client", "invalid client"))?;

    if client.is_confidential() {
        let provided = secret.unwrap_or_default();
        if !client.verify_secret(&provided) {
            return Err(oauth_error(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "invalid client",
            ));
        }
    } else {
        // Public client：不得携带 secret，不得使用 Basic。
        if used_basic {
            return Err(oauth_error(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "public client must authenticate with client_id only",
            ));
        }
        if secret.is_some() {
            return Err(oauth_error(
                StatusCode::UNAUTHORIZED,
                "invalid_client",
                "public client must not send a client_secret",
            ));
        }
    }
    Ok(client)
}

/// 解析表单 body（检测重复参数歧义）。
fn parse_form_body(body: &Bytes) -> Result<HashMap<String, String>, Response> {
    let raw = String::from_utf8_lossy(body);
    let pairs = protocol::parse_params(&raw);
    let mut map = HashMap::with_capacity(pairs.len());
    for (k, v) in pairs {
        if map.contains_key(&k) {
            return Err(oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("parameter '{k}' was supplied more than once"),
            ));
        }
        map.insert(k, v);
    }
    Ok(map)
}

fn require_param<'a>(form: &'a HashMap<String, String>, key: &str) -> Result<&'a str, Response> {
    form.get(key)
        .map(|s| s.as_str())
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("missing required parameter '{key}'"),
            )
        })
}

// ─────────────────────────── Discovery ───────────────────────────

/// GET /.well-known/openid-configuration
async fn well_known_config(State(state): State<AppState>) -> Response {
    let issuer = match issuer_of(&state) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let doc = protocol::discovery_document(&issuer);
    (
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "public, max-age=300"),
        ],
        serde_json::to_vec(&doc).unwrap_or_default(),
    )
        .into_response()
}

// ─────────────────────────── Authorize ───────────────────────────

/// GET /oauth/authorize
///
/// 完整校验授权请求（PKCE S256 / scope / nonce / redirect 精确匹配），
/// 未登录 → 303 登录页（保留原请求）；已登录 → 创建 interaction →
/// 303 consent 页。错误按 §14：redirect URI 有效时重定向回带 `error`，
/// URI 无效时本地显示错误。
async fn oauth_authorize(State(state): State<AppState>, uri: Uri, auth: AuthSession) -> Response {
    let pool = state.db.as_deref();
    let raw_query = uri.query().unwrap_or("");
    let params = match protocol::params_map(raw_query) {
        Ok(p) => p,
        Err(e) => return oauth_error_response(&e),
    };

    // 参数长度限制（防滥用）。
    for (k, v) in &params {
        if v.len() > 4096 {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                &format!("parameter '{k}' is too long"),
            );
        }
    }

    let response_type = params
        .get("response_type")
        .map(String::as_str)
        .unwrap_or("");
    let client_id = params.get("client_id").map(String::as_str).unwrap_or("");
    let redirect_uri = params.get("redirect_uri").map(String::as_str).unwrap_or("");
    let scope = params.get("scope").map(String::as_str).unwrap_or("");
    let state_param = params.get("state").map(String::as_str);
    let nonce = params.get("nonce").map(String::as_str);
    let code_challenge = params.get("code_challenge").map(String::as_str);
    let code_challenge_method = params
        .get("code_challenge_method")
        .map(String::as_str)
        .unwrap_or("plain");

    // Client 与 redirect 校验（redirect 无效时本地报错，不重定向）。
    let Some(pool) = pool else {
        return oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "server error",
        );
    };
    let client = match crate::oidc::clients::fetch_client_by_client_id(pool, client_id).await {
        Ok(Some(c)) => c,
        Ok(None) => {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "unauthorized_client",
                "unknown client_id",
            )
        }
        Err(e) => return oauth_error_response(&e),
    };
    let registered_redirect = client
        .redirect_uris()
        .into_iter()
        .find(|u| protocol::redirect_uri_matches(u, redirect_uri));
    let Some(registered_redirect) = registered_redirect else {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            "redirect_uri is not registered for this client",
        );
    };

    // 之后的错误可以安全重定向回（redirect URI 已验证）。
    let redirect_error = |error: &str, description: &str| -> Response {
        let mut query: Vec<(String, String)> = vec![
            ("error".to_string(), error.to_string()),
            ("error_description".to_string(), description.to_string()),
        ];
        if let Some(state) = state_param {
            query.push(("state".to_string(), state.to_string()));
        }
        redirect_to(&registered_redirect, &query)
    };

    if response_type != "code" {
        return redirect_error(
            "unsupported_response_type",
            "only the authorization code flow (response_type=code) is supported",
        );
    }
    if !client.is_active() {
        return redirect_error("unauthorized_client", "client is disabled");
    }
    // scope：必须含 openid，且 ⊆ {openid, profile, email}。
    let scopes = match protocol::parse_scopes(scope) {
        Ok(s) => s,
        Err(_) => return redirect_error("invalid_scope", "invalid or unsupported scope"),
    };
    for s in &scopes {
        if !client.scopes().contains(s) {
            return redirect_error("invalid_scope", "scope is not granted to this client");
        }
    }
    // nonce：OIDC v1 要求。
    let nonce = match nonce {
        Some(n) if !n.is_empty() && n.len() <= crate::oidc::NONCE_MAX_LEN => n,
        Some(_) => return redirect_error("invalid_request", "nonce is too long"),
        None => return redirect_error("invalid_request", "nonce is required for OpenID Connect"),
    };
    // PKCE：S256 必须。
    let challenge = match code_challenge {
        Some(c) if protocol::is_valid_code_challenge(c) => c,
        _ => {
            return redirect_error(
                "invalid_request",
                "code_challenge is required (S256, 43-128 base64url chars)",
            )
        }
    };
    if code_challenge_method != "S256" {
        return redirect_error(
            "invalid_request",
            "code_challenge_method must be S256 (plain is not supported)",
        );
    }
    if let Some(state) = state_param {
        if state.len() > crate::oidc::STATE_MAX_LEN {
            return redirect_error("invalid_request", "state is too long");
        }
    }

    // 未登录：303 到登录页并保留完整原请求。
    let Some(user) = &auth.user else {
        let issuer = match issuer_of(&state) {
            Ok(i) => i,
            Err(r) => return r,
        };
        let next = format!("/oauth/authorize?{raw_query}");
        let encoded_next: String = url::form_urlencoded::byte_serialize(next.as_bytes()).collect();
        return redirect_to(&format!("{issuer}/auth/login?next={encoded_next}"), &[]);
    };

    // 账号状态门：非 active 一律 access_denied（不泄漏细节）。
    let active = match crate::oidc::tokens::fetch_user(pool, &user.id).await {
        Ok(Some(u)) => u.is_active(),
        Ok(None) => false,
        Err(e) => return oauth_error_response(&e),
    };
    if !active {
        return redirect_error("access_denied", "access denied");
    }

    let req = AuthorizeRequest {
        response_type: response_type.to_string(),
        client_id: client.client_id.clone(),
        redirect_uri: registered_redirect.clone(),
        scope: scopes.join(" "),
        state: state_param.map(str::to_string),
        nonce: Some(nonce.to_string()),
        code_challenge: challenge.to_string(),
        code_challenge_method: code_challenge_method.to_string(),
    };
    let master_key = state.config.oidc_key_encryption_key.as_bytes();
    let interaction_id = match crate::oidc::interactions::create_interaction(
        pool,
        &client.id,
        &user.id,
        &req,
        master_key,
        crate::oidc::now_millis(),
    )
    .await
    {
        Ok(id) => id,
        Err(e) => return oauth_error_response(&e),
    };
    let issuer = match issuer_of(&state) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let consent_url = format!("{issuer}/auth/consent/{interaction_id}");
    redirect_to(&consent_url, &[])
}

/// 303 See Other 重定向。
fn redirect_to(location: &str, query: &[(String, String)]) -> Response {
    let mut url = location.to_string();
    if !query.is_empty() {
        let mut pairs: Vec<String> = query
            .iter()
            .map(|(k, v)| {
                format!(
                    "{k}={}",
                    url::form_urlencoded::byte_serialize(v.as_bytes()).collect::<String>()
                )
            })
            .collect();
        pairs.sort();
        let sep = if url.contains('?') { "&" } else { "?" };
        url.push_str(&format!("{sep}{}", pairs.join("&")));
    }
    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, url)
        .header(header::CACHE_CONTROL, "no-store")
        .body(axum::body::Body::empty())
        .unwrap_or_else(|_| {
            oauth_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "server_error",
                "server error",
            )
        })
}

// ─────────────────────────── Token ───────────────────────────

/// POST /oauth/token
async fn oauth_token(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let form = match parse_form_body(&body) {
        Ok(f) => f,
        Err(r) => return r,
    };
    let client = match authenticate_client(&state, &headers, &form).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    if !client.is_active() {
        return oauth_error(
            StatusCode::BAD_REQUEST,
            "invalid_grant",
            "client is disabled",
        );
    }
    let Some(pool) = state.db.as_deref() else {
        return oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "server error",
        );
    };
    let grant_type = form.get("grant_type").map(String::as_str).unwrap_or("");
    let now = crate::oidc::now_millis();
    let issuer = match issuer_of(&state) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let master_key = state.config.oidc_key_encryption_key.as_bytes();

    let result = match grant_type {
        "authorization_code" => {
            let code = match require_param(&form, "code") {
                Ok(c) => c,
                Err(r) => return r,
            };
            let redirect_uri = match require_param(&form, "redirect_uri") {
                Ok(r) => r,
                Err(r) => return r,
            };
            let verifier = match require_param(&form, "code_verifier") {
                Ok(v) => v,
                Err(r) => return r,
            };
            crate::oidc::tokens::exchange_authorization_code(
                pool,
                &client,
                code,
                redirect_uri,
                verifier,
                &issuer,
                master_key,
                now,
            )
            .await
        }
        "refresh_token" => {
            let refresh = match require_param(&form, "refresh_token") {
                Ok(r) => r,
                Err(r) => return r,
            };
            let scope = form.get("scope").map(String::as_str);
            crate::oidc::tokens::refresh_tokens(pool, &client, refresh, scope, now).await
        }
        other => Err(OidcError::InvalidRequest(format!(
            "unsupported grant_type '{other}'"
        ))),
    };

    match result {
        Ok(resp) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "private, no-store"),
            ],
            serde_json::to_vec(&resp.to_json()).unwrap_or_default(),
        )
            .into_response(),
        Err(e) => oauth_error_response(&e),
    }
}

// ─────────────────────────── UserInfo ───────────────────────────

/// GET /oauth/userinfo
async fn oauth_userinfo(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let bearer = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("bearer "))
        });
    let Some(token) = bearer else {
        return (
            StatusCode::UNAUTHORIZED,
            [
                (header::WWW_AUTHENTICATE, "Bearer"),
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            axum::Json(
                json!({ "error": "invalid_token", "error_description": "access token is missing" }),
            ),
        )
            .into_response();
    };
    let Some(pool) = state.db.as_deref() else {
        return oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "server error",
        );
    };
    let now = crate::oidc::now_millis();
    let validated = match crate::oidc::tokens::validate_access_token(pool, token, now).await {
        Ok(v) => v,
        Err(e) => return oauth_error_response(&e),
    };
    let Some((row, _family, user, client)) = validated else {
        return (
            StatusCode::UNAUTHORIZED,
            [
                (header::WWW_AUTHENTICATE, "Bearer"),
                (header::CONTENT_TYPE, "application/json"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            axum::Json(json!({ "error": "invalid_token", "error_description": "the access token is invalid or has expired" })),
        )
            .into_response();
    };
    let issuer = match issuer_of(&state) {
        Ok(i) => i,
        Err(r) => return r,
    };
    let sub = protocol::pairwise_subject(&issuer, &user.id, &client.client_id);
    let scopes = protocol::split_scopes(&row.scope);
    let claims = protocol::userinfo_claims(
        &sub,
        &scopes,
        Some(&user.username_normalized),
        user.display_name.as_deref(),
        Some(user.updated_at / 1000),
        Some(&user.email_normalized),
        user.email_verified != 0,
    );
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        serde_json::to_vec(&claims).unwrap_or_default(),
    )
        .into_response()
}

// ─────────────────────────── Revoke ───────────────────────────

/// POST /oauth/revoke
async fn oauth_revoke(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    let form = match parse_form_body(&body) {
        Ok(f) => f,
        Err(r) => return r,
    };
    let client = match authenticate_client(&state, &headers, &form).await {
        Ok(c) => c,
        Err(r) => return r,
    };
    let Some(pool) = state.db.as_deref() else {
        return oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "server error",
        );
    };
    let raw_token = match require_param(&form, "token") {
        Ok(t) => t,
        Err(r) => return r,
    };
    let hint = form.get("token_type_hint").map(String::as_str);
    if let Err(e) =
        crate::oidc::tokens::revoke_token(pool, &client, raw_token, hint, "oauth_revoke").await
    {
        return oauth_error_response(&e);
    }
    // 恒定 200，避免 token 枚举（RFC 7009）。
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "private, no-store")],
        "{}",
    )
        .into_response()
}

// ─────────────────────────── Logout ───────────────────────────

/// RP-Initiated Logout（GET/POST 共用逻辑）。
async fn logout_impl(
    state: &AppState,
    params: &HashMap<String, String>,
    auth: &AuthSession,
) -> Response {
    let Some(pool) = state.db.as_deref() else {
        return oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "server error",
        );
    };
    let id_token_hint = params.get("id_token_hint").map(String::as_str);
    let post_logout = params.get("post_logout_redirect_uri").map(String::as_str);
    let state_param = params.get("state").map(String::as_str);
    let client_id_param = params.get("client_id").map(String::as_str);
    let now = crate::oidc::now_millis();
    let master_key = state.config.oidc_key_encryption_key.as_bytes();

    // 从 id_token_hint 识别 client（签名 + 过期校验）。
    let client_id_from_hint: Option<String> = if let Some(hint) = id_token_hint {
        match crate::oidc::keys::verify_id_token_hint(pool, master_key, hint, now / 1000).await {
            Ok(payload) => payload
                .get("aud")
                .and_then(Value::as_str)
                .map(str::to_string),
            Err(e) => return oauth_error_response(&e),
        }
    } else {
        None
    };

    let client_id = client_id_from_hint.as_deref().or(client_id_param);

    if let Some(target) = post_logout {
        let Some(client_id) = client_id else {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "cannot identify the client for post_logout_redirect_uri",
            );
        };
        let client = match crate::oidc::clients::fetch_client_by_client_id(pool, client_id).await {
            Ok(Some(c)) => c,
            Ok(None) => {
                return oauth_error(StatusCode::BAD_REQUEST, "invalid_request", "unknown client")
            }
            Err(e) => return oauth_error_response(&e),
        };
        let matched = client
            .post_logout_uris()
            .into_iter()
            .any(|u| protocol::redirect_uri_matches(&u, target));
        if !matched {
            return oauth_error(
                StatusCode::BAD_REQUEST,
                "invalid_request",
                "post_logout_redirect_uri is not registered for this client",
            );
        }
    }

    // 撤销当前本地会话（若存在）。
    if let Some(session_id) = &auth.session_id {
        if let Some(user) = &auth.user {
            let _ = crate::auth::session::revoke_session_by_id(
                pool,
                &user.id,
                session_id,
                "oidc_logout",
            )
            .await;
        }
    }

    match post_logout {
        Some(target) => {
            let mut query: Vec<(String, String)> = Vec::new();
            if let Some(s) = state_param {
                query.push(("state".to_string(), s.to_string()));
            }
            redirect_to(target, &query)
        }
        None => (
            StatusCode::OK,
            [(header::CACHE_CONTROL, "private, no-store")],
            "{}",
        )
            .into_response(),
    }
}

async fn oauth_logout_get(State(state): State<AppState>, uri: Uri, auth: AuthSession) -> Response {
    let params = match protocol::params_map(uri.query().unwrap_or("")) {
        Ok(p) => p,
        Err(e) => return oauth_error_response(&e),
    };
    logout_impl(&state, &params, &auth).await
}

async fn oauth_logout_post(
    State(state): State<AppState>,
    auth: AuthSession,
    body: Bytes,
) -> Response {
    let params = match parse_form_body(&body) {
        Ok(p) => p,
        Err(r) => return r,
    };
    logout_impl(&state, &params, &auth).await
}

// ─────────────────────────── JWKS ───────────────────────────

/// GET /oauth/jwks.json
///
/// 精确缓存头：ETag + `Cache-Control: public, max-age=300`；命中
/// `If-None-Match` 返回 304。
async fn oauth_jwks(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(pool) = state.db.as_deref() else {
        return oauth_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "server_error",
            "server error",
        );
    };
    let doc = match crate::oidc::keys::jwks_document(pool).await {
        Ok(d) => d,
        Err(e) => return oauth_error_response(&e),
    };
    let payload = serde_json::to_vec(&doc).unwrap_or_default();
    let etag = format!(
        "\"{}\"",
        protocol::sha256_hex(&String::from_utf8_lossy(&payload))
    );
    if headers
        .get(header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v == etag)
    {
        return (
            StatusCode::NOT_MODIFIED,
            [
                (header::CACHE_CONTROL, "public, max-age=300"),
                (header::ETAG, etag.as_str()),
            ],
            (),
        )
            .into_response();
    }
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "public, max-age=300"),
            (header::ETAG, etag.as_str()),
        ],
        payload,
    )
        .into_response()
}

// ─────────────────────────── Interactions ───────────────────────────

/// GET /api/v1/oauth/interactions/{id} — consent 页查询（Session）。
async fn get_interaction(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
) -> Response {
    let request_id = "get_interaction";
    let user = match auth.require_auth(request_id) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    let Some(pool) = state.db.as_deref() else {
        return crate::error::AppError::internal("database not configured", request_id)
            .into_response();
    };
    let interaction =
        match crate::oidc::interactions::load_interaction_for_owner(pool, &id, &user.id).await {
            Ok(i) => i,
            Err(OidcError::NotFound(_)) => {
                return crate::error::AppError::not_found("interaction not found", request_id)
                    .into_response()
            }
            Err(e) => {
                return crate::error::AppError::internal(e.to_string(), request_id).into_response()
            }
        };
    let view = match crate::oidc::interactions::interaction_view(
        pool,
        &interaction,
        crate::oidc::now_millis(),
    )
    .await
    {
        Ok(v) => v,
        Err(e) => {
            return crate::error::AppError::internal(e.to_string(), request_id).into_response()
        }
    };
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "private, no-store")],
        axum::Json(view),
    )
        .into_response()
}

/// POST /api/v1/oauth/interactions/{id}/decision — Session + CSRF。
async fn decide_interaction(
    State(state): State<AppState>,
    auth: AuthSession,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let request_id = "decide_interaction";
    let user = match auth.require_auth(request_id) {
        Ok(u) => u,
        Err(e) => return e.into_response(),
    };
    let decision: Value = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => {
            return crate::error::AppError::bad_request(
                "request body must be JSON",
                request_id,
                None,
            )
            .into_response()
        }
    };
    let allow = match decision.get("decision").and_then(Value::as_str) {
        Some("allow") => true,
        Some("deny") => false,
        _ => {
            return crate::error::AppError::bad_request(
                "decision must be 'allow' or 'deny'",
                request_id,
                None,
            )
            .into_response()
        }
    };
    let Some(pool) = state.db.as_deref() else {
        return crate::error::AppError::internal("database not configured", request_id)
            .into_response();
    };
    let interaction =
        match crate::oidc::interactions::load_interaction_for_owner(pool, &id, &user.id).await {
            Ok(i) => i,
            Err(OidcError::NotFound(_)) => {
                return crate::error::AppError::not_found("interaction not found", request_id)
                    .into_response()
            }
            Err(e) => {
                return crate::error::AppError::internal(e.to_string(), request_id).into_response()
            }
        };
    let master_key = state.config.oidc_key_encryption_key.as_bytes();
    let outcome = match crate::oidc::interactions::decide_interaction(
        pool,
        &interaction,
        allow,
        master_key,
        request_id,
        crate::oidc::now_millis(),
    )
    .await
    {
        Ok(o) => o,
        Err(OidcError::InvalidRequest(d)) => {
            return crate::error::AppError::bad_request(d, request_id, None).into_response()
        }
        Err(e) => {
            return crate::error::AppError::internal(e.to_string(), request_id).into_response()
        }
    };
    let redirect_to_url = match outcome {
        crate::oidc::interactions::InteractionOutcome::Approved {
            code,
            state,
            redirect_uri,
            ..
        } => {
            let mut query = vec![("code".to_string(), code)];
            if let Some(s) = state {
                query.push(("state".to_string(), s));
            }
            crate::oidc::interactions::decision_redirect_url(&redirect_uri, &query)
        }
        crate::oidc::interactions::InteractionOutcome::Denied {
            state,
            redirect_uri,
        } => {
            let mut query = vec![("error".to_string(), "access_denied".to_string())];
            if let Some(s) = state {
                query.push(("state".to_string(), s));
            }
            crate::oidc::interactions::decision_redirect_url(&redirect_uri, &query)
        }
    };
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "private, no-store")],
        axum::Json(json!({
            "redirect_to": redirect_to_url,
        })),
    )
        .into_response()
}
