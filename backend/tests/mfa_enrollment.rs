//! M02-MFA-02：TOTP enrollment——challenge（二维码最小数据）、确认启用、
//! 取消未完成 enrollment；secret 只存 AEAD 密文。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::{
    base32_decode, begin_enrollment, cancel_enrollment, confirm_enrollment, decrypt_secret,
    totp_at, MfaError, TOTP_PERIOD_SECS,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";
const KEY: &[u8] = b"test-encryption-key-material";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-mfaen-{}", uuid::Uuid::now_v7()));
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

/// 当前时间步（秒级）。
fn now_secs() -> u64 {
    (now_millis() / 1000) as u64
}

/// 生成指定 step 的 6 位 code 字符串。
fn code_at(secret: &[u8], step: u64) -> String {
    format!("{:06}", totp_at(secret, step))
}

/// 读取用户 pending/active TOTP 行状态。
async fn totp_state(pool: &DatabasePool, user_id: &str) -> Vec<(bool, bool, String)> {
    // (confirmed, revoked, encrypted_secret)
    match pool {
        Either::Left(p) => {
            let rows: Vec<(i64, i64, String)> = sqlx::query_as(
                "SELECT confirmed_at IS NOT NULL, revoked_at IS NOT NULL, encrypted_secret
                 FROM totp_credentials WHERE user_id = ?",
            )
            .bind(user_id)
            .fetch_all(p)
            .await
            .unwrap();
            rows.into_iter()
                .map(|(c, r, s)| (c != 0, r != 0, s))
                .collect()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// begin → challenge 含二维码最小数据；DB 行为 pending 且密文不含明文。
#[tokio::test]
async fn begin_enrollment_creates_pending_row_and_challenge() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "alice").await;

    let challenge = begin_enrollment(&pool, &user_id, "BBLBB", "alice@example.com", KEY)
        .await
        .unwrap();
    assert_eq!(
        challenge.secret_base32.len(),
        32,
        "20 字节 secret 的 base32 为 32 字符"
    );
    assert!(challenge.otpauth_uri.starts_with("otpauth://totp/"));
    assert!(challenge.otpauth_uri.contains("secret="));
    assert_eq!(challenge.issuer, "BBLBB");
    assert_eq!(challenge.account, "alice@example.com");

    let states = totp_state(&pool, &user_id).await;
    assert_eq!(states.len(), 1);
    let (confirmed, revoked, encrypted) = &states[0];
    assert!(!confirmed, "enrollment 初始必须为 pending（未确认）");
    assert!(!revoked, "enrollment 初始必须未撤销");
    assert!(
        !encrypted.contains(&challenge.secret_base32),
        "数据库必须只存密文，不得含 base32 明文"
    );
    // 密文可解密回原始 secret
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    assert_eq!(decrypt_secret(KEY, encrypted).unwrap(), secret);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 正确 code → 确认启用（confirmed_at + last_accepted_step）。
#[tokio::test]
async fn confirm_enrollment_with_valid_code_enables() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "bob").await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBB", "bob@example.com", KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;

    confirm_enrollment(&pool, &user_id, &code_at(&secret, step), KEY, now_secs())
        .await
        .unwrap();

    let states = totp_state(&pool, &user_id).await;
    let (confirmed, revoked, _) = &states[0];
    assert!(confirmed, "确认后必须启用");
    assert!(!revoked);
    // last_accepted_step 已记录该 step
    let stored_step: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT last_accepted_step FROM totp_credentials WHERE user_id = ?")
                .bind(&user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(stored_step, step as i64);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误 code → InvalidCode，行保持 pending。
#[tokio::test]
async fn confirm_enrollment_with_wrong_code_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "carol").await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBB", "carol@example.com", KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();

    // 用一个确定的错误 code（000000）
    let err = confirm_enrollment(&pool, &user_id, "000000", KEY, now_secs())
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::InvalidCode), "{err:?}");

    let (confirmed, revoked, _) = &totp_state(&pool, &user_id).await[0];
    assert!(!confirmed, "错误 code 不得启用");
    assert!(!revoked, "错误 code 不得撤销 enrollment");
    let _ = secret;

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 无 pending enrollment → NoPendingEnrollment。
#[tokio::test]
async fn confirm_without_pending_enrollment_rejected() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "dave").await;

    let err = confirm_enrollment(&pool, &user_id, "123456", KEY, now_secs())
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::NoPendingEnrollment), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 确认后再次确认（同 code 重放）→ AlreadyConfirmed。
#[tokio::test]
async fn confirm_twice_rejected_as_already_confirmed() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "erin").await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBB", "erin@example.com", KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;
    let code = code_at(&secret, step);

    confirm_enrollment(&pool, &user_id, &code, KEY, now_secs())
        .await
        .unwrap();
    let err = confirm_enrollment(&pool, &user_id, &code, KEY, now_secs())
        .await
        .unwrap_err();
    assert!(
        matches!(err, MfaError::AlreadyConfirmed),
        "重复确认必须拒绝: {err:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 取消未完成 enrollment → pending 撤销；其后 confirm 无 pending。
#[tokio::test]
async fn cancel_enrollment_revokes_pending() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "frank").await;
    begin_enrollment(&pool, &user_id, "BBLBB", "frank@example.com", KEY)
        .await
        .unwrap();

    assert!(cancel_enrollment(&pool, &user_id).await.unwrap());
    let (_, revoked, _) = &totp_state(&pool, &user_id).await[0];
    assert!(revoked, "取消后 enrollment 必须撤销");

    let err = confirm_enrollment(&pool, &user_id, "123456", KEY, now_secs())
        .await
        .unwrap_err();
    assert!(matches!(err, MfaError::NoPendingEnrollment), "{err:?}");

    // 再次取消（无 pending）→ false
    assert!(!cancel_enrollment(&pool, &user_id).await.unwrap());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 重新 enrollment：旧 TOTP（含已启用）被撤销，新 pending 生效（撤销旧+新建）。
#[tokio::test]
async fn begin_enrollment_revokes_existing_totp() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "grace").await;

    // 第一次：完成确认（启用）
    let c1 = begin_enrollment(&pool, &user_id, "BBLBB", "grace@example.com", KEY)
        .await
        .unwrap();
    let secret1 = base32_decode(&c1.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;
    confirm_enrollment(&pool, &user_id, &code_at(&secret1, step), KEY, now_secs())
        .await
        .unwrap();

    // 第二次 begin：旧行撤销，新 pending 行出现
    let _c2 = begin_enrollment(&pool, &user_id, "BBLBB", "grace@example.com", KEY)
        .await
        .unwrap();
    let states = totp_state(&pool, &user_id).await;
    assert_eq!(states.len(), 2, "撤销旧 + 新建，共 2 行");
    let revoked_count = states.iter().filter(|(_, r, _)| *r).count();
    assert_eq!(revoked_count, 1, "旧 TOTP 必须撤销");
    let active_pending = states
        .iter()
        .filter(|(confirmed, revoked, _)| !*confirmed && !*revoked)
        .count();
    assert_eq!(active_pending, 1, "新 enrollment 必须为 pending");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误加密密钥 → confirm 无法解密 → Encryption 错误。
#[tokio::test]
async fn confirm_with_wrong_encryption_key_fails() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "heidi").await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBB", "heidi@example.com", KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;

    let err = confirm_enrollment(
        &pool,
        &user_id,
        &code_at(&secret, step),
        b"wrong-key-material",
        now_secs(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, MfaError::Encryption),
        "错误密钥必须解密失败: {err:?}"
    );

    let (confirmed, _, _) = &totp_state(&pool, &user_id).await[0];
    assert!(!confirmed, "解密失败不得启用");

    close_pool(&pool).await;
    cleanup(&dir);
}
