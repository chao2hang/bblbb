//! M09-TASKS 集成测试：幂等入队、任务状态机（queued→running→succeeded/dead/
//! retry_wait/cancelled）、取消、错误分类、consent 重确认（SQLite 真实数据库，
//! mock ProviderClient，无真实网络）。

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use bblbb_backend::ai::gateway::BudgetCounter;
use bblbb_backend::ai::tasks::RetryClass;
use bblbb_backend::ai::{
    cancel_task, classify_error, enqueue_task, execute_task, task_state, EgressPolicy,
    GatewayError, OutboundRequest, OutboundResponse, ProviderClient, TaskError, TaskKind,
};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

/// mock ProviderClient：固定响应或固定错误。
struct MockClient {
    status: Option<u16>,
    body: Option<String>,
    err: Option<GatewayError>,
}

impl MockClient {
    fn ok(status: u16, body: &str) -> Self {
        MockClient {
            status: Some(status),
            body: Some(body.to_string()),
            err: None,
        }
    }
    fn fail(e: GatewayError) -> Self {
        MockClient {
            status: None,
            body: None,
            err: Some(e),
        }
    }
}

impl ProviderClient for MockClient {
    fn post_json(
        &self,
        _req: &OutboundRequest,
    ) -> Pin<Box<dyn Future<Output = Result<OutboundResponse, GatewayError>> + Send + '_>> {
        let result = if let (Some(status), Some(body)) = (&self.status, &self.body) {
            Ok(OutboundResponse {
                status: *status,
                body: body.clone(),
            })
        } else {
            Err(self.err.clone().unwrap())
        };
        Box::pin(async move { result })
    }
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-aitask-{}", uuid::Uuid::now_v7()));
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
                    None => q = q.bind(None::<String>),
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

#[tokio::test]
async fn enqueue_is_idempotent_by_key() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();

    let first = enqueue_task(
        &pool,
        TaskKind::Formatting,
        "draft",
        "draft-1",
        &user,
        &provider,
        3,
        1,
        None,
        "key-1",
        100,
        now,
    )
    .await
    .unwrap();
    assert_eq!(first.status, "queued");
    assert_eq!(first.attempt, 0);

    let second = enqueue_task(
        &pool,
        TaskKind::Formatting,
        "draft",
        "draft-1",
        &user,
        &provider,
        3,
        1,
        None,
        "key-1",
        100,
        now,
    )
    .await
    .unwrap();
    assert_eq!(first.id, second.id, "同 key 必须返回同一任务");

    let count = scalar(&pool, "SELECT COUNT(*) FROM ai_tasks").await;
    assert_eq!(count, 1, "不能重复入队");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn task_state_is_scoped_to_owner() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let owner = insert_user(&pool, "a").await;
    let other = insert_user(&pool, "b").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();

    let task = enqueue_task(
        &pool,
        TaskKind::Seo,
        "post",
        "post-1",
        &owner,
        &provider,
        1,
        1,
        None,
        "key-seo",
        50,
        now,
    )
    .await
    .unwrap();

    // 本人可见。
    let view = task_state(&pool, &owner, &task.id).await.unwrap();
    assert_eq!(view.task_type, "seo");
    // 他人不可见。
    assert!(matches!(
        task_state(&pool, &other, &task.id).await,
        Err(TaskError::NotFound(_))
    ));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn cancel_moves_queued_to_cancelled_and_is_idempotent() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();

    let task = enqueue_task(
        &pool,
        TaskKind::Moderation,
        "post",
        "post-1",
        &user,
        &provider,
        2,
        1,
        None,
        "key-mod",
        100,
        now,
    )
    .await
    .unwrap();

    let cancelled = cancel_task(&pool, &user, &task.id, now).await.unwrap();
    assert_eq!(cancelled.status, "cancelled");
    // 再次取消 → NotFound。
    assert!(matches!(
        cancel_task(&pool, &user, &task.id, now).await,
        Err(TaskError::NotFound(_))
    ));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn execute_success_marks_succeeded() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();

    let task = enqueue_task(
        &pool,
        TaskKind::Formatting,
        "draft",
        "draft-1",
        &user,
        &provider,
        1,
        1,
        None,
        "key-ok",
        100,
        now,
    )
    .await
    .unwrap();

    let client = MockClient::ok(200, "{\"content\":\"ok\"}");
    let policy = EgressPolicy::default();
    execute_task(&pool, &task.id, &policy, &client, now)
        .await
        .unwrap();

    let view = task_state(&pool, &user, &task.id).await.unwrap();
    assert_eq!(view.status, "succeeded");
    assert_eq!(view.attempt, 1);
    // 结果已落库。
    let result =
        sqlx::query_scalar::<_, Option<String>>("SELECT result_json FROM ai_tasks WHERE id = ?")
            .bind(&task.id)
            .fetch_one(match &pool {
                Either::Left(p) => p,
                Either::Right(_) => panic!("SQLite only"),
            })
            .await
            .unwrap();
    assert_eq!(result.as_deref(), Some("{\"content\":\"ok\"}"));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn execute_retries_5xx_then_dead_after_max_attempts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();

    let task = enqueue_task(
        &pool,
        TaskKind::Formatting,
        "draft",
        "draft-1",
        &user,
        &provider,
        1,
        1,
        None,
        "key-retry",
        100,
        now,
    )
    .await
    .unwrap();

    let client = MockClient::fail(GatewayError::Invalid("status 500 upstream".into()));
    let policy = EgressPolicy::default();

    // 第 1 次：queued→running(1)→retry_wait。
    assert!(execute_task(&pool, &task.id, &policy, &client, now)
        .await
        .is_err());
    let v1 = task_state(&pool, &user, &task.id).await.unwrap();
    assert_eq!(v1.status, "retry_wait");
    assert_eq!(v1.attempt, 1);

    // 第 2 次：retry_wait→running(2)→retry_wait。
    assert!(execute_task(&pool, &task.id, &policy, &client, now)
        .await
        .is_err());
    let v2 = task_state(&pool, &user, &task.id).await.unwrap();
    assert_eq!(v2.status, "retry_wait");
    assert_eq!(v2.attempt, 2);

    // 第 3 次：attempt=3 == max → dead。
    assert!(execute_task(&pool, &task.id, &policy, &client, now)
        .await
        .is_err());
    let v3 = task_state(&pool, &user, &task.id).await.unwrap();
    assert_eq!(v3.status, "dead");
    assert_eq!(v3.attempt, 3);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn execute_4xx_marks_dead_immediately() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();

    let task = enqueue_task(
        &pool,
        TaskKind::Formatting,
        "draft",
        "draft-1",
        &user,
        &provider,
        1,
        1,
        None,
        "key-4xx",
        100,
        now,
    )
    .await
    .unwrap();

    let client = MockClient::fail(GatewayError::Invalid("status 400 bad_request".into()));
    let policy = EgressPolicy::default();
    assert!(execute_task(&pool, &task.id, &policy, &client, now)
        .await
        .is_err());

    let view = task_state(&pool, &user, &task.id).await.unwrap();
    assert_eq!(view.status, "dead");
    assert_eq!(view.attempt, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn execute_rechecks_consent_and_blocks_revoked() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let provider = insert_provider(&pool, "mock").await;
    let now = now_millis();

    // 授予 consent。
    exec(
        &pool,
        "INSERT INTO ai_consents
            (id, user_id, provider_id, purpose, data_mode, disclosure_version, disclosure_hash, disclosure_text, scope, granted_at, created_at, updated_at)
         VALUES (?, ?, ?, 'formatting', 'full_with_consent', 1, 'h', 'disclosure text', 'per_task', ?, ?, ?)",
        &[
            Some(&uuid::Uuid::now_v7().to_string()),
            Some(&user),
            Some(&provider),
            Some(&now.to_string()),
            Some(&now.to_string()),
            Some(&now.to_string()),
        ],
    )
    .await;
    let consent_id = sqlx::query_scalar::<_, String>(
        "SELECT id FROM ai_consents WHERE user_id = ? AND provider_id = ?",
    )
    .bind(&user)
    .bind(&provider)
    .fetch_one(match &pool {
        Either::Left(p) => p,
        Either::Right(_) => panic!("SQLite only"),
    })
    .await
    .unwrap();

    let task = enqueue_task(
        &pool,
        TaskKind::Formatting,
        "draft",
        "draft-1",
        &user,
        &provider,
        1,
        1,
        Some(&consent_id),
        "key-consent",
        100,
        now,
    )
    .await
    .unwrap();

    // 撤回 consent。
    exec(
        &pool,
        "UPDATE ai_consents SET revoked_at = ?, revoke_reason = 'user' WHERE id = ?",
        &[Some(&now.to_string()), Some(&consent_id)],
    )
    .await;

    let client = MockClient::ok(200, "{\"content\":\"ok\"}");
    let policy = EgressPolicy::default();
    let err = execute_task(&pool, &task.id, &policy, &client, now).await;
    assert!(matches!(err, Err(TaskError::Stale { .. })));

    let view = task_state(&pool, &user, &task.id).await.unwrap();
    assert_eq!(view.status, "dead");
    assert_eq!(view.error_class.as_deref(), Some("consent_revoked"));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[test]
fn classify_error_maps_retry_and_dead() {
    assert_eq!(
        classify_error(&GatewayError::Timeout("x".into())),
        RetryClass::Retry
    );
    assert_eq!(
        classify_error(&GatewayError::Invalid("status 429 too many".into())),
        RetryClass::Retry
    );
    assert_eq!(
        classify_error(&GatewayError::Invalid("status 503 unavailable".into())),
        RetryClass::Retry
    );
    assert_eq!(
        classify_error(&GatewayError::Invalid("status 400 bad_request".into())),
        RetryClass::Dead
    );
    assert_eq!(
        classify_error(&GatewayError::HostNotAllowed("x".into())),
        RetryClass::Dead
    );
}

#[test]
fn budget_counter_backs_concurrency_limit() {
    let mut b = BudgetCounter::new(100_000, 1);
    assert!(b.reserve(10).is_ok());
    assert!(b.reserve(10).is_err());
    b.release(10);
    assert!(b.reserve(10).is_ok());
}
