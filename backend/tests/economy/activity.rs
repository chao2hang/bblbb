//! M07-LEVELS-08：等级重建、签到（幂等/日界线/并发）、访问校验、反刷与
//! 管理配置/任务测试（SQLite 全量）。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::economy::activity::checkin::{
    activity_day_for, parse_timezone_offset, validate_visit, VisitContext,
};
use bblbb_backend::economy::activity::service::{
    claim_check_in, claim_content_reward, claim_reaction_reward, create_activity_task,
    ensure_default_activity_config, revoke_claim, update_activity_config, ActivityConfigUpdate,
    ActivityError, TaskInput,
};
use bblbb_backend::economy::ledger::service as ledger;
use bblbb_backend::economy::ledger::service::{get_account, CURRENCY_EXP};
use bblbb_backend::economy::levels::{self, RecomputeOutcome};
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

#[path = "../common/mod.rs"]
mod common;

// ─── 基建 ──────────────────────────────────────────────────────────────

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-activity-{}", uuid::Uuid::now_v7()));
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

/// 插入用户（email_verified_at ~30 天前；可选时区与状态）。
async fn insert_user_with(pool: &DatabasePool, tag: &str, timezone: &str, status: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let unique = uuid::Uuid::now_v7().simple().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users
                     (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, timezone, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', ?, 1, 1, ?, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{unique}"))
            .bind(format!("{tag}_{unique}@example.com"))
            .bind(status)
            .bind(now - 30 * 86_400 * 1000)
            .bind(timezone)
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    insert_user_with(pool, tag, "UTC", "active").await
}

async fn exp_balance(pool: &DatabasePool, user_id: &str) -> i64 {
    match get_account(pool, user_id, CURRENCY_EXP).await {
        Ok(account) => account.balance,
        Err(_) => 0,
    }
}

async fn claim_count(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM activity_claims WHERE user_id = ?")
                .bind(user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn activity_operation_count(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM point_operations WHERE idempotency_scope = 'activity'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id: String = match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(role_name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
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

/// 固定测试时刻：2026-08-06T18:30:00Z（+08:00 → 08-07，UTC → 08-06）。
fn fixed_now() -> i64 {
    chrono::DateTime::parse_from_rfc3339("2026-08-06T18:30:00Z")
        .unwrap()
        .timestamp_millis()
}

// ─── M07-LEVELS-02：等级重建 + level_events ───────────────────────────

#[tokio::test]
async fn level_recompute_writes_level_events_and_syncs_users() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "lv").await;
    let now = fixed_now();
    ensure_default_activity_config(&pool, now).await.unwrap();

    // 首次：余额 0 → L1（初始事件 from=NULL）。
    let first = levels::recompute_level(&pool, &user, "test", now)
        .await
        .unwrap();
    assert!(first.changed);
    assert_eq!(first.to_level_id.clone().unwrap(), level_id(&pool, 1).await);
    assert_eq!(
        user_level(&pool, &user).await,
        1,
        "users.level 缓存同步为 L1"
    );

    // 入账 500 exp → L3（threshold 300；500 < 600）。
    let op = ledger::credit(
        &pool,
        LedgerCmd::award(&user, "lvl-award-1", 500),
        now + 1000,
    )
    .await
    .unwrap();
    let second: RecomputeOutcome = levels::recompute_level(&pool, &user, "test.award", now + 2000)
        .await
        .unwrap();
    assert!(second.changed);
    assert_eq!(
        second.from_level_id.clone().unwrap(),
        level_id(&pool, 1).await
    );
    assert_eq!(
        second.to_level_id.clone().unwrap(),
        level_id(&pool, 3).await
    );
    assert_eq!(user_level(&pool, &user).await, 3);

    // 撤销 → 余额 0 → 降级回 L1（事件 from L3 to L1）。
    ledger::reversal(
        &pool,
        "test",
        "lvl-rev-1",
        None,
        &op.operation_id,
        "test",
        now + 3000,
    )
    .await
    .unwrap();
    let third = levels::recompute_level(&pool, &user, "test.reversal", now + 4000)
        .await
        .unwrap();
    assert!(third.changed);
    assert_eq!(
        third.from_level_id.clone().unwrap(),
        level_id(&pool, 3).await
    );
    assert_eq!(third.to_level_id.clone().unwrap(), level_id(&pool, 1).await);

    // level_events 只追加：3 条，from/to 正确。
    let events: Vec<(Option<String>, String, String)> = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT from_level_id, to_level_id, reason FROM level_events WHERE user_id = ? ORDER BY created_at",
        )
        .bind(&user)
        .fetch_all(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(events.len(), 3, "升降级各写一条事件");
    assert_eq!(events[0].0, None);
    assert_eq!(events[0].1, level_id(&pool, 1).await);
    assert_eq!(events[1].1, level_id(&pool, 3).await);
    assert_eq!(events[2].0.clone().unwrap(), level_id(&pool, 3).await);
    assert_eq!(events[2].1, level_id(&pool, 1).await);

    // 缓存重建不改变账本：仍是 award + reversal 两条 operation。
    let ops: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM point_operations")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(ops, 2, "重建只写缓存与事件，不动账本");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── M07-LEVELS-05：签到首次奖励 + 重放去重 ───────────────────────────

#[tokio::test]
async fn checkin_first_claim_grants_then_replay_dedupes() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "ck").await;
    let now = fixed_now();

    let first = claim_check_in(&pool, &user, now).await.unwrap();
    assert!(first.first_today, "首次访问应领取奖励");
    assert!(first.checked_in_today);
    assert_eq!(first.today_earned.len(), 1);
    assert_eq!(first.today_earned[0].currency, "exp");
    assert_eq!(first.today_earned[0].amount, 10);
    assert!(first.point_operation_id.is_some());
    assert_eq!(first.activity_day, "2026-08-06", "UTC 用户日界线");
    assert_eq!(exp_balance(&pool, &user).await, 10, "奖励入账");
    assert_eq!(claim_count(&pool, &user).await, 1);
    assert_eq!(activity_operation_count(&pool).await, 1);

    // 重放：同日再次访问 → 幂等，不再奖励。
    let op_id = first.point_operation_id.clone().unwrap();
    let replay = claim_check_in(&pool, &user, now + 60_000).await.unwrap();
    assert!(!replay.first_today);
    assert!(replay.checked_in_today);
    assert!(replay.today_earned.is_empty());
    assert_eq!(replay.point_operation_id.as_deref(), Some(op_id.as_str()));
    assert_eq!(exp_balance(&pool, &user).await, 10, "不重复奖励");
    assert_eq!(claim_count(&pool, &user).await, 1);
    assert_eq!(activity_operation_count(&pool).await, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── M07-LEVELS-04：时区日界线翻转 ────────────────────────────────────

#[tokio::test]
async fn checkin_activity_day_follows_user_timezone_boundary() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = fixed_now();

    // 同一 UTC 时刻，不同用户时区 → 不同本地日。
    let east = insert_user_with(&pool, "tz8", "+08:00", "active").await;
    let west = insert_user_with(&pool, "tzm", "-05:00", "active").await;
    let utc_user = insert_user(&pool, "tzutc").await;

    let east_out = claim_check_in(&pool, &east, now).await.unwrap();
    let west_out = claim_check_in(&pool, &west, now).await.unwrap();
    let utc_out = claim_check_in(&pool, &utc_user, now).await.unwrap();
    assert_eq!(east_out.activity_day, "2026-08-07", "+08:00 本地日跨天");
    assert_eq!(west_out.activity_day, "2026-08-06");
    assert_eq!(utc_out.activity_day, "2026-08-06");

    // 日界线翻转：UTC 午夜后 10 分钟，-05:00 用户仍是前一天。
    let after_midnight = chrono::DateTime::parse_from_rfc3339("2026-08-07T00:10:00Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(
        claim_check_in(&pool, &east, after_midnight)
            .await
            .unwrap()
            .activity_day,
        "2026-08-07"
    );
    assert_eq!(
        claim_check_in(&pool, &west, after_midnight)
            .await
            .unwrap()
            .activity_day,
        "2026-08-06",
        "-05:00 在 UTC 午夜后仍为前一天"
    );

    // 同一用户在不同本地日各领取一次（不重复、不合并）。
    let day1 = claim_count(&pool, &west).await;
    assert_eq!(day1, 1);
    assert_eq!(exp_balance(&pool, &west).await, 10);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[test]
fn timezone_offset_parser() {
    assert_eq!(parse_timezone_offset("UTC"), Some(0));
    assert_eq!(parse_timezone_offset("Asia/Shanghai"), Some(8 * 3600));
    assert_eq!(
        parse_timezone_offset("Etc/GMT-8"),
        Some(8 * 3600),
        "POSIX 反号"
    );
    assert_eq!(parse_timezone_offset("Etc/GMT+5"), Some(-5 * 3600));
    assert_eq!(parse_timezone_offset("UTC+8"), Some(8 * 3600));
    assert_eq!(parse_timezone_offset("+08:00"), Some(8 * 3600));
    assert_eq!(parse_timezone_offset("-05:30"), Some(-(5 * 3600 + 1800)));
    assert_eq!(parse_timezone_offset("bogus/zone"), None);
    assert_eq!(parse_timezone_offset(""), None);
    assert_eq!(activity_day_for(8 * 3600, fixed_now()), "2026-08-07");
    assert_eq!(activity_day_for(0, fixed_now()), "2026-08-06");
}

// ─── M07-LEVELS-05：并发去重 ───────────────────────────────────────────

#[tokio::test]
async fn concurrent_visits_claim_once() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "cc").await;
    let now = fixed_now();
    // 预引导配置，避免并发首次创建配置的竞态（真实去重目标：claim 唯一键）。
    ensure_default_activity_config(&pool, now).await.unwrap();

    let p1 = pool.clone();
    let p2 = pool.clone();
    let u1 = user.clone();
    let u2 = user.clone();
    let (r1, r2) = tokio::join!(
        async move { claim_check_in(&p1, &u1, now).await },
        async move { claim_check_in(&p2, &u2, now + 1).await },
    );
    let o1 = r1.expect("first concurrent claim ok");
    let o2 = r2.expect("second concurrent claim ok");
    assert!(
        o1.checked_in_today && o2.checked_in_today,
        "并发双方都看到已签到"
    );
    assert!(
        o1.first_today || o2.first_today,
        "至少一方领取成功（pending 补完成可能双方都报首次）"
    );
    assert_eq!(claim_count(&pool, &user).await, 1, "并发只落一条 claim");
    assert_eq!(exp_balance(&pool, &user).await, 10, "并发只奖励一次");
    assert_eq!(
        activity_operation_count(&pool).await,
        1,
        "账本只一条 activity 流水"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── M07-LEVELS-03：访问校验（匿名/爬虫/预取/静态/健康检查）───────────

#[tokio::test]
async fn visit_validation_rejects_crawler_prefetch_static_health() {
    let ok = |path: &str| {
        validate_visit(&VisitContext {
            path,
            user_agent: Some("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 Chrome/126.0 Safari/537.36"),
            sec_purpose: None,
            purpose: None,
            sec_fetch_dest: None,
        })
    };
    assert!(ok("/boards/general").is_ok());
    assert!(ok("/posts/019123456789").is_ok());
    assert!(ok("/").is_ok());
    assert!(ok("/users/abc?page=2").is_ok());

    // 爬虫/健康检查 UA
    let reject = |ua: &str| {
        validate_visit(&VisitContext {
            path: "/boards/general",
            user_agent: Some(ua),
            sec_purpose: None,
            purpose: None,
            sec_fetch_dest: None,
        })
        .is_err()
    };
    assert!(reject(
        "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)"
    ));
    assert!(reject("curl/8.1.2"));
    assert!(reject("Go-http-client/1.1"));
    assert!(reject("UptimeRobot/2.0"));

    // 预取
    let prefetch = validate_visit(&VisitContext {
        path: "/boards/general",
        user_agent: Some("Mozilla/5.0 Chrome/126.0 Safari/537.36"),
        sec_purpose: Some("prefetch"),
        purpose: None,
        sec_fetch_dest: None,
    });
    assert!(prefetch.is_err());
    let dest_empty = validate_visit(&VisitContext {
        path: "/boards/general",
        user_agent: Some("Mozilla/5.0 Chrome/126.0 Safari/537.36"),
        sec_purpose: None,
        purpose: None,
        sec_fetch_dest: Some("empty"),
    });
    assert!(dest_empty.is_err());

    // 健康检查 / 静态 / API / 非业务页面
    for bad in [
        "/healthz",
        "/readyz",
        "/api/v1/posts",
        "/favicon.ico",
        "/assets/app.css",
        "/_app/immutable/entry.js",
        "/static/logo.png",
        "/uploads/abc.pdf",
        "/posts/1/attachments/x.jpg",
        "/auth/login",
        "/password-reset/confirm",
        "",
        "boards",
    ] {
        assert!(ok(bad).is_err(), "路径 {bad} 必须被拒绝");
    }
}

// ─── M07-LEVELS-07：反刷 ──────────────────────────────────────────────

#[tokio::test]
async fn banned_and_unverified_users_cannot_claim() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let now = fixed_now();
    let banned = insert_user_with(&pool, "ban", "UTC", "banned").await;
    let pending = insert_user_with(&pool, "pend", "UTC", "pending").await;

    let err = claim_check_in(&pool, &banned, now).await.unwrap_err();
    assert!(
        matches!(err, ActivityError::NotEligible(_)),
        "封禁用户拒绝: {err}"
    );
    assert_eq!(claim_count(&pool, &banned).await, 0);
    assert_eq!(exp_balance(&pool, &banned).await, 0, "封禁用户不产生奖励");

    let err = claim_check_in(&pool, &pending, now).await.unwrap_err();
    assert!(matches!(err, ActivityError::NotEligible(_)));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn self_reaction_excluded_and_dup_reaction_deduped() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "rea").await;
    let owner = insert_user(&pool, "own").await;
    let now = fixed_now();
    // 配置 reaction 奖励规则。
    create_activity_task(
        &pool,
        &user,
        "test setup",
        &TaskInput {
            kind: Some("reaction".to_string()),
            amount: Some(5),
            daily_limit: Some(10),
            currency_id: Some(CURRENCY_EXP.to_string()),
            ..TaskInput::default()
        },
        now,
    )
    .await
    .unwrap();

    // 自赞 → 拒绝且不落 claim。
    let err = claim_reaction_reward(&pool, &user, &user, "post", "p-1", "like", now)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ActivityError::NotEligible(_)),
        "自赞拒绝: {err}"
    );

    // 有效点赞 → 奖励 5。
    let first = claim_reaction_reward(&pool, &user, &owner, "post", "p-1", "like", now)
        .await
        .unwrap();
    assert!(first.claimed);
    assert_eq!(exp_balance(&pool, &user).await, 5);

    // 撤赞重赞（同 dedup 周期）→ 不重复奖励。
    let dup = claim_reaction_reward(&pool, &user, &owner, "post", "p-1", "like", now + 1000)
        .await
        .unwrap_err();
    assert!(
        matches!(dup, ActivityError::AlreadyClaimed),
        "重放拒绝: {dup}"
    );
    assert_eq!(exp_balance(&pool, &user).await, 5);
    assert_eq!(claim_count(&pool, &user).await, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn content_reward_respects_daily_limit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "post").await;
    let now = fixed_now();
    create_activity_task(
        &pool,
        &user,
        "test setup",
        &TaskInput {
            kind: Some("post".to_string()),
            amount: Some(20),
            daily_limit: Some(1),
            currency_id: Some(CURRENCY_EXP.to_string()),
            ..TaskInput::default()
        },
        now,
    )
    .await
    .unwrap();

    let first = claim_content_reward(&pool, &user, "post", "post-1", now)
        .await
        .unwrap();
    assert!(first.claimed);
    assert_eq!(exp_balance(&pool, &user).await, 20);

    // 同日第二个目标 → 每日上限拒绝（不同 dedup key 受 daily_limit 约束）。
    let err = claim_content_reward(&pool, &user, "post", "post-2", now + 1000)
        .await
        .unwrap_err();
    assert!(
        matches!(err, ActivityError::NotEligible(_)),
        "每日上限: {err}"
    );
    assert_eq!(exp_balance(&pool, &user).await, 20);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── M07-LEVELS-06：撤销（延迟确认/撤销）──────────────────────────────

#[tokio::test]
async fn revoke_claim_appends_reversal_without_mutating_history() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "rev").await;
    let now = fixed_now();
    let out = claim_check_in(&pool, &user, now).await.unwrap();
    assert_eq!(exp_balance(&pool, &user).await, 10);
    let op_id = out.point_operation_id.clone().unwrap();

    let claim_id: String = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT id FROM activity_claims WHERE user_id = ? ORDER BY created_at LIMIT 1",
        )
        .bind(&user)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };

    revoke_claim(
        &pool,
        &user,
        &claim_id,
        "reward revoked for abuse",
        now + 1000,
    )
    .await
    .unwrap();

    let status: String = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT status FROM activity_claims WHERE id = ?")
            .bind(&claim_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(status, "revoked");
    assert_eq!(exp_balance(&pool, &user).await, 0, "撤销走反向补偿流水");
    let rev: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT reverses_operation_id FROM point_operations WHERE reverses_operation_id = ?",
        )
        .bind(&op_id)
        .fetch_optional(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(rev.is_some(), "reversal 引用原 operation");
    // 原流水只追加：原 op 仍在。
    let orig: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM point_operations WHERE id = ?")
            .bind(&op_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(orig, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── M07-LEVELS-09：管理配置版本化 + 审计 ──────────────────────────────

#[tokio::test]
async fn admin_config_update_versions_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let admin = insert_user(&pool, "adm").await;
    let now = fixed_now();
    let v1 = ensure_default_activity_config(&pool, now).await.unwrap();
    assert_eq!(v1.version, 1);
    assert_eq!(v1.check_in_amount, 10);

    let v2 = update_activity_config(
        &pool,
        &admin,
        &ActivityConfigUpdate {
            site_timezone: Some("Asia/Shanghai".to_string()),
            check_in_amount: Some(15),
            check_in_daily_limit: Some(1),
            rewards_enabled: Some(true),
            reason: "调整签到奖励".to_string(),
            ..ActivityConfigUpdate::default()
        },
        now + 1000,
    )
    .await
    .unwrap();
    assert_eq!(v2.version, 2, "每次更新创建新 version");
    assert_eq!(v2.site_timezone, "Asia/Shanghai");
    assert_eq!(v2.check_in_amount, 15);

    let v3 = update_activity_config(
        &pool,
        &admin,
        &ActivityConfigUpdate {
            check_in_enabled: Some(false),
            reason: "临时关闭签到".to_string(),
            ..ActivityConfigUpdate::default()
        },
        now + 2000,
    )
    .await
    .unwrap();
    assert_eq!(v3.version, 3);
    assert!(!v3.check_in_enabled);

    // 审计：两次 config_update 均落库（reason/effective_role/policy）。
    type AuditRow = (String, Option<String>, Option<String>, Option<String>);
    let audits: Vec<AuditRow> = match &pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT action, reason, effective_role, policy_version FROM audit_logs
             WHERE action = 'admin.activity.config_update' ORDER BY created_at",
        )
        .fetch_all(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[0].0, "admin.activity.config_update");
    assert_eq!(audits[0].1.as_deref(), Some("调整签到奖励"));
    assert_eq!(audits[0].2.as_deref(), Some("administrator"));
    assert_eq!(
        audits[0].3.as_deref(),
        Some(bblbb_backend::authz::decision::AUTHZ_POLICY_VERSION)
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_task_create_list_update() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let admin = insert_user(&pool, "adm2").await;
    let now = fixed_now();

    let rule = create_activity_task(
        &pool,
        &admin,
        "新增发帖任务",
        &TaskInput {
            kind: Some("task".to_string()),
            amount: Some(30),
            daily_limit: Some(3),
            cooldown_seconds: Some(60),
            is_enabled: Some(true),
            ..TaskInput::default()
        },
        now,
    )
    .await
    .unwrap();
    assert_eq!(rule.kind, "task");
    assert_eq!(rule.amount, 30);
    assert_eq!(rule.version, 1);

    let listed = bblbb_backend::economy::activity::service::list_activity_tasks(&pool)
        .await
        .unwrap();
    let items = listed["items"].as_array().unwrap();
    assert!(
        items
            .iter()
            .any(|i| i["kind"] == "task" && i["amount"] == 30),
        "任务列表包含新建规则"
    );

    let updated = bblbb_backend::economy::activity::service::update_activity_task(
        &pool,
        &admin,
        "调整任务奖励",
        &rule.id,
        &TaskInput {
            amount: Some(50),
            is_enabled: Some(false),
            ..TaskInput::default()
        },
        now + 1000,
    )
    .await
    .unwrap();
    assert_eq!(updated.amount, 50);
    assert_eq!(updated.version, 2, "更新创建新 version");
    assert!(!updated.is_enabled);

    let audits: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM audit_logs WHERE action IN ('admin.activity.task_create','admin.activity.task_update')",
            )
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audits, 2);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── HTTP：summary + visit（含匿名/爬虫拒绝）───────────────────────────

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn session_csrf(app: &Router, session: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    body["token"].as_str().unwrap().to_string()
}

async fn authed_json(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
    extra_headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-csrf-token", csrf)
        .header("cookie", session);
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

#[tokio::test]
async fn http_summary_and_visit_flow() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let user = insert_user(&pool, "http").await;
    let session = common::direct_session_cookie(&pool, &user).await;
    let csrf = session_csrf(&app, &session).await;

    // 初始 summary：未签到。
    let (status, body) = authed_json(
        &app,
        "GET",
        "/api/v1/activity/summary",
        &session,
        &csrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "summary: {body}");
    assert_eq!(body["checked_in_today"], false);
    assert_eq!(body["streak_days"], 0);
    assert!(body["experience"]["balance"].is_number());
    assert!(body["level"].is_object(), "等级投影存在: {body}");

    // 首次 visit：自动签到。
    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/activity/visit",
        &session,
        &csrf,
        json!({ "path": "/boards/general" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "visit: {body}");
    assert_eq!(body["checked_in_today"], true);
    assert_eq!(body["streak_days"], 1);
    let earned = body["today_earned"].as_array().unwrap();
    assert_eq!(earned.len(), 1);
    assert_eq!(earned[0]["currency"], "exp");
    assert_eq!(earned[0]["amount"], 10);
    assert!(body["point_operation_id"].is_string());

    // 同日重放 visit：不重复奖励。
    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/activity/visit",
        &session,
        &csrf,
        json!({ "path": "/posts/019123456789" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["checked_in_today"], true);
    assert!(
        body["today_earned"].as_array().unwrap().is_empty(),
        "重放无奖励: {body}"
    );
    assert_eq!(exp_balance(&pool, &user).await, 10);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn http_visit_rejects_anonymous_crawler_and_health_paths() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let user = insert_user(&pool, "http2").await;
    let session = common::direct_session_cookie(&pool, &user).await;
    let csrf = session_csrf(&app, &session).await;

    // 匿名 → 401。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/activity/visit")
                .header("content-type", "application/json")
                .body(Body::from(json!({ "path": "/boards/general" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);

    // 爬虫 UA → 400（不领取）。
    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/activity/visit",
        &session,
        &csrf,
        json!({ "path": "/boards/general" }),
        &[("user-agent", "Mozilla/5.0 (compatible; Googlebot/2.1)")],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "crawler: {body}");
    assert_eq!(claim_count(&pool, &user).await, 0, "爬虫访问不签到");
    assert_eq!(exp_balance(&pool, &user).await, 0);

    // 健康检查/静态路径 → 400。
    for path in ["/healthz", "/api/v1/activity/summary", "/assets/app.js"] {
        let (status, _) = authed_json(
            &app,
            "POST",
            "/api/v1/activity/visit",
            &session,
            &csrf,
            json!({ "path": path }),
            &[],
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "path {path} 必须拒绝");
    }
    assert_eq!(claim_count(&pool, &user).await, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_http_config_and_task_permission_gates() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());

    // 普通成员 → 403。
    let member = insert_user(&pool, "mem").await;
    let msession = common::direct_session_cookie(&pool, &member).await;
    let mcsrf = session_csrf(&app, &msession).await;
    let (status, _) = authed_json(
        &app,
        "GET",
        "/api/v1/admin/activity/config",
        &msession,
        &mcsrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "member 读配置必须 403");

    // 管理员 → 200 + PATCH 版本化。
    let admin = insert_user(&pool, "adm3").await;
    assign_global_role(&pool, &admin, "administrator").await;
    common::enroll_totp(&pool, &admin).await;
    let asession = common::direct_session_cookie(&pool, &admin).await;
    let acsrf = session_csrf(&app, &asession).await;

    let (status, body) = authed_json(
        &app,
        "GET",
        "/api/v1/admin/activity/config",
        &asession,
        &acsrf,
        Value::Null,
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "admin config: {body}");
    assert_eq!(body["version"], 1);

    let (status, body) = authed_json(
        &app,
        "PATCH",
        "/api/v1/admin/activity/config",
        &asession,
        &acsrf,
        json!({ "site_timezone": "UTC+8", "check_in": { "amount": 12 }, "reason": "调整时区与奖励" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "config patch: {body}");
    assert_eq!(body["version"], 2, "PATCH 创建新 version");
    assert_eq!(body["site_timezone"], "UTC+8");
    assert_eq!(body["check_in"]["amount"], 12);

    // 缺 reason → 400。
    let (status, _) = authed_json(
        &app,
        "PATCH",
        "/api/v1/admin/activity/config",
        &asession,
        &acsrf,
        json!({ "site_timezone": "UTC" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 reason 必须 400");

    // 创建任务 + 审计落库。
    let (status, body) = authed_json(
        &app,
        "POST",
        "/api/v1/admin/activity/tasks",
        &asession,
        &acsrf,
        json!({ "kind": "comment", "amount": 5, "daily_limit": 10, "reason": "新增评论奖励" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "task create: {body}");
    let task_id = body["id"].as_str().unwrap().to_string();
    assert_eq!(body["kind"], "comment");

    let (status, body) = authed_json(
        &app,
        "PATCH",
        &format!("/api/v1/admin/activity/tasks/{task_id}"),
        &asession,
        &acsrf,
        json!({ "amount": 8, "reason": "调高评论奖励" }),
        &[],
    )
    .await;
    assert_eq!(status, StatusCode::OK, "task patch: {body}");
    assert_eq!(body["amount"], 8);
    assert_eq!(body["version"], 2);

    let audits: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar(
                "SELECT COUNT(*) FROM audit_logs WHERE action IN ('admin.activity.config_update','admin.activity.task_create','admin.activity.task_update')",
            )
            .fetch_one(p)
            .await
            .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audits, 3, "配置 1 + 任务创建 1 + 任务更新 1 审计");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─── 助手 ──────────────────────────────────────────────────────────────

/// 默认方案下第 n 级（1 起）的 level_id。
async fn level_id(pool: &DatabasePool, nth: i64) -> String {
    let id: String = match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT l.id FROM levels l
             JOIN level_schemes s ON s.id = l.scheme_id
             WHERE s.is_active = 1
             ORDER BY l.sort_order LIMIT 1 OFFSET ?",
        )
        .bind(nth - 1)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    id
}

async fn user_level(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

struct LedgerCmd;

impl LedgerCmd {
    fn award(user_id: &str, key: &str, amount: i64) -> ledger::LedgerCommand {
        ledger::LedgerCommand {
            idempotency_scope: "test".to_string(),
            idempotency_key: key.to_string(),
            kind: ledger::LedgerKind::Award,
            actor_id: None,
            user_id: user_id.to_string(),
            currency_id: CURRENCY_EXP.to_string(),
            delta_balance: amount,
            delta_frozen: 0,
            source_type: None,
            source_id: None,
            memo: "test award".to_string(),
            reverses_operation_id: None,
        }
    }
}
