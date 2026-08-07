//! M11-OIDC Provider 集成测试（真实 HTTP 路径 + SQLite 真库 + 全量迁移）。
//!
//! 覆盖：
//! - M11-PROTOCOL-01..10：discovery/JWKS 缓存头、authorize（code + PKCE S256）、
//!   精确 redirect、state/nonce/code 一次性与绑定、scope/pairwise sub、
//!   RS256 ID Token、userinfo/revoke/logout、refresh rotation + family reuse、
//!   feature-off 故障行为；
//! - M11-CONSENT-01..07、11：逐 Client/Scope consent、interaction decision
//!   （Session + CSRF + request 绑定）、key rotation（publish-then-switch +
//!   retire margin + fail-closed）、admin Client CRUD（reason/recent-auth/
//!   审计/URI 校验）、scope 永不扣款、disabled client / banned user /
//!   consent revoke / key rotation / family reuse；
//! - M11-OIDC-SCHEMA-01..06：迁移覆盖与 hash-only / 加密存储断言。
//!
//! 全部走线上代码真实入口（`bblbb_backend::*`），不重实现被测逻辑。

mod common;

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{header, HeaderMap, Request, StatusCode},
    Router,
};
use bblbb_backend::config::flags::{FeatureFlags, FeatureName};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::oidc::clients::OAuthClient;
use bblbb_backend::oidc::protocol::{pairwise_subject, pkce_s256_challenge};
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, build_router_with_flags, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

// ─────────────────────────── 测试常量 ───────────────────────────

const ISSUER: &str = "https://bblbb.test";
const REDIRECT: &str = "https://client.example/cb";
const POST_LOGOUT: &str = "https://client.example/bye";
const MASTER_KEY: &str = "test-oidc-master-key-material";
/// RFC 7636 附录 B 测试向量 verifier（43 字符 base64url）。
const VERIFIER: &str = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";

// ─────────────────────────── 脚手架 ───────────────────────────

async fn sqlite_pool() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-oidc-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
    (pool, dir)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn test_config() -> AppConfig {
    AppConfig {
        public_origin: ISSUER.to_string(),
        oidc_key_encryption_key: MASTER_KEY.to_string(),
        ..AppConfig::default()
    }
}

fn oidc_flags() -> FeatureFlags {
    let mut flags = FeatureFlags::all_default();
    flags
        .set(
            FeatureName::Oidc,
            true,
            1,
            0,
            "test",
            "enable oidc for integration test",
            1_700_000_000_000,
        )
        .unwrap();
    flags
}

fn app_with_oidc(pool: DatabasePool) -> Router {
    build_router_with_flags(test_config(), Some(pool), oidc_flags())
}

fn app_without_oidc(pool: DatabasePool) -> Router {
    build_router(test_config(), Some(pool))
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let sql = "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
               VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)";
    match pool {
        Either::Left(p) => {
            sqlx::query(sql)
                .bind(&user_id)
                .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
                .bind(format!(
                    "{tag}_{}@example.com",
                    uuid::Uuid::now_v7().simple()
                ))
                .bind(now - 30 * 86_400 * 1000)
                .bind(now)
                .bind(now)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

async fn set_user_status(pool: &DatabasePool, user_id: &str, status: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET status = ? WHERE id = ?")
                .bind(status)
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn role_id(pool: &DatabasePool, name: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn assign_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id = role_id(pool, role_name).await;
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 经服务层创建 Confidential Client（全 scope），返回 (client, 明文 secret)。
async fn create_confidential_client(pool: &DatabasePool, tag: &str) -> (OAuthClient, String) {
    let input = bblbb_backend::oidc::clients::ClientCreateInput {
        name: format!("Test Client {tag}"),
        client_type: "confidential".into(),
        redirect_uris: vec![REDIRECT.to_string()],
        post_logout_uris: vec![POST_LOGOUT.to_string()],
        scopes: vec![
            "openid".to_string(),
            "profile".to_string(),
            "email".to_string(),
        ],
    };
    let (client, secret) =
        bblbb_backend::oidc::clients::create_client(pool, &input, "test-operator", now_millis())
            .await
            .unwrap();
    (
        client,
        secret.expect("confidential client must return a secret"),
    )
}

/// 经服务层创建 Public Client（openid scope）。
async fn create_public_client(pool: &DatabasePool, tag: &str) -> OAuthClient {
    let input = bblbb_backend::oidc::clients::ClientCreateInput {
        name: format!("Public Client {tag}"),
        client_type: "public".into(),
        redirect_uris: vec![REDIRECT.to_string()],
        post_logout_uris: vec![POST_LOGOUT.to_string()],
        scopes: vec!["openid".to_string()],
    };
    let (client, _) =
        bblbb_backend::oidc::clients::create_client(pool, &input, "test-operator", now_millis())
            .await
            .unwrap();
    client
}

/// 发送请求，返回 (status, headers, JSON body)。空 body → Value::Null。
async fn do_request(app: Router, req: Request<Body>) -> (StatusCode, HeaderMap, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.into_body().collect().await.unwrap().to_bytes();
    let json: Value = if body.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&body).unwrap_or(Value::Null)
    };
    (status, headers, json)
}

fn form_body(pairs: &[(&str, &str)]) -> String {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        s.append_pair(k, v);
    }
    s.finish()
}

fn basic_auth(client_id: &str, secret: impl AsRef<str>) -> String {
    use base64::Engine;
    let raw = format!("{client_id}:{}", secret.as_ref());
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw)
    )
}

/// 基础 authorize 参数（response_type=code + PKCE S256）。
fn base_pairs<'a>(
    client_id: &'a str,
    scope: &'a str,
    state: &'a str,
    nonce: &'a str,
    challenge: &'a str,
) -> Vec<(&'a str, &'a str)> {
    vec![
        ("response_type", "code"),
        ("client_id", client_id),
        ("redirect_uri", REDIRECT),
        ("scope", scope),
        ("state", state),
        ("nonce", nonce),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
    ]
}

async fn start_authorize(
    app: Router,
    cookie: Option<&str>,
    pairs: &[(&str, &str)],
) -> (StatusCode, HeaderMap, Value) {
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        s.append_pair(k, v);
    }
    let uri = format!("/oauth/authorize?{}", s.finish());
    let mut builder = Request::builder().method("GET").uri(&uri);
    if let Some(c) = cookie {
        builder = builder.header("cookie", c);
    }
    do_request(app, builder.body(Body::empty()).unwrap()).await
}

async fn get_csrf(app: Router, cookie: &str) -> String {
    let (status, _, body) = do_request(
        app,
        Request::builder()
            .method("GET")
            .uri("/api/v1/auth/csrf")
            .header("cookie", cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "csrf endpoint must be 200");
    body["token"].as_str().unwrap().to_string()
}

async fn decide(
    app: Router,
    cookie: &str,
    csrf: &str,
    interaction_id: &str,
    decision: &str,
) -> (StatusCode, HeaderMap, Value) {
    do_request(
        app,
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/oauth/interactions/{interaction_id}/decision"
            ))
            .header("cookie", cookie)
            .header("x-csrf-token", csrf)
            .header("content-type", "application/json")
            .body(Body::from(json!({ "decision": decision }).to_string()))
            .unwrap(),
    )
    .await
}

/// 从 303 Location（/auth/consent/{id}）提取 interaction id。
fn extract_interaction_id(location: &str) -> String {
    let url = url::Url::parse(location).expect("Location must be a URL");
    let id = url.path().rsplit('/').next().unwrap().to_string();
    assert!(
        !id.is_empty(),
        "consent URL must carry interaction id: {location}"
    );
    id
}

fn query_param(url_str: &str, key: &str) -> Option<String> {
    let url = url::Url::parse(url_str).ok()?;
    url.query_pairs()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

/// 完整授权码流程（Confidential 客户端）：
/// authorize → consent(allow) → code → token，返回全部 TokenSet。
struct TokenSet {
    code: String,
    state: String,
    access_token: String,
    refresh_token: String,
    id_token: String,
    session_cookie: String,
}

#[allow(clippy::too_many_arguments)]
async fn complete_confidential_flow(
    app: &Router,
    pool: &DatabasePool,
    user_id: &str,
    client: &OAuthClient,
    secret: &str,
    scope: &str,
    verifier: &str,
    nonce: &str,
    state: &str,
) -> TokenSet {
    let session_cookie = common::direct_session_cookie(pool, user_id).await;
    let challenge = pkce_s256_challenge(verifier);
    let pairs = base_pairs(&client.client_id, scope, state, nonce, &challenge);
    let (status, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &pairs).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let interaction_id = extract_interaction_id(&location);

    let csrf = get_csrf(app.clone(), &session_cookie).await;
    let (dstatus, _, dbody) = decide(
        app.clone(),
        &session_cookie,
        &csrf,
        &interaction_id,
        "allow",
    )
    .await;
    assert_eq!(dstatus, StatusCode::OK, "decision body: {dbody}");
    let redirect_to = dbody["redirect_to"].as_str().unwrap().to_string();
    let code = query_param(&redirect_to, "code").expect("redirect_to must carry code");
    let returned_state = query_param(&redirect_to, "state");

    let form = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT),
        ("code_verifier", verifier),
    ]);
    let (tstatus, _, tbody) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(header::AUTHORIZATION, basic_auth(&client.client_id, secret))
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(tstatus, StatusCode::OK, "token body: {tbody}");
    TokenSet {
        code,
        state: returned_state.unwrap_or_default(),
        access_token: tbody["access_token"].as_str().unwrap().to_string(),
        refresh_token: tbody["refresh_token"].as_str().unwrap().to_string(),
        id_token: tbody["id_token"].as_str().unwrap().to_string(),
        session_cookie,
    }
}

// ─────────────────────────── M11-PROTOCOL-01：Discovery + JWKS ───────────────────────────

#[tokio::test]
async fn discovery_and_jwks_have_contract_shape_and_cache_headers() {
    let (pool, dir) = sqlite_pool().await;
    // 预置签名密钥使 JWKS 非空（生产首签时惰性供给）。
    bblbb_backend::oidc::keys::active_signing_key(&pool, MASTER_KEY.as_bytes())
        .await
        .unwrap();
    let app = app_with_oidc(pool.clone());

    let (status, headers, body) = do_request(
        app.clone(),
        Request::builder()
            .uri("/.well-known/openid-configuration")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=300"
    );
    assert_eq!(
        headers.get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );
    assert_eq!(body["issuer"], ISSUER);
    assert_eq!(
        body["authorization_endpoint"],
        format!("{ISSUER}/oauth/authorize")
    );
    assert_eq!(body["token_endpoint"], format!("{ISSUER}/oauth/token"));
    assert_eq!(
        body["userinfo_endpoint"],
        format!("{ISSUER}/oauth/userinfo")
    );
    assert_eq!(body["jwks_uri"], format!("{ISSUER}/oauth/jwks.json"));
    assert_eq!(
        body["revocation_endpoint"],
        format!("{ISSUER}/oauth/revoke")
    );
    assert_eq!(
        body["end_session_endpoint"],
        format!("{ISSUER}/oauth/logout")
    );
    assert_eq!(body["response_types_supported"], json!(["code"]));
    assert_eq!(
        body["grant_types_supported"],
        json!(["authorization_code", "refresh_token"])
    );
    assert_eq!(body["subject_types_supported"], json!(["pairwise"]));
    assert_eq!(
        body["id_token_signing_alg_values_supported"],
        json!(["RS256"])
    );
    assert_eq!(
        body["scopes_supported"],
        json!(["openid", "profile", "email"])
    );
    assert_eq!(body["code_challenge_methods_supported"], json!(["S256"]));

    // JWKS：精确缓存头 + ETag + If-None-Match → 304。
    let (status, headers, body) = do_request(
        app.clone(),
        Request::builder()
            .uri("/oauth/jwks.json")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers.get(header::CACHE_CONTROL).unwrap(),
        "public, max-age=300"
    );
    let etag = headers
        .get(header::ETAG)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(etag.starts_with('"') && etag.len() > 2);
    let keys = body["keys"].as_array().expect("jwks.keys must be an array");
    assert!(!keys.is_empty());
    for key in keys {
        assert_eq!(key["kty"], "RSA");
        assert_eq!(key["alg"], "RS256");
        assert_eq!(key["use"], "sig");
        assert!(!key["kid"].as_str().unwrap().is_empty());
        assert!(!key["n"].as_str().unwrap().is_empty());
        assert!(!key["e"].as_str().unwrap().is_empty());
    }

    let (status, headers, _) = do_request(
        app,
        Request::builder()
            .uri("/oauth/jwks.json")
            .header(header::IF_NONE_MATCH, &etag)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_MODIFIED, "ETag 命中必须 304");
    assert_eq!(
        headers
            .get(header::ETAG)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string(),
        etag,
        "304 必须携带相同 ETag"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-PROTOCOL-02/03/04：authorize ───────────────────────────

#[tokio::test]
async fn anonymous_authorize_redirects_to_login_preserving_request() {
    let (pool, dir) = sqlite_pool().await;
    let (client, _) = create_confidential_client(&pool, "anon").await;
    let app = app_with_oidc(pool.clone());
    let challenge = pkce_s256_challenge(VERIFIER);

    let pairs = base_pairs(
        &client.client_id,
        "openid profile email",
        "st-123",
        "n-456",
        &challenge,
    );
    let (status, headers, _) = start_authorize(app.clone(), None, &pairs).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.starts_with(&format!("{ISSUER}/auth/login?next=")),
        "未登录必须 303 到登录页: {location}"
    );
    assert!(
        location.contains("oauth%2Fauthorize%3F"),
        "必须保留原请求: {location}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn authorize_creates_interaction_visible_only_to_owner() {
    let (pool, dir) = sqlite_pool().await;
    let (client, _) = create_confidential_client(&pool, "inter").await;
    let user = insert_user(&pool, "alice").await;
    let other = insert_user(&pool, "mallory").await;
    let app = app_with_oidc(pool.clone());
    let session_cookie = common::direct_session_cookie(&pool, &user).await;
    let challenge = pkce_s256_challenge(VERIFIER);

    let pairs = base_pairs(
        &client.client_id,
        "openid profile",
        "state-abc",
        "nonce-xyz",
        &challenge,
    );
    let (status, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &pairs).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.contains("/auth/consent/"),
        "必须 303 到 consent 页"
    );
    let interaction_id = extract_interaction_id(&location);

    // owner 可见：已验证 Client/scope 摘要 + request hash。
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/oauth/interactions/{interaction_id}"))
            .header("cookie", &session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "interaction body: {body}");
    assert_eq!(body["client"]["client_id"], client.client_id);
    assert_eq!(body["client"]["name"], client.name);
    assert_eq!(body["redirect_domain"], "client.example");
    assert_eq!(body["scope"], json!(["openid", "profile"]));
    assert_eq!(body["status"], "pending");
    assert_eq!(body["previously_consented"], false);
    let request_hash = body["request_hash"].as_str().unwrap();
    assert_eq!(request_hash.len(), 64, "request_hash 必须是 SHA-256 hex");

    // 其他用户不可见 → 404。
    let other_cookie = common::direct_session_cookie(&pool, &other).await;
    let (status, _, _) = do_request(
        app,
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/oauth/interactions/{interaction_id}"))
            .header("cookie", &other_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND, "他人不得查看 interaction");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn consent_decision_requires_csrf_and_binds_request() {
    let (pool, dir) = sqlite_pool().await;
    let (client, _) = create_confidential_client(&pool, "csrf").await;
    let user = insert_user(&pool, "bob").await;
    let app = app_with_oidc(pool.clone());
    let session_cookie = common::direct_session_cookie(&pool, &user).await;
    let challenge = pkce_s256_challenge(VERIFIER);

    let pairs = base_pairs(&client.client_id, "openid", "st1", "n1", &challenge);
    let (_, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &pairs).await;
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let interaction_id = extract_interaction_id(&location);

    // 无 CSRF → 403 csrf_failed。
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/oauth/interactions/{interaction_id}/decision"
            ))
            .header("cookie", &session_cookie)
            .header("content-type", "application/json")
            .body(Body::from(r#"{"decision":"allow"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "无 CSRF 必须 403: {body}");
    assert_eq!(body["code"], "csrf_failed");

    // 错误 CSRF → 403。
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri(format!(
                "/api/v1/oauth/interactions/{interaction_id}/decision"
            ))
            .header("cookie", &session_cookie)
            .header("x-csrf-token", "wrong-token")
            .header("content-type", "application/json")
            .body(Body::from(r#"{"decision":"allow"}"#))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "错误 CSRF 必须 403: {body}");

    // 正确 CSRF + allow → 200 且 redirect_to 携带 code 与 state。
    let csrf = get_csrf(app.clone(), &session_cookie).await;
    let (status, _, body) = decide(
        app.clone(),
        &session_cookie,
        &csrf,
        &interaction_id,
        "allow",
    )
    .await;
    assert_eq!(status, StatusCode::OK, "decision body: {body}");
    let redirect_to = body["redirect_to"].as_str().unwrap().to_string();
    assert!(
        redirect_to.starts_with(&format!("{REDIRECT}?")),
        "{redirect_to}"
    );
    let code = query_param(&redirect_to, "code").expect("must carry code");
    assert_eq!(query_param(&redirect_to, "state").as_deref(), Some("st1"));
    assert_eq!(code.len(), 43, "授权码必须是 32 字节 base64url（43 字符）");

    // 重复 decision → 400（interaction 已消费）。
    let (status, _, body) = decide(
        app.clone(),
        &session_cookie,
        &csrf,
        &interaction_id,
        "allow",
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "重复 decision 必须拒绝: {body}"
    );

    // deny 流程：独立 interaction → access_denied 重定向。
    let pairs = base_pairs(&client.client_id, "openid", "st2", "n2", &challenge);
    let (_, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &pairs).await;
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let interaction2 = extract_interaction_id(&location);
    let (status, _, body) =
        decide(app.clone(), &session_cookie, &csrf, &interaction2, "deny").await;
    assert_eq!(status, StatusCode::OK, "deny body: {body}");
    let redirect_to = body["redirect_to"].as_str().unwrap().to_string();
    assert_eq!(
        query_param(&redirect_to, "error").as_deref(),
        Some("access_denied")
    );
    assert_eq!(query_param(&redirect_to, "state").as_deref(), Some("st2"));

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-PROTOCOL-06：Token 端点 + ID Token ───────────────────────────

#[tokio::test]
async fn token_exchange_issues_opaque_tokens_and_valid_rs256_id_token() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "token").await;
    let user = insert_user(&pool, "carol").await;
    let app = app_with_oidc(pool.clone());
    let nonce = "nonce-42";
    let state = "state-7";

    let ts = complete_confidential_flow(
        &app,
        &pool,
        &user,
        &client,
        &secret,
        "openid profile email",
        VERIFIER,
        nonce,
        state,
    )
    .await;
    assert_eq!(ts.state, state);

    // opaque Access Token：非 JWT、高熵。
    assert!(ts.access_token.len() >= 40);
    assert_eq!(
        ts.access_token.split('.').count(),
        1,
        "access token 必须是 opaque 非 JWT"
    );
    assert!(!ts.refresh_token.is_empty());
    assert!(ts.id_token.split('.').count() == 3, "id_token 必须是 JWT");

    // 用线上签名验证路径校验 ID Token（RS256 + kid + exp + 签名）。
    let now_secs = now_millis() / 1000;
    let payload = bblbb_backend::oidc::keys::verify_id_token_hint(
        &pool,
        MASTER_KEY.as_bytes(),
        &ts.id_token,
        now_secs,
    )
    .await
    .expect("id_token 必须通过线上签名验证");
    assert_eq!(payload["iss"], ISSUER);
    assert_eq!(payload["aud"], client.client_id);
    assert_eq!(payload["nonce"], nonce);
    let sub = payload["sub"].as_str().unwrap();
    assert_eq!(sub.len(), 43, "pairwise subject 43 字符");
    assert_eq!(sub, pairwise_subject(ISSUER, &user, &client.client_id));
    assert!(payload["jti"].as_str().unwrap().len() >= 16);
    assert!(payload["auth_time"].is_i64());
    let iat = payload["iat"].as_i64().unwrap();
    let exp = payload["exp"].as_i64().unwrap();
    assert_eq!(exp - iat, 300, "ID Token 有效期 300s");
    assert!(payload.get("azp").is_none(), "单 audience 不要求 azp");
    // profile/email scope → 身份 claim 投影。
    assert_eq!(payload["preferred_username"], payload["preferred_username"]);
    assert!(payload.get("email").is_some());

    // 数据库 hash-only：授权码/access/refresh 只存 SHA-256 hex。
    let code_hash: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT code_hash FROM oauth_authorization_codes")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(code_hash, bblbb_backend::auth::token::hash_token(&ts.code));
    assert_ne!(code_hash, ts.code);
    let (access_hash, refresh_hash): (String, String) = match &pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT access_token_hash, refresh_token_hash FROM oauth_tokens ORDER BY issued_at DESC LIMIT 1",
            )
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(
        access_hash,
        bblbb_backend::auth::token::hash_token(&ts.access_token)
    );
    assert_eq!(
        refresh_hash,
        bblbb_backend::auth::token::hash_token(&ts.refresh_token)
    );
    assert_ne!(access_hash, ts.access_token);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-PROTOCOL-05/07：userinfo ───────────────────────────

#[tokio::test]
async fn userinfo_projects_pairwise_sub_and_scope_filtered_claims() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "ui").await;
    let public_client = create_public_client(&pool, "ui-pub").await;
    let user = insert_user(&pool, "dave").await;
    let app = app_with_oidc(pool.clone());

    // 全 scope 客户端。
    let ts = complete_confidential_flow(
        &app,
        &pool,
        &user,
        &client,
        &secret,
        "openid profile email",
        VERIFIER,
        "n-ui",
        "s-ui",
    )
    .await;
    // 设定显示名使 profile scope 的 name claim 可投影。
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET display_name = 'Dave' WHERE id = ?")
                .bind(&user)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .header(header::AUTHORIZATION, format!("Bearer {}", ts.access_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "userinfo body: {body}");
    let sub = body["sub"].as_str().unwrap();
    assert_eq!(sub.len(), 43);
    assert_eq!(sub, pairwise_subject(ISSUER, &user, &client.client_id));
    assert!(body.get("preferred_username").is_some());
    assert!(body.get("name").is_some());
    assert!(body.get("email").is_some());
    assert_eq!(body["email_verified"], true);
    assert!(body.get("updated_at").is_some());

    // openid-only 公共客户端 → 只有 sub。
    let session_cookie = common::direct_session_cookie(&pool, &user).await;
    let challenge = pkce_s256_challenge(VERIFIER);
    let pairs = base_pairs(
        &public_client.client_id,
        "openid",
        "s-pub",
        "n-pub",
        &challenge,
    );
    let (_, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &pairs).await;
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let interaction_id = extract_interaction_id(&location);
    let csrf = get_csrf(app.clone(), &session_cookie).await;
    let (_, _, dbody) = decide(
        app.clone(),
        &session_cookie,
        &csrf,
        &interaction_id,
        "allow",
    )
    .await;
    let redirect_to = dbody["redirect_to"].as_str().unwrap().to_string();
    let code = query_param(&redirect_to, "code").unwrap();
    let form = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT),
        ("code_verifier", VERIFIER),
        ("client_id", &public_client.client_id),
    ]);
    let (tstatus, _, tbody) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(tstatus, StatusCode::OK, "public client token: {tbody}");
    let pub_access = tbody["access_token"].as_str().unwrap().to_string();
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .header(header::AUTHORIZATION, format!("Bearer {pub_access}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        body,
        json!({ "sub": pairwise_subject(ISSUER, &user, &public_client.client_id) }),
        "openid-only 只能返回 sub"
    );

    // 无 token → 401 invalid_token。
    let (status, _, body) = do_request(
        app,
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "invalid_token");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-PROTOCOL-08：Refresh rotation + family reuse ───────────────────────────

#[tokio::test]
async fn refresh_rotation_rotates_and_reuse_revokes_entire_family() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "refresh").await;
    let user = insert_user(&pool, "erin").await;
    let app = app_with_oidc(pool.clone());
    let ts = complete_confidential_flow(
        &app,
        &pool,
        &user,
        &client,
        &secret,
        "openid profile",
        VERIFIER,
        "n-rf",
        "s-rf",
    )
    .await;

    // 正常 refresh：签发新 token 对。
    let form = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &ts.refresh_token),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "refresh body: {body}");
    let refreshed = body["refresh_token"].as_str().unwrap().to_string();
    assert_ne!(refreshed, ts.refresh_token, "refresh 必须轮换");
    let new_access = body["access_token"].as_str().unwrap().to_string();
    assert_ne!(new_access, ts.access_token, "access 必须轮换");
    assert_eq!(body["token_type"], "Bearer");

    // 旧 refresh 重用 → family 全部撤销。
    let form = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &ts.refresh_token),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "重用必须拒绝: {body}");
    assert_eq!(body["error"], "invalid_grant");

    // 最新 refresh 也已失效（family 整体撤销）。
    let form = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &refreshed),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "family 撤销后最新 token 也失效: {body}"
    );
    assert_eq!(body["error"], "invalid_grant");

    // DB：family revoked + reason。
    let (revoked, reason): (Option<i64>, Option<String>) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT revoked_at, revoke_reason FROM oauth_token_families WHERE client_id = ?",
        )
        .bind(&client.id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(revoked.is_some(), "family 必须被撤销");
    assert_eq!(reason.as_deref(), Some("refresh_token_reuse"));

    // 安全通知（oauth_refresh_reuse → security 类别）。
    let notify: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND category = 'security' AND resource_type = 'oauth_token_family'",
            )
            .bind(&user)
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(notify >= 1, "重用必须通知用户");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-PROTOCOL-07：revoke ───────────────────────────

#[tokio::test]
async fn revoke_access_token_blocks_userinfo() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "revoke").await;
    let user = insert_user(&pool, "frank").await;
    let app = app_with_oidc(pool.clone());
    let ts = complete_confidential_flow(
        &app, &pool, &user, &client, &secret, "openid", VERIFIER, "n-rev", "s-rev",
    )
    .await;

    // 撤销前 userinfo 200。
    let (status, _, _) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .header(header::AUTHORIZATION, format!("Bearer {}", ts.access_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // revoke（恒定 200，不泄漏 token 存在性）。
    let form = form_body(&[
        ("token", &ts.access_token),
        ("token_type_hint", "access_token"),
    ]);
    let (status, _, _) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/revoke")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "revoke 必须恒定 200");

    // 撤销后 userinfo → 401 invalid_token。
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .header(header::AUTHORIZATION, format!("Bearer {}", ts.access_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "invalid_token");

    // 撤销一个完全不存在的 token → 仍 200（防枚举）。
    let form = form_body(&[("token", "does-not-exist-anywhere")]);
    let (status, _, _) = do_request(
        app,
        Request::builder()
            .method("POST")
            .uri("/oauth/revoke")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-PROTOCOL-07：RP-Initiated Logout ───────────────────────────

#[tokio::test]
async fn rp_initiated_logout_honors_registered_redirect_and_revokes_session() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "logout").await;
    let user = insert_user(&pool, "grace").await;
    let app = app_with_oidc(pool.clone());
    let ts = complete_confidential_flow(
        &app, &pool, &user, &client, &secret, "openid", VERIFIER, "n-lo", "s-lo",
    )
    .await;

    // 登录会话有效。
    let (status, _, _) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/api/v1/auth/csrf")
            .header("cookie", &ts.session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    // RP-Initiated Logout：id_token_hint + 已注册 post_logout_redirect_uri + state。
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    s.append_pair("id_token_hint", &ts.id_token);
    s.append_pair("post_logout_redirect_uri", POST_LOGOUT);
    s.append_pair("state", "logout-state");
    let uri = format!("/oauth/logout?{}", s.finish());
    let (status, headers, _) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(&uri)
            .header("cookie", &ts.session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::SEE_OTHER, "logout 必须重定向");
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert!(
        location.starts_with(&format!("{POST_LOGOUT}?")),
        "{location}"
    );
    assert_eq!(
        query_param(&location, "state").as_deref(),
        Some("logout-state")
    );

    // 本地会话已被撤销：DB 中会话行 revoked_at 非空。
    let session_token = ts
        .session_cookie
        .split('=')
        .nth(1)
        .expect("session cookie format");
    let revoked: Option<i64> = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT revoked_at FROM user_sessions WHERE token_hash = ?")
                .bind(bblbb_backend::auth::token::hash_token(session_token))
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        revoked.is_some(),
        "logout 后本地会话必须已撤销（revoked_at 非空）"
    );

    // 未注册的 post_logout_redirect_uri → 400 invalid_request。
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    s.append_pair("id_token_hint", &ts.id_token);
    s.append_pair("post_logout_redirect_uri", "https://evil.example/steal");
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!("/oauth/logout?{}", s.finish()))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "未注册 post-logout URI 必须拒绝"
    );
    assert_eq!(body["error"], "invalid_request");

    // 篡改的 id_token_hint（签名损坏）→ 400 invalid_request。
    let mut s = url::form_urlencoded::Serializer::new(String::new());
    let tampered = format!("{}X", &ts.id_token[..ts.id_token.len() - 4]);
    s.append_pair("id_token_hint", &tampered);
    s.append_pair("post_logout_redirect_uri", POST_LOGOUT);
    let (status, _, body) = do_request(
        app,
        Request::builder()
            .method("GET")
            .uri(format!("/oauth/logout?{}", s.finish()))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "篡改 hint 必须拒绝");
    assert_eq!(body["error"], "invalid_request");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-PROTOCOL-09：协议负例 ───────────────────────────

#[tokio::test]
async fn protocol_negative_cases_return_standard_oauth_errors() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "neg").await;
    let user = insert_user(&pool, "heidi").await;
    let app = app_with_oidc(pool.clone());
    let ts = complete_confidential_flow(
        &app, &pool, &user, &client, &secret, "openid", VERIFIER, "n-neg", "s-neg",
    )
    .await;

    // 错误 PKCE verifier → invalid_grant。
    let form = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &ts.code),
        ("redirect_uri", REDIRECT),
        ("code_verifier", "x".repeat(43).as_str()),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_grant");

    // 授权码重放（已消费）→ invalid_grant。
    let form = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &ts.code),
        ("redirect_uri", REDIRECT),
        ("code_verifier", VERIFIER),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_grant");

    // 错误的 redirect_uri（与授权请求不一致）→ invalid_grant。
    let session_cookie = common::direct_session_cookie(&pool, &user).await;
    let challenge = pkce_s256_challenge(VERIFIER);
    let pairs = base_pairs(&client.client_id, "openid", "s-rd", "n-rd", &challenge);
    let (_, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &pairs).await;
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let interaction_id = extract_interaction_id(&location);
    let csrf = get_csrf(app.clone(), &session_cookie).await;
    let (_, _, dbody) = decide(
        app.clone(),
        &session_cookie,
        &csrf,
        &interaction_id,
        "allow",
    )
    .await;
    let code = query_param(dbody["redirect_to"].as_str().unwrap(), "code").unwrap();
    let form = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", "https://client.example/cb/"),
        ("code_verifier", VERIFIER),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "redirect 不匹配必须拒绝");
    assert_eq!(body["error"], "invalid_grant");

    // 未知 client → 401 invalid_client。
    let form = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &ts.refresh_token),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth("unknown-client-id", "whatever"),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "invalid_client");

    // 错误 secret → 401 invalid_client。
    let form = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &ts.refresh_token),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, "wrong-secret"),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error"], "invalid_client");

    // 不支持的 grant_type → 400 invalid_request。
    let form = form_body(&[("grant_type", "password"), ("username", "a")]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "invalid_request");

    // authorize：implicit（response_type=token）→ 重定向回带 unsupported_response_type。
    let implicit_pairs = vec![
        ("response_type", "token"),
        ("client_id", client.client_id.as_str()),
        ("redirect_uri", REDIRECT),
        ("scope", "openid"),
        ("state", "s-im"),
        ("nonce", "n-im"),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ];
    let (status, headers, _) =
        start_authorize(app.clone(), Some(&session_cookie), &implicit_pairs).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        query_param(&location, "error").as_deref(),
        Some("unsupported_response_type")
    );
    assert_eq!(query_param(&location, "state").as_deref(), Some("s-im"));

    // authorize：缺失 PKCE → invalid_request（重定向回）。
    let no_pkce = base_pairs(&client.client_id, "openid", "s-pk", "n-pk", &challenge);
    let no_pkce: Vec<(&str, &str)> = no_pkce
        .iter()
        .filter(|(k, _)| *k != "code_challenge")
        .map(|(k, v)| (*k, *v))
        .collect();
    let (status, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &no_pkce).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        query_param(&location, "error").as_deref(),
        Some("invalid_request")
    );

    // authorize：plain PKCE → 拒绝。
    let plain_pairs = base_pairs(&client.client_id, "openid", "s-pl", "n-pl", &challenge);
    let plain_pairs: Vec<(&str, &str)> = plain_pairs
        .iter()
        .map(|(k, v)| {
            if *k == "code_challenge_method" {
                ("code_challenge_method", "plain")
            } else {
                (*k, *v)
            }
        })
        .collect();
    let (status, headers, _) =
        start_authorize(app.clone(), Some(&session_cookie), &plain_pairs).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        query_param(&location, "error").as_deref(),
        Some("invalid_request")
    );

    // authorize：未注册 redirect URI → 本地 400，不重定向。
    let bad_redirect = vec![
        ("response_type", "code"),
        ("client_id", client.client_id.as_str()),
        ("redirect_uri", "https://evil.example/steal"),
        ("scope", "openid"),
        ("state", "s-ur"),
        ("nonce", "n-ur"),
        ("code_challenge", challenge.as_str()),
        ("code_challenge_method", "S256"),
    ];
    let (status, headers, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(&{
                let mut s = url::form_urlencoded::Serializer::new(String::new());
                for (k, v) in &bad_redirect {
                    s.append_pair(k, v);
                }
                format!("/oauth/authorize?{}", s.finish())
            })
            .header("cookie", &session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "未注册 redirect 必须本地报错"
    );
    assert!(headers.get(header::LOCATION).is_none());
    assert_eq!(body["error"], "invalid_request");

    // authorize：缺失 nonce → invalid_request。
    let no_nonce = base_pairs(&client.client_id, "openid", "s-no", "n-no", &challenge);
    let no_nonce: Vec<(&str, &str)> = no_nonce
        .iter()
        .filter(|(k, _)| *k != "nonce")
        .map(|(k, v)| (*k, *v))
        .collect();
    let (status, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &no_nonce).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        query_param(&location, "error").as_deref(),
        Some("invalid_request")
    );

    // authorize：重复参数（歧义）→ invalid_request。
    let dup_pairs = base_pairs(&client.client_id, "openid", "s-dup", "n-dup", &challenge);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(&{
                let mut s = url::form_urlencoded::Serializer::new(String::new());
                for (k, v) in &dup_pairs {
                    s.append_pair(k, v);
                }
                s.append_pair("state", "second");
                format!("/oauth/authorize?{}", s.finish())
            })
            .header("cookie", &session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "重复参数必须拒绝");
    assert_eq!(body["error"], "invalid_request");

    // authorize：未知 scope（非 openid 开头/白名单外）→ 重定向 invalid_scope。
    let bad_scope = base_pairs(
        &client.client_id,
        "openid money",
        "s-sc",
        "n-sc",
        &challenge,
    );
    let (status, headers, _) = start_authorize(app, Some(&session_cookie), &bad_scope).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        query_param(&location, "error").as_deref(),
        Some("invalid_scope")
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-PROTOCOL-10：默认关闭故障行为 ───────────────────────────

#[tokio::test]
async fn feature_off_returns_409_and_core_forum_stays_available() {
    let (pool, dir) = sqlite_pool().await;
    let app = app_without_oidc(pool.clone());

    for path in [
        "/.well-known/openid-configuration",
        "/oauth/authorize",
        "/oauth/token",
        "/oauth/userinfo",
        "/oauth/jwks.json",
        "/oauth/logout",
        "/oauth/revoke",
        "/api/v1/oauth/interactions/x",
    ] {
        let (status, _, body) = do_request(
            app.clone(),
            Request::builder()
                .uri(path)
                .method("POST")
                .body(Body::from("{}"))
                .unwrap(),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "path {path}");
        assert_eq!(body["code"], "feature_disabled", "path {path}: {body}");
    }
    // GET 路径同样 409。
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .uri("/.well-known/openid-configuration")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["code"], "feature_disabled");

    // 核心论坛与本地认证不受影响。
    for path in [
        "/healthz",
        "/api/v1/auth/csrf",
        "/api/v1/posts",
        "/api/v1/boards",
        "/api/v1/me",
        "/api/v1/openapi.json",
    ] {
        let (status, _, _) = do_request(
            app.clone(),
            Request::builder().uri(path).body(Body::empty()).unwrap(),
        )
        .await;
        assert_ne!(status, StatusCode::CONFLICT, "核心路径被误拦截: {path}");
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-CONSENT-05/07：Admin Client CRUD ───────────────────────────

async fn admin_context(pool: &DatabasePool, tag: &str) -> (String, String) {
    let admin = insert_user(pool, tag).await;
    assign_role(pool, &admin, "administrator").await;
    common::enroll_totp(pool, &admin).await;
    let cookie = common::direct_session_cookie(pool, &admin).await;
    (admin, cookie)
}

#[tokio::test]
async fn admin_oauth_client_crud_requires_permissions_and_hashes_secrets() {
    let (pool, dir) = sqlite_pool().await;
    let app = app_with_oidc(pool.clone());
    let (_, admin_cookie) = admin_context(&pool, "admin").await;
    let csrf = get_csrf(app.clone(), &admin_cookie).await;

    // 创建：admin.manage + reason + recent-auth → 201，返回 client + secret。
    let body = json!({
        "name": "Partner App",
        "client_type": "confidential",
        "redirect_uris": [REDIRECT],
        "post_logout_uris": [POST_LOGOUT],
        "scopes": ["openid", "profile"],
        "reason": "M11-CONSENT-05 integration drill",
    });
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/api/v1/admin/oauth-clients")
            .header("cookie", &admin_cookie)
            .header("x-csrf-token", &csrf)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "create body: {rbody}");
    let client_id = rbody["client"]["client_id"].as_str().unwrap().to_string();
    let client_internal_id = rbody["client"]["id"].as_str().unwrap().to_string();
    let secret = rbody["client"]["secret"]
        .as_str()
        .expect("confidential 创建必须返回明文 secret（仅一次）")
        .to_string();
    assert_eq!(rbody["client"]["secret_configured"], true);
    assert_eq!(rbody["client"]["client_type"], "confidential");
    assert_eq!(rbody["client"]["status"], "active");

    // DB：secret 只存 SHA-256 hash，绝不落明文。
    let stored_hash: String = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT client_secret_hash FROM oauth_clients WHERE id = ?")
                .bind(&client_internal_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(
        stored_hash,
        bblbb_backend::auth::token::hash_token(&secret),
        "secret 必须以 SHA-256 hash 存储"
    );
    assert_ne!(stored_hash, secret);
    assert_eq!(stored_hash.len(), 64);

    // 审计。
    let audit: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'oauth_client.create'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit, 1, "创建必须写审计");

    // 列表 + 单个 GET。
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/api/v1/admin/oauth-clients")
            .header("cookie", &admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let found = rbody["clients"]
        .as_array()
        .unwrap()
        .iter()
        .any(|c| c["client_id"] == client_id);
    assert!(found, "列表必须包含新 client: {rbody}");
    // 列表视图不得含明文 secret。
    for c in rbody["clients"].as_array().unwrap() {
        assert!(c.get("secret").is_none(), "列表不得暴露明文 secret");
    }
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/admin/oauth-clients/{client_internal_id}"))
            .header("cookie", &admin_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(rbody["client"]["client_id"], client_id);

    // 无效 redirect URI → 400（精确校验）。
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/admin/oauth-clients/{client_internal_id}"))
            .header("cookie", &admin_cookie)
            .header("x-csrf-token", &csrf)
            .header("content-type", "application/json")
            .header("if-match", "1")
            .body(Body::from(
                json!({
                    "redirect_uris": ["https://evil.example/steal", "http://insecure.example/cb"],
                    "reason": "test bad uri",
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "无效 redirect 必须拒绝: {rbody}"
    );

    // 版本冲突：错误 If-Match → 409。
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/admin/oauth-clients/{client_internal_id}"))
            .header("cookie", &admin_cookie)
            .header("x-csrf-token", &csrf)
            .header("content-type", "application/json")
            .header("if-match", "99")
            .body(Body::from(
                json!({ "name": "Renamed", "reason": "conflict drill" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::CONFLICT,
        "If-Match 冲突必须 409: {rbody}"
    );

    // 停用（status=disabled + reason + If-Match 1）→ 200；version 2。
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/admin/oauth-clients/{client_internal_id}"))
            .header("cookie", &admin_cookie)
            .header("x-csrf-token", &csrf)
            .header("content-type", "application/json")
            .header("if-match", "1")
            .body(Body::from(
                json!({ "status": "disabled", "reason": "compromise drill" }).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "停用 body: {rbody}");
    assert_eq!(rbody["client"]["status"], "disabled");
    assert_eq!(rbody["client"]["version"], 2);

    // 停用后 authorize 拒绝（unauthorized_client 重定向）。
    let user = insert_user(&pool, "member").await;
    let user_cookie = common::direct_session_cookie(&pool, &user).await;
    let challenge = pkce_s256_challenge(VERIFIER);
    let pairs = base_pairs(&client_id, "openid", "s-dis", "n-dis", &challenge);
    let (status, headers, _) = start_authorize(app.clone(), Some(&user_cookie), &pairs).await;
    assert_eq!(status, StatusCode::SEE_OTHER);
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    assert_eq!(
        query_param(&location, "error").as_deref(),
        Some("unauthorized_client"),
        "disabled client 不得再授权"
    );

    // 缺失 reason → 400。
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("PATCH")
            .uri(format!("/api/v1/admin/oauth-clients/{client_internal_id}"))
            .header("cookie", &admin_cookie)
            .header("x-csrf-token", &csrf)
            .header("content-type", "application/json")
            .body(Body::from(json!({ "status": "active" }).to_string()))
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::BAD_REQUEST,
        "缺失 reason 必须 400: {rbody}"
    );

    // 非管理员 → 403。
    let member_cookie = {
        let member = insert_user(&pool, "normie").await;
        common::direct_session_cookie(&pool, &member).await
    };
    let member_csrf = get_csrf(app.clone(), &member_cookie).await;
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/api/v1/admin/oauth-clients")
            .header("cookie", &member_cookie)
            .header("x-csrf-token", &member_csrf)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": "Nope",
                    "client_type": "public",
                    "redirect_uris": [REDIRECT],
                    "scopes": ["openid"],
                    "reason": "should fail",
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "非管理员必须 403: {rbody}");

    // step-up 过期 → 403 step_up_required（recent-auth 强制）。
    let (_, stale_cookie) = admin_context(&pool, "stale_admin").await;
    let stale_csrf = get_csrf(app.clone(), &stale_cookie).await;
    let stale_token = stale_cookie.split('=').nth(1).unwrap().to_string();
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET auth_verified_at = ? WHERE token_hash = ?")
                .bind(now_millis() - 60 * 60 * 1000)
                .bind(bblbb_backend::auth::token::hash_token(&stale_token))
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let (status, _, rbody) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/api/v1/admin/oauth-clients")
            .header("cookie", &stale_cookie)
            .header("x-csrf-token", &stale_csrf)
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "name": "Stale",
                    "client_type": "public",
                    "redirect_uris": [REDIRECT],
                    "scopes": ["openid"],
                    "reason": "recent-auth drill",
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "过期认证必须 403: {rbody}");
    assert_eq!(rbody["code"], "step_up_required");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-CONSENT-01：consent revoke / re-consent ───────────────────────────

#[tokio::test]
async fn consent_revoke_revokes_family_and_requires_reconsent() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "consent").await;
    let user = insert_user(&pool, "iris").await;
    let app = app_with_oidc(pool.clone());
    let ts = complete_confidential_flow(
        &app,
        &pool,
        &user,
        &client,
        &secret,
        "openid profile",
        VERIFIER,
        "n-cs",
        "s-cs",
    )
    .await;

    // 初始 consent 行（逐 scope）。
    let scopes: Vec<(String, Option<i64>)> = match &pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT scope, revoked_at FROM oauth_consents WHERE user_id = ? AND client_id = ? ORDER BY scope",
            )
            .bind(&user)
            .bind(&client.id)
            .fetch_all(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(
        scopes.iter().map(|(s, _)| s.as_str()).collect::<Vec<_>>(),
        vec!["openid", "profile"]
    );
    assert!(scopes.iter().all(|(_, r)| r.is_none()));

    // consent grant 通知。
    let notify: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND category = 'security' AND resource_type = 'oauth_interaction'",
            )
            .bind(&user)
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(notify, 1, "首次授权必须通知用户");

    // 撤销 consent → 同步撤销 refresh family。
    bblbb_backend::oidc::consent::revoke_consents_for_client(
        &pool,
        &user,
        &client.id,
        "user revoke drill",
        "test",
        now_millis(),
    )
    .await
    .unwrap();

    // userinfo 旧 access → 401（family 撤销）。
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .header(header::AUTHORIZATION, format!("Bearer {}", ts.access_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "撤销后旧 token 必须失效: {body}"
    );

    // 撤销通知。
    let notify: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM notifications WHERE user_id = ? AND category = 'security' AND resource_type = 'oauth_client'",
            )
            .bind(&user)
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(notify >= 1, "撤销必须通知用户");

    // 重新授权：interaction 标记 previously_consented=false。
    let session_cookie = common::direct_session_cookie(&pool, &user).await;
    let challenge = pkce_s256_challenge(VERIFIER);
    let pairs = base_pairs(
        &client.client_id,
        "openid profile",
        "s-re",
        "n-re",
        &challenge,
    );
    let (_, headers, _) = start_authorize(app.clone(), Some(&session_cookie), &pairs).await;
    let location = headers
        .get(header::LOCATION)
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let interaction_id = extract_interaction_id(&location);
    let (_, _, ibody) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri(format!("/api/v1/oauth/interactions/{interaction_id}"))
            .header("cookie", &session_cookie)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(ibody["previously_consented"], false);

    // 重新同意 → 新 code 可换 token（重新激活 consent）。
    let csrf = get_csrf(app.clone(), &session_cookie).await;
    let (_, _, dbody) = decide(
        app.clone(),
        &session_cookie,
        &csrf,
        &interaction_id,
        "allow",
    )
    .await;
    let code = query_param(dbody["redirect_to"].as_str().unwrap(), "code").unwrap();
    let form = form_body(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT),
        ("code_verifier", VERIFIER),
    ]);
    let (status, _, tbody) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "re-consent 后必须可换 token: {tbody}"
    );
    assert!(tbody["access_token"].as_str().unwrap().len() >= 40);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-CONSENT-07：banned user / family reuse ───────────────────────────

#[tokio::test]
async fn banned_user_blocks_userinfo_and_refresh() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "ban").await;
    let user = insert_user(&pool, "jack").await;
    let app = app_with_oidc(pool.clone());
    let ts = complete_confidential_flow(
        &app, &pool, &user, &client, &secret, "openid", VERIFIER, "n-ban", "s-ban",
    )
    .await;

    set_user_status(&pool, &user, "banned").await;

    // userinfo → 401。
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .header(header::AUTHORIZATION, format!("Bearer {}", ts.access_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "封禁用户 token 必须失效: {body}"
    );

    // refresh → invalid_grant。
    let form = form_body(&[
        ("grant_type", "refresh_token"),
        ("refresh_token", &ts.refresh_token),
    ]);
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/oauth/token")
            .header("content-type", "application/x-www-form-urlencoded")
            .header(
                header::AUTHORIZATION,
                basic_auth(&client.client_id, &secret),
            )
            .body(Body::from(form))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "封禁用户不得刷新: {body}");
    assert_eq!(body["error"], "invalid_grant");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-CONSENT-03/04/07：key rotation + fail-closed ───────────────────────────

#[tokio::test]
async fn key_rotation_publishes_then_switches_and_old_tokens_stay_valid() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "keys").await;
    let user = insert_user(&pool, "kate").await;
    let app = app_with_oidc(pool.clone());

    // 首次供给：active 密钥 kid1。
    let (row1, _) = bblbb_backend::oidc::keys::active_signing_key(&pool, MASTER_KEY.as_bytes())
        .await
        .unwrap();
    let kid1 = row1.kid.clone();
    // 私钥加密保存：非空、非 PEM 明文。
    assert!(!row1.private_key_ciphertext.is_empty());
    assert!(
        !row1.private_key_ciphertext.contains("PRIVATE KEY"),
        "私钥不得以 PEM 明文落库"
    );
    assert!(
        row1.private_key_ciphertext
            .bytes()
            .all(|b| b.is_ascii_hexdigit()),
        "密文必须是 hex"
    );

    // 完整流程：ID Token 用 kid1 签发。
    let ts1 = complete_confidential_flow(
        &app, &pool, &user, &client, &secret, "openid", VERIFIER, "n-k1", "s-k1",
    )
    .await;
    let header1 = bblbb_backend::oidc::protocol::decode_jwt_header(&ts1.id_token).unwrap();
    assert_eq!(header1["kid"], kid1);

    // 轮换：先发布新 key 再切换 active；旧 key 保留（retiring）。
    bblbb_backend::oidc::keys::rotate_signing_key(
        &pool,
        MASTER_KEY.as_bytes(),
        "tester",
        "rotation drill",
    )
    .await
    .unwrap();

    // JWKS 同时包含旧（retiring）与新（active）密钥。
    let jwks = bblbb_backend::oidc::keys::jwks_document(&pool)
        .await
        .unwrap();
    let kids: Vec<String> = jwks["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kid"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(kids.len(), 2, "轮换后 JWKS 必须含新旧两把 key: {kids:?}");
    assert!(kids.contains(&kid1), "旧 key 必须保留: {kids:?}");

    // 轮换后新流程用新 kid 签发。
    let ts2 = complete_confidential_flow(
        &app, &pool, &user, &client, &secret, "openid", VERIFIER, "n-k2", "s-k2",
    )
    .await;
    let header2 = bblbb_backend::oidc::protocol::decode_jwt_header(&ts2.id_token).unwrap();
    let kid2 = header2["kid"].as_str().unwrap().to_string();
    assert_ne!(kid2, kid1, "轮换后必须用新 key 签发");
    assert!(kids.contains(&kid2));

    // 轮换前签发的 access token 仍有效（opaque family 语义，不依赖签名 key）。
    let (status, _, _) = do_request(
        app.clone(),
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .header(
                header::AUTHORIZATION,
                format!("Bearer {}", ts1.access_token),
            )
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "轮换期间旧 access token 必须仍有效");

    // 旧 key 过期 + 安全余量后由 purge 移除。
    let cutoff_past = now_millis() - 40 * 24 * 3600 * 1000;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE oauth_signing_keys SET retired_at = ? WHERE kid = ?")
                .bind(cutoff_past)
                .bind(&kid1)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let purged = bblbb_backend::oidc::keys::purge_expired_keys(&pool, now_millis())
        .await
        .unwrap();
    assert_eq!(purged, 1, "超过保留期必须移除旧 key");
    let jwks = bblbb_backend::oidc::keys::jwks_document(&pool)
        .await
        .unwrap();
    let kids: Vec<String> = jwks["keys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|k| k["kid"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(kids, vec![kid2]);

    // fail-closed：主密钥不可用/错误 → 不临时生成新 key 掩盖丢失。
    let err = bblbb_backend::oidc::keys::active_signing_key(&pool, b"wrong-master-key").await;
    assert!(err.is_err(), "主密钥错误必须失败而非重新生成 key 掩盖丢失");
    assert_eq!(
        bblbb_backend::oidc::keys::active_signing_key(&pool, b"")
            .await
            .err()
            .unwrap()
            .to_string(),
        "server_error: OIDC signing key cannot be decrypted: master key unavailable or wrong"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-CONSENT-06：scope 永不扣款 ───────────────────────────

#[tokio::test]
async fn oidc_scopes_cannot_debit_and_are_closed_world() {
    let (pool, dir) = sqlite_pool().await;
    // 1) 协议层：OIDC scope 白名单封闭，marketplace.purchase 被拒绝。
    assert!(bblbb_backend::oidc::protocol::parse_scopes("openid marketplace.purchase").is_err());
    assert!(bblbb_backend::oidc::protocol::parse_scopes("openid profile").is_ok());

    // 2) Client 定义层：服务端登记也拒绝非身份 scope。
    let bad_input = bblbb_backend::oidc::clients::ClientCreateInput {
        name: "Bad".into(),
        client_type: "confidential".into(),
        redirect_uris: vec![REDIRECT.into()],
        post_logout_uris: vec![],
        scopes: vec!["openid".into(), "marketplace.purchase".into()],
    };
    let err = bblbb_backend::oidc::clients::create_client(&pool, &bad_input, "t", now_millis())
        .await
        .expect_err("扣款 scope 不得登记为 OIDC client scope");
    assert!(err.to_string().contains("unsupported scope"));

    // 3) 发现层：scopes_supported 只声明身份 scope。
    let doc = bblbb_backend::oidc::protocol::discovery_document(ISSUER);
    assert_eq!(
        doc["scopes_supported"],
        json!(["openid", "profile", "email"])
    );

    // 4) 传输层：OIDC access token 不能调用任何 Session 业务端点（含记账/签到）。
    let (client, secret) = create_confidential_client(&pool, "no-debit").await;
    let user = insert_user(&pool, "lee").await;
    let app = app_with_oidc(pool.clone());
    let ts = complete_confidential_flow(
        &app,
        &pool,
        &user,
        &client,
        &secret,
        "openid profile email",
        VERIFIER,
        "n-nd",
        "s-nd",
    )
    .await;
    let (status, _, body) = do_request(
        app.clone(),
        Request::builder()
            .method("POST")
            .uri("/api/v1/activity/visit")
            .header(header::AUTHORIZATION, format!("Bearer {}", ts.access_token))
            .header("content-type", "application/json")
            .body(Body::from("{}"))
            .unwrap(),
    )
    .await;
    assert_eq!(
        status,
        StatusCode::UNAUTHORIZED,
        "OIDC token 不得调用业务端点: {body}"
    );
    // userinfo 是该 token 唯一合法的入口。
    let (status, _, _) = do_request(
        app,
        Request::builder()
            .method("GET")
            .uri("/oauth/userinfo")
            .header(header::AUTHORIZATION, format!("Bearer {}", ts.access_token))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── M11-OIDC-SCHEMA-01..06：迁移与存储断言 ───────────────────────────

#[tokio::test]
async fn schema_enforces_hashed_codes_closed_enums_and_encrypted_keys() {
    let (pool, dir) = sqlite_pool().await;
    let (client, secret) = create_confidential_client(&pool, "schema").await;
    let user = insert_user(&pool, "mia").await;
    let app = app_with_oidc(pool.clone());
    let ts = complete_confidential_flow(
        &app,
        &pool,
        &user,
        &client,
        &secret,
        "openid email",
        VERIFIER,
        "n-sch",
        "s-sch",
    )
    .await;

    // oauth_clients：secret 只存 hash；client_type/status 封闭枚举。
    let (stored_hash, ctype, status): (Option<String>, String, String) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT client_secret_hash, client_type, status FROM oauth_clients WHERE id = ?",
        )
        .bind(&client.id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(ctype, "confidential");
    assert_eq!(status, "active");
    assert_eq!(
        stored_hash.as_deref(),
        Some(bblbb_backend::auth::token::hash_token(&secret).as_str())
    );

    // 封闭枚举：非法 client_type 插入被 CHECK 拒绝。
    let bad_insert = match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO oauth_clients
                    (id, name, client_type, client_id, client_secret_hash, redirect_uris_json,
                     post_logout_uris_json, scopes_json, status, version, created_by, created_at, updated_by, updated_at)
                 VALUES (?, 'x', 'spy', ?, NULL, '[]', NULL, '[]', 'active', 1, 't', ?, 't', ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(now_millis())
            .bind(now_millis())
            .execute(p)
            .await
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(bad_insert.is_err(), "非法 client_type 必须被 CHECK 拒绝");

    // oauth_authorization_codes：高熵 code 只存 hash；一次性消费。
    let (code_hash, consumed_at, request_hash): (String, Option<i64>, Option<String>) =
        match &pool {
            Either::Left(p) => {
                sqlx::query_as(
                    "SELECT code_hash, consumed_at, request_hash FROM oauth_authorization_codes WHERE client_id = ?",
                )
                .bind(&client.id)
                .fetch_one(p)
                .await
                .unwrap()
            }
            Either::Right(_) => panic!("SQLite only"),
        };
    assert_eq!(code_hash, bblbb_backend::auth::token::hash_token(&ts.code));
    assert!(consumed_at.is_some(), "授权码必须已被一次性消费");
    assert!(
        request_hash.is_some(),
        "授权码必须绑定 request hash（state/nonce/PKCE 恢复绑定）"
    );

    // oauth_tokens / oauth_token_families：hash-only + revoke_reason + usage。
    let (access_hash, refresh_hash): (String, Option<String>) = match &pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT access_token_hash, refresh_token_hash FROM oauth_tokens WHERE client_id = ? ORDER BY issued_at DESC LIMIT 1",
            )
            .bind(&client.id)
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(
        access_hash,
        bblbb_backend::auth::token::hash_token(&ts.access_token)
    );
    assert_eq!(
        refresh_hash.as_deref(),
        Some(bblbb_backend::auth::token::hash_token(&ts.refresh_token).as_str())
    );
    let family_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM oauth_token_families WHERE client_id = ?")
                .bind(&client.id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(family_count, 1);

    // oauth_signing_keys：加密私钥（非 PEM 明文）+ JWKS revision（public JWK）+ 审计。
    let (ciphertext, jwk, audit): (String, String, Option<String>) = match &pool {
        Either::Left(p) => {
            sqlx::query_as(
                "SELECT private_key_ciphertext, public_jwk_json, key_audit_json FROM oauth_signing_keys LIMIT 1",
            )
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(!ciphertext.is_empty());
    assert!(!ciphertext.contains("PRIVATE KEY"));
    let jwk_val: Value = serde_json::from_str(&jwk).unwrap();
    assert_eq!(jwk_val["kty"], "RSA");
    assert!(audit.is_some(), "密钥必须有轮换/供给审计");

    // oauth_interactions：request binding 摘要 + 状态机枚举。
    let (status, expires_at): (String, i64) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT status, expires_at FROM oauth_interactions")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(status, "approved");
    assert!(expires_at > now_millis());

    // oauth_consents：逐 (user, client, scope) 唯一 + revoked_at 可空。
    let scope_rows: Vec<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT scope FROM oauth_consents WHERE user_id = ? AND client_id = ? ORDER BY scope",
        )
        .bind(&user)
        .bind(&client.id)
        .fetch_all(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(scope_rows, vec!["email".to_string(), "openid".to_string()]);

    // 授权码重放由协议层保护：消费后 lookup 仍只有一行（唯一 code_hash）。
    let row_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM oauth_authorization_codes WHERE client_id = ?")
                .bind(&client.id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(row_count, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}
