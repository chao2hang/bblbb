//! M03-PROFILE-08：注销生命周期测试——
//! 请求（active → pending_delete + 入队执行 Job + 事务内审计）、
//! 冷却期（默认 30 天，available_at = 请求时间 + 冷却期）、取消（恢复 active +
//! 取消排队 Job + 审计）、到期执行（幂等匿名化 + 不可删除审计）、
//! 法律保留例外（禁止请求；冷却期内设置 → 到期 Job 跳过并写审计）、
//! 状态机拒绝（未验证/封禁不可请求；已注销不可再请求/取消）。
//!
//! 状态机参考 docs/STATE-MACHINES.md §2 User；策略参考
//! docs/RETENTION-PRIVACY.md（注销延迟 30 天 / 法律保留优先级 1）。
//! 服务层直接以 SQL 造 active 用户（含偏好/隐私行与未撤销会话），
//! 不依赖 HTTP 注册/登录路径。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::jobs::worker::{claim_batch, complete_job};
use bblbb_backend::jobs::worker_loop::JobOutcome;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::users::deletion::{
    cancel_deletion, execute_account_deletion, handle_account_deletion, request_deletion,
    CancelDeletionError, DeletionExecution, DeletionRequestError, ACCOUNT_DELETION_JOB_KIND,
    DELETION_COOLDOWN_MS,
};
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-del-life-{}", uuid::Uuid::now_v7()));
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

/// 插入 active 用户（含偏好/隐私行与一个未撤销会话），返回 (user_id, username)。
async fn setup_active_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let email = format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy-hash', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(&email)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO user_preferences (user_id, timezone, locale, updated_at)
                 VALUES (?, 'UTC', 'zh-CN', ?)",
            )
            .bind(&user_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO user_privacy (user_id, email_visible_to, profile_visible_to, updated_at)
                 VALUES (?, 'nobody', 'everyone', ?)",
            )
            .bind(&user_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO user_sessions
                     (id, user_id, token_hash, csrf_secret_hash, created_at, last_seen_at,
                      idle_expires_at, absolute_expires_at)
                 VALUES (?, ?, 'th', 'ch', ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&user_id)
            .bind(now)
            .bind(now)
            .bind(now + 3_600_000)
            .bind(now + 86_400_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (user_id, username)
}

/// 直接插入指定状态的用户行（未验证/封禁测试用）。
async fn insert_raw_user(pool: &DatabasePool, id: &str, username: &str, email: &str, status: &str) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users
                     (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'x', ?, ?, ?)",
            )
            .bind(id)
            .bind(username)
            .bind(email)
            .bind(status)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 读取用户注销相关状态：status / delete_requested_at / legal_hold_at / version。
async fn user_state(pool: &DatabasePool, user_id: &str) -> (String, Option<i64>, Option<i64>, i64) {
    match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, Option<i64>, Option<i64>, i64)>(
            "SELECT status, delete_requested_at, legal_hold_at, version FROM users WHERE id = ?",
        )
        .bind(user_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 读取用户执行 Job（按去重键）：(status, available_at)。
async fn job_by_dedup(pool: &DatabasePool, user_id: &str) -> Option<(String, i64)> {
    let dedup = format!("account_deletion:{user_id}");
    match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, i64)>(
            "SELECT status, available_at FROM jobs WHERE deduplication_key = ?",
        )
        .bind(&dedup)
        .fetch_optional(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 审计记录计数（不可删除审计断言）。
async fn count_audit(pool: &DatabasePool, actor: &str, action: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM audit_logs WHERE actor_id = ? AND action = ?",
        )
        .bind(actor)
        .bind(action)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 行计数辅助（表 + user_id 过滤）。
async fn count_by_user(pool: &DatabasePool, sql: &str, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>(sql)
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 读取用户名（匿名化断言）。
async fn username_of(pool: &DatabasePool, user_id: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT username_normalized FROM users WHERE id = ?")
            .bind(user_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 设置法律保留。
async fn set_legal_hold(pool: &DatabasePool, user_id: &str, ts: i64) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET legal_hold_at = ? WHERE id = ?")
                .bind(ts)
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 把请求时间提前到冷却期之前，并让执行 Job 立即到期（模拟冷却结束）。
async fn backdate_deletion(pool: &DatabasePool, user_id: &str) {
    let delta = DELETION_COOLDOWN_MS + 60_000;
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE users SET delete_requested_at = delete_requested_at - ? WHERE id = ?",
            )
            .bind(delta)
            .bind(user_id)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "UPDATE jobs SET available_at = ? WHERE kind = ? AND deduplication_key = ?",
            )
            .bind(now - 1000)
            .bind(ACCOUNT_DELETION_JOB_KIND)
            .bind(format!("account_deletion:{user_id}"))
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

// ── 请求：active → pending_delete + 入队 + 审计 ─────────────────────────────

#[tokio::test]
async fn request_sets_pending_delete_enqueues_job_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "req").await;

    let req = request_deletion(&pool, &user_id)
        .await
        .expect("请求必须成功");
    assert_eq!(
        req.executes_at - req.requested_at,
        DELETION_COOLDOWN_MS,
        "冷却期必须等于 30 天"
    );

    let (status, requested_at, legal_hold, version) = user_state(&pool, &user_id).await;
    assert_eq!(status, "pending_delete");
    assert_eq!(requested_at, Some(req.requested_at));
    assert!(legal_hold.is_none());
    assert_eq!(version, 2, "注销请求必须 version+1");

    let job = job_by_dedup(&pool, &user_id)
        .await
        .expect("必须入队执行 Job");
    assert_eq!(job.0, "queued");
    assert_eq!(job.1, req.executes_at, "available_at = 冷却结束");

    assert_eq!(
        count_audit(&pool, &user_id, "user.deletion_requested").await,
        1
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn duplicate_request_is_idempotent() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "dup").await;

    let first = request_deletion(&pool, &user_id).await.unwrap();
    // 幂等成功：返回原请求时间与截止时间，不重复入队
    let again = request_deletion(&pool, &user_id)
        .await
        .expect("重复请求必须幂等成功");
    assert_eq!(again.requested_at, first.requested_at, "原请求时间必须保留");
    assert_eq!(again.executes_at, first.executes_at, "原截止时间必须保留");

    let (_, requested_at, _, _) = user_state(&pool, &user_id).await;
    assert_eq!(requested_at, Some(first.requested_at), "原请求时间必须保留");
    assert_eq!(
        count_by_user(
            &pool,
            "SELECT COUNT(*) FROM jobs WHERE deduplication_key = ?",
            &format!("account_deletion:{user_id}")
        )
        .await,
        1,
        "去重键必须保证至多一个执行 Job"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── 取消：冷却期内本人撤销 ─────────────────────────────────────────────────

#[tokio::test]
async fn cancel_restores_active_cancels_job_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "can").await;

    request_deletion(&pool, &user_id).await.unwrap();
    cancel_deletion(&pool, &user_id)
        .await
        .expect("取消必须成功");

    let (status, requested_at, _, version) = user_state(&pool, &user_id).await;
    assert_eq!(status, "active", "取消必须恢复 active");
    assert_eq!(requested_at, None, "delete_requested_at 必须清空");
    assert_eq!(version, 3, "取消必须 version+1");

    let job = job_by_dedup(&pool, &user_id).await.expect("Job 行保留");
    assert_eq!(job.0, "cancelled", "排队中的执行 Job 必须取消");

    assert_eq!(
        count_audit(&pool, &user_id, "user.deletion_cancelled").await,
        1
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn cancel_without_pending_fails() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "nocan").await;

    assert_eq!(
        cancel_deletion(&pool, &user_id).await,
        Err(CancelDeletionError::NotPending),
        "未请求注销时取消必须失败"
    );
    let (status, _, _, _) = user_state(&pool, &user_id).await;
    assert_eq!(status, "active");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn re_request_after_cancel_reenqueues_job() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "rer").await;

    request_deletion(&pool, &user_id).await.unwrap();
    cancel_deletion(&pool, &user_id).await.unwrap();
    let again = request_deletion(&pool, &user_id)
        .await
        .expect("取消后再次请求必须成功");

    let job = job_by_dedup(&pool, &user_id).await.expect("必须重武装 Job");
    assert_eq!(job.0, "queued", "取消后的历史 Job 必须重入队");
    assert_eq!(job.1, again.executes_at, "重入队必须使用新冷却结束时间");
    assert_eq!(
        count_by_user(
            &pool,
            "SELECT COUNT(*) FROM jobs WHERE deduplication_key = ?",
            &format!("account_deletion:{user_id}")
        )
        .await,
        1,
        "去重键仍保证至多一个执行 Job"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── 状态机拒绝路径 ─────────────────────────────────────────────────────────

#[tokio::test]
async fn unverified_and_banned_cannot_request() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let unv_id = format!("unv_{}", &uuid::Uuid::now_v7().simple().to_string()[..20]);
    let ban_id = format!("ban_{}", &uuid::Uuid::now_v7().simple().to_string()[..20]);
    // v7 UUID 前 14 位在同一毫秒内相同，用户名/邮箱后缀必须用全量 id
    insert_raw_user(
        &pool,
        &unv_id,
        &format!("unverified_{unv_id}"),
        &format!("{unv_id}@example.com"),
        "pending",
    )
    .await;
    insert_raw_user(
        &pool,
        &ban_id,
        &format!("banned_{ban_id}"),
        &format!("{ban_id}@example.com"),
        "banned",
    )
    .await;

    assert_eq!(
        request_deletion(&pool, &unv_id).await,
        Err(DeletionRequestError::Unverified),
        "未验证账户不可发起注销"
    );
    assert_eq!(
        request_deletion(&pool, &ban_id).await,
        Err(DeletionRequestError::Banned),
        "封禁账户不可自助注销（不得绕过 sanction/案件链路）"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn request_after_deletion_fails() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "gone").await;

    request_deletion(&pool, &user_id).await.unwrap();
    backdate_deletion(&pool, &user_id).await;
    assert_eq!(
        execute_account_deletion(&pool, &user_id).await,
        Ok(DeletionExecution::Executed)
    );

    assert_eq!(
        request_deletion(&pool, &user_id).await,
        Err(DeletionRequestError::AlreadyDeleted),
        "已注销用户不可再请求"
    );
    assert_eq!(
        cancel_deletion(&pool, &user_id).await,
        Err(CancelDeletionError::NotPending),
        "已注销用户不可取消"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── 法律保留例外 ───────────────────────────────────────────────────────────

#[tokio::test]
async fn legal_hold_blocks_request() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "hold").await;

    set_legal_hold(&pool, &user_id, now_millis()).await;
    assert_eq!(
        request_deletion(&pool, &user_id).await,
        Err(DeletionRequestError::LegalHold),
        "法律保留优先于注销请求"
    );
    let (status, requested_at, legal_hold, _) = user_state(&pool, &user_id).await;
    assert_eq!(status, "active", "法律保留下不得进入 pending_delete");
    assert_eq!(requested_at, None);
    assert!(legal_hold.is_some(), "legal_hold_at 必须保留");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn legal_hold_defers_due_execution() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, username) = setup_active_user(&pool, "def").await;

    request_deletion(&pool, &user_id).await.unwrap();
    // 冷却期内设置法律保留
    set_legal_hold(&pool, &user_id, now_millis()).await;
    backdate_deletion(&pool, &user_id).await;

    assert_eq!(
        execute_account_deletion(&pool, &user_id).await,
        Ok(DeletionExecution::DeferredByLegalHold),
        "法律保留到期必须跳过执行"
    );

    let (status, _, legal_hold, _) = user_state(&pool, &user_id).await;
    assert_eq!(status, "pending_delete", "保留期间账户保持 pending_delete");
    assert!(legal_hold.is_some());
    assert_eq!(
        username_of(&pool, &user_id).await,
        username,
        "法律保留跳过执行时用户名不得匿名化"
    );
    assert_eq!(
        count_audit(&pool, &user_id, "user.deletion_deferred_legal_hold").await,
        1,
        "法律保留跳过必须写不可删除审计"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── 冷却未到 / 到期执行 ────────────────────────────────────────────────────

#[tokio::test]
async fn not_yet_due_is_deferred() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "wait").await;

    request_deletion(&pool, &user_id).await.unwrap();
    assert_eq!(
        execute_account_deletion(&pool, &user_id).await,
        Ok(DeletionExecution::NotYetDue),
        "冷却期内不得执行"
    );
    let (status, _, _, _) = user_state(&pool, &user_id).await;
    assert_eq!(status, "pending_delete");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn due_execution_anonymizes_and_preserves_audit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, username) = setup_active_user(&pool, "due").await;

    request_deletion(&pool, &user_id).await.unwrap();
    // 前置：偏好/隐私行与未撤销会话存在
    assert_eq!(
        count_by_user(
            &pool,
            "SELECT COUNT(*) FROM user_preferences WHERE user_id = ?",
            &user_id
        )
        .await,
        1
    );
    assert_eq!(
        count_by_user(
            &pool,
            "SELECT COUNT(*) FROM user_sessions WHERE user_id = ? AND revoked_at IS NULL",
            &user_id
        )
        .await,
        1
    );

    backdate_deletion(&pool, &user_id).await;
    assert_eq!(
        execute_account_deletion(&pool, &user_id).await,
        Ok(DeletionExecution::Executed)
    );

    let (status, requested_at, _, _) = user_state(&pool, &user_id).await;
    assert_eq!(status, "deleted");
    assert_eq!(requested_at, None, "匿名化必须清空 delete_requested_at");
    let name = username_of(&pool, &user_id).await;
    assert!(
        name.starts_with("deleted_user_") && !name.contains(&username),
        "到期执行必须匿名化用户名: {name}"
    );
    assert_eq!(
        count_by_user(
            &pool,
            "SELECT COUNT(*) FROM user_preferences WHERE user_id = ?",
            &user_id
        )
        .await,
        0,
        "匿名化必须清理偏好行"
    );
    assert_eq!(
        count_by_user(
            &pool,
            "SELECT COUNT(*) FROM user_sessions WHERE user_id = ? AND revoked_at IS NULL",
            &user_id
        )
        .await,
        0,
        "匿名化必须撤销全部会话"
    );

    // 不可删除审计：请求 + 执行记录全部保留
    assert_eq!(
        count_audit(&pool, &user_id, "user.deletion_requested").await,
        1
    );
    assert_eq!(
        count_audit(&pool, &user_id, "user.deletion_executed").await,
        1
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ── worker 管道：claim → handle → complete ────────────────────────────────

#[tokio::test]
async fn worker_pipeline_completes_due_deletion() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (user_id, _username) = setup_active_user(&pool, "wk").await;

    request_deletion(&pool, &user_id).await.unwrap();
    backdate_deletion(&pool, &user_id).await;

    let claimed = claim_batch(&pool, "test-worker", "default", 8, 30_000)
        .await
        .expect("领取必须成功");
    assert_eq!(claimed.len(), 1, "冷却到期后必须恰好领取 1 个执行 Job");
    assert_eq!(claimed[0].kind, ACCOUNT_DELETION_JOB_KIND);

    let outcome = handle_account_deletion(&pool, &claimed[0]).await;
    match outcome {
        JobOutcome::Succeeded => {}
        JobOutcome::Failed { class, error } => {
            panic!("预期 Job 成功，实际失败 {class:?}: {error}")
        }
    }
    assert!(
        complete_job(&pool, "test-worker", &claimed[0].id)
            .await
            .unwrap(),
        "owner 必须能完成 Job"
    );

    let (status, _, _, _) = user_state(&pool, &user_id).await;
    assert_eq!(status, "deleted", "worker 管道必须完成匿名化");
    let job = job_by_dedup(&pool, &user_id).await.unwrap();
    assert_eq!(job.0, "succeeded", "Job 必须标记成功");
    assert_eq!(
        count_audit(&pool, &user_id, "user.deletion_executed").await,
        1
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
