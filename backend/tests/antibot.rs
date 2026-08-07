//! M08-CRAWL 集成测试：HTTP 层风控中间件端到端。
//!
//! 覆盖：AI 训练爬虫默认拒绝、可信代理链、分桶限流（429 + Retry-After +
//! RateLimit-*）、挑战流程（一次性 token → 放行）、挑战失败 → 临时封禁、
//! 健康检查豁免、伪造代理头不能绕过限流。

use std::sync::Arc;

use axum::{
    body::Body,
    http::{Request, StatusCode},
    middleware,
    routing::{get, post},
    Router,
};
use bblbb_backend::antibot::{antibot_guard, AntibotConfig, AntibotEngine};
use bblbb_backend::app::AppState;
use bblbb_backend::config::{flags::FeatureFlags, AppConfig};
use bblbb_backend::ratelimit::RateLimiter;
use http_body_util::BodyExt;
use tower::ServiceExt;

#[allow(clippy::field_reassign_with_default)] // 桶上限需逐个插入，逐字段显式
fn test_config() -> AntibotConfig {
    let mut c = AntibotConfig::default();
    c.challenge_enabled = true;
    c.bucket_limits.insert("search", (3, 60_000));
    c.bucket_limits.insert("anonymous", (2, 60_000));
    c.bucket_limits.insert("rss", (1, 60_000));
    c.challenge_fail_ban_threshold = 2;
    c.temp_ban_ms = 600_000;
    c
}

fn build_router(engine: AntibotEngine) -> Router {
    let state = AppState {
        config: Arc::new(AppConfig::default()),
        db: None,
        flags: FeatureFlags::default(),
        limiter: Arc::new(RateLimiter::new()),
        storage: None,
        antibot: Arc::new(engine),
    };
    Router::new()
        .route("/api/v1/search", get(|| async { "ok" }))
        .route("/api/v1/rss", get(|| async { "ok" }))
        .route("/api/v1/auth/login", post(|| async { "ok" }))
        .route("/api/v1/users/me", get(|| async { "ok" }))
        .route("/healthz", get(|| async { "ok" }))
        .layer(middleware::from_fn_with_state(state.clone(), antibot_guard))
        .with_state(state)
}

async fn call(
    router: &Router,
    path: &str,
    method: &str,
    ua: Option<&str>,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, axum::http::HeaderMap, String) {
    let mut builder = Request::builder().method(method).uri(path);
    if let Some(ua) = ua {
        builder = builder.header("user-agent", ua);
    }
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let request = builder.body(Body::empty()).unwrap();
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&body).to_string();
    (status, headers, body)
}

#[tokio::test]
async fn ai_training_crawler_denied_by_default() {
    let router = build_router(AntibotEngine::with_config(test_config()));
    for ua in [
        "GPTBot/1.0",
        "CCBot/2.0",
        "ClaudeBot/1.0",
        "Google-Extended",
    ] {
        let (status, _h, body) = call(&router, "/api/v1/search", "GET", Some(ua), &[]).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "UA {ua} 应被拒绝");
        assert!(body.contains("\"code\":\"crawler_denied\""), "body: {body}");
    }
}

#[tokio::test]
async fn normal_search_engine_and_browser_allowed() {
    let router = build_router(AntibotEngine::with_config(test_config()));
    let (status, _h, body) = call(
        &router,
        "/api/v1/search",
        "GET",
        Some("Mozilla/5.0 Googlebot/2.1"),
        &[],
    )
    .await;
    assert_eq!(
        status,
        StatusCode::OK,
        "普通搜索引擎允许访问（body: {body}）"
    );
    let (status, _h, body) = call(
        &router,
        "/api/v1/search",
        "GET",
        Some("Mozilla/5.0 (Macintosh)"),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "浏览器允许访问（body: {body}）");
}

#[tokio::test]
async fn search_bucket_rate_limits_to_429() {
    let mut c = test_config();
    c.challenge_enabled = false; // 直接 429
    let router = build_router(AntibotEngine::with_config(c));
    // 3 次放行。
    for _ in 0..3 {
        let (status, _h, _b) = call(&router, "/api/v1/search", "GET", None, &[]).await;
        assert_eq!(status, StatusCode::OK);
    }
    // 第 4 次 → 429 + 头。
    let (status, headers, body) = call(&router, "/api/v1/search", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS, "body: {body}");
    assert!(body.contains("\"code\":\"rate_limited\""), "body: {body}");
    assert!(headers.contains_key("retry-after"));
    assert!(headers.contains_key("ratelimit-limit"));
    assert!(headers.contains_key("ratelimit-remaining"));
    assert!(headers.contains_key("ratelimit-reset"));
}

#[tokio::test]
async fn challenge_flow_issues_token_and_allows_one_retry() {
    let router = build_router(AntibotEngine::with_config(test_config()));
    for _ in 0..3 {
        let (status, _h, _b) = call(&router, "/api/v1/search", "GET", None, &[]).await;
        assert_eq!(status, StatusCode::OK);
    }
    // 超限 → challenge_required + 一次性 token。
    let (status, headers, body) = call(&router, "/api/v1/search", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    assert!(
        body.contains("\"code\":\"challenge_required\""),
        "body: {body}"
    );
    let token = headers
        .get("x-bblbb-challenge")
        .and_then(|v| v.to_str().ok())
        .expect("challenge token header");
    assert!(!token.is_empty());
    assert!(headers.contains_key("retry-after"));

    // 带 token 重试 → 放行。
    let (status, _h, body) = call(
        &router,
        "/api/v1/search",
        "GET",
        None,
        &[("x-bblbb-challenge", token)],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "有效挑战应放行（body: {body}）");

    // 再超限 → 新 token。
    let (status, _h, body) = call(&router, "/api/v1/search", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body: {body}");
    assert!(
        body.contains("\"code\":\"challenge_required\""),
        "body: {body}"
    );
}

#[tokio::test]
async fn challenge_replay_is_rejected() {
    let router = build_router(AntibotEngine::with_config(test_config()));
    for _ in 0..3 {
        let (_s, _h, _b) = call(&router, "/api/v1/search", "GET", None, &[]).await;
    }
    let (status, headers, _b) = call(&router, "/api/v1/search", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let token = headers
        .get("x-bblbb-challenge")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();
    // 第一次使用 → 通过。
    let (status, _h, _b) = call(
        &router,
        "/api/v1/search",
        "GET",
        None,
        &[("x-bblbb-challenge", &token)],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    // 重放同一 token → 拒绝（一次性）。
    let (status, _h, _b) = call(
        &router,
        "/api/v1/search",
        "GET",
        None,
        &[("x-bblbb-challenge", &token)],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "一次性 token 重放必须拒绝");
}

#[tokio::test]
async fn repeated_challenge_failures_trigger_temp_ban() {
    let router = build_router(AntibotEngine::with_config(test_config()));
    // 阈值 2：两次伪造 token → 触发封禁。
    let (_s, _h, body) = call(
        &router,
        "/api/v1/search",
        "GET",
        None,
        &[("x-bblbb-challenge", "bogus-token")],
    )
    .await;
    assert!(body.contains("\"code\":\"challenge_required\""));
    let (_s, _h, body) = call(
        &router,
        "/api/v1/search",
        "GET",
        None,
        &[("x-bblbb-challenge", "bogus-token")],
    )
    .await;
    assert!(body.contains("\"code\":\"challenge_required\""));
    // 第 3 次 → 已封禁。
    let (status, _h, body) = call(&router, "/api/v1/search", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    assert!(
        body.contains("\"code\":\"temporarily_banned\""),
        "body: {body}"
    );
}

#[tokio::test]
async fn healthz_is_exempt_from_ratelimits() {
    let mut c = test_config();
    c.challenge_enabled = false;
    c.bucket_limits.insert("rss", (1, 60_000));
    let router = build_router(AntibotEngine::with_config(c));
    for _ in 0..20 {
        let (status, _h, _b) = call(&router, "/healthz", "GET", None, &[]).await;
        assert_eq!(status, StatusCode::OK, "健康检查不参与风控");
    }
}

#[tokio::test]
async fn forged_proxy_headers_cannot_bypass_ratelimit() {
    let mut c = test_config();
    c.challenge_enabled = false;
    // 匿名桶 2 次/窗口；不同伪造 XFF 共享 unknown 桶。
    let router = build_router(AntibotEngine::with_config(c));
    let forged = [
        "1.2.3.4, 203.0.113.9",
        "9.9.9.9, 198.51.100.1",
        "8.8.4.4, 203.0.113.77",
    ];
    for (i, xff) in forged.iter().enumerate() {
        let (status, _h, body) = call(
            &router,
            "/api/v1/users/me",
            "GET",
            None,
            &[("x-forwarded-for", xff)],
        )
        .await;
        if i < 2 {
            assert_eq!(
                status,
                StatusCode::OK,
                "第 {} 次应放行（body: {body}）",
                i + 1
            );
        } else {
            assert_eq!(
                status,
                StatusCode::TOO_MANY_REQUESTS,
                "伪造代理头不能绕过限流（body: {body}）"
            );
        }
    }
}

#[tokio::test]
async fn buckets_are_isolated() {
    let mut c = test_config();
    c.challenge_enabled = false;
    // rss 桶 1 次 → 第 2 次 429，但 search 不受影响。
    let router = build_router(AntibotEngine::with_config(c));
    let (s1, _h, _b) = call(&router, "/api/v1/rss", "GET", None, &[]).await;
    assert_eq!(s1, StatusCode::OK);
    let (s2, _h, _b) = call(&router, "/api/v1/rss", "GET", None, &[]).await;
    assert_eq!(s2, StatusCode::TOO_MANY_REQUESTS, "RSS 桶应独立耗尽");
    let (s3, _h, _b) = call(&router, "/api/v1/search", "GET", None, &[]).await;
    assert_eq!(s3, StatusCode::OK, "Search 桶不受 RSS 影响");
}
