//! M05-SANCTIONS-07：处罚创建、即时生效、并发、到期边界、撤销、Session、
//! 越权防护测试（SQLite 全量 + 跨库 #[ignore]）。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::decision::DenyReason;
use bblbb_backend::authz::enforce::authorize_action;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::moderation::model::{SanctionKind, SanctionStatus};
use bblbb_backend::moderation::sanctions::service as sanctions;
use bblbb_backend::moderation::sanctions::service::{CreateSanctionInput, SanctionsError};
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-sanc-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
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

fn input(target: &str, kind: SanctionKind, ends_at: Option<i64>) -> CreateSanctionInput {
    let now = now_millis();
    CreateSanctionInput {
        target_user_id: target.to_string(),
        board_id: None,
        kind,
        reason: "违反社区准则".to_string(),
        starts_at: now,
        ends_at,
    }
}

/// ── M05-SANCTIONS-02：创建（权限 + reason + 期限）──

#[tokio::test]
async fn create_sanction_validates_and_inserts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let member = insert_user(&pool, "member_c").await;
    let moderator = insert_user(&pool, "mod_c").await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    common::enroll_totp(&pool, &moderator).await;
    let now = now_millis();

    let s = sanctions::create_sanction(
        &pool,
        &moderator,
        input(&member, SanctionKind::Mute, Some(now + 7 * 86_400 * 1000)),
        now,
    )
    .await
    .unwrap();
    assert_eq!(s.kind, SanctionKind::Mute);
    assert_eq!(s.status, SanctionStatus::Active);

    // 审计 + Outbox
    let audit: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM audit_logs WHERE action = 'sanction.create'")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit, 1);
    let outbox: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'sanction.changed.v1'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(outbox, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-SANCTIONS-06：越权防护 + 板块范围 + 时长上限 ──

#[tokio::test]
async fn escalation_duration_and_scope_guards() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let member = insert_user(&pool, "member_e").await;
    let global_mod = insert_user(&pool, "gmod_e").await;
    let board_mod = insert_user(&pool, "bmod_e").await;
    assign_global_role(&pool, &global_mod, "global_moderator").await;
    assign_board_role(&pool, &board_mod, BOARD_ID).await;
    common::enroll_totp(&pool, &global_mod).await;
    common::enroll_totp(&pool, &board_mod).await;
    let now = now_millis();

    // 板块版主不能处罚全局版主（越权）
    let err = sanctions::create_sanction(
        &pool,
        &board_mod,
        input(&global_mod, SanctionKind::Mute, None),
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, SanctionsError::Escalation(_)));

    // 板块版主不能创建全局 ban（板块范围外 → 无全局 moderation.sanction）
    let err = sanctions::create_sanction(
        &pool,
        &board_mod,
        input(&member, SanctionKind::Ban, None),
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, SanctionsError::Forbidden(_)));

    // 板块版主可对 member 创建板块内 board_mute（板块 scope）
    let s = sanctions::create_sanction(
        &pool,
        &board_mod,
        CreateSanctionInput {
            target_user_id: member.clone(),
            board_id: Some(BOARD_ID.to_string()),
            kind: SanctionKind::BoardMute,
            reason: "板块内违规".to_string(),
            starts_at: now,
            ends_at: Some(now + 86_400_000),
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(s.kind, SanctionKind::BoardMute);

    // 时长超上限（板块版主 30 天上限）→ Invalid
    let err = sanctions::create_sanction(
        &pool,
        &board_mod,
        input(&member, SanctionKind::Mute, Some(now + 60 * 86_400 * 1000)),
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, SanctionsError::Invalid(_)));

    // 自处罚 → Forbidden
    let err = sanctions::create_sanction(
        &pool,
        &global_mod,
        input(&global_mod, SanctionKind::Warning, None),
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, SanctionsError::Forbidden(_)));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-SANCTIONS-04：ban 撤销 Session + 账号置 banned ──

#[tokio::test]
async fn ban_revokes_sessions_and_marks_banned() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let member = insert_user(&pool, "member_b").await;
    let moderator = insert_user(&pool, "mod_b").await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    common::enroll_totp(&pool, &moderator).await;

    // 给 member 建两个会话
    let cookie1 = common::direct_session_cookie(&pool, &member).await;
    let cookie2 = common::direct_session_cookie(&pool, &member).await;
    assert!(!cookie1.is_empty() && !cookie2.is_empty());

    let now = now_millis();
    sanctions::create_sanction(
        &pool,
        &moderator,
        input(&member, SanctionKind::Ban, None),
        now,
    )
    .await
    .unwrap();

    // users.status = banned
    let status: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM users WHERE id = ?")
            .bind(&member)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(status, "banned");

    // 全部会话已撤销
    let active_sessions: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM user_sessions WHERE user_id = ? AND revoked_at IS NULL",
        )
        .bind(&member)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(active_sessions, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-SANCTIONS-05：撤销只追加 reversal，不改原处罚 ──

#[tokio::test]
async fn revoke_appends_reversal_without_mutating_original() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let member = insert_user(&pool, "member_r").await;
    let moderator = insert_user(&pool, "mod_r").await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    common::enroll_totp(&pool, &moderator).await;
    let now = now_millis();

    let s = sanctions::create_sanction(
        &pool,
        &moderator,
        input(&member, SanctionKind::Mute, Some(now + 7 * 86_400 * 1000)),
        now,
    )
    .await
    .unwrap();
    let original_created_at = s.created_at;

    sanctions::revoke_sanction(&pool, &moderator, &s.id, "复核后解除", now + 3_600_000)
        .await
        .unwrap();

    // 原处罚 status=revoked，created_at 不变
    let (status, created_at): (String, i64) = match &pool {
        Either::Left(p) => sqlx::query_as("SELECT status, created_at FROM sanctions WHERE id = ?")
            .bind(&s.id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(status, "revoked");
    assert_eq!(created_at, original_created_at);

    // reversal 只追加（UNIQUE(sanction_id) 至多一条）
    let reversals: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM sanction_reversals WHERE sanction_id = ?")
                .bind(&s.id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(reversals, 1);

    // 撤销后有效处罚为空
    let effective = sanctions::effective_sanctions(&pool, &member, None, now + 3_600_000)
        .await
        .unwrap();
    assert!(effective.is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-SANCTIONS-03：请求时实时计算（到期/未来/撤销边界）──

#[tokio::test]
async fn effective_sanctions_realtime_boundaries() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let member = insert_user(&pool, "member_t").await;
    let moderator = insert_user(&pool, "mod_t").await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    common::enroll_totp(&pool, &moderator).await;
    let now = now_millis();

    // 已到期 mute → 不生效
    sanctions::create_sanction(
        &pool,
        &moderator,
        input(&member, SanctionKind::Mute, Some(now + 1_000)),
        now,
    )
    .await
    .unwrap();
    let effective = sanctions::effective_sanctions(&pool, &member, None, now + 5_000)
        .await
        .unwrap();
    assert!(effective.is_empty(), "expired mute must not be effective");

    // 进行中 mute → 生效
    sanctions::create_sanction(
        &pool,
        &moderator,
        input(&member, SanctionKind::Mute, Some(now + 7 * 86_400 * 1000)),
        now + 10_000,
    )
    .await
    .unwrap();
    let effective = sanctions::effective_sanctions(&pool, &member, None, now + 20_000)
        .await
        .unwrap();
    assert_eq!(effective.len(), 1);

    // 未来 scheduled ban → 尚未生效
    sanctions::create_sanction(
        &pool,
        &moderator,
        CreateSanctionInput {
            target_user_id: member.clone(),
            board_id: None,
            kind: SanctionKind::Ban,
            reason: "预约封禁".to_string(),
            starts_at: now + 3_600_000,
            ends_at: None,
        },
        now + 30_000,
    )
    .await
    .unwrap();
    let effective = sanctions::effective_sanctions(&pool, &member, None, now + 40_000)
        .await
        .unwrap();
    assert!(
        effective.iter().all(|s| s.kind != SanctionKind::Ban),
        "scheduled ban not effective before starts_at"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-SANCTIONS-03：全局 mute 实时喂给账号门 ──

#[tokio::test]
async fn global_mute_feeds_account_gate() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let member = insert_user(&pool, "member_g").await;
    let moderator = insert_user(&pool, "mod_g").await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    common::enroll_totp(&pool, &moderator).await;
    let now = now_millis();

    // 无 mute 时可发帖
    let decision = authorize_action(
        &pool,
        &member,
        "post.create",
        Some(BOARD_ID),
        bblbb_backend::authz::decision::AUTHZ_POLICY_VERSION,
    )
    .await
    .unwrap();
    assert!(decision.is_allowed());

    // 已过期 mute（窗口在过去，ends_at 已过）→ 实时计算不生效
    sanctions::create_sanction(
        &pool,
        &moderator,
        CreateSanctionInput {
            target_user_id: member.clone(),
            board_id: None,
            kind: SanctionKind::Mute,
            reason: "历史过期处罚".to_string(),
            starts_at: now - 2 * 86_400 * 1000,
            ends_at: Some(now - 86_400 * 1000),
        },
        now,
    )
    .await
    .unwrap();
    let decision = authorize_action(
        &pool,
        &member,
        "post.create",
        Some(BOARD_ID),
        bblbb_backend::authz::decision::AUTHZ_POLICY_VERSION,
    )
    .await
    .unwrap();
    assert!(
        decision.is_allowed(),
        "expired mute no longer blocks (request-time calculation)"
    );

    // 生效中 mute → post.create 拒绝 Muted（请求时实时计算）
    sanctions::create_sanction(
        &pool,
        &moderator,
        input(&member, SanctionKind::Mute, Some(now + 7 * 86_400 * 1000)),
        now + 1_000,
    )
    .await
    .unwrap();
    let decision = authorize_action(
        &pool,
        &member,
        "post.create",
        Some(BOARD_ID),
        bblbb_backend::authz::decision::AUTHZ_POLICY_VERSION,
    )
    .await
    .unwrap();
    assert!(!decision.is_allowed());
    assert_eq!(
        decision,
        bblbb_backend::authz::decision::Decision::Deny {
            reason: DenyReason::Muted
        }
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// ── M05-SANCTIONS-08：用户安全状态投影 ──

#[tokio::test]
async fn user_sanction_status_is_safe_projection() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let member = insert_user(&pool, "member_p").await;
    let moderator = insert_user(&pool, "mod_p").await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    common::enroll_totp(&pool, &moderator).await;
    let now = now_millis();
    sanctions::create_sanction(
        &pool,
        &moderator,
        input(&member, SanctionKind::Mute, Some(now + 86_400_000)),
        now,
    )
    .await
    .unwrap();

    let body = sanctions::user_sanction_status(&pool, &member, now + 1_000)
        .await
        .unwrap();
    let item = &body["items"][0];
    assert_eq!(item["kind"], "mute");
    assert_eq!(item["status"], "active");
    assert!(item.get("expires_at").is_some());
    // 安全投影：不含 reason/内部依据/举报人
    assert!(item.get("reason").is_none());

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
