//! M02-MFA-09：MFA 边缘测试——时钟偏移边界、旧 code 重放、并发恢复码、
//! Session 旋转与 step-up 保持。
//!
//! 既有覆盖（不重复）：窗口 ±1 接受/窗口外拒绝/同 step 重放/并发同码
//! （mfa_verify.rs）、并发同恢复码（mfa_recovery.rs）、封禁会话立即失效
//! （session_status.rs）、封禁登录统一 401（session_login.rs）、step-up
//! 纯函数与端到端（mfa_stepup.rs）。角色降权（administrator→member）测试
//! 依赖 M3-AUTHZ-02 角色聚合，见阻塞项 M02-MFA-05/06——本文件只覆盖
//! M2 服务层等价物：旋转保持重认证、撤销会话 fail-closed。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::session::{
    create_session, is_step_up_required_for_session, mark_step_up, rotate_session,
    DEFAULT_STEP_UP_WINDOW_SECS,
};
use bblbb_backend::auth::{
    base32_decode, begin_enrollment, confirm_enrollment, consume_recovery_code,
    generate_recovery_codes, totp_at, verify_totp_login, MfaError, RECOVERY_CODE_COUNT,
    TOTP_PERIOD_SECS,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";
const KEY: &[u8] = b"edges-encryption-key";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-mfaedges-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir()).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    (pool, dir)
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir);
    let _ = std::fs::remove_file(format!("{}-wal", dir.display()));
    let _ = std::fs::remove_file(format!("{}-shm", dir.display()));
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_user"))
            .bind(format!("{tag}@example.com"))
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

fn now_secs() -> u64 {
    (now_millis() / 1000) as u64
}

fn code_at(secret: &[u8], step: u64) -> String {
    format!("{:06}", totp_at(secret, step))
}

/// 完成 enrollment（消费 confirm_step），返回 (secret, confirm_step)。
async fn enabled_totp(pool: &DatabasePool, user_id: &str) -> (Vec<u8>, u64) {
    let challenge = begin_enrollment(pool, user_id, "BBLBB", "u@example.com", KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let confirm_step = now_secs() / TOTP_PERIOD_SECS;
    confirm_enrollment(
        pool,
        user_id,
        &code_at(&secret, confirm_step),
        KEY,
        now_secs(),
    )
    .await
    .unwrap();
    (secret, confirm_step)
}

// ─────────────────────────── 时钟偏移边界 ───────────────────────────

/// 窗口边界含端点：WINDOW=1 时 current-1 与 current+1 均接受。
#[tokio::test]
async fn clock_offset_exact_window_boundary_is_inclusive() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "alice").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    // 模拟 2 个周期后：current = confirm_step + 2
    let current = confirm_step + 2;
    let now = current * TOTP_PERIOD_SECS + 15;

    // 边界 current-1（客户端时钟慢 30s）→ 接受
    let outcome = verify_totp_login(&pool, &user_id, &code_at(&secret, current - 1), KEY, now, 1)
        .await
        .expect("窗口下边界必须含端点");
    assert_eq!(outcome.step, current - 1);

    // 边界 current+1（客户端时钟快 30s）→ 接受
    let outcome = verify_totp_login(&pool, &user_id, &code_at(&secret, current + 1), KEY, now, 1)
        .await
        .expect("窗口上边界必须含端点");
    assert_eq!(outcome.step, current + 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 窗口外一步（current-2，WINDOW=1）→ 拒绝；同时该 step 未超过 last_accepted。
#[tokio::test]
async fn clock_offset_beyond_window_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "bob").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let current = confirm_step + 2;
    let now = current * TOTP_PERIOD_SECS + 15;

    let err = verify_totp_login(&pool, &user_id, &code_at(&secret, current - 2), KEY, now, 1)
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// WINDOW=0（不允许漂移）：仅当前步接受，±1 均拒绝。
#[tokio::test]
async fn clock_offset_zero_window_only_current_step() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "carol").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let current = confirm_step + 1;
    let now = current * TOTP_PERIOD_SECS + 15;

    verify_totp_login(&pool, &user_id, &code_at(&secret, current), KEY, now, 0)
        .await
        .expect("零窗口下当前步必须接受");

    let err = verify_totp_login(&pool, &user_id, &code_at(&secret, current + 1), KEY, now, 0)
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── code 重放 ───────────────────────────

/// 已接受的 step 在时间推进后再重放 → 拒绝（无论窗口如何，防重放长期有效）。
#[tokio::test]
async fn replay_of_accepted_step_rejected_after_time_advance() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "dave").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let accepted = confirm_step + 1;
    let now_a = accepted * TOTP_PERIOD_SECS + 15;

    verify_totp_login(&pool, &user_id, &code_at(&secret, accepted), KEY, now_a, 1)
        .await
        .unwrap();

    // 时间推进 3 个周期后重放同一 code
    let now_b = (accepted + 3) * TOTP_PERIOD_SECS + 15;
    let err = verify_totp_login(&pool, &user_id, &code_at(&secret, accepted), KEY, now_b, 1)
        .await
        .unwrap_err();
    assert!(
        matches!(err, MfaError::InvalidCode),
        "重放必须拒绝: {err:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 已接受 step 之前的旧 step（仍在窗口内但 < last_accepted）→ 拒绝。
#[tokio::test]
async fn step_below_last_accepted_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "erin").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let accepted = confirm_step + 1;
    let now = (accepted + 1) * TOTP_PERIOD_SECS + 15; // current = accepted + 1

    verify_totp_login(&pool, &user_id, &code_at(&secret, accepted), KEY, now, 1)
        .await
        .unwrap();

    // step accepted-1 = confirm_step：在窗口内（current-1），但等于 last_accepted → 拒绝
    let err = verify_totp_login(
        &pool,
        &user_id,
        &code_at(&secret, accepted - 1),
        KEY,
        now,
        1,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, MfaError::InvalidCode),
        "等于/小于 last_accepted 的 step 必须拒绝: {err:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── 并发恢复码 ───────────────────────────

/// 并发消费两个不同的未用恢复码 → 都成功（互不干扰）。
#[tokio::test]
async fn concurrent_different_recovery_codes_both_succeed() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "frank").await;
    let codes = generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-1")
        .await
        .unwrap();

    let p1 = pool.clone();
    let p2 = pool.clone();
    let u1 = user_id.clone();
    let u2 = user_id.clone();
    let c1 = codes[0].clone();
    let c2 = codes[1].clone();
    let (r1, r2) = tokio::join!(
        async move { consume_recovery_code(&p1, &u1, &c1, "req-2").await },
        async move { consume_recovery_code(&p2, &u2, &c2, "req-3").await },
    );
    assert!(r1.is_ok(), "codes[0] 消费失败: {r1:?}");
    assert!(r2.is_ok(), "codes[1] 消费失败: {r2:?}");

    // 两个都不可再消费（已消费 → InvalidCode）
    let err = consume_recovery_code(&pool, &user_id, &codes[0], "req-4")
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode));
    let err = consume_recovery_code(&pool, &user_id, &codes[1], "req-5")
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode));

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── Session 旋转 + step-up 保持 ───────────────────────────

/// 旋转后新 token 保持 step-up 已认证（auth_verified_at=now）；旧 token
/// 已撤销 → fail-closed 需要 step-up（降权等高权限变化场景的 M2 等价物）。
#[tokio::test]
async fn rotation_preserves_step_up_clearance_and_old_token_fail_closed() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "grace").await;
    let old_token = create_session(&pool, &user_id, None, false).await.unwrap();

    // 完整认证（登录即 auth_verified_at=now）→ 不需要 step-up
    assert!(
        !is_step_up_required_for_session(&pool, &old_token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap()
    );

    // 旋转（登录/权限提升/改密/高风险重认证场景）→ 新 token 仍不需要 step-up
    let new_token = rotate_session(&pool, &old_token, "login").await.unwrap();
    assert!(
        !is_step_up_required_for_session(&pool, &new_token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap(),
        "旋转后新 token 必须保持重认证状态"
    );

    // 旧 token 已撤销 → fail-closed 需要 step-up
    assert!(
        is_step_up_required_for_session(&pool, &old_token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap(),
        "已撤销旧 token 必须要求 step-up"
    );

    // mark_step_up 对已撤销旧 token → RowNotFound（fail closed）
    let err = mark_step_up(&pool, &old_token).await.unwrap_err();
    assert!(matches!(err, sqlx::Error::RowNotFound));

    close_pool(&pool).await;
    cleanup(&dir);
}
