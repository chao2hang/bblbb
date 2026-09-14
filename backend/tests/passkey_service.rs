//! M02-MFA-PK：Passkey 第二因素服务级测试（sqlite + 软件认证器）。
//!
//! 覆盖：
//! - 注册 begin/confirm 往返（webauthn-rs state 存取、凭据落库）；
//! - 登录断言往返（challenge 绑定 MFA login challenge、一次性消费）；
//! - 撤销后登录 options 拒绝（NoCredentials）；
//! - `has_second_factor` 聚合（TOTP 或 Passkey 任一）；
//! - `complete_mfa_login` 的 Passkey 分支（三选一 OR 语义，签发会话）。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::{
    begin_passkey_login, begin_passkey_registration, build_webauthn, complete_mfa_login,
    confirm_passkey_registration, has_active_passkey, has_second_factor, revoke_passkey,
    start_mfa_login, verify_passkey_login, PasskeyError,
};
use bblbb_backend::config::AppConfig;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;
use webauthn_authenticator_rs::prelude::Url;
use webauthn_authenticator_rs::softpasskey::SoftPasskey;
use webauthn_authenticator_rs::WebauthnAuthenticator;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-passkey-{}", uuid::Uuid::now_v7()));
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
        Either::Right(p) => {
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
    }
    user_id
}

fn test_config() -> AppConfig {
    AppConfig {
        passkey_rp_id: "localhost".to_owned(),
        passkey_rp_name: "BBLBB".to_owned(),
        public_origin: "http://localhost:8080".to_owned(),
        ..AppConfig::default()
    }
}

/// 注册一把 Passkey（begin → 软件认证器 → confirm），返回认证器（可继续断言）。
async fn enroll_passkey(pool: &DatabasePool, user_id: &str) -> WebauthnAuthenticator<SoftPasskey> {
    let webauthn = build_webauthn(&test_config()).expect("webauthn instance");
    let ccr = begin_passkey_registration(pool, &webauthn, user_id, "passkey_user", "passkey_user")
        .await
        .expect("begin passkey registration");

    let mut authenticator = WebauthnAuthenticator::new(SoftPasskey::new(true));
    let origin = Url::parse("http://localhost:8080").unwrap();
    let reg = authenticator
        .do_registration(origin, ccr)
        .expect("software authenticator registration");
    let credential = serde_json::to_value(&reg).unwrap();
    confirm_passkey_registration(pool, &webauthn, user_id, Some("测试密钥"), &credential)
        .await
        .expect("confirm passkey registration");
    authenticator
}

#[tokio::test]
async fn register_roundtrip_and_login_assertion() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "reg").await;
    assert!(!has_active_passkey(&pool, &user_id).await.unwrap());
    assert!(!has_second_factor(&pool, &user_id).await.unwrap());

    let mut authenticator = enroll_passkey(&pool, &user_id).await;
    assert!(has_active_passkey(&pool, &user_id).await.unwrap());
    // Passkey 注册即满足第二因素（与 TOTP OR 共存）
    assert!(has_second_factor(&pool, &user_id).await.unwrap());

    let webauthn = build_webauthn(&test_config()).unwrap();
    let origin = Url::parse("http://localhost:8080").unwrap();

    // ── 断言往返：challenge 绑定本次两步登录，验证成功后一次性消费 ──
    let login_token = start_mfa_login(&pool, &user_id, false).await.unwrap();
    let rcr = begin_passkey_login(&pool, &webauthn, &login_token)
        .await
        .expect("begin passkey login");
    let assertion = authenticator
        .do_authentication(origin.clone(), rcr)
        .expect("software authenticator assertion");
    let assertion_value = serde_json::to_value(&assertion).unwrap();
    verify_passkey_login(&pool, &webauthn, &login_token, &user_id, &assertion_value)
        .await
        .expect("verify passkey login");

    // 同一断言重放 → challenge 已消费，统一失败（防重放）
    let err = verify_passkey_login(&pool, &webauthn, &login_token, &user_id, &assertion_value)
        .await
        .expect_err("replayed assertion must fail");
    assert!(matches!(err, PasskeyError::NoPendingChallenge), "{err:?}");

    // ── complete_mfa_login 的 Passkey 分支：三选一 OR 语义，签发会话 ──
    let login_token = start_mfa_login(&pool, &user_id, false).await.unwrap();
    let rcr = begin_passkey_login(&pool, &webauthn, &login_token)
        .await
        .expect("begin passkey login (complete)");
    let assertion = authenticator
        .do_authentication(origin, rcr)
        .expect("software authenticator assertion (complete)");
    let assertion_value = serde_json::to_value(&assertion).unwrap();
    let completed = complete_mfa_login(
        &pool,
        &login_token,
        None,
        None,
        Some(&assertion_value),
        None,
        b"test-encryption-key-material",
        Some(&webauthn),
        "passkey-test",
    )
    .await
    .expect("complete mfa login with passkey");
    assert_eq!(completed.user_id, user_id);
    assert!(!completed.session_token.is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn login_options_rejects_without_challenge_or_credentials() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "opts").await;
    let webauthn = build_webauthn(&test_config()).unwrap();

    // MFA challenge 无效 → NoPendingChallenge（统一，防枚举）
    let err = begin_passkey_login(&pool, &webauthn, "not-a-real-token")
        .await
        .expect_err("invalid challenge must fail");
    assert!(matches!(err, PasskeyError::NoPendingChallenge), "{err:?}");

    // 有效 challenge 但未注册 Passkey → NoCredentials
    let login_token = start_mfa_login(&pool, &user_id, false).await.unwrap();
    let err = begin_passkey_login(&pool, &webauthn, &login_token)
        .await
        .expect_err("no credentials must fail");
    assert!(matches!(err, PasskeyError::NoCredentials), "{err:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn revoke_disables_second_factor() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "revoke").await;
    let _authenticator = enroll_passkey(&pool, &user_id).await;
    assert!(has_second_factor(&pool, &user_id).await.unwrap());

    let webauthn = build_webauthn(&test_config()).unwrap();
    let login_token = start_mfa_login(&pool, &user_id, false).await.unwrap();
    begin_passkey_login(&pool, &webauthn, &login_token)
        .await
        .expect("login options before revoke");

    // 撤销唯一一把 Passkey
    let passkeys = bblbb_backend::auth::passkey::list_passkeys(&pool, &user_id)
        .await
        .unwrap();
    assert_eq!(passkeys.len(), 1);
    assert_eq!(passkeys[0].name, "测试密钥");
    assert!(revoke_passkey(&pool, &user_id, &passkeys[0].id)
        .await
        .unwrap());
    assert!(!has_active_passkey(&pool, &user_id).await.unwrap());
    assert!(!has_second_factor(&pool, &user_id).await.unwrap());

    // 撤销后登录 options → NoCredentials（不再提供 Passkey 入口）
    let login_token = start_mfa_login(&pool, &user_id, false).await.unwrap();
    let err = begin_passkey_login(&pool, &webauthn, &login_token)
        .await
        .expect_err("revoked passkey must not provide login options");
    assert!(matches!(err, PasskeyError::NoCredentials), "{err:?}");

    // 重复撤销 → NotFound 语义（false）
    assert!(!revoke_passkey(&pool, &user_id, &passkeys[0].id)
        .await
        .unwrap());

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn webauthn_without_config_returns_none() {
    let config = AppConfig::default();
    assert!(build_webauthn(&config).is_none());
}
