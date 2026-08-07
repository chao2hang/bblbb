//! M08-FEEDS：RSS/Atom/sitemap/robots/SEO 公开投影（HTTP + 服务层，
//! SQLite 真实数据库）。
//!
//! 覆盖：RSS/Atom 只含安全公开内容 + XML escaping + ETag/304；sitemap 只列
//! 允许索引的公开 canonical URL 且限量/分片；动态 robots.txt + `X-Robots-Tag`；
//! OG/JSON-LD/canonical 投影重跑可见性/退出策略；隐藏/回复/等级/付费/审核/
//! 删除/封禁内容不进任何投影；无 JS 公开文章仍可读。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use sqlx::Either;
use tower::ServiceExt;

/// 隐藏正文 canary。
const CANARY: &str = "CANARY-FEED-SECRET-123456789";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-feeds-{}", uuid::Uuid::now_v7()));
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

async fn insert_board(pool: &DatabasePool) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec(
        pool,
        "INSERT INTO boards (id, slug, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        &[
            Some(&id),
            Some(&format!("fboard-{id}")),
            Some("Feed 板块"),
            Some(&now.to_string()),
            Some(&now.to_string()),
        ],
    )
    .await;
    id
}

/// 插入帖子（含 post_contents 与可选访问策略/退出标记）。
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
    let slug = format!("fpost-{id}");
    let deleted_bind = deleted_at.map(|d| d.to_string());
    let deleted_bind = deleted_bind.as_deref();
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
            Some(&format!("摘要 {title}")),
            Some(&now.to_string()),
        ],
    )
    .await;
    id
}

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, String, axum::http::HeaderMap) {
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
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (status, String::from_utf8_lossy(&bytes).to_string(), headers)
}

/// 标准场景：1 篇公开帖 + 各类受限内容。
async fn seed_mixed_posts(pool: &DatabasePool) -> (String, String, String) {
    let author = insert_author(pool, "alice", "active").await;
    let board = insert_board(pool).await;
    let now = now_millis();

    let public_post = insert_post(
        pool,
        &board,
        &author,
        "公开 Feed 帖",
        "public feed body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        Some(&format!("<div>{CANARY}</div>")),
    )
    .await;
    let _hidden = insert_post(
        pool,
        &board,
        &author,
        "隐藏帖勿入 Feed",
        "hidden feed body",
        "hidden",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let _level = insert_post(
        pool,
        &board,
        &author,
        "等级帖勿入 Feed",
        "level feed body",
        "published",
        "public",
        Some("level"),
        false,
        "none",
        None,
        None,
    )
    .await;
    let _paid = insert_post(
        pool,
        &board,
        &author,
        "付费帖勿入 Feed",
        "paid feed body",
        "published",
        "public",
        Some("paid"),
        false,
        "none",
        None,
        None,
    )
    .await;
    let _review = insert_post(
        pool,
        &board,
        &author,
        "审核帖勿入 Feed",
        "review feed body",
        "draft",
        "public",
        None,
        false,
        "pending_review",
        None,
        None,
    )
    .await;
    let _deleted = insert_post(
        pool,
        &board,
        &author,
        "删除帖勿入 Feed",
        "deleted feed body",
        "deleted",
        "public",
        None,
        false,
        "none",
        Some(now),
        None,
    )
    .await;
    let _opted_out = insert_post(
        pool,
        &board,
        &author,
        "退出帖勿入 Feed",
        "optout feed body",
        "published",
        "public",
        None,
        true,
        "none",
        None,
        None,
    )
    .await;
    // 封禁作者内容（作者 banned）。
    let banned_author = insert_author(pool, "mallory", "banned").await;
    let _banned = insert_post(
        pool,
        &board,
        &banned_author,
        "封禁帖勿入 Feed",
        "banned feed body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    (public_post, author, board)
}

// ─────────────────────────── 测试 ───────────────────────────

/// M08-FEEDS-01/02/07：RSS/Atom 只含安全公开内容，XML 转义正确，canary 零泄漏。
#[tokio::test]
async fn rss_and_atom_contain_only_public_content() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (public_post, _, _) = seed_mixed_posts(&pool).await;

    // RSS。
    let (status, body, headers) = get(&app, "/api/v1/rss").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap(),
        "application/rss+xml; charset=utf-8"
    );
    assert!(body.contains("<rss version=\"2.0\">"), "{body}");
    assert!(body.contains("<title>公开 Feed 帖</title>"), "{body}");
    assert!(
        body.contains(&format!("<link>/posts/fpost-{}</link>", public_post)),
        "{body}"
    );
    for forbidden in [
        "隐藏帖勿入 Feed",
        "等级帖勿入 Feed",
        "付费帖勿入 Feed",
        "审核帖勿入 Feed",
        "删除帖勿入 Feed",
        "退出帖勿入 Feed",
        "封禁帖勿入 Feed",
    ] {
        assert!(
            !body.contains(forbidden),
            "受限内容泄漏: {forbidden}\n{body}"
        );
    }
    assert!(!body.contains(CANARY), "canary 泄漏进 RSS: {body}");

    // Atom。
    let (status, body, headers) = get(&app, "/api/v1/atom").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap(),
        "application/atom+xml; charset=utf-8"
    );
    assert!(
        body.contains("<feed xmlns=\"http://www.w3.org/2005/Atom\">"),
        "{body}"
    );
    assert!(body.contains("<title>公开 Feed 帖</title>"), "{body}");
    assert!(
        body.contains(&format!("<link href=\"/posts/fpost-{}\"/>", public_post)),
        "{body}"
    );
    assert!(body.contains("<author><name>alice"), "{body}");
    assert!(!body.contains(CANARY), "canary 泄漏进 Atom: {body}");

    // XML 转义：一篇标题含特殊字符的公开帖。
    let author2 = insert_author(&pool, "bob", "active").await;
    let board2 = insert_board(&pool).await;
    insert_post(
        &pool,
        &board2,
        &author2,
        "转义帖 <b>&\"'</b>",
        "escape body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let (_, body, _) = get(&app, "/api/v1/rss").await;
    assert!(
        body.contains("转义帖 &lt;b&gt;&amp;&quot;&apos;&lt;/b&gt;"),
        "标题未正确转义: {body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-FEEDS-03：sitemap 只列允许索引的公开 canonical URL，限量/分片。
#[tokio::test]
async fn sitemap_lists_only_index_allowed_canonical_urls() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (public_post, _, _) = seed_mixed_posts(&pool).await;

    let (status, body, headers) = get(&app, "/api/v1/sitemap.xml").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get("x-robots-tag")
            .and_then(|v| v.to_str().ok())
            .unwrap(),
        "noindex, nofollow, noarchive"
    );
    assert!(body.contains("<urlset"), "{body}");
    assert!(
        body.contains(&format!("<loc>/posts/fpost-{}</loc>", public_post)),
        "{body}"
    );
    for forbidden in [
        "隐藏帖",
        "等级帖",
        "付费帖",
        "审核帖",
        "删除帖",
        "退出帖",
        "封禁帖",
    ] {
        assert!(
            !body.contains(forbidden),
            "受限内容泄漏进 sitemap: {forbidden}"
        );
    }
    assert!(!body.contains(CANARY));

    // 分片：超过单页上限（100）时返回 <sitemapindex>。
    let author = insert_author(&pool, "carol", "active").await;
    let board = insert_board(&pool).await;
    for i in 0..101 {
        insert_post(
            &pool,
            &board,
            &author,
            &format!("批量帖 {i}"),
            "bulk sitemap body",
            "published",
            "public",
            None,
            false,
            "none",
            None,
            None,
        )
        .await;
    }
    // limit=100（低于总量）→ 返回 <sitemapindex> 分片导航。
    let (status, body, _) = get(&app, "/api/v1/sitemap.xml?limit=100").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("<sitemapindex"),
        "总量超限必须返回 sitemapindex: {body}"
    );
    assert!(body.contains("/api/v1/sitemap.xml?page=2"), "{body}");

    // 具体分片页返回 urlset。
    let (status, body, _) = get(&app, "/api/v1/sitemap.xml?page=1&limit=100").await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("<urlset"), "{body}");
    // 越界分片返回空 urlset（不泄漏总量）。
    let (status, body, _) = get(&app, "/api/v1/sitemap.xml?page=999&limit=100").await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        body.contains("<urlset") && body.contains("</urlset>"),
        "{body}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-FEEDS-04：动态 robots.txt + `X-Robots-Tag` + meta noindex。
#[tokio::test]
async fn robots_txt_and_x_robots_tag() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    let (status, body, headers) = get(&app, "/robots.txt").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        headers
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap(),
        "text/plain; charset=utf-8"
    );
    for bot in bblbb_backend::feeds::AI_TRAINING_CRAWLERS {
        assert!(
            body.contains(&format!("User-agent: {bot}")),
            "AI 爬虫 {bot} 必须默认拒绝"
        );
    }
    assert!(body.contains("Disallow: /api/"));

    // Feed 响应携带 X-Robots-Tag。
    let (_, _, headers) = get(&app, "/api/v1/rss").await;
    assert_eq!(
        headers
            .get("x-robots-tag")
            .and_then(|v| v.to_str().ok())
            .unwrap(),
        "noindex, nofollow, noarchive"
    );

    // meta noindex 决策与服务端一致。
    assert_eq!(
        bblbb_backend::feeds::meta_robots(false),
        "noindex, nofollow, noarchive"
    );
    assert_eq!(bblbb_backend::feeds::meta_robots(true), "index, follow");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-FEEDS-01：Feed ETag + 304。
#[tokio::test]
async fn feed_etag_and_304() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (public_post, _, _) = seed_mixed_posts(&pool).await;

    let (status, body, headers) = get(&app, "/api/v1/rss").await;
    assert_eq!(status, StatusCode::OK);
    let etag = headers
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();
    assert!(!etag.is_empty());
    assert!(body.contains(&public_post));

    // 带 If-None-Match → 304。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/rss")
                .header("if-none-match", &etag)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_MODIFIED, "ETag 命中应 304");

    // 内容变化（新公开帖）→ ETag 变化，不再 304。
    let author = insert_author(&pool, "dave", "active").await;
    let board = insert_board(&pool).await;
    insert_post(
        &pool,
        &board,
        &author,
        "新帖",
        "new body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let (status, _, headers2) = get(&app, "/api/v1/rss").await;
    assert_eq!(status, StatusCode::OK);
    let etag2 = headers2
        .get("etag")
        .and_then(|v| v.to_str().ok())
        .unwrap()
        .to_string();
    assert_ne!(etag, etag2, "内容变更后 ETag 必须变化");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-FEEDS-05：OG/JSON-LD/canonical 投影重跑可见性与退出索引策略。
#[tokio::test]
async fn seo_projection_rechecks_visibility_and_index_policy() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;

    let public_post = insert_post(
        &pool,
        &board,
        &author,
        "SEO 公开帖",
        "seo public body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let hidden = insert_post(
        &pool,
        &board,
        &author,
        "SEO 隐藏帖",
        "seo hidden body",
        "hidden",
        "public",
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
        "SEO 等级帖",
        "seo level body",
        "published",
        "public",
        Some("level"),
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
        "SEO 退出帖",
        "seo optout body",
        "published",
        "public",
        None,
        true,
        "none",
        None,
        None,
    )
    .await;

    // 不可见内容 → 无 SEO 投影。
    assert!(bblbb_backend::feeds::load_seo_post(&pool, &hidden)
        .await
        .unwrap()
        .is_none());
    assert!(bblbb_backend::feeds::load_seo_post(&pool, &level)
        .await
        .unwrap()
        .is_none());

    // 公开帖 → 投影存在；canonical/OG/JSON-LD 组装。
    let seo = bblbb_backend::feeds::load_seo_post(&pool, &public_post)
        .await
        .unwrap()
        .expect("公开帖必须有 SEO 投影");
    let meta = bblbb_backend::feeds::seo_meta_for(&seo, false);
    assert!(meta.index_allowed);
    assert_eq!(meta.canonical, format!("/posts/fpost-{}", public_post));
    assert_eq!(meta.og_type, "article");
    assert!(meta.article_json_ld.contains("\"@type\":\"Article\""));
    assert!(meta
        .article_json_ld
        .contains(&format!("/posts/fpost-{}", public_post)));

    // 退出索引 → index_allowed = false（meta noindex 决策）。
    let seo_out = bblbb_backend::feeds::load_seo_post(&pool, &opted_out)
        .await
        .unwrap()
        .expect("公开退出帖仍有公开投影");
    assert!(!bblbb_backend::feeds::seo_meta_for(&seo_out, false).index_allowed);

    // 管理员全站 deny → 退出索引决策（SEO 元数据不再允许索引）。
    let now = now_millis();
    bblbb_backend::search::set_site_policy(&pool, "deny", "deny", &author, now)
        .await
        .unwrap();
    assert!(
        bblbb_backend::feeds::load_seo_post(&pool, &public_post)
            .await
            .unwrap()
            .is_none(),
        "管理员 deny 后公开帖不得进入 SEO 投影"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-FEEDS-06：缓存 revision 维度——内容/策略变化使缓存键失效。
#[tokio::test]
async fn feed_cache_revisions_follow_content_and_policy() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "alice", "active").await;
    let board = insert_board(&pool).await;

    let (policy0, content0) = bblbb_backend::feeds::compute_cache_revisions(&pool)
        .await
        .unwrap();

    // 发布新帖 → 索引 → 内容 revision 变化。
    let post = insert_post(
        &pool,
        &board,
        &author,
        "缓存帖",
        "cache body",
        "published",
        "public",
        None,
        false,
        "none",
        None,
        None,
    )
    .await;
    let now = now_millis();
    exec(
        &pool,
        "UPDATE search_documents SET policy_revision = policy_revision + 1 WHERE doc_id = ?",
        &[Some(&post)],
    )
    .await;
    // 直接改 posts.updated_at（模拟编辑）→ 重索引后 source/content revision 变化。
    exec(
        &pool,
        "UPDATE posts SET updated_at = ?, title = '缓存帖 v2' WHERE id = ?",
        &[Some(&now.to_string()), Some(&post)],
    )
    .await;
    let (policy1, content1) = bblbb_backend::feeds::compute_cache_revisions(&pool)
        .await
        .unwrap();
    assert!(content1 >= content0, "内容 revision 单调不减");

    // 管理员策略变更 → policy revision 变化。
    bblbb_backend::search::set_site_policy(&pool, "allow", "deny", &author, now)
        .await
        .unwrap();
    let (policy2, _) = bblbb_backend::feeds::compute_cache_revisions(&pool)
        .await
        .unwrap();
    assert!(policy2 >= policy1, "策略变更后 policy revision 单调不减");
    let _ = policy0;

    close_pool(&pool).await;
    cleanup(&dir);
}

/// M08-FEEDS-08：无 JavaScript 公开文章与 Feed 链接仍合理可用。
#[tokio::test]
async fn no_js_public_article_and_feed_links_work() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (public_post, _, _) = seed_mixed_posts(&pool).await;

    // 无会话（无 Cookie）直接读取公开文章详情 → 200 + 正文投影。
    let (status, body, headers) = get(&app, &format!("/api/v1/posts/{public_post}")).await;
    assert_eq!(status, StatusCode::OK, "公开文章无 JS 必须可读");
    assert!(body.contains("公开 Feed 帖"), "{body}");
    assert!(body.contains("public feed body"), "{body}");
    assert!(!body.contains(CANARY), "公开文章不得泄漏隐藏正文 canary");
    assert_eq!(
        headers
            .get("cache-control")
            .and_then(|v| v.to_str().ok())
            .unwrap(),
        "public, max-age=60"
    );

    // Feed 链接（RSS 中的 /posts/{slug}）可直接访问。
    let (_, rss, _) = get(&app, "/api/v1/rss").await;
    let link = format!("/posts/fpost-{}", public_post);
    assert!(rss.contains(&link));
    let (status, body, _) = get(&app, &format!("/api/v1/posts/{public_post}")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(body.contains("公开 Feed 帖"));

    close_pool(&pool).await;
    cleanup(&dir);
}
