//! M04-VISIBILITY-11：等级边界、并发降级、after_reply、paid、封禁与
//! 管理员显式查看的边界测试（HTTP 集成 + evaluate 断言）。
//!
//! 覆盖：
//! 1. level 策略：低于 min_level → 锁定（正文缺失、required_level 暴露）；
//!    达到 min_level → 解锁；
//! 2. 作者降级后编辑 → 422 `visibility_level_exceeds_author`（版本不变）；
//! 3. 封禁账号不获得提升访问（等级/策略照常判定，无特权）；
//! 4. 管理员显式查看（moderator_override）解锁 after_reply（evaluate 断言）；
//! 5. paid grant 边界（购买 grant 解锁、撤销后重锁）——服务层 evaluate 断言；
//!    after_reply 回复 grant 的创建/撤销由 `visibility_grants.rs` 覆盖。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::service::publish_new_post;
use bblbb_backend::content::visibility::evaluate::{
    evaluate, post_grant_key, AccessContent, Actor, DbGrantLookup, EvaluateContext,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::domain::posts::AccessPolicy;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Either;
use tower::ServiceExt;

mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-bound-{}", uuid::Uuid::now_v7()));
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

async fn insert_author(pool: &DatabasePool, tag: &str, level: i64) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(level)
            .bind(now - 25 * 3600 * 1000)
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

/// 发布公开帖并附加 level 策略行（min_level）。
/// 注：当前发布路径（publish_new_post）不携带策略明细（min_level 等），
/// 故先按 public 发布，再直接附加 level 策略行（与线上建策略路径一致）。
async fn publish_level_post(pool: &DatabasePool, author_id: &str, min_level: i64) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: "level 边界主题".to_string(),
            markdown: "正文 level 边界".to_string(),
            board_id: BOARD_ID.to_string(),
            visibility_level: Some(min_level as u32),
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("lv-{}-{}", min_level, uuid::Uuid::now_v7().simple()),
        },
        5,
        now_millis(),
    )
    .unwrap();
    let published = publish_new_post(pool, &cmd, author_id, now_millis())
        .await
        .unwrap();
    let post_id = published.post.id;

    let policy_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO content_access_policies
                 (id, kind, min_level, currency_id, amount, reply_grant_persists, policy_version, created_by, created_at)
                 VALUES (?, 'level', ?, NULL, NULL, 0, 1, ?, ?)",
            )
            .bind(&policy_id)
            .bind(min_level)
            .bind(author_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query("UPDATE posts SET access_policy_id = ? WHERE id = ?")
                .bind(&policy_id)
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    post_id
}

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn get(app: &axum::Router, uri: &str, cookie: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method("GET").uri(uri);
    if let Some(c) = cookie {
        builder = builder.header("cookie", c);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

/// level 策略边界：低于 min_level 锁定、达到解锁。
#[tokio::test]
async fn level_policy_boundary_locks_below_and_unlocks_at_min() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner_lv", 5).await;
    let low = insert_author(&pool, "low_lv", 3).await;
    let post_id = publish_level_post(&pool, &author, 5).await;

    // 匿名 → 锁定
    let (status, body) = get(&app, &format!("/api/v1/posts/{post_id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["access_summary"]["policy"], "level");
    assert_eq!(body["access_summary"]["required_level"], 5);
    assert_eq!(body["access_summary"]["unlocked"], false);
    assert!(body.get("body_html").is_none(), "低等级必须省略正文");

    // 等级 3 < min_level 5 → 锁定
    let low_cookie = common::direct_session_cookie(&pool, &low).await;
    let (status, body) = get(&app, &format!("/api/v1/posts/{post_id}"), Some(&low_cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["access_summary"]["unlocked"], false);
    assert!(body.get("body_html").is_none());

    // 作者等级 5 == min_level → 解锁
    let author_cookie = common::direct_session_cookie(&pool, &author).await;
    let (status, body) = get(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        Some(&author_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    eprintln!("AUTHOR-GET-BODY: {body}");
    assert_eq!(body["access_summary"]["unlocked"], true);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 作者降级后编辑 → 422 `visibility_level_exceeds_author`（版本不变）。
#[tokio::test]
async fn author_downgrade_then_edit_returns_422() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner_dg", 5).await;
    let post_id = publish_level_post(&pool, &author, 4).await;

    // 降级作者到 3（level 策略 min_level=4 > 3）
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET level = 3 WHERE id = ?")
                .bind(&author)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let cookie = common::direct_session_cookie(&pool, &author).await;
    // 写端点需要 Session CSRF：先取 CSRF token
    let csrf_resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", &cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let csrf_bytes = csrf_resp.into_body().collect().await.unwrap().to_bytes();
    let csrf_body: Value = serde_json::from_slice(&csrf_bytes).unwrap();
    let csrf = csrf_body["token"].as_str().unwrap().to_string();

    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("PATCH")
                .uri(format!("/api/v1/posts/{post_id}"))
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .header("if-match", "1")
                .header("content-type", "application/json")
                .body(Body::from(r#"{"title":"新标题"}"#))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(
        status,
        StatusCode::UNPROCESSABLE_ENTITY,
        "降级后编辑必须 422，实际 {status}: {body}"
    );
    assert_eq!(
        body["code"], "visibility_level_exceeds_author",
        "稳定错误码，实际 {body}"
    );

    // 版本未被破坏（仍是 1）
    match &pool {
        Either::Left(p) => {
            let v: i64 = sqlx::query_scalar("SELECT version FROM posts WHERE id = ?")
                .bind(&post_id)
                .fetch_one(p)
                .await
                .unwrap();
            assert_eq!(v, 1, "阻断必须发生在任何写之前（版本不变）");
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 封禁账号不获得提升访问（等级策略照常判定）。
#[tokio::test]
async fn banned_actor_gets_no_elevated_access() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "owner_bn", 5).await;
    let banned = insert_author(&pool, "banned_bn", 3).await;
    let post_id = publish_level_post(&pool, &author, 5).await;

    // 封禁账号（level 3）→ 与普通低等级一致，仍锁定
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET status = 'banned' WHERE id = ?")
                .bind(&banned)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let banned_cookie = common::direct_session_cookie(&pool, &banned).await;
    let (status, body) = get(
        &app,
        &format!("/api/v1/posts/{post_id}"),
        Some(&banned_cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["access_summary"]["unlocked"], false);
    assert!(body.get("body_html").is_none(), "封禁账号不得获得提升访问");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 管理员显式查看（moderator_override）解锁 after_reply（evaluate 断言）。
#[tokio::test]
async fn moderator_override_unlocks_after_reply() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "owner_mo", 5).await;
    let moderator = insert_author(&pool, "mod_mo", 5).await;
    let post_id = publish_level_post(&pool, &author, 1).await;

    let key = post_grant_key(&post_id);
    let lookup = DbGrantLookup { pool: &pool };
    let content = AccessContent {
        grant_target_key: Some(&key),
        author_id: Some(&author),
        policy: AccessPolicy::AfterReply,
        min_level: None,
        visibility_level: 1,
        author_level: 5,
    };
    let actor = Actor {
        id: &moderator,
        level: 5,
        username: "mod_mo_x",
    };
    // 无 grant + 无 override → 锁定
    let locked = evaluate(
        Some(&actor),
        &content,
        &EvaluateContext {
            grants: &lookup,
            now: now_millis(),
            moderator_override: false,
        },
    )
    .await;
    assert!(!locked.unlocked, "无 grant 且非作者必须锁定");

    // 管理 override → 解锁
    let unlocked = evaluate(
        Some(&actor),
        &content,
        &EvaluateContext {
            grants: &lookup,
            now: now_millis(),
            moderator_override: true,
        },
    )
    .await;
    assert!(unlocked.unlocked, "管理 override 必须解锁 after_reply");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// paid grant 边界：购买 grant 解锁、撤销后重锁（服务层 evaluate 断言；
/// grant 创建/扣款属 M7，本测试直接插行）。
#[tokio::test]
async fn paid_grant_unlocks_then_revocation_relocks() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "owner_pd", 5).await;
    let buyer = insert_author(&pool, "buyer_pd", 1).await;
    let post_id = publish_level_post(&pool, &author, 1).await;

    let key = post_grant_key(&post_id);
    let content = AccessContent {
        grant_target_key: Some(&key),
        author_id: Some(&author),
        policy: AccessPolicy::Paid,
        min_level: None,
        visibility_level: 1,
        author_level: 5,
    };
    let actor = Actor {
        id: &buyer,
        level: 1,
        username: "buyer_pd_x",
    };

    // 无 purchase grant → 锁定（paid 只认 grant）
    let lookup = DbGrantLookup { pool: &pool };
    let g = evaluate(
        Some(&actor),
        &content,
        &EvaluateContext {
            grants: &lookup,
            now: now_millis(),
            moderator_override: false,
        },
    )
    .await;
    assert!(!g.unlocked, "无 purchase grant 必须锁定");

    // 插入有效 purchase grant → 解锁（policy_id 引用主题的 paid 策略行）
    let now = now_millis();
    let policy_id = uuid::Uuid::now_v7().to_string();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO content_access_policies
                 (id, kind, min_level, currency_id, amount, reply_grant_persists, policy_version, created_by, created_at)
                 VALUES (?, 'paid', NULL, 'coin', 100, 0, 1, ?, ?)",
            )
            .bind(&policy_id)
            .bind(&author)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query("UPDATE posts SET access_policy_id = ? WHERE id = ?")
                .bind(&policy_id)
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let grant_id = uuid::Uuid::now_v7().to_string();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO content_access_grants
                 (id, user_id, post_id, comment_id, policy_id, source_kind, source_id, point_operation_id, grant_target_key, granted_at, revoked_at)
                 VALUES (?, ?, ?, NULL, ?, 'purchase', 'purchase-test', NULL, ?, ?, NULL)",
            )
            .bind(&grant_id)
            .bind(&buyer)
            .bind(&post_id)
            .bind(&policy_id)
            .bind(&key)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let lookup = DbGrantLookup { pool: &pool };
    let g = evaluate(
        Some(&actor),
        &content,
        &EvaluateContext {
            grants: &lookup,
            now: now_millis(),
            moderator_override: false,
        },
    )
    .await;
    assert!(g.unlocked, "有效 purchase grant 必须解锁 paid");

    // 撤销（revoked_at 置位）→ 重新锁定
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE content_access_grants SET revoked_at = ? WHERE id = ?")
                .bind(now)
                .bind(&grant_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let lookup = DbGrantLookup { pool: &pool };
    let g = evaluate(
        Some(&actor),
        &content,
        &EvaluateContext {
            grants: &lookup,
            now: now_millis(),
            moderator_override: false,
        },
    )
    .await;
    assert!(!g.unlocked, "撤销后的 grant 必须重锁 paid 内容");

    close_pool(&pool).await;
    cleanup(&dir);
}
