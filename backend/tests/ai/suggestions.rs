//! M09-SUGGESTIONS 集成测试：模型输出解析/校验、建议落库幂等、读取鉴权、
//! 采纳（base_revision 防旧覆盖新 + If-Match 幂等）（SQLite 真实数据库）。

use std::path::{Path, PathBuf};

use bblbb_backend::ai::suggestions::get_suggestion;
use bblbb_backend::ai::{
    accept_suggestion, create_suggestion, enqueue_task, parse_suggestion_payload,
    validate_suggestion, SuggestionError, SuggestionKind, TaskKind,
};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use serde_json::{json, Value};
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-aisug-{}", uuid::Uuid::now_v7()));
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
                    None => q = q.bind(None::<String>),
                }
            }
            q.execute(p).await.unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec(
        pool,
        "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level,
            email_verified, email_verified_at, created_at, updated_at)
         VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
        &[
            Some(&id),
            Some(&format!("{tag}_{}", &id[..8])),
            Some(&format!("{tag}_{}@example.com", &id[..8])),
            Some(&(now - 30 * 24 * 3600 * 1000).to_string()),
            Some(&now.to_string()),
            Some(&now.to_string()),
        ],
    )
    .await;
    id
}

async fn insert_provider(pool: &DatabasePool, name: &str) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec(
        pool,
        "INSERT INTO ai_providers
            (id, name, adapter_type, base_url, api_type, default_model, status, secret_configured, data_mode,
             timeout_ms, max_input_tokens, max_output_tokens, max_concurrency, version, created_at, updated_at)
         VALUES (?, ?, 'openai_compatible', 'https://api.mock.example/v1', 'chat', 'mock-model', 'enabled', 0,
             'redacted', 15000, 8000, 2000, 4, 1, ?, ?)",
        &[
            Some(&id),
            Some(name),
            Some(&now.to_string()),
            Some(&now.to_string()),
        ],
    )
    .await;
    id
}

/// 建一个 formatting 任务（建议的父任务）。
async fn make_task(
    pool: &DatabasePool,
    user: &str,
    provider: &str,
    revision: i64,
    key: &str,
) -> String {
    let now = now_millis();
    enqueue_task(
        pool,
        TaskKind::Formatting,
        "draft",
        "draft-1",
        user,
        provider,
        revision,
        1,
        None,
        key,
        100,
        now,
    )
    .await
    .unwrap()
    .id
}

fn formatting_payload() -> Value {
    json!({ "content": "## 新标题\n\n正文", "changes": ["标题层级"] })
}

#[tokio::test]
async fn create_and_read_suggestion_roundtrip() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();
    let task_id = make_task(&pool, &user, &provider, 3, "key-roundtrip").await;

    let payload = formatting_payload();
    let created = create_suggestion(
        &pool,
        &task_id,
        SuggestionKind::Formatting,
        "draft",
        "draft-1",
        &user,
        3,
        &payload,
        now,
    )
    .await
    .unwrap();
    assert_eq!(created["decision"], "pending");
    assert_eq!(created["payload"]["content"], "## 新标题\n\n正文");

    let id = created["id"].as_str().unwrap().to_string();
    let read = get_suggestion(&pool, &user, &id).await.unwrap();
    assert_eq!(read["decision"], "pending");
    assert_eq!(read["suggestion_type"], "formatting");
    assert_eq!(read["base_revision"], 3);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_is_idempotent_for_same_task() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();
    let task_id = make_task(&pool, &user, &provider, 3, "key-idem").await;

    let payload = formatting_payload();
    let a = create_suggestion(
        &pool,
        &task_id,
        SuggestionKind::Formatting,
        "draft",
        "draft-1",
        &user,
        3,
        &payload,
        now,
    )
    .await
    .unwrap();
    let b = create_suggestion(
        &pool,
        &task_id,
        SuggestionKind::Formatting,
        "draft",
        "draft-1",
        &user,
        3,
        &payload,
        now,
    )
    .await
    .unwrap();
    assert_eq!(a["id"], b["id"], "同 task 幂等返回既有建议");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn read_is_forbidden_for_non_author() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "a").await;
    let other = insert_user(&pool, "b").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();
    let task_id = make_task(&pool, &owner, &provider, 2, "key-authz").await;

    let created = create_suggestion(
        &pool,
        &task_id,
        SuggestionKind::Seo,
        "post",
        "post-1",
        &owner,
        2,
        &json!({ "title": "优化标题", "summary": "摘要" }),
        now,
    )
    .await
    .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    assert!(matches!(
        get_suggestion(&pool, &other, &id).await,
        Err(SuggestionError::Forbidden(_))
    ));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn accept_checks_base_revision_then_is_idempotent() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();
    let task_id = make_task(&pool, &user, &provider, 5, "key-accept").await;

    let created = create_suggestion(
        &pool,
        &task_id,
        SuggestionKind::Formatting,
        "draft",
        "draft-1",
        &user,
        5,
        &formatting_payload(),
        now,
    )
    .await
    .unwrap();
    let id = created["id"].as_str().unwrap().to_string();

    // 版本不匹配（If-Match 陈旧）→ VersionConflict。
    assert!(matches!(
        accept_suggestion(&pool, &user, &id, 4, 4, None, now).await,
        Err(SuggestionError::VersionConflict { .. })
    ));

    // 正确版本采纳。
    let accepted = accept_suggestion(&pool, &user, &id, 6, 6, None, now)
        .await
        .unwrap();
    assert_eq!(accepted["decision"], "accepted");

    // 重复采纳 → AlreadyAccepted（幂等）。
    assert!(matches!(
        accept_suggestion(&pool, &user, &id, 6, 6, None, now).await,
        Err(SuggestionError::AlreadyAccepted)
    ));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[test]
fn parse_validation_blocks_injection() {
    assert!(parse_suggestion_payload(
        "{\"content\":\"## 新标题\\n\\n正文\",\"changes\":[\"标题层级\"]}",
        SuggestionKind::Formatting,
    )
    .is_ok());
    assert!(matches!(
        parse_suggestion_payload(
            r#"{"content":"<script>alert(1)</script>"}"#,
            SuggestionKind::Formatting
        ),
        Err(SuggestionError::Invalid(_))
    ));
    assert!(matches!(
        parse_suggestion_payload("not json", SuggestionKind::Formatting),
        Err(SuggestionError::Invalid(_))
    ));
    assert!(matches!(
        parse_suggestion_payload(r#"{"tags":"not-array"}"#, SuggestionKind::Tagging),
        Err(SuggestionError::Invalid(_))
    ));
    assert!(matches!(
        parse_suggestion_payload(r#"{"score":5}"#, SuggestionKind::Moderation),
        Err(SuggestionError::Invalid(_))
    ));
    assert!(validate_suggestion(&json!({"content": "hello"})).is_ok());
    assert!(validate_suggestion(&json!({"content": "x <iframe>y"})).is_err());
}
