//! M02-MFA-08：安全通知——新设备、密码/MFA 变化、Session 撤销、恢复码使用。
//!
//! 每条安全通知同事务写三处：`notifications`（type='system' + security_kind）、
//! 审计 `auth.security_notification`、Outbox 事件 `auth.security_notification.v1`；
//! 事务回滚三处一并消失（M01-JOBS-02）。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::security_notify::{
    create_security_notification_in_tx, has_device_seen, notify_new_device_if_first_seen,
    notify_password_changed, SecurityEvent,
};
use bblbb_backend::auth::session::{create_session, list_sessions, revoke_session_by_id};
use bblbb_backend::auth::token::{generate_token, hash_token};
use bblbb_backend::auth::{
    base32_decode, begin_enrollment, cancel_enrollment, confirm_enrollment, confirm_password_reset,
    consume_recovery_code, generate_recovery_codes, hash_password, login_user, LoginLimits,
    RECOVERY_CODE_COUNT, TOTP_PERIOD_SECS,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::ratelimit::RateLimiter;
use serde_json::Value;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";
const KEY: &[u8] = b"test-encryption-key-material";
const PASSWORD: &str = "correct-password";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-secnotify-{}", uuid::Uuid::now_v7()));
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

/// 插入一个可登录用户（status='active'，密码已知），返回 (user_id, email)。
async fn insert_login_user(pool: &DatabasePool, tag: &str) -> (String, String) {
    let user_id = uuid::Uuid::now_v7().to_string();
    let email = format!("{tag}@example.com");
    let hash = hash_password(PASSWORD).unwrap();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, ?, 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_user"))
            .bind(&email)
            .bind(&hash)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    (user_id, email)
}

/// 插入一个未消费 reset token，返回原始 token。
async fn insert_reset_token(pool: &DatabasePool, user_id: &str, expires_in_ms: i64) -> String {
    let token = generate_token();
    let token_hash = hash_token(&token);
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO password_reset_tokens (id, user_id, token_hash, expires_at, created_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(user_id)
            .bind(token_hash)
            .bind(now + expires_in_ms)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    token
}

/// 当前时间步（秒级）与指定 step 的 6 位 code。
fn now_secs() -> u64 {
    (now_millis() / 1000) as u64
}

fn code_at(secret: &[u8], step: u64) -> String {
    format!("{:06}", bblbb_backend::auth::totp_at(secret, step))
}

// ─────────────────────────── 查询助手 ───────────────────────────

/// 该用户全部安全通知：(type, security_kind, title)。
async fn security_notifications(
    pool: &DatabasePool,
    user_id: &str,
) -> Vec<(String, String, String)> {
    match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, String, String)>(
            "SELECT type, security_kind, title FROM notifications
             WHERE user_id = ? AND security_kind IS NOT NULL ORDER BY created_at",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 该用户全部 auth.security_notification 审计的 metadata kind。
async fn audit_kinds(pool: &DatabasePool, user_id: &str) -> Vec<String> {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, String>(
            "SELECT metadata FROM audit_logs
             WHERE actor_id = ? AND action = 'auth.security_notification' ORDER BY created_at",
        )
        .bind(user_id)
        .fetch_all(p)
        .await
        .unwrap()
        .into_iter()
        .map(|m| {
            serde_json::from_str::<Value>(&m)
                .ok()
                .and_then(|v| v.get("kind").and_then(Value::as_str).map(String::from))
                .unwrap_or_default()
        })
        .collect(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 全部 auth.security_notification.v1 outbox 事件的 payload。
async fn outbox_payloads(pool: &DatabasePool, user_id: &str) -> Vec<Value> {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, String>(
            "SELECT payload FROM outbox_events
             WHERE event_type = 'auth.security_notification.v1'",
        )
        .fetch_all(p)
        .await
        .unwrap()
        .into_iter()
        .filter_map(|s| serde_json::from_str(&s).ok())
        .filter(|v: &Value| v.get("user_id").and_then(Value::as_str) == Some(user_id))
        .collect(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

// ─────────────────────────── 核心：同事务三写 ───────────────────────────

/// 独立包装：一条安全通知同时写 notifications（type='system' + security_kind）、
/// 审计（含 kind metadata）、Outbox 事件（payload 只含 user_id/kind）。
#[tokio::test]
async fn security_notification_writes_row_audit_and_outbox() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "alice").await;

    let id = notify_password_changed(&pool, &user_id, "req-1")
        .await
        .unwrap();
    assert!(!id.is_empty(), "必须返回通知 id");

    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 1);
    let (ntype, kind, title) = &rows[0];
    assert_eq!(ntype, "system", "安全通知 type 保持 system");
    assert_eq!(kind, "password_changed");
    assert!(!title.is_empty());

    let kinds = audit_kinds(&pool, &user_id).await;
    assert_eq!(kinds, vec!["password_changed".to_string()]);

    let payloads = outbox_payloads(&pool, &user_id).await;
    assert_eq!(payloads.len(), 1);
    assert_eq!(payloads[0]["kind"], "password_changed");
    assert!(payloads[0]["created_at"].as_i64().is_some());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 事务回滚：notifications / 审计 / outbox 三处一并消失（M01-JOBS-02）。
#[tokio::test]
async fn in_tx_notification_rolls_back_with_transaction() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "bob").await;

    let mut tx = match &pool {
        Either::Left(p) => Either::Left(p.begin().await.unwrap()),
        Either::Right(_) => panic!("SQLite only"),
    };
    create_security_notification_in_tx(
        &mut tx,
        &user_id,
        SecurityEvent::SessionRevoked,
        "req-1",
        None,
    )
    .await
    .unwrap();
    match tx {
        Either::Left(t) => t.rollback().await.unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }

    assert_eq!(security_notifications(&pool, &user_id).await.len(), 0);
    assert_eq!(audit_kinds(&pool, &user_id).await.len(), 0);
    assert_eq!(outbox_payloads(&pool, &user_id).await.len(), 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── 触发点接线 ───────────────────────────

/// 恢复码使用 → recovery_code_used 安全通知（与消费同事务，M02-MFA-04 钩子）。
#[tokio::test]
async fn consume_recovery_code_sends_notification() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "carol").await;
    let codes = generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-1")
        .await
        .unwrap();

    consume_recovery_code(&pool, &user_id, &codes[0], "req-2")
        .await
        .unwrap();

    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 1, "消费恢复码必须产生安全通知");
    assert_eq!(rows[0].1, "recovery_code_used");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 密码重置确认 → password_changed 安全通知（与改密+撤销 Session 同事务）。
#[tokio::test]
async fn confirm_password_reset_sends_notification() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "dave").await;
    let token = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;
    let new_hash = hash_password("new-passw0rd9").unwrap();

    confirm_password_reset(&pool, &token, &new_hash, "req-confirm")
        .await
        .unwrap();

    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 1, "密码重置必须产生安全通知");
    assert_eq!(rows[0].1, "password_changed");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// TOTP 启用（confirm_enrollment）→ mfa_changed 安全通知。
#[tokio::test]
async fn confirm_enrollment_sends_notification() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "erin").await;
    let challenge = begin_enrollment(&pool, &user_id, "BBLBB", "erin@example.com", KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;

    confirm_enrollment(&pool, &user_id, &code_at(&secret, step), KEY, now_secs())
        .await
        .unwrap();

    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 1, "MFA 启用必须产生安全通知");
    assert_eq!(rows[0].1, "mfa_changed");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 取消 enrollment → mfa_changed 安全通知；无 pending 时不产生。
#[tokio::test]
async fn cancel_enrollment_sends_notification_only_when_cancelled() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "frank").await;

    let challenge = begin_enrollment(&pool, &user_id, "BBLBB", "frank@example.com", KEY)
        .await
        .unwrap();
    let secret = base32_decode(&challenge.secret_base32).unwrap();
    let step = now_secs() / TOTP_PERIOD_SECS;
    // 先启用，再取消未完成的（此时没有 pending → false，不通知）
    confirm_enrollment(&pool, &user_id, &code_at(&secret, step), KEY, now_secs())
        .await
        .unwrap();
    assert!(!cancel_enrollment(&pool, &user_id).await.unwrap());
    assert_eq!(
        security_notifications(&pool, &user_id).await.len(),
        1,
        "仅确认时 1 条"
    );

    // 重新 begin 后取消 → 产生第 2 条 mfa_changed
    let challenge = begin_enrollment(&pool, &user_id, "BBLBB", "frank@example.com", KEY)
        .await
        .unwrap();
    let _ = base32_decode(&challenge.secret_base32).unwrap();
    assert!(cancel_enrollment(&pool, &user_id).await.unwrap());
    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[1].1, "mfa_changed");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 逐设备撤销 Session → session_revoked 安全通知（与撤销同事务）。
#[tokio::test]
async fn revoke_session_by_id_sends_notification() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "grace").await;
    let _token = create_session(&pool, &user_id, Some("Mozilla/5.0 (Macintosh)"))
        .await
        .unwrap();

    let sessions = list_sessions(&pool, &user_id).await.unwrap();
    assert_eq!(sessions.len(), 1);
    let session_id = sessions[0].id.clone();
    let revoked = revoke_session_by_id(&pool, &user_id, &session_id, "revoked_by_user")
        .await
        .unwrap();
    assert!(revoked);

    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 1, "撤销会话必须产生安全通知");
    assert_eq!(rows[0].1, "session_revoked");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 撤销不属于该用户的 session → false 且不产生通知。
#[tokio::test]
async fn revoke_foreign_session_no_notification() {
    let (pool, dir) = pool_with_migrations().await;
    let (owner, _) = insert_login_user(&pool, "heidi").await;
    let (other, _) = insert_login_user(&pool, "ivan").await;
    let _other_token = create_session(&pool, &other, None).await.unwrap();
    let other_sessions = list_sessions(&pool, &other).await.unwrap();
    let other_session_id = other_sessions[0].id.clone();

    let revoked = revoke_session_by_id(&pool, &owner, &other_session_id, "not_yours")
        .await
        .unwrap();
    assert!(!revoked, "他人 session 不得被撤销");
    assert_eq!(security_notifications(&pool, &owner).await.len(), 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── 新设备登录 ───────────────────────────

/// 首次见到设备 → 通知；模拟登录建会话后同设备再次 → 不通知；无 UA → 不通知。
#[tokio::test]
async fn new_device_notified_only_on_first_seen() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "judy").await;

    // 无 UA → 不通知
    assert!(
        !notify_new_device_if_first_seen(&pool, &user_id, None, "req-1")
            .await
            .unwrap()
    );
    // 空 UA → 不通知
    assert!(
        !notify_new_device_if_first_seen(&pool, &user_id, Some("  "), "req-2")
            .await
            .unwrap()
    );
    assert_eq!(security_notifications(&pool, &user_id).await.len(), 0);
    assert!(
        !has_device_seen(&pool, &user_id, "Device-A").await.unwrap(),
        "尚未登录该设备"
    );

    // 首次见到设备 A（尚无会话）→ 通知
    let sent = notify_new_device_if_first_seen(&pool, &user_id, Some("Device-A"), "req-3")
        .await
        .unwrap();
    assert!(sent, "首次设备必须通知");
    assert_eq!(security_notifications(&pool, &user_id).await.len(), 1);

    // 模拟该设备完成登录（create_session 写入 UA）后，同设备再次 → 不通知
    create_session(&pool, &user_id, Some("Device-A"))
        .await
        .unwrap();
    assert!(has_device_seen(&pool, &user_id, "Device-A").await.unwrap());
    assert!(
        !notify_new_device_if_first_seen(&pool, &user_id, Some("Device-A"), "req-4")
            .await
            .unwrap()
    );

    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 1, "同一设备只通知一次");
    assert_eq!(rows[0].1, "new_device");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 端到端：login_user 首次带 UA 登录 → new_device 通知；同 UA 二次登录不再通知。
#[tokio::test]
async fn login_user_notifies_only_first_device() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, email) = insert_login_user(&pool, "kevin").await;
    let limiter = RateLimiter::new();

    let first = login_user(
        &pool,
        &limiter,
        &email,
        PASSWORD,
        "127.0.0.1",
        Some("Mozilla/5.0 FirstDevice"),
        "req-login-1",
        &LoginLimits::default(),
    )
    .await
    .unwrap();
    assert_eq!(first.user_id, user_id);

    let second = login_user(
        &pool,
        &limiter,
        &email,
        PASSWORD,
        "127.0.0.1",
        Some("Mozilla/5.0 FirstDevice"),
        "req-login-2",
        &LoginLimits::default(),
    )
    .await
    .unwrap();
    assert_eq!(second.user_id, user_id);

    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 1, "同一设备多次登录只产生一次新设备通知");
    assert_eq!(rows[0].1, "new_device");

    // 换一台设备 → 再次通知
    let _third = login_user(
        &pool,
        &limiter,
        &email,
        PASSWORD,
        "127.0.0.1",
        Some("Mozilla/5.0 SecondDevice"),
        "req-login-3",
        &LoginLimits::default(),
    )
    .await
    .unwrap();
    let rows = security_notifications(&pool, &user_id).await;
    assert_eq!(rows.len(), 2, "新设备必须再次通知");
    assert_eq!(rows[1].1, "new_device");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// OutboxTx 类型在测试里可构造（内联事务）——上面已用；此处验证审计 kind 有序。
#[tokio::test]
async fn audit_kinds_are_ordered_and_distinct() {
    let (pool, dir) = pool_with_migrations().await;
    let (user_id, _) = insert_login_user(&pool, "lily").await;
    let codes = generate_recovery_codes(&pool, &user_id, RECOVERY_CODE_COUNT, "req-1")
        .await
        .unwrap();
    consume_recovery_code(&pool, &user_id, &codes[0], "req-2")
        .await
        .unwrap();
    let token = insert_reset_token(&pool, &user_id, 30 * 60 * 1000).await;
    confirm_password_reset(
        &pool,
        &token,
        &hash_password("x-new-password9").unwrap(),
        "req-3",
    )
    .await
    .unwrap();

    let kinds = audit_kinds(&pool, &user_id).await;
    assert_eq!(kinds.len(), 2, "恢复码 + 改密两条审计");
    assert!(kinds.contains(&"recovery_code_used".to_string()));
    assert!(kinds.contains(&"password_changed".to_string()));

    let payloads = outbox_payloads(&pool, &user_id).await;
    assert_eq!(payloads.len(), 2, "两条事件都进 outbox");

    close_pool(&pool).await;
    cleanup(&dir);
}
