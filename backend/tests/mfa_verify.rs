//! M02-MFA-03：允许时间窗口 + 已接受 time step 防重放 + 不输出 code/secret。
//!
//! 语义：`confirm_enrollment` 消费确认时的 step（写入 last_accepted_step），
//! 因此登录验证必须使用后续 step（> last_accepted_step）——重放已接受的
//! step 一律拒绝。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::{
    base32_decode, begin_enrollment, confirm_enrollment, totp_at, verify_totp_login, MfaError,
    TOTP_PERIOD_SECS,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";
const KEY: &[u8] = b"verify-encryption-key";
const WINDOW: u64 = 1;

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-mfaver-{}", uuid::Uuid::now_v7()));
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

/// 完成 enrollment：begin + 用当前步 code 确认（消费该 step）。
/// 返回 (原始 secret, 确认消费的 step)。
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

/// 正确 code（后续 step）→ 接受并记录 step。
#[tokio::test]
async fn verify_login_accepts_valid_code() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "alice").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let login_step = confirm_step + 1;
    let now = login_step * TOTP_PERIOD_SECS + 15; // 该 step 中段

    let outcome = verify_totp_login(
        &pool,
        &user_id,
        &code_at(&secret, login_step),
        KEY,
        now,
        WINDOW,
    )
    .await
    .unwrap();
    assert_eq!(outcome.step, login_step);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 窗口内步（±1）被接受（容忍时钟漂移）。
#[tokio::test]
async fn verify_login_accepts_window_step() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "bob").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let current = confirm_step + 2; // 模拟 60s 后
    let now = current * TOTP_PERIOD_SECS + 15;

    // +1 步（客户端时钟快 30s）→ 窗口内接受
    let outcome = verify_totp_login(
        &pool,
        &user_id,
        &code_at(&secret, current + 1),
        KEY,
        now,
        WINDOW,
    )
    .await
    .unwrap();
    assert_eq!(outcome.step, current + 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 窗口外步（±2）→ 拒绝。
#[tokio::test]
async fn verify_login_rejects_step_outside_window() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "carol").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let current = confirm_step + 5;
    let now = current * TOTP_PERIOD_SECS + 15;

    let err = verify_totp_login(
        &pool,
        &user_id,
        &code_at(&secret, current + 2),
        KEY,
        now,
        WINDOW,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 已接受的 step 重放 → 拒绝（防重放）。
#[tokio::test]
async fn verify_login_rejects_replayed_step() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "dave").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let login_step = confirm_step + 1;
    let now = login_step * TOTP_PERIOD_SECS + 15;
    let code = code_at(&secret, login_step);

    verify_totp_login(&pool, &user_id, &code, KEY, now, WINDOW)
        .await
        .unwrap();

    // 同一 code 再验证 → 重放拒绝（step 已接受）
    let err = verify_totp_login(&pool, &user_id, &code, KEY, now, WINDOW)
        .await
        .unwrap_err();
    assert!(
        matches!(err, MfaError::InvalidCode),
        "重放必须拒绝: {err:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误 code → 拒绝。
#[tokio::test]
async fn verify_login_rejects_wrong_code() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "erin").await;
    enabled_totp(&pool, &user_id).await;

    let err = verify_totp_login(&pool, &user_id, "000000", KEY, now_secs(), WINDOW)
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未启用 TOTP → TotpNotEnabled。
#[tokio::test]
async fn verify_login_rejects_when_totp_not_enabled() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "frank").await;

    let err = verify_totp_login(&pool, &user_id, "123456", KEY, now_secs(), WINDOW)
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::TotpNotEnabled), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 并发同 code：原子推进 last_accepted_step，恰好一个成功。
#[tokio::test]
async fn verify_login_concurrent_same_step_only_one_succeeds() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "grace").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let login_step = confirm_step + 1;
    let now = login_step * TOTP_PERIOD_SECS + 15;
    let code = code_at(&secret, login_step);

    let p1 = pool.clone();
    let p2 = pool.clone();
    let u1 = user_id.clone();
    let u2 = user_id.clone();
    let c1 = code.clone();
    let (r1, r2) = tokio::join!(
        async move { verify_totp_login(&p1, &u1, &c1, KEY, now, WINDOW).await },
        async move { verify_totp_login(&p2, &u2, &code, KEY, now, WINDOW).await },
    );
    let ok_count = [r1, r2].iter().filter(|r| r.is_ok()).count();
    assert_eq!(ok_count, 1, "同一 step 并发验证必须恰好一个成功");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误信息与结果不包含 code 或 secret（不落日志/响应）。
#[tokio::test]
async fn verify_login_never_exposes_code_or_secret() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "heidi").await;
    let (secret, confirm_step) = enabled_totp(&pool, &user_id).await;
    let login_step = confirm_step + 2;
    let now = login_step * TOTP_PERIOD_SECS + 15;
    let wrong = "111111";

    // 错误路径：错误信息不得含 code 或 base32 secret
    let secret_b32 = bblbb_backend::auth::base32_encode(&secret);
    let err = verify_totp_login(&pool, &user_id, wrong, KEY, now, WINDOW)
        .await
        .unwrap_err();
    let rendered = format!("{err:?} {err}");
    assert!(!rendered.contains(wrong), "错误信息不得含 code");
    assert!(!rendered.contains(&secret_b32), "错误信息不得含 secret");

    // 成功路径：返回值不含 code/secret
    let outcome = verify_totp_login(
        &pool,
        &user_id,
        &code_at(&secret, login_step),
        KEY,
        now,
        WINDOW,
    )
    .await
    .unwrap();
    assert_eq!(outcome.step, login_step);

    close_pool(&pool).await;
    cleanup(&dir);
}
