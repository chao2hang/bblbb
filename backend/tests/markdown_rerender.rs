//! M04-MARKDOWN-05：渲染策略版本持久化 + 升级重渲染 Job（SQLite）。
//!
//! - `renderer_version`（组合策略版本）随 post_contents/post_revisions 落库；
//! - 策略升级后 `enqueue_rerender_jobs` 为所有陈旧行入队 `markdown.rerender`
//!   Job（幂等去重）；
//! - `handle_rerender_job` 用当前策略重渲染并覆盖渲染产物；行缺失/已最新 →
//!   幂等成功；无效 payload → 永久死信。

use std::path::{Path, PathBuf};

use bblbb_backend::content::markdown::policy::policy_version;
use bblbb_backend::content::markdown::rerender::{
    enqueue_rerender_jobs, handle_rerender_job, RERENDER_JOB_KIND,
};
use bblbb_backend::content::model::{Post, PostContent, PostRevision, PostStatus, PostType};
use bblbb_backend::content::repository::{
    get_post_revision, insert_post, insert_post_revision, load_post_content, save_post_content,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::retry::RetryClass;
use bblbb_backend::jobs::worker::ClaimedJob;
use bblbb_backend::jobs::worker_loop::JobOutcome;
use bblbb_backend::outbox::now_millis;
use serde_json::{json, Value};
use sqlx::Either;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general' (0005→0006 归一化)
const OLD_VERSION: &str = "markdown-v0+ammonia-v0";

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-rer-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
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

async fn insert_user(pool: &DatabasePool) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("u_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("u_{}@example.com", uuid::Uuid::now_v7().simple()))
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

fn post(id: &str, board_id: &str, author_id: &str) -> Post {
    let now = now_millis();
    Post {
        id: id.to_string(),
        board_id: board_id.to_string(),
        author_id: author_id.to_string(),
        post_type: PostType::Article,
        slug: Some(id.to_string()),
        title: format!("post {id}"),
        status: PostStatus::Published,
        version: 1,
        scheduled_at: None,
        published_at: Some(now),
        pinned_at: None,
        featured_at: None,
        closed_at: None,
        canonical_url: None,
        seo_title: None,
        seo_description: None,
        view_count: 0,
        reply_count: 0,
        last_reply_id: None,
        last_reply_at: None,
        created_at: now,
        updated_at: now,
        deleted_at: None,
    }
}

fn stale_content(post_id: &str) -> PostContent {
    PostContent {
        post_id: post_id.to_string(),
        body_markdown: "**旧版** 正文".to_string(),
        body_html: "<p>old html</p>".to_string(),
        restricted_markdown: Some("> 受限旧版".to_string()),
        restricted_html: Some("<p>old restricted</p>".to_string()),
        renderer_version: OLD_VERSION.to_string(),
        excerpt: "old excerpt".to_string(),
        updated_at: now_millis(),
    }
}

fn stale_revision(id: &str, post_id: &str, editor_id: &str, version: i64) -> PostRevision {
    PostRevision {
        id: id.to_string(),
        post_id: post_id.to_string(),
        editor_id: editor_id.to_string(),
        body_markdown: "# 修订旧版".to_string(),
        body_html: "<p>old rev html</p>".to_string(),
        restricted_markdown: None,
        restricted_html: None,
        renderer_version: OLD_VERSION.to_string(),
        change_reason: Some("v1".to_string()),
        version,
        created_at: now_millis(),
    }
}

fn claimed(job_id: &str, payload: Value) -> ClaimedJob {
    ClaimedJob {
        id: job_id.to_string(),
        queue: "default".to_string(),
        kind: RERENDER_JOB_KIND.to_string(),
        payload,
        payload_version: 1,
        attempts: 1,
        max_attempts: 5,
        locked_until: now_millis() + 60_000,
    }
}

async fn job_payloads(pool: &DatabasePool) -> Vec<Value> {
    match pool {
        Either::Left(p) => {
            let rows: Vec<(String,)> =
                sqlx::query_as("SELECT payload FROM jobs WHERE kind = ? ORDER BY created_at")
                    .bind(RERENDER_JOB_KIND)
                    .fetch_all(p)
                    .await
                    .unwrap();
            rows.into_iter()
                .map(|(s,)| serde_json::from_str(&s).unwrap())
                .collect()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

#[tokio::test]
async fn upgrade_rerenders_content_and_revisions() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool).await;
    let p1 = post("p1", BOARD_ID, &user);
    let p2 = post("p2", BOARD_ID, &user);
    insert_post(&pool, &p1).await.unwrap();
    insert_post(&pool, &p2).await.unwrap();
    save_post_content(&pool, &stale_content("p1"))
        .await
        .unwrap();
    insert_post_revision(&pool, &stale_revision("r1", "p2", &user, 1))
        .await
        .unwrap();

    // 策略升级：入队 2 个重渲染 Job
    let enqueued = enqueue_rerender_jobs(&pool, 100).await.unwrap();
    assert_eq!(enqueued, 2, "陈旧内容+修订各入队一个");

    // 幂等去重：重复入队不新增
    let again = enqueue_rerender_jobs(&pool, 100).await.unwrap();
    assert_eq!(again, 0, "重复入队必须被 dedup 合并");

    let payloads = job_payloads(&pool).await;
    assert_eq!(payloads.len(), 2);
    for payload in &payloads {
        let outcome = handle_rerender_job(&pool, &claimed("job-x", payload.clone())).await;
        assert!(
            matches!(outcome, JobOutcome::Succeeded),
            "重渲染 Job 必须成功: {payload}"
        );
    }

    // 内容行被覆盖为新策略版本渲染产物
    let content = load_post_content(&pool, "p1").await.unwrap().unwrap();
    assert_eq!(content.renderer_version, policy_version());
    assert!(
        content.body_html.contains("<strong>旧版</strong>"),
        "正文必须用当前策略重渲染: {}",
        content.body_html
    );
    assert!(
        content
            .restricted_html
            .as_deref()
            .unwrap()
            .contains("<blockquote>"),
        "受限正文必须重渲染: {}",
        content.restricted_html.as_deref().unwrap()
    );
    assert!(
        content.excerpt.contains("旧版"),
        "摘要必须重新生成: {}",
        content.excerpt
    );

    // 修订行被覆盖（markdown 快照与元数据不变）
    let rev = get_post_revision(&pool, "r1").await.unwrap().unwrap();
    assert_eq!(rev.renderer_version, policy_version());
    assert!(
        rev.body_html.contains("<h1 id=\"修订旧版\">修订旧版</h1>"),
        "修订必须用当前策略重渲染: {}",
        rev.body_html
    );
    assert_eq!(rev.body_markdown, "# 修订旧版", "markdown 快照不可变");
    assert_eq!(rev.version, 1, "版本号不可变");

    // 全部重渲染后不再有陈旧行
    let after = enqueue_rerender_jobs(&pool, 100).await.unwrap();
    assert_eq!(after, 0, "全部重渲染后不应再有陈旧行");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn handle_rerender_job_is_idempotent_for_missing_row() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    // 行不存在（帖子删除级联）→ 幂等成功
    let outcome = handle_rerender_job(
        &pool,
        &claimed("j1", json!({ "target": "content", "id": "ghost" })),
    )
    .await;
    assert!(
        matches!(outcome, JobOutcome::Succeeded),
        "缺失行必须幂等成功"
    );
    let outcome = handle_rerender_job(
        &pool,
        &claimed("j2", json!({ "target": "revision", "id": "ghost" })),
    )
    .await;
    assert!(
        matches!(outcome, JobOutcome::Succeeded),
        "缺失修订必须幂等成功"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn handle_rerender_job_is_noop_for_current_row() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool).await;
    let p = post("p1", BOARD_ID, &user);
    insert_post(&pool, &p).await.unwrap();
    let content = stale_content("p1");
    // 直接以当前版本写入（相当于已被并发 Job 处理过）
    let mut fresh = content.clone();
    fresh.renderer_version = policy_version();
    save_post_content(&pool, &fresh).await.unwrap();

    let outcome = handle_rerender_job(
        &pool,
        &claimed("j1", json!({ "target": "content", "id": "p1" })),
    )
    .await;
    assert!(matches!(outcome, JobOutcome::Succeeded));
    let after = load_post_content(&pool, "p1").await.unwrap().unwrap();
    assert_eq!(after.body_html, fresh.body_html, "已最新行不得被改写");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn handle_rerender_job_rejects_invalid_payload() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    for payload in [
        json!({}),
        json!({ "target": "bogus", "id": "x" }),
        json!({ "target": "content" }),
        json!({ "target": "content", "id": "" }),
    ] {
        let outcome = handle_rerender_job(&pool, &claimed("j-bad", payload)).await;
        match outcome {
            JobOutcome::Failed { class, .. } => {
                assert_eq!(class, RetryClass::Permanent, "无效 payload 应永久死信")
            }
            _ => panic!("无效 payload 必须失败，不得成功"),
        }
    }
    close_pool(&pool).await;
    cleanup(&dir);
}
