//! M08-INDEX：公开投影搜索（HTTP + 服务层，SQLite 真实数据库）。
//!
//! 覆盖：公开投影（标题/slug/摘要/标签/作者）、统一排除（访问策略/审核/删除/
//! 隐藏/退出）、作者 opt-out 与管理员全站/板块策略优先、重建与旧 revision 守卫、
//! 限制（长度/语法/分页深度/匿名频率/高亮长度）、返回前实时重检（封禁/隐藏不
//! 经重索引即失效）、隐藏正文 canary 零泄漏（索引/excerpt/highlight/响应体）。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::worker::ClaimedJob;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::search::{
    handle_index_job, rebuild_all_index, ANON_SEARCH_LIMIT, HIGHLIGHT_MAX_LEN,
};
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

/// 隐藏正文 canary（受限正文必须零泄漏）。
const CANARY: &str = "CANARY-SECRET-BODY-987654321";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-sear-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
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

/// 通用 SQL 执行（SQLite；None 绑定 NULL）。
async fn exec(pool: &DatabasePool, sql: &str, args: &[Option<&str>]) {
    match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(sql);
            for a in args {
                match a {
                    Some(v) => q = q.bind(v),
                    None => q = q.bind(None::<&str>),
                }
            }
            q.execute(p).await.unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn scalar(pool: &DatabasePool, sql: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn scalar_where(pool: &DatabasePool, sql: &str, arg: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .bind(arg)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn scalar_str(pool: &DatabasePool, sql: &str, arg: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .bind(arg)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn claimed(entity_type: &str, entity_id: &str) -> ClaimedJob {
    ClaimedJob {
        id: format!("job-{entity_type}-{entity_id}"),
        queue: "default".to_string(),
        kind: "search.index".to_string(),
        payload: json!({ "entity_type": entity_type, "entity_id": entity_id }),
        payload_version: 1,
        attempts: 1,
        max_attempts: 5,
        locked_until: now_millis() + 30_000,
    }
}

/// 插入已验证作者（email_verified_at ≈ 30 天前）。
async fn insert_author(pool: &DatabasePool, tag: &str, status: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec(
        pool,
        "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level,
            email_verified, email_verified_at, created_at, updated_at)
         VALUES (?, ?, ?, 'dummy', ?, 5, 1, ?, ?, ?)",
        &[
            Some(&id),
            Some(&format!("{tag}_{}", &id[..8])),
            Some(&format!("{tag}_{}@example.com", &id[..8])),
            Some(status),
            Some(&(now - 30 * 24 * 3600 * 1000).to_string()),
            Some(&now.to_string()),
            Some(&now.to_string()),
        ],
    )
    .await;
    id
}

/// 插入启用且 public 的板块。
async fn insert_board(pool: &DatabasePool) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec(
        pool,
        "INSERT INTO boards (id, slug, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        &[
            Some(&id),
            Some(&format!("board-{id}")),
            Some("测试板块"),
            Some(&now.to_string()),
            Some(&now.to_string()),
        ],
    )
    .await;
    id
}

/// 插入帖子（含 post_contents；受限正文可注入 canary）。
#[allow(clippy::too_many_arguments)]
async fn insert_post(
    pool: &DatabasePool,
    board_id: &str,
    author_id: &str,
    title: &str,
    markdown: &str,
    status: &str,
    visibility: &str,
    policy_kind: Option<&str>,
    search_index_opt_out: bool,
    review_status: &str,
    deleted_at: Option<i64>,
    restricted_markdown: Option<&str>,
) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let slug = format!("post-{id}");
    let deleted_at_str = deleted_at.map(|d| d.to_string());
    let deleted_bind = deleted_at_str.as_deref();
    let opt_out = if search_index_opt_out { "1" } else { "0" };

    let mut policy_id: Option<String> = None;
    if let Some(kind) = policy_kind {
        let pid = uuid::Uuid::now_v7().to_string();
        let (min_level, currency_id, amount) = match kind {
            "level" => ("2", "0", "0"),
            "paid" => ("0", "cny", "100"),
            _ => ("0", "0", "0"),
        };
        exec(
            pool,
            "INSERT INTO content_access_policies
                (id, kind, min_level, currency_id, amount, reply_grant_persists, policy_version, created_by, created_at)
             VALUES (?, ?, ?, ?, ?, 0, 1, ?, ?)",
            &[
                Some(&pid),
                Some(kind),
                Some(min_level),
                Some(currency_id),
                Some(amount),
                Some(author_id),
                Some(&now.to_string()),
            ],
        )
        .await;
        policy_id = Some(pid);
    }

    match &policy_id {
        Some(pid) => {
            exec(
                pool,
                "INSERT INTO posts (
                    id, board_id, author_id, post_type, slug, title, status, visibility, version,
                    published_at, content, review_status, search_index_opt_out,
                    ai_summary_opt_out, access_policy_id, deleted_at, created_at, updated_at
                 ) VALUES (?, ?, ?, 'article', ?, ?, ?, ?, 1, ?, '', ?, ?, ?, ?, ?, ?, ?)",
                &[
                    Some(&id),
                    Some(board_id),
                    Some(author_id),
                    Some(&slug),
                    Some(title),
                    Some(status),
                    Some(visibility),
                    Some(&now.to_string()),
                    Some(review_status),
                    Some(opt_out),
                    Some("0"),
                    Some(pid),
                    deleted_bind,
                    Some(&now.to_string()),
                    Some(&now.to_string()),
                ],
            )
            .await;
        }
        None => {
            exec(
                pool,
                "INSERT INTO posts (
                    id, board_id, author_id, post_type, slug, title, status, visibility, version,
                    published_at, content, review_status, search_index_opt_out,
                    ai_summary_opt_out, deleted_at, created_at, updated_at
                 ) VALUES (?, ?, ?, 'article', ?, ?, ?, ?, 1, ?, '', ?, ?, ?, ?, ?, ?)",
                &[
                    Some(&id),
                    Some(board_id),
                    Some(author_id),
                    Some(&slug),
                    Some(title),
                    Some(status),
                    Some(visibility),
                    Some(&now.to_string()),
                    Some(review_status),
                    Some(opt_out),
                    Some("0"),
                    deleted_bind,
                    Some(&now.to_string()),
                    Some(&now.to_string()),
                ],
            )
            .await;
        }
    }

    exec(
        pool,
        "INSERT INTO post_contents (
            post_id, body_markdown, body_html, restricted_markdown, restricted_html,
            renderer_version, excerpt, updated_at
         ) VALUES (?, ?, ?, ?, NULL, 'test', ?, ?)",
        &[
            Some(&id),
            Some(markdown),
            Some(&format!("<p>{markdown}</p>")),
            restricted_markdown,
            Some(&markdown.chars().take(80).collect::<String>()),
            Some(&now.to_string()),
        ],
    )
    .await;
    id
}

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn get_search(app: &axum::Router, uri: &str) -> (StatusCode, Value, Option<String>) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let retry_after = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value, retry_after)
}

// ─────────────────────────── 测试 ───────────────────────────

/// M08-INDEX-01/08：公开投影 + 隐藏正文 canary 零泄漏。
#[tokio::test]
async fn public_search_projections_and_canary_never_leak() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;
    let post_id = insert_post(
        &pool,
        &board,
        &author,
        "SQLite FTS 公开投影",
        "sqlite fulltext search index projection body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        Some(&format!("<div>隐藏正文 {CANARY}</div>")),
    )
    .await;

    // 索引 + FTS 同步。
    handle_index_job(&pool, &claimed("post", &post_id)).await;

    // 索引文档本身不得包含 canary（受限正文从未进入索引输入面）。
    let doc_body = scalar_str(
        &pool,
        "SELECT body FROM search_documents WHERE doc_id = ?",
        &post_id,
    )
    .await;
    assert!(!doc_body.contains(CANARY), "索引正文泄漏 canary");

    // 直接 FTS 查询 canary token → 零命中（索引、highlight 都不可能命中）。
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH '\"CANARY\"'"
        )
        .await,
        0,
        "FTS 不得命中 canary"
    );

    // HTTP 搜索命中帖子。
    let (status, body, _) = get_search(&app, "/api/v1/search?q=sqlite").await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "必须命中公开帖: {body}");
    let item = &items[0];
    assert_eq!(item["id"], post_id);
    assert_eq!(item["type"], "post");
    assert_eq!(item["title"], "SQLite FTS 公开投影");
    assert_eq!(item["url"], format!("/posts/post-{post_id}"));
    assert!(item["excerpt"].as_str().unwrap().contains("sqlite"));

    // 全响应（含 highlight/错误信息）不得包含 canary。
    let raw = body.to_string();
    assert!(!raw.contains(CANARY), "搜索响应泄漏 canary: {raw}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-INDEX-02：统一排除——访问策略（公开/登录/回复/等级/付费）与审核/删除。
#[tokio::test]
async fn unified_exclusion_rules_keep_non_public_out() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;
    let now = now_millis();

    let published_id = insert_post(
        &pool,
        &board,
        &author,
        "公开帖",
        "public body unique",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let logged_in = insert_post(
        &pool,
        &board,
        &author,
        "登录可见",
        "logged in body",
        "published",
        "logged_in",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let level = insert_post(
        &pool,
        &board,
        &author,
        "等级可见",
        "level body",
        "published",
        "public",
        Some("level"),
        false,
        "none",
        None,
        None,
    )
    .await;
    let paid = insert_post(
        &pool,
        &board,
        &author,
        "付费可见",
        "paid body",
        "published",
        "public",
        Some("paid"),
        false,
        "none",
        None,
        None,
    )
    .await;
    let after_reply = insert_post(
        &pool,
        &board,
        &author,
        "回复可见",
        "reply body",
        "published",
        "public",
        Some("after_reply"),
        false,
        "none",
        None,
        None,
    )
    .await;
    let under_review = insert_post(
        &pool,
        &board,
        &author,
        "审核中",
        "review body",
        "draft",
        "public",
        None,
        false,
        "pending_review",
        None,
        None,
    )
    .await;
    let hidden = insert_post(
        &pool,
        &board,
        &author,
        "隐藏帖",
        "hidden body",
        "hidden",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let deleted = insert_post(
        &pool,
        &board,
        &author,
        "已删除",
        "deleted body",
        "deleted",
        "public",
        None,
        false,
        "none",
        Some(now),
        None,
    )
    .await;

    for id in [
        &published_id,
        &logged_in,
        &level,
        &paid,
        &after_reply,
        &under_review,
        &hidden,
        &deleted,
    ] {
        handle_index_job(&pool, &claimed("post", id)).await;
    }

    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE entity_type = 'post'"
        )
        .await,
        1,
        "只有公开帖可入索引"
    );
    // HTTP：只有公开帖命中。
    let (status, body, _) = get_search(&app, "/api/v1/search?q=body").await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "仅公开帖可被搜索: {body}");
    assert_eq!(items[0]["id"], published_id);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-INDEX-03：作者逐帖 opt-out 与管理员全站/板块策略优先。
#[tokio::test]
async fn author_opt_out_and_admin_policy_precedence() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;

    let normal = insert_post(
        &pool,
        &board,
        &author,
        "普通帖",
        "normal body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let opted_out = insert_post(
        &pool,
        &board,
        &author,
        "退出帖",
        "opted out body",
        "published",
        "public",
        None,
        true,
        "none",
        None,
        None,
    )
    .await;

    for id in [&normal, &opted_out] {
        handle_index_job(&pool, &claimed("post", id)).await;
    }
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE entity_type = 'post'"
        )
        .await,
        1,
        "退出索引的帖子不得入索引"
    );
    // 重新入队索引 Job 后依然退出（幂等保持排除）。
    handle_index_job(&pool, &claimed("post", &opted_out)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE entity_type = 'post'"
        )
        .await,
        1
    );

    // 管理员板块 deny → 板块内全部退出（即便作者 allow）。
    let now = now_millis();
    bblbb_backend::search::set_board_policy(&pool, &board, "deny", "deny", &author, now)
        .await
        .unwrap();
    handle_index_job(&pool, &claimed("post", &normal)).await;
    handle_index_job(&pool, &claimed("post", &opted_out)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE entity_type = 'post'"
        )
        .await,
        0,
        "管理员板块 deny 必须优先于作者 allow"
    );

    // 管理员全站 deny → 全部退出。
    let board2 = insert_board(&pool).await;
    let post2 = insert_post(
        &pool,
        &board2,
        &author,
        "另一板块帖",
        "another body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    handle_index_job(&pool, &claimed("post", &post2)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE entity_type = 'post'"
        )
        .await,
        1
    );
    bblbb_backend::search::set_site_policy(&pool, "deny", "deny", &author, now + 1)
        .await
        .unwrap();
    handle_index_job(&pool, &claimed("post", &post2)).await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE entity_type = 'post'"
        )
        .await,
        0,
        "管理员全站 deny 必须清空公开索引"
    );

    // HTTP 搜索确认空结果（实时重检同样生效）。
    let (status, body, _) = get_search(&app, "/api/v1/search?q=body").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].as_array().unwrap().is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-INDEX-07：返回前实时重检——作者封禁/隐藏后无需重索引即从结果消失。
#[tokio::test]
async fn live_recheck_drops_banned_and_hidden_without_reindex() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;
    let post_id = insert_post(
        &pool,
        &board,
        &author,
        "重检帖",
        "recheck body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    handle_index_job(&pool, &claimed("post", &post_id)).await;

    let (status, body, _) = get_search(&app, "/api/v1/search?q=recheck").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["items"].as_array().unwrap().len(), 1);

    // 作者封禁（不重索引）：索引仍有文档，但搜索结果必须排除。
    let now = now_millis();
    exec(
        &pool,
        "UPDATE users SET status = 'banned', updated_at = ? WHERE id = ?",
        &[Some(&now.to_string()), Some(&author)],
    )
    .await;
    assert_eq!(
        scalar_where(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = ?",
            &post_id
        )
        .await,
        1,
        "索引仍保留文档（未重索引）"
    );
    let (status, body, _) = get_search(&app, "/api/v1/search?q=recheck").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body["items"].as_array().unwrap().is_empty(),
        "实时重检必须排除封禁作者内容: {body}"
    );

    // 隐藏（不重索引）：同样立即消失。
    exec(
        &pool,
        "UPDATE posts SET status = 'hidden', updated_at = ? WHERE id = ?",
        &[Some(&now.to_string()), Some(&post_id)],
    )
    .await;
    let (status, body, _) = get_search(&app, "/api/v1/search?q=recheck").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body["items"].as_array().unwrap().is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-INDEX-06：限制——长度/语法/分页深度/匿名频率/高亮长度。
#[tokio::test]
async fn search_limits_length_syntax_depth_rate_highlight() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;
    let post_id = insert_post(
        &pool,
        &board,
        &author,
        "限制测试帖",
        "limit body 内容",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    handle_index_job(&pool, &claimed("post", &post_id)).await;

    // 查询过长 → 400。
    let long_q = "x".repeat(201);
    let (status, body, _) = get_search(&app, &format!("/api/v1/search?q={long_q}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{body}");

    // 空查询 → 400。
    let (status, _, _) = get_search(&app, "/api/v1/search?q=%20%20").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 非法 token（纯标点）→ 400。
    let (status, _, _) = get_search(&app, "/api/v1/search?q=%2B%2B%2B%2B").await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 分页深度超限 → 400（cursor 内编码页码）。
    let deep = bblbb_backend::search::encode_cursor(11, 1, "doc");
    let (status, _, _) = get_search(&app, &format!("/api/v1/search?q=limit&after={deep}")).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 高亮存在且不超过长度上限。
    let (status, body, _) = get_search(&app, "/api/v1/search?q=limit").await;
    assert_eq!(status, StatusCode::OK);
    let item = &body["items"][0];
    let highlight = item["highlight"].as_str().unwrap();
    assert!(
        highlight.chars().count() <= HIGHLIGHT_MAX_LEN + 2,
        "高亮必须受限: {highlight}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-INDEX-06：匿名搜索频率限制（antibot Search 独立桶，M08-CRAWL-03）——
/// 超过额度后拒绝（429 rate_limited 或 403 challenge_required）。
#[tokio::test]
async fn anonymous_search_rate_limit_returns_rejection() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    for i in 0..ANON_SEARCH_LIMIT {
        let (status, _, _) = get_search(&app, "/api/v1/search?q=anything").await;
        assert_eq!(status, StatusCode::OK, "第 {i} 次应放行");
    }
    let (status, body, retry) = get_search(&app, "/api/v1/search?q=anything").await;
    assert!(
        status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::FORBIDDEN,
        "超限必须被拒绝（429 或 403 挑战），实际 {status}: {body}"
    );
    let code = body["code"].as_str().unwrap_or("");
    assert!(
        code == "rate_limited" || code == "challenge_required",
        "稳定错误码必须是 rate_limited 或 challenge_required: {code}"
    );
    if code == "rate_limited" {
        assert!(retry.is_some(), "429 必须携带 Retry-After");
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-INDEX-05：重建按当前策略重新生成；旧 revision 不覆盖新。
#[tokio::test]
async fn rebuild_applies_current_policy_and_guard_blocks_stale() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;
    let keep = insert_post(
        &pool,
        &board,
        &author,
        "保留帖",
        "keep body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let drop_me = insert_post(
        &pool,
        &board,
        &author,
        "退出帖",
        "drop body",
        "published",
        "public",
        None,
        true,
        "none",
        None,
        None,
    )
    .await;
    let deleted_post = insert_post(
        &pool,
        &board,
        &author,
        "删除帖",
        "deleted source",
        "published",
        "public",
        None,
        false,
        "none",
        Some(now_millis()),
        None,
    )
    .await;

    // 首次索引（已退出/已删除的帖子不落索引）。
    for id in [&keep, &drop_me, &deleted_post] {
        handle_index_job(&pool, &claimed("post", id)).await;
    }
    // 手工造一条残留文档（模拟旧数据）。
    exec(
        &pool,
        "INSERT OR IGNORE INTO search_documents
            (rowid, doc_id, entity_type, title, body, excerpt, slug, author_id, tags_json,
             source_revision, policy_revision, indexed_at)
         VALUES (NULL, 'stale-ghost', 'post', 'ghost', 'ghost body', 'ghost', 'ghost', NULL,
                 '[]', 1, 1, 1)",
        &[],
    )
    .await;
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE entity_type = 'post'"
        )
        .await,
        2
    );

    // 全量重建：按当前策略重新生成 + 清理残留。
    let summary = rebuild_all_index(&pool).await.unwrap();
    assert_eq!(summary.posts, 3, "posts 源行数: {summary:?}");
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE entity_type = 'post'"
        )
        .await,
        1,
        "重建后只剩保留帖"
    );
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_documents WHERE doc_id = 'stale-ghost'"
        )
        .await,
        0,
        "残留文档必须被清理"
    );
    // FTS 重建后可命中。
    assert_eq!(
        scalar(
            &pool,
            "SELECT COUNT(*) FROM search_fts WHERE search_fts MATCH '\"keep\"'"
        )
        .await,
        1
    );

    // 旧 revision 守卫：stored.policy_revision 更大时重复写被拒绝。
    exec(
        &pool,
        "UPDATE search_documents SET policy_revision = policy_revision + 1000000 WHERE doc_id = ?",
        &[Some(&keep)],
    )
    .await;
    let bumped = scalar_where(
        &pool,
        "SELECT policy_revision FROM search_documents WHERE doc_id = ?",
        &keep,
    )
    .await;
    handle_index_job(&pool, &claimed("post", &keep)).await;
    let after = scalar_where(
        &pool,
        "SELECT policy_revision FROM search_documents WHERE doc_id = ?",
        &keep,
    )
    .await;
    assert_eq!(after, bumped, "旧 revision 不得覆盖新 revision");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-INDEX-04/07：分页 + 标签投影。
#[tokio::test]
async fn pagination_and_tags() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;
    let now = now_millis();

    // 3 篇可命中的帖子。
    let mut ids = Vec::new();
    for i in 0..3 {
        let id = insert_post(
            &pool,
            &board,
            &author,
            &format!("分页帖 {i}"),
            &format!("pagination body unique{i}"),
            "published",
            "public",
            None,
            false,
            "none",
            None,
            None,
        )
        .await;
        ids.push(id);
    }
    for id in &ids {
        handle_index_job(&pool, &claimed("post", id)).await;
    }
    // 给最后一篇帖子打一个启用标签（tags_json 投影）。
    let tag_id = uuid::Uuid::now_v7().to_string();
    exec(
        &pool,
        "INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)",
        &[Some(&tag_id), Some("pagetag"), Some(&now.to_string())],
    )
    .await;
    exec(
        &pool,
        "INSERT INTO post_tags (post_id, tag_id) VALUES (?, ?)",
        &[Some(&ids[2]), Some(&tag_id)],
    )
    .await;
    handle_index_job(&pool, &claimed("post", &ids[2])).await;
    let tags_json = scalar_str(
        &pool,
        "SELECT tags_json FROM search_documents WHERE doc_id = ?",
        &ids[2],
    )
    .await;
    assert!(tags_json.contains("pagetag"), "启用标签必须进入 tags_json");

    // limit=1 分页：第一页 has_more + cursor。
    let (status, page1, _) = get_search(&app, "/api/v1/search?q=pagination&limit=1").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page1["items"].as_array().unwrap().len(), 1);
    assert_eq!(page1["page"]["has_more"], true);
    let cursor = page1["page"]["next_cursor"].as_str().unwrap().to_string();

    // 第二页。
    let (status, page2, _) = get_search(
        &app,
        &format!("/api/v1/search?q=pagination&limit=1&after={cursor}"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
    assert_ne!(
        page1["items"][0]["id"], page2["items"][0]["id"],
        "分页不得重复"
    );

    // 一次取完。
    let (status, all, _) = get_search(&app, "/api/v1/search?q=pagination&limit=5").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(all["items"].as_array().unwrap().len(), 3);

    close_pool(&pool).await;
    cleanup(&dir);
}
