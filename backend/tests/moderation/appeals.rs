//! M05-APPEALS-06/07：申诉创建规则、撤回、复核人资格、决定只追加、
//! uphold 撤销联动、并发决定与双投影测试（SQLite 全量 + 跨库 #[ignore]）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::moderation::appeals::service as appeals;
use bblbb_backend::moderation::appeals::service::{
    own_appeal_projection, AppealsError, CreateAppealInput,
};
use bblbb_backend::moderation::model::AppealDecisionValue;
use bblbb_backend::moderation::sanctions::service as sanctions;
use bblbb_backend::moderation::sanctions::service::CreateSanctionInput;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'
const OTHER_BOARD_ID: &str = "01911fd5-f001-758e-a95d-a58489fbb61d"; // seeded 'tech'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-appeal-{}", uuid::Uuid::now_v7()));
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

async fn count_rows(pool: &DatabasePool, table: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 管理角色调用 `authorize_action`（如 create_sanction 内的 moderation.sanction）
/// 会因 MFA 强制注册降级（M02-SESSION）；先 enroll TOTP 避免降级。
async fn enroll_totp_for(pool: &DatabasePool, user_id: &str) {
    common::enroll_totp(pool, user_id).await;
}

async fn create_mute_sanction(
    pool: &DatabasePool,
    moderator_id: &str,
    target_user_id: &str,
    now: i64,
) -> String {
    sanctions::create_sanction(
        pool,
        moderator_id,
        CreateSanctionInput {
            target_user_id: target_user_id.to_string(),
            board_id: None,
            kind: bblbb_backend::moderation::model::SanctionKind::Mute,
            reason: "test mute".to_string(),
            starts_at: now,
            ends_at: Some(now + 86_400_000),
        },
        now,
    )
    .await
    .unwrap()
    .id
}

/// 直接写入过去创建的处罚（窗口测试用）。
async fn insert_old_sanction(
    pool: &DatabasePool,
    user_id: &str,
    created_by: &str,
    created_at: i64,
) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at)
                 VALUES (?, ?, NULL, 'mute', 'active', 'old', ?, NULL, ?, ?)",
            )
            .bind(&id)
            .bind(user_id)
            .bind(created_at)
            .bind(created_by)
            .bind(created_at)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    id
}

#[tokio::test]
async fn create_appeal_validates_rules_and_window() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();
    let appellant = insert_user(&pool, "appellant").await;
    let moderator = insert_user(&pool, "mod").await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    enroll_totp_for(&pool, &moderator).await;
    let other = insert_user(&pool, "other").await;

    let sanction = create_mute_sanction(&pool, &moderator, &appellant, now).await;

    // 合法创建
    let appeal = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: " 我被处罚了，理由有误  ".to_string(),
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(appeal.status.as_str(), "submitted");
    assert_eq!(count_rows(&pool, "appeals").await, 1);

    // 文字规则：空 / 超长 / 附件引用
    let err = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "   ".to_string(),
        },
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppealsError::Invalid(_)));

    let long = "字".repeat(5001);
    let err = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: long,
        },
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppealsError::Invalid(_)));

    let err = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "证据见 ![screenshot](http://x/a.png)".to_string(),
        },
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppealsError::Invalid(_)));

    // 越权：非本人处罚
    let err = appeals::create_appeal(
        &pool,
        &other,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "不是我的处罚".to_string(),
        },
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppealsError::Forbidden(_)));

    // 重复提交：每处罚至多一条
    let err = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "第二次申诉".to_string(),
        },
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppealsError::Conflict(_)));

    // 窗口：处罚创建 8 天前 → 不可申诉
    let old = insert_old_sanction(&pool, &other, &moderator, now - 8 * 86_400_000).await;
    let err = appeals::create_appeal(
        &pool,
        &other,
        CreateAppealInput {
            sanction_id: old,
            message: "过期窗口".to_string(),
        },
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppealsError::Conflict(_)));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn withdraw_before_decision_only() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();
    let appellant = insert_user(&pool, "appellant").await;
    let moderator = insert_user(&pool, "mod").await;
    assign_global_role(&pool, &moderator, "global_moderator").await;
    enroll_totp_for(&pool, &moderator).await;

    let sanction = create_mute_sanction(&pool, &moderator, &appellant, now).await;
    let appeal = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "要撤回".to_string(),
        },
        now,
    )
    .await
    .unwrap();

    // 未审理前撤回
    let withdrawn = appeals::withdraw_appeal(&pool, &appellant, &appeal.id, now + 1000)
        .await
        .unwrap();
    assert_eq!(withdrawn.status.as_str(), "withdrawn");

    // 已撤回不可再撤回
    let err = appeals::withdraw_appeal(&pool, &appellant, &appeal.id, now + 2000)
        .await
        .unwrap_err();
    assert!(matches!(err, AppealsError::Conflict(_)));

    // 非本人不可撤回
    let other = insert_user(&pool, "other").await;
    let err = appeals::withdraw_appeal(&pool, &other, &appeal.id, now + 3000)
        .await
        .unwrap_err();
    assert!(matches!(err, AppealsError::NotFound(_)));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn reviewer_eligibility_exclusions() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();
    let appellant = insert_user(&pool, "appellant").await;
    let issuer = insert_user(&pool, "issuer").await; // 原处理者
    assign_global_role(&pool, &issuer, "global_moderator").await;
    enroll_totp_for(&pool, &issuer).await;
    let reviewer = insert_user(&pool, "reviewer").await; // 合格全局复核人
    assign_global_role(&pool, &reviewer, "global_moderator").await;
    let board_mod = insert_user(&pool, "boardmod").await; // 合格板块复核人
    assign_board_role(&pool, &board_mod, BOARD_ID).await;
    let outsider = insert_user(&pool, "outsider").await; // 无 assignment
    let other_board_mod = insert_user(&pool, "otherboard").await; // 超范围
    assign_board_role(&pool, &other_board_mod, OTHER_BOARD_ID).await;

    // 全局处罚
    let sanction = create_mute_sanction(&pool, &issuer, &appellant, now).await;
    let appeal = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "请求复核".to_string(),
        },
        now,
    )
    .await
    .unwrap();
    let v0 = appeal.updated_at;

    // 申诉人本人
    let err = appeals::assign_reviewer(&pool, &reviewer, &appeal.id, &appellant, v0, now)
        .await
        .unwrap_err();
    assert!(matches!(err, AppealsError::ReviewerConflict(_)));

    // 原处理者
    let err = appeals::assign_reviewer(&pool, &reviewer, &appeal.id, &issuer, v0, now)
        .await
        .unwrap_err();
    assert!(matches!(err, AppealsError::ReviewerConflict(_)));

    // 无有效 assignment（普通成员）
    let err = appeals::assign_reviewer(&pool, &reviewer, &appeal.id, &outsider, v0, now)
        .await
        .unwrap_err();
    assert!(matches!(err, AppealsError::ReviewerConflict(_)));

    // 板块版主对全局处罚：仅板块 scope，无全局角色 → 超范围
    let err = appeals::assign_reviewer(&pool, &reviewer, &appeal.id, &board_mod, v0, now)
        .await
        .unwrap_err();
    assert!(matches!(err, AppealsError::ReviewerConflict(_)));

    // 合格全局复核人
    let assigned = appeals::assign_reviewer(&pool, &reviewer, &appeal.id, &reviewer, v0, now)
        .await
        .unwrap();
    assert_eq!(assigned.status.as_str(), "reviewing");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn board_scope_reviewer_must_be_in_scope() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();
    let appellant = insert_user(&pool, "appellant").await;
    let issuer = insert_user(&pool, "issuer").await;
    assign_global_role(&pool, &issuer, "global_moderator").await;
    enroll_totp_for(&pool, &issuer).await;
    let board_mod = insert_user(&pool, "boardmod").await;
    assign_board_role(&pool, &board_mod, BOARD_ID).await;
    let outsider = insert_user(&pool, "outsider").await;
    assign_board_role(&pool, &outsider, OTHER_BOARD_ID).await; // 超范围

    // 板块处罚（board_mute on BOARD_ID）
    let sanction_id = sanctions::create_sanction(
        &pool,
        &issuer,
        CreateSanctionInput {
            target_user_id: appellant.clone(),
            board_id: Some(BOARD_ID.to_string()),
            kind: bblbb_backend::moderation::model::SanctionKind::BoardMute,
            reason: "板块处罚".to_string(),
            starts_at: now,
            ends_at: Some(now + 86_400_000),
        },
        now,
    )
    .await
    .unwrap()
    .id;
    let appeal = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id,
            message: "板块处罚申诉".to_string(),
        },
        now,
    )
    .await
    .unwrap();
    let v0 = appeal.updated_at;

    // 超范围板块版主（其他板块）被排除
    let err = appeals::assign_reviewer(&pool, &outsider, &appeal.id, &outsider, v0, now)
        .await
        .unwrap_err();
    assert!(matches!(err, AppealsError::ReviewerConflict(_)));

    // 同板块版主合格
    let assigned = appeals::assign_reviewer(&pool, &board_mod, &appeal.id, &board_mod, v0, now)
        .await
        .unwrap();
    assert_eq!(assigned.status.as_str(), "reviewing");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn decide_reject_and_partial_are_append_only() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();
    let appellant = insert_user(&pool, "appellant").await;
    let issuer = insert_user(&pool, "issuer").await;
    assign_global_role(&pool, &issuer, "global_moderator").await;
    enroll_totp_for(&pool, &issuer).await;
    let reviewer = insert_user(&pool, "reviewer").await;
    assign_global_role(&pool, &reviewer, "global_moderator").await;

    let sanction = create_mute_sanction(&pool, &issuer, &appellant, now).await;
    let appeal = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "请复核".to_string(),
        },
        now,
    )
    .await
    .unwrap();
    let v0 = appeal.updated_at;

    // 拒绝：只追加决定记录，处罚保持 active
    let result = appeals::decide_appeal(
        &pool,
        &reviewer,
        &appeal.id,
        AppealDecisionValue::Rejected,
        "证据不足",
        v0,
        now + 1000,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "rejected");
    assert_eq!(count_rows(&pool, "appeal_decisions").await, 1);
    let sanction_status: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM sanctions WHERE id = ?")
            .bind(&sanction)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(sanction_status, "active");

    // 已决定后不可再决定（并发/重复）
    let err = appeals::decide_appeal(
        &pool,
        &reviewer,
        &appeal.id,
        AppealDecisionValue::Rejected,
        "再拒一次",
        now + 1000,
        now + 2000,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppealsError::Conflict(_)));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn decide_uphold_revokes_sanction_and_restores_ban() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();
    let appellant = insert_user(&pool, "appellant").await;
    let issuer = insert_user(&pool, "issuer").await;
    assign_global_role(&pool, &issuer, "global_moderator").await;
    enroll_totp_for(&pool, &issuer).await;
    let reviewer = insert_user(&pool, "reviewer").await;
    assign_global_role(&pool, &reviewer, "global_moderator").await;

    // 立即生效的 ban：会话撤销 + 账号 banned
    let sanction_id = sanctions::create_sanction(
        &pool,
        &issuer,
        CreateSanctionInput {
            target_user_id: appellant.clone(),
            board_id: None,
            kind: bblbb_backend::moderation::model::SanctionKind::Ban,
            reason: "严重违规".to_string(),
            starts_at: now,
            ends_at: None,
        },
        now,
    )
    .await
    .unwrap()
    .id;
    let user_status: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM users WHERE id = ?")
            .bind(&appellant)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(user_status, "banned");

    let appeal = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction_id.clone(),
            message: "封禁有误，请求撤销".to_string(),
        },
        now,
    )
    .await
    .unwrap();
    let v0 = appeal.updated_at;

    // uphold：追加 sanction_reversals、处罚 revoked、ban 账号恢复 active
    let result = appeals::decide_appeal(
        &pool,
        &reviewer,
        &appeal.id,
        AppealDecisionValue::Upheld,
        "处罚不当，予以撤销",
        v0,
        now + 1000,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "upheld");
    assert_eq!(count_rows(&pool, "appeal_decisions").await, 1);
    assert_eq!(count_rows(&pool, "sanction_reversals").await, 1);

    let (sanction_status, reversals): (String, i64) = match &pool {
        Either::Left(p) => {
            let status: String = sqlx::query_scalar("SELECT status FROM sanctions WHERE id = ?")
                .bind(&sanction_id)
                .fetch_one(p)
                .await
                .unwrap();
            let n: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM sanction_reversals WHERE sanction_id = ?")
                    .bind(&sanction_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            (status, n)
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(sanction_status, "revoked");
    assert_eq!(reversals, 1);
    let user_status: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM users WHERE id = ?")
            .bind(&appellant)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(user_status, "active");

    // 历史不删：原处罚行仍存在（created_at 不变，镜像 revoked）
    let original_created_at: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT created_at FROM sanctions WHERE id = ?")
            .bind(&sanction_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(original_created_at, now);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn concurrent_decision_stale_version() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();
    let appellant = insert_user(&pool, "appellant").await;
    let issuer = insert_user(&pool, "issuer").await;
    assign_global_role(&pool, &issuer, "global_moderator").await;
    enroll_totp_for(&pool, &issuer).await;
    let reviewer = insert_user(&pool, "reviewer").await;
    assign_global_role(&pool, &reviewer, "global_moderator").await;

    let sanction = create_mute_sanction(&pool, &issuer, &appellant, now).await;
    let appeal = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "并发决定".to_string(),
        },
        now,
    )
    .await
    .unwrap();
    let v0 = appeal.updated_at;

    // 先指派复核人：updated_at 前进到 v1
    let assigned = appeals::assign_reviewer(&pool, &reviewer, &appeal.id, &reviewer, v0, now + 500)
        .await
        .unwrap();
    let v1 = assigned.updated_at;
    assert!(v1 > v0);

    // 带着旧版本决定 → StaleVersion
    let err = appeals::decide_appeal(
        &pool,
        &reviewer,
        &appeal.id,
        AppealDecisionValue::Rejected,
        "旧版本",
        v0,
        now + 1000,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, AppealsError::StaleVersion));

    // 用新版本决定 → 成功
    let result = appeals::decide_appeal(
        &pool,
        &reviewer,
        &appeal.id,
        AppealDecisionValue::Rejected,
        "新版本",
        v1,
        now + 1000,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "rejected");
    assert_eq!(count_rows(&pool, "appeal_decisions").await, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn projections_do_not_cross_boundaries() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = now_millis();
    let appellant = insert_user(&pool, "appellant").await;
    let issuer = insert_user(&pool, "issuer").await;
    assign_global_role(&pool, &issuer, "global_moderator").await;
    enroll_totp_for(&pool, &issuer).await;
    let reviewer = insert_user(&pool, "reviewer").await;
    assign_global_role(&pool, &reviewer, "global_moderator").await;

    let sanction = create_mute_sanction(&pool, &issuer, &appellant, now).await;
    let appeal = appeals::create_appeal(
        &pool,
        &appellant,
        CreateAppealInput {
            sanction_id: sanction.clone(),
            message: "双投影".to_string(),
        },
        now,
    )
    .await
    .unwrap();
    let v0 = appeal.updated_at;
    let decided = appeals::decide_appeal(
        &pool,
        &reviewer,
        &appeal.id,
        AppealDecisionValue::Rejected,
        "内部判断：证据不足",
        v0,
        now + 1000,
    )
    .await
    .unwrap();

    // 申诉人侧：不含内部 note / 复核人 / 利益冲突
    let own = own_appeal_projection(&appeal);
    assert!(own.get("message").is_some(), "申诉人可看自己的说明");
    for key in [
        "reviewed_by",
        "decision_note",
        "conflict_of_interest",
        "decisions",
        "user_id",
    ] {
        assert!(own.get(key).is_none(), "申诉人侧投影不得泄漏 {key}: {own}");
    }

    // 审核员侧：含内部 note 与复核人
    assert!(decided.get("reviewed_by").is_some());
    assert!(decided.get("user_id").is_some());
    assert!(decided.get("decisions").is_some());

    close_pool(&pool).await;
    cleanup(&dir);
}
