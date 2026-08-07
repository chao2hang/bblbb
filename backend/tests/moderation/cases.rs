//! M05-CASES-10：举报、案件与内容动作测试（SQLite 全量 + 跨库 #[ignore]）。
//!
//! 覆盖：举报创建/详情限长/窗口去重/统一响应（M05-CASES-01/02）、撤回、
//! 案件开单/状态机/派单板块范围与利益冲突（M05-CASES-03/04/05）、
//! 内容动作 hide/delete/restore 与审计/只追加动作（M05-CASES-06/07/08/09）。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::service::publish_new_post;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::moderation::cases::service as cases;
use bblbb_backend::moderation::cases::service::{CasesError, ContentAction, CreateReportInput};
use bblbb_backend::moderation::model::{
    CasePriority, CaseStatus, ReportReasonCode, ReportTargetType,
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
const BOARD_TECH: &str = "01911fd5-f001-758e-a95d-a58489fbb61d"; // seeded 'tech'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-case-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str, level: i64) -> String {
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
            .bind(now - 30 * 86_400 * 1000)
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
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

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
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

async fn assign_board_role(pool: &DatabasePool, user_id: &str, board_id: &str) {
    let role_id = role_id(pool, "board_moderator").await;
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, ?, ?, NULL, ?, NULL)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(board_id)
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

async fn publish_post(pool: &DatabasePool, author: &str, title: &str, markdown: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: title.to_string(),
            markdown: markdown.to_string(),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("case-{}-{}", title, uuid::Uuid::now_v7().simple()),
        },
        5,
        now_millis(),
    )
    .unwrap();
    publish_new_post(pool, &cmd, author, now_millis())
        .await
        .unwrap()
        .post
        .id
}

fn report_input(target_id: &str, reason: ReportReasonCode) -> CreateReportInput {
    CreateReportInput {
        target_type: ReportTargetType::Post,
        target_id: target_id.to_string(),
        reason_code: reason,
        details: None,
    }
}

async fn post_status(pool: &DatabasePool, post_id: &str) -> (String, Option<i64>) {
    match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, Option<i64>)>(
            "SELECT status, deleted_at FROM posts WHERE id = ?",
        )
        .bind(post_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// ── M05-CASES-01/02：举报创建、去重、统一响应 ──

#[tokio::test]
async fn create_report_validates_and_inserts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let reporter = insert_user(&pool, "rep", 5).await;
    let author = insert_user(&pool, "victim", 5).await;
    let post_id = publish_post(&pool, &author, "target post", "内容").await;

    let now = now_millis();
    let report = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Spam),
        now,
    )
    .await
    .unwrap();
    assert_eq!(report.status.as_str(), "open");
    assert_eq!(report.target_id, post_id);

    // 详情超长 → DetailTooLong
    let long_details = "x".repeat(2_001);
    let err = cases::create_report(
        &pool,
        &reporter,
        CreateReportInput {
            details: Some(long_details),
            ..report_input(&post_id, ReportReasonCode::Harassment)
        },
        now + 1_000,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, CasesError::DetailTooLong));

    // 目标不存在 → TargetNotFound
    let err = cases::create_report(
        &pool,
        &reporter,
        report_input(&uuid::Uuid::now_v7().to_string(), ReportReasonCode::Spam),
        now + 2_000,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, CasesError::TargetNotFound));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn report_dedup_window_returns_existing() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let reporter = insert_user(&pool, "repd", 5).await;
    let author = insert_user(&pool, "victimd", 5).await;
    let post_id = publish_post(&pool, &author, "dedup target", "内容").await;

    let now = now_millis();
    let first = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Spam),
        now,
    )
    .await
    .unwrap();

    // 同窗口内同键重复 → DuplicateReport（统一响应，返回既有 id）
    let err = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Spam),
        now + 3_600_000,
    )
    .await
    .unwrap_err();
    match err {
        CasesError::DuplicateReport { existing_id } => assert_eq!(existing_id, first.id),
        other => panic!("expected DuplicateReport, got {other:?}"),
    }

    // 不同原因 → 允许
    let second = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Harassment),
        now + 3_600_000,
    )
    .await
    .unwrap();
    assert_ne!(second.id, first.id);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn withdraw_report_own_only_and_unprocessed() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let reporter = insert_user(&pool, "repw", 5).await;
    let other = insert_user(&pool, "otherw", 5).await;
    let author = insert_user(&pool, "victimw", 5).await;
    let post_id = publish_post(&pool, &author, "withdraw target", "内容").await;

    let now = now_millis();
    let report = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Spam),
        now,
    )
    .await
    .unwrap();

    // 他人撤回 → Forbidden
    let err = cases::withdraw_report(&pool, &other, &report.id, now + 1_000)
        .await
        .unwrap_err();
    assert!(matches!(err, CasesError::Forbidden(_)));

    // 本人撤回 → ok，状态 withdrawn
    cases::withdraw_report(&pool, &reporter, &report.id, now + 2_000)
        .await
        .unwrap();
    let (_, status): (String, String) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT target_id, status FROM reports WHERE id = ?")
            .bind(&report.id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(status, "withdrawn");

    // 已处理完成 → 再撤回 Forbidden
    let err = cases::withdraw_report(&pool, &reporter, &report.id, now + 3_000)
        .await
        .unwrap_err();
    assert!(matches!(err, CasesError::Forbidden(_)));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-CASES-03/04/05：案件开单、状态机、派单与利益冲突 ──

#[tokio::test]
async fn open_case_from_report_creates_case_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let reporter = insert_user(&pool, "repc", 5).await;
    let author = insert_user(&pool, "victimc", 5).await;
    let moderator = insert_user(&pool, "modc", 5).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    let post_id = publish_post(&pool, &author, "case target", "内容").await;

    let now = now_millis();
    let report = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Harassment),
        now,
    )
    .await
    .unwrap();

    let case_id = cases::open_case_from_report(
        &pool,
        &moderator,
        &report.id,
        CasePriority::High,
        now + 1_000,
    )
    .await
    .unwrap();

    // 案件存在 + 状态 open
    let (case_status, priority): (String, String) = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT status, priority FROM moderation_cases WHERE id = ?")
                .bind(&case_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(case_status, "open");
    assert_eq!(priority, "high");

    // 举报被 triaged
    let report_status: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM reports WHERE id = ?")
            .bind(&report.id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(report_status, "triaged");

    // 审计 + Outbox
    let audit_count: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE action = 'case.open'")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit_count, 1);
    let outbox_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'moderation.case_changed.v1'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(outbox_count, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn open_case_blocked_for_self_report_with_audit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let reporter = insert_user(&pool, "repc2", 5).await;
    let author = insert_user(&pool, "victimc2", 5).await;
    assign_global_role(&pool, &reporter, "global_moderator").await;
    let post_id = publish_post(&pool, &author, "conflict target", "内容").await;

    let now = now_millis();
    let report = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Spam),
        now,
    )
    .await
    .unwrap();

    // 举报人本人开案 → Forbidden + 阻断审计
    let err = cases::open_case_from_report(
        &pool,
        &reporter,
        &report.id,
        CasePriority::Normal,
        now + 1_000,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, CasesError::Forbidden(_)));
    let blocked_audit: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'case.block_conflict'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(blocked_audit, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn case_state_machine_transitions() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let reporter = insert_user(&pool, "reps", 5).await;
    let author = insert_user(&pool, "victims", 5).await;
    let moderator = insert_user(&pool, "mods", 5).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    let post_id = publish_post(&pool, &author, "sm target", "内容").await;

    let now = now_millis();
    let report = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Misinformation),
        now,
    )
    .await
    .unwrap();
    let case_id = cases::open_case_from_report(
        &pool,
        &moderator,
        &report.id,
        CasePriority::Normal,
        now + 1_000,
    )
    .await
    .unwrap();

    // open → investigating（合法）
    cases::transition_case(
        &pool,
        &moderator,
        &case_id,
        CaseStatus::Investigating,
        None,
        now + 2_000,
    )
    .await
    .unwrap();
    // investigating → resolved（合法）
    cases::transition_case(
        &pool,
        &moderator,
        &case_id,
        CaseStatus::Resolved,
        Some("confirmed spam"),
        now + 3_000,
    )
    .await
    .unwrap();
    // resolved → reopened（合法）
    cases::transition_case(
        &pool,
        &moderator,
        &case_id,
        CaseStatus::Reopened,
        None,
        now + 4_000,
    )
    .await
    .unwrap();

    // investigating → triaged 非法（当前 reopened → triaged 合法，先验证非法对）
    // reopened → reopened 同状态 → InvalidTransition
    let err = cases::transition_case(
        &pool,
        &moderator,
        &case_id,
        CaseStatus::Reopened,
        None,
        now + 5_000,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, CasesError::InvalidTransition { .. }));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn assign_case_scope_and_conflict() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let reporter = insert_user(&pool, "repa", 5).await;
    let author = insert_user(&pool, "victima", 5).await;
    let moderator = insert_user(&pool, "moda", 5).await;
    let other_board_mod = insert_user(&pool, "othermod", 5).await;
    let global_mod = insert_user(&pool, "globala", 5).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    assign_global_role(&pool, &global_mod, "global_moderator").await;
    // other_board_mod 只在 tech 板块（目标在 general）→ scope 外
    assign_board_role(&pool, &other_board_mod, BOARD_TECH).await;
    // elevated 角色账号需完成 TOTP 才不被降级（强制启用规则）
    common::enroll_totp(&pool, &moderator).await;
    common::enroll_totp(&pool, &other_board_mod).await;
    common::enroll_totp(&pool, &global_mod).await;
    let post_id = publish_post(&pool, &author, "assign target", "内容").await;

    let now = now_millis();
    let report = cases::create_report(
        &pool,
        &reporter,
        report_input(&post_id, ReportReasonCode::Spam),
        now,
    )
    .await
    .unwrap();
    let case_id = cases::open_case_from_report(
        &pool,
        &moderator,
        &report.id,
        CasePriority::Normal,
        now + 1_000,
    )
    .await
    .unwrap();

    // 指派给举报人本人 → Forbidden（利益冲突）
    let err = cases::assign_case(&pool, &moderator, &case_id, &reporter, None, now + 2_000)
        .await
        .unwrap_err();
    assert!(matches!(err, CasesError::Forbidden(_)));

    // 指派给板块外版主 → Forbidden（scope）
    let err = cases::assign_case(
        &pool,
        &moderator,
        &case_id,
        &other_board_mod,
        None,
        now + 3_000,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, CasesError::Forbidden(_)));

    // 指派给全局版主 → ok；指派历史只追加
    cases::assign_case(
        &pool,
        &moderator,
        &case_id,
        &global_mod,
        Some("接手"),
        now + 4_000,
    )
    .await
    .unwrap();
    let assignments: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM case_assignments WHERE case_id = ?")
                .bind(&case_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(
        assignments >= 2,
        "open assigns moderator + reassign appends"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-CASES-06/07/08/09：内容动作 ──

#[tokio::test]
async fn content_action_hide_removes_from_public_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "authorh", 5).await;
    let moderator = insert_user(&pool, "modh", 5).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    let post_id = publish_post(&pool, &author, "hide target", "内容").await;

    let now = now_millis();
    cases::apply_post_action(
        &pool,
        &moderator,
        &post_id,
        ContentAction::Hide,
        "违反社区准则",
        now,
    )
    .await
    .unwrap();

    let (status, deleted_at) = post_status(&pool, &post_id).await;
    assert_eq!(status, "hidden");
    assert!(deleted_at.is_none());

    // 只追加动作 + 审计
    let (action_count, audit_count): (i64, i64) = match &pool {
        Either::Left(p) => {
            let a: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM moderation_actions WHERE target_id = ? AND action = 'hide_content'",
            )
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap();
            let b: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE action = 'hide_content'")
                    .fetch_one(p)
                    .await
                    .unwrap();
            (a, b)
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(action_count, 1);
    assert_eq!(audit_count, 1);

    // 公开列表不再包含（status='published' 过滤）
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    let resp = app
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/posts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert!(
        !body["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p["id"] == Value::String(post_id.clone())),
        "hidden post must not appear in public listing"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn content_action_delete_and_restore() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "authord", 5).await;
    let moderator = insert_user(&pool, "modd", 5).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    let post_id = publish_post(&pool, &author, "delete target", "内容").await;

    let now = now_millis();
    cases::apply_post_action(
        &pool,
        &moderator,
        &post_id,
        ContentAction::Delete,
        "删除违规内容",
        now,
    )
    .await
    .unwrap();
    let (status, deleted_at) = post_status(&pool, &post_id).await;
    assert_eq!(status, "deleted");
    assert!(deleted_at.is_some());

    // restore → published
    cases::apply_post_action(
        &pool,
        &moderator,
        &post_id,
        ContentAction::Restore,
        "复核后恢复",
        now + 1_000,
    )
    .await
    .unwrap();
    let (status, deleted_at) = post_status(&pool, &post_id).await;
    assert_eq!(status, "published");
    assert!(deleted_at.is_none());

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn content_action_own_content_blocked_with_audit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let moderator = insert_user(&pool, "modown", 5).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    let post_id = publish_post(&pool, &moderator, "own content", "内容").await;

    let now = now_millis();
    let err = cases::apply_post_action(
        &pool,
        &moderator,
        &post_id,
        ContentAction::Hide,
        "隐藏自己内容",
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, CasesError::Forbidden(_)));
    let blocked_audit: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'content_action_blocked_conflict'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(blocked_audit, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn content_action_requires_reason() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "authorr", 5).await;
    let moderator = insert_user(&pool, "modr", 5).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    let post_id = publish_post(&pool, &author, "reason target", "内容").await;

    let err = cases::apply_post_action(
        &pool,
        &moderator,
        &post_id,
        ContentAction::Hide,
        "  ",
        now_millis(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, CasesError::InvalidReason(_)));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── HTTP 集成（M05-CASES-01/02）──

#[tokio::test]
async fn report_http_create_dedup_and_invalid_reason() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let reporter = insert_user(&pool, "httprep", 5).await;
    let author = insert_user(&pool, "httpvictim", 5).await;
    let post_id = publish_post(&pool, &author, "http target", "内容").await;
    let cookie = common::direct_session_cookie(&pool, &reporter).await;
    let app = build_router(AppConfig::default(), Some(pool.clone()));
    // 写端点需要 Session CSRF：先取 token
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

    // 正常创建 → 201
    let body = serde_json::json!({
        "target_type": "post",
        "target_id": post_id,
        "reason": "spam",
    });
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CREATED);

    // 重复 → 409（统一响应）
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);

    // 非法 reason → 400
    let bad = serde_json::json!({
        "target_type": "post",
        "target_id": post_id,
        "reason": "not-a-reason",
    });
    let resp = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/reports")
                .header("content-type", "application/json")
                .header("cookie", &cookie)
                .header("x-csrf-token", &csrf)
                .body(Body::from(bad.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

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
    close_pool(&pool).await;
    Ok(())
}

#[tokio::test]
#[ignore = "requires BBLBB_TEST_MYSQL_URL"]
async fn mysql_migrations_apply_cleanly() {
    crossdb_flow().await.expect("mysql flow");
}
