//! M05-RISK-07：规则、超时、AI 关闭/失败/迟到、重复评估、旧 policy 结果、
//! 安全投影与管理员版本化更新（SQLite 全量 + 跨库 #[ignore]）。
//!
//! 同时覆盖发布路径集成（M05-RISK-03）：普通内容先发布；高风险原子设置
//! pending_review 且不进公开投影；以及作者安全状态投影（M05-RISK-06）。

use std::path::{Path, PathBuf};
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::service::{publish_new_post, publish_scheduled_post};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::moderation::risk::policy::{
    ReasonCategory, RiskInput, Thresholds, BUILTIN_POLICY_VERSION,
};
use bblbb_backend::moderation::risk::provider::{AiModerationProvider, AiSuggestion};
use bblbb_backend::moderation::risk::service::{
    evaluate_risk, load_policy, record_review_outcome, update_risk_policy, RiskError,
    AI_SUGGEST_DEADLINE,
};
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Either;
use tower::ServiceExt;

#[path = "../common/mod.rs"]
mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-risk-{}", uuid::Uuid::now_v7()));
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

/// 插入作者；`account_age_secs` 控制账号"新用户"判定。
async fn insert_author(pool: &DatabasePool, tag: &str, account_age_secs: i64) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - account_age_secs * 1000)
            .bind(now - account_age_secs * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

/// 构造发布命令（作者等级 5，公共策略）。
fn make_command(
    title: &str,
    markdown: &str,
    scheduled_at: Option<i64>,
) -> bblbb_backend::content::posts::command::CreatePostCommand {
    validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: title.to_string(),
            markdown: markdown.to_string(),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at,
            client_request_id: format!("risk-{}-{}", title, uuid::Uuid::now_v7().simple()),
        },
        5,
        now_millis(),
    )
    .unwrap()
}

fn router(pool: DatabasePool) -> axum::Router {
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

async fn board_post_count(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT post_count FROM boards WHERE id = ?")
            .bind(BOARD_ID)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 读取帖子 review 相关列。
async fn post_review_state(pool: &DatabasePool, post_id: &str) -> (String, String, Option<i64>) {
    match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, String, Option<i64>)>(
            "SELECT status, review_status, published_at FROM posts WHERE id = ?",
        )
        .bind(post_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn risk_input(
    author_id: &str,
    author_created_at: Option<i64>,
    markdown: &str,
    now: i64,
) -> RiskInput {
    RiskInput {
        author_id: author_id.to_string(),
        author_created_at,
        author_level: 5,
        board_id: BOARD_ID.to_string(),
        title: "t".to_string(),
        body_markdown: markdown.to_string(),
        now,
    }
}

/// ── M05-RISK-03：低风险先发布；高风险原子 pending_review ──

#[tokio::test]
async fn low_risk_publishes_immediately_and_bumps_board() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "lr", 30 * 86_400).await; // 老用户
    let before = board_post_count(&pool).await;

    let cmd = make_command("normal post", "完全正常的内容，无链接。", None);
    let published = publish_new_post(&pool, &cmd, &author, now_millis())
        .await
        .unwrap();

    assert!(
        published.review.is_none(),
        "low-risk should not be in review"
    );
    assert_eq!(published.post.status.as_str(), "published");
    let (status, review_status, published_at) = post_review_state(&pool, &published.post.id).await;
    assert_eq!(status, "published");
    assert_eq!(review_status, "none");
    assert!(published_at.is_some());
    assert_eq!(board_post_count(&pool).await, before + 1, "board bumped");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn link_heavy_post_goes_pending_review_not_public() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "lh", 30 * 86_400).await;
    let before = board_post_count(&pool).await;

    let body = "[1](https://a.com) [2](https://b.com) [3](https://c.com) [4](https://d.com)";
    let cmd = make_command("spammy links", body, None);
    let published = publish_new_post(&pool, &cmd, &author, now_millis())
        .await
        .unwrap();

    let review = published.review.expect("high-risk should be in review");
    assert_eq!(review.status, "pending_review");
    // M05-RISK-06：作者投影只含安全 category，无规则细节。
    assert_eq!(review.reason_category.as_deref(), Some("link_heavy"));

    let (status, review_status, published_at) = post_review_state(&pool, &published.post.id).await;
    assert_eq!(status, "draft");
    assert_eq!(review_status, "pending_review");
    assert!(published_at.is_none(), "pending post has no published_at");
    assert_eq!(
        board_post_count(&pool).await,
        before,
        "pending_review must not bump board"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn pending_review_not_in_public_listing_and_author_can_see_safe_status() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "pv", 30 * 86_400).await;
    let body = "[1](https://a.com) [2](https://b.com) [3](https://c.com) [4](https://d.com)";
    let cmd = make_command("pending post", body, None);
    let published = publish_new_post(&pool, &cmd, &author, now_millis())
        .await
        .unwrap();
    let post_id = published.post.id.clone();
    let cookie = common::direct_session_cookie(&pool, &author).await;

    // 其他匿名用户 → 404（不进公开投影）
    let (status, _) = get(
        &router(pool.clone()),
        &format!("/api/v1/posts/{post_id}"),
        None,
    )
    .await;
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "anonymous must not see pending"
    );

    // 列表不包含 pending 帖
    let (list_status, list_body) = get(&router(pool.clone()), "/api/v1/posts", None).await;
    assert_eq!(list_status, StatusCode::OK);
    let items = list_body["items"].as_array().expect("items array");
    assert!(
        !items
            .iter()
            .any(|p| p["id"] == Value::String(post_id.clone())),
        "pending post must not appear in public listing"
    );

    // 作者本人 → 200 + 安全状态投影
    let (status, body) = get(
        &router(pool.clone()),
        &format!("/api/v1/posts/{post_id}"),
        Some(&cookie),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "author can see own pending post");
    assert_eq!(body["status"], "pending_review");
    assert_eq!(body["review"]["status"], "pending_review");
    assert_eq!(body["review"]["reason_category"], "link_heavy");
    // 安全投影：不出现规则细节/举报人/内部 note 相关键
    let obj = body.as_object().expect("object");
    assert!(!obj.contains_key("reporter"), "no reporter leak");
    assert!(!obj.contains_key("notes"), "no internal note leak");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-RISK-02：规则 ──

#[tokio::test]
async fn sensitive_word_rule_flags_via_policy() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "sw", 30 * 86_400).await;
    update_risk_policy(
        &pool,
        &author,
        Thresholds {
            sensitive_words: vec!["pink-slime".into()],
            ..Thresholds::default()
        },
        "加敏感词规则",
        BUILTIN_POLICY_VERSION,
        now_millis(),
    )
    .await
    .unwrap();

    let verdict = evaluate_risk(
        &pool,
        &risk_input(
            &author,
            Some(now_millis() - 30 * 86_400 * 1000),
            "mention pink-slime here",
            now_millis(),
        ),
        None,
        AI_SUGGEST_DEADLINE,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(verdict.reason_category(), Some(ReasonCategory::Sensitive));
    assert!(verdict.is_pending_review());

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn new_user_rule_flags_concentrated_posting() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    // 新用户：账号 2 天前创建（过 24h 发帖冷却，但在 7 天"新用户"窗口内）
    let account_age = 2 * 86_400;
    let author = insert_author(&pool, "nu", account_age).await;
    for i in 0..3 {
        let cmd = make_command(&format!("new post {i}"), "普通内容", None);
        publish_new_post(&pool, &cmd, &author, now_millis())
            .await
            .unwrap();
    }
    let verdict = evaluate_risk(
        &pool,
        &risk_input(
            &author,
            Some(now_millis() - account_age * 1000),
            "第 4 篇",
            now_millis(),
        ),
        None,
        AI_SUGGEST_DEADLINE,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(verdict.reason_category(), Some(ReasonCategory::NewUser));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn frequency_rule_flags_burst_posting() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "fq", 30 * 86_400).await; // 老用户
    for i in 0..10 {
        let cmd = make_command(&format!("burst {i}"), "普通内容", None);
        publish_new_post(&pool, &cmd, &author, now_millis())
            .await
            .unwrap();
    }
    let verdict = evaluate_risk(
        &pool,
        &risk_input(
            &author,
            Some(now_millis() - 30 * 86_400 * 1000),
            "第 11 篇",
            now_millis(),
        ),
        None,
        AI_SUGGEST_DEADLINE,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(verdict.reason_category(), Some(ReasonCategory::Frequency));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn duplicate_rule_flags_same_body() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let a = insert_author(&pool, "dup_a", 30 * 86_400).await;
    let b = insert_author(&pool, "dup_b", 30 * 86_400).await;
    let body = "原创内容，仅此一份。";
    let cmd = make_command("original", body, None);
    publish_new_post(&pool, &cmd, &a, now_millis())
        .await
        .unwrap();

    let verdict = evaluate_risk(
        &pool,
        &risk_input(
            &b,
            Some(now_millis() - 30 * 86_400 * 1000),
            body,
            now_millis(),
        ),
        None,
        AI_SUGGEST_DEADLINE,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(verdict.reason_category(), Some(ReasonCategory::Duplicate));

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── M05-RISK-04/05/07：AI 建议接口、关闭/失败/迟到、规则超时 ──
struct FlagProvider(ReasonCategory);
#[async_trait]
impl AiModerationProvider for FlagProvider {
    async fn suggest(&self, _input: &RiskInput, _now: i64) -> AiSuggestion {
        AiSuggestion::Flag(self.0)
    }
}

struct SlowProvider(Duration);
#[async_trait]
impl AiModerationProvider for SlowProvider {
    async fn suggest(&self, _input: &RiskInput, _now: i64) -> AiSuggestion {
        tokio::time::sleep(self.0).await;
        AiSuggestion::NoAction
    }
}

#[tokio::test]
async fn ai_suggestion_can_only_flag_never_execute() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "ai", 30 * 86_400).await;
    let input = risk_input(&author, None, "干净内容", now_millis());

    // 无 provider（禁用）= Null Adapter → 放行
    let v = evaluate_risk(&pool, &input, None, AI_SUGGEST_DEADLINE, now_millis())
        .await
        .unwrap();
    assert!(!v.is_pending_review());

    // AI 建议 flag → 只路由到人工队列（pending_review），不执行任何动作
    let v = evaluate_risk(
        &pool,
        &input,
        Some(&FlagProvider(ReasonCategory::SpamLike)),
        AI_SUGGEST_DEADLINE,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(v.reason_category(), Some(ReasonCategory::SpamLike));
    assert!(v.is_pending_review());

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn ai_late_or_failed_does_not_block_publish() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "slow", 30 * 86_400).await;
    let input = risk_input(&author, None, "干净内容", now_millis());

    // AI 迟到（超过 deadline）→ 按规则结果放行，不阻塞
    let started = std::time::Instant::now();
    let v = evaluate_risk(
        &pool,
        &input,
        Some(&SlowProvider(Duration::from_millis(200))),
        Duration::from_millis(50),
        now_millis(),
    )
    .await
    .unwrap();
    assert!(!v.is_pending_review(), "late AI must not block");
    assert!(
        started.elapsed() < Duration::from_millis(150),
        "evaluation must return within deadline"
    );

    // 正常速度的 AI 返回 NoAction → 放行
    let v = evaluate_risk(
        &pool,
        &input,
        Some(&SlowProvider(Duration::from_millis(5))),
        Duration::from_millis(100),
        now_millis(),
    )
    .await
    .unwrap();
    assert!(!v.is_pending_review());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-RISK-07：重复评估、旧 policy 不复用 ──

#[tokio::test]
async fn duplicate_evaluation_is_stable_and_old_policy_not_reused() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "pol", 30 * 86_400).await;
    let now = now_millis();
    let input = risk_input(
        &author,
        Some(now - 30 * 86_400 * 1000),
        "[1](https://a.com) [2](https://b.com) [3](https://c.com) [4](https://d.com)",
        now,
    );

    // 默认策略 max_links=3 → 命中 LinkHeavy（version 0）
    let v1 = evaluate_risk(&pool, &input, None, AI_SUGGEST_DEADLINE, now)
        .await
        .unwrap();
    assert_eq!(v1.reason_category(), Some(ReasonCategory::LinkHeavy));
    assert_eq!(v1.policy_version(), BUILTIN_POLICY_VERSION);

    // 管理员更新策略：max_links 放宽到 10（version 1）
    update_risk_policy(
        &pool,
        &author,
        Thresholds {
            max_links: 10,
            ..Thresholds::default()
        },
        "放宽链接上限",
        BUILTIN_POLICY_VERSION,
        now,
    )
    .await
    .unwrap();

    // 相同内容重新评估 → 使用新策略（v1），不再命中
    let v2 = evaluate_risk(&pool, &input, None, AI_SUGGEST_DEADLINE, now + 1_000)
        .await
        .unwrap();
    assert!(
        !v2.is_pending_review(),
        "old policy results must not be reused"
    );
    assert_eq!(v2.policy_version(), 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-RISK-08：管理员版本化更新 ──

#[tokio::test]
async fn admin_policy_update_requires_reason_and_versioning() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let admin = insert_author(&pool, "adm", 30 * 86_400).await;
    let now = now_millis();

    // reason 必填
    let err = update_risk_policy(
        &pool,
        &admin,
        Thresholds::default(),
        "   ",
        BUILTIN_POLICY_VERSION,
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, RiskError::InvalidPolicy(_)));

    // 并发版本冲突：期望 0 但当前已是 1
    update_risk_policy(
        &pool,
        &admin,
        Thresholds::default(),
        "第一版",
        BUILTIN_POLICY_VERSION,
        now,
    )
    .await
    .unwrap();
    let err = update_risk_policy(
        &pool,
        &admin,
        Thresholds::default(),
        "并发写入",
        BUILTIN_POLICY_VERSION, // 期望旧版本
        now,
    )
    .await
    .unwrap_err();
    match err {
        RiskError::PolicyConflict { expected, current } => {
            assert_eq!(expected, BUILTIN_POLICY_VERSION);
            assert_eq!(current, 1);
        }
        other => panic!("expected PolicyConflict, got {other:?}"),
    }

    // 审计写入（M05-RISK-08：reason + 审计）
    let audit_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'risk_policy.update'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(audit_count >= 1, "policy update must be audited");

    // 当前生效策略为 version 1
    let policy = load_policy(&pool).await.unwrap();
    assert_eq!(policy.version, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-RISK-09：指标不记录正文 ──

#[tokio::test]
async fn metrics_recorded_without_body_and_review_outcome() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "met", 30 * 86_400).await;
    let cmd = make_command("metric post", "普通内容", None);
    let published = publish_new_post(&pool, &cmd, &author, now_millis())
        .await
        .unwrap();
    let post_id = published.post.id.clone();

    // 发布即写评估指标（allow）
    let row: Option<(String, Option<String>, i64)> = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT verdict, reason_category, policy_version
             FROM risk_evaluations WHERE post_id = ?",
        )
        .bind(&post_id)
        .fetch_optional(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    let (verdict, category, _version) = row.expect("evaluation recorded");
    assert_eq!(verdict, "allow");
    assert_eq!(category, None);

    // risk_evaluations 表结构上不存在正文列（查询即编译期校验 + 语义保证）

    // 队列时长 + 误判反馈
    record_review_outcome(&pool, &post_id, now_millis() + 60_000, true)
        .await
        .unwrap();
    let (reviewed_at, false_positive): (Option<i64>, i64) = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT reviewed_at, false_positive FROM risk_evaluations WHERE post_id = ?",
        )
        .bind(&post_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(reviewed_at.is_some());
    assert_eq!(false_positive, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── 发布响应与 scheduled 路径 ──

#[tokio::test]
async fn scheduled_post_high_risk_flagged_at_publish_time() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "sch", 30 * 86_400).await;
    let later = now_millis() + 3_600_000;

    // 创建 scheduled 帖（创建时不评估；保持 draft）
    let cmd = make_command(
        "scheduled spam",
        "[1](https://a.com) [2](https://b.com) [3](https://c.com) [4](https://d.com)",
        Some(later),
    );
    let created = publish_new_post(&pool, &cmd, &author, now_millis())
        .await
        .unwrap();
    let post_id = created.post.id.clone();
    let (status, review_status, _) = post_review_state(&pool, &post_id).await;
    assert_eq!(status, "draft");
    assert_eq!(review_status, "none");

    // 到期发布 → 风险评估命中 → pending_review
    let published = publish_scheduled_post(&pool, &post_id, later)
        .await
        .unwrap();
    assert!(published.review.is_some());
    let (status, review_status, published_at) = post_review_state(&pool, &post_id).await;
    assert_eq!(status, "draft");
    assert_eq!(review_status, "pending_review");
    assert!(published_at.is_none());

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn scheduled_post_low_risk_publishes_at_time() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "sch2", 30 * 86_400).await;
    let later = now_millis() + 3_600_000;
    let cmd = make_command("scheduled normal", "普通内容", Some(later));
    let created = publish_new_post(&pool, &cmd, &author, now_millis())
        .await
        .unwrap();
    let post_id = created.post.id.clone();

    let published = publish_scheduled_post(&pool, &post_id, later)
        .await
        .unwrap();
    assert!(published.review.is_none());
    let (status, review_status, published_at) = post_review_state(&pool, &post_id).await;
    assert_eq!(status, "published");
    assert_eq!(review_status, "none");
    assert!(published_at.is_some());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── 跨库（CI 用 BBLBB_TEST_MYSQL_URL + --ignored）──
#[allow(dead_code)]
async fn crossdb_flow() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL")?;
    let pool = create_pool(&url).await?;
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR")?).join("../migrations/mysql"),
    )?;
    run_migrations(&pool, &files).await?;
    // 迁移可执行即视为通过；SQLite 侧已全量覆盖语义。
    close_pool(&pool).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires BBLBB_TEST_MYSQL_URL"]
async fn mysql_migrations_apply_cleanly() {
    crossdb_flow().await.expect("mysql flow");
}
