//! M06-DOWNLOAD：策略解析、授权、扣费、幂等与 URL 重签测试（SQLite）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::download::service::{download, get_authorization, sign_url, DownloadError};
use bblbb_backend::economy::ledger::service as ledger;
use bblbb_backend::economy::ledger::service::{LedgerKind, CURRENCY_COIN};
use bblbb_backend::outbox::now_millis;
use bblbb_backend::storage::StorageService;
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

async fn setup() -> (DatabasePool, PathBuf, StorageService) {
    let dir = std::env::temp_dir().join(format!("bblbb-dl-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    let dir = dir.canonicalize().unwrap();
    let url = format!("sqlite://{}", dir.join("db.sqlite").display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
    let storage = StorageService::local_only(dir.join("uploads")).unwrap();
    (pool, dir, storage)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
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
                 VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    user_id
}

/// 造一个 ready 的 local 附件（storage_key 指向真实文件）。
async fn insert_attachment(
    pool: &DatabasePool,
    storage: &StorageService,
    owner_id: &str,
    backend: &str,
    status: &str,
) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let key = format!("u/{owner_id}/{}/file.bin", uuid::Uuid::now_v7());
    let now = now_millis();
    // 写入对象内容
    let data = b"hello download";
    let adapter = storage
        .adapter(bblbb_backend::storage::model::StorageBackend::Local)
        .unwrap();
    adapter.write_object(&key, data, None).await.unwrap();
    let sha = hex::encode({
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(data);
        h.finalize().to_vec()
    });
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO attachments
                     (id, owner_id, storage_backend, storage_key, original_name, media_type, size_bytes, sha256, status, quota_bytes_charged, is_public, ref_count, processing_version, created_at)
                 VALUES (?, ?, ?, ?, 'f.bin', 'application/octet-stream', ?, ?, ?, 14, 0, 0, 0, ?)",
            )
            .bind(&id)
            .bind(owner_id)
            .bind(backend)
            .bind(&key)
            .bind(data.len() as i64)
            .bind(&sha)
            .bind(status)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO attachments
                     (id, owner_id, storage_backend, storage_key, original_name, media_type, size_bytes, sha256, status, quota_bytes_charged, is_public, ref_count, processing_version, created_at)
                 VALUES (?, ?, ?, ?, 'f.bin', 'application/octet-stream', ?, ?, ?, 14, 0, 0, 0, ?)",
            )
            .bind(&id)
            .bind(owner_id)
            .bind(backend)
            .bind(&key)
            .bind(data.len() as i64)
            .bind(&sha)
            .bind(status)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    id
}

/// 配置附件级计费策略。
async fn set_attachment_policy(pool: &DatabasePool, attachment_id: &str, mode: &str, amount: i64) {
    let now = now_millis();
    let id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO download_billing_policies
                     (id, scope_type, scope_id, mode, currency_id, amount, authorization_ttl_seconds, grace_on_disable, version, is_enabled, created_at, updated_at)
                 VALUES (?, 'attachment', ?, ?, ?, ?, 3600, 1, 1, 1, ?, ?)",
            )
            .bind(&id)
            .bind(attachment_id)
            .bind(mode)
            .bind(CURRENCY_COIN)
            .bind(amount)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO download_billing_policies
                     (id, scope_type, scope_id, mode, currency_id, amount, authorization_ttl_seconds, grace_on_disable, version, is_enabled, created_at, updated_at)
                 VALUES (?, 'attachment', ?, ?, ?, ?, 3600, 1, 1, 1, ?, ?)",
            )
            .bind(&id)
            .bind(attachment_id)
            .bind(mode)
            .bind(CURRENCY_COIN)
            .bind(amount)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
}

async fn credit_user(pool: &DatabasePool, user_id: &str, amount: i64) {
    let now = now_millis();
    let cmd = ledger::LedgerCommand {
        idempotency_scope: "test".to_string(),
        idempotency_key: uuid::Uuid::now_v7().to_string(),
        kind: LedgerKind::Award,
        actor_id: Some(user_id.to_string()),
        user_id: user_id.to_string(),
        currency_id: CURRENCY_COIN.to_string(),
        delta_balance: amount,
        delta_frozen: 0,
        source_type: None,
        source_id: None,
        memo: "test credit".to_string(),
        reverses_operation_id: None,
    };
    ledger::apply_operation(pool, cmd, now).await.unwrap();
}

async fn balance_of(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(CURRENCY_COIN)
        .fetch_optional(p)
        .await
        .unwrap()
        .unwrap_or(0),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT balance FROM point_accounts WHERE user_id = ? AND currency_id = ?",
        )
        .bind(user_id)
        .bind(CURRENCY_COIN)
        .fetch_optional(p)
        .await
        .unwrap()
        .unwrap_or(0),
    }
}

#[tokio::test]
async fn free_download_creates_authorization_without_charge() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let att = insert_attachment(&pool, &storage, &owner, "local", "ready").await;
    set_attachment_policy(&pool, &att, "free", 0).await;

    let result = download(&pool, &storage, &user, &att, "idem-1")
        .await
        .unwrap();
    let auth_id = result["authorization_id"].as_str().unwrap();
    assert_eq!(result["local"], true);
    assert_eq!(result["url"], format!("/api/v1/attachments/{att}/content"));
    let auth = get_authorization(&pool, &user, auth_id).await.unwrap();
    assert_eq!(auth["status"], "active");
    assert_eq!(auth["charged_amount"], 0);
    assert_eq!(balance_of(&pool, &user).await, 0, "免费下载不扣款");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn paid_download_charges_and_is_idempotent() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let att = insert_attachment(&pool, &storage, &owner, "local", "ready").await;
    set_attachment_policy(&pool, &att, "fixed", 100).await;
    credit_user(&pool, &user, 1000).await;

    let first = download(&pool, &storage, &user, &att, "idem-pay")
        .await
        .unwrap();
    assert_eq!(balance_of(&pool, &user).await, 900);
    // 同幂等键重放：返回原授权，不重复扣款。
    let second = download(&pool, &storage, &user, &att, "idem-pay")
        .await
        .unwrap();
    assert_eq!(first["authorization_id"], second["authorization_id"]);
    assert_eq!(balance_of(&pool, &user).await, 900, "不得重复扣款");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn insufficient_balance_rolls_back_no_authorization() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let att = insert_attachment(&pool, &storage, &owner, "local", "ready").await;
    set_attachment_policy(&pool, &att, "fixed", 100).await;
    // 不给钱
    let err = download(&pool, &storage, &user, &att, "idem-poor")
        .await
        .unwrap_err();
    assert!(matches!(err, DownloadError::Forbidden(_)));
    let count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM download_authorizations")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(p) => sqlx::query_scalar("SELECT COUNT(*) FROM download_authorizations")
            .fetch_one(p)
            .await
            .unwrap(),
    };
    assert_eq!(count, 0, "失败不得留下授权");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn not_ready_attachment_never_leaks() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let att = insert_attachment(&pool, &storage, &owner, "local", "pending").await;
    set_attachment_policy(&pool, &att, "free", 0).await;
    let err = download(&pool, &storage, &user, &att, "idem-nr")
        .await
        .unwrap_err();
    assert!(
        matches!(err, DownloadError::NotFound(_)),
        "未 ready 必须 NotFound"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn disabled_policy_rejects_download() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let att = insert_attachment(&pool, &storage, &owner, "local", "ready").await;
    set_attachment_policy(&pool, &att, "disabled", 0).await;
    let err = download(&pool, &storage, &user, &att, "idem-dis")
        .await
        .unwrap_err();
    assert!(matches!(err, DownloadError::Forbidden(_)));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn sign_url_after_authorization_does_not_recharge() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let att = insert_attachment(&pool, &storage, &owner, "local", "ready").await;
    set_attachment_policy(&pool, &att, "fixed", 100).await;
    credit_user(&pool, &user, 1000).await;

    let first = download(&pool, &storage, &user, &att, "idem-sign")
        .await
        .unwrap();
    let auth_id = first["authorization_id"].as_str().unwrap();
    let before = balance_of(&pool, &user).await;
    let re = sign_url(&pool, &storage, &user, auth_id).await.unwrap();
    assert_eq!(re["authorization_id"], first["authorization_id"]);
    let after = balance_of(&pool, &user).await;
    assert_eq!(before, after, "重签不重复扣款");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn audit_and_outbox_written_in_same_tx() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user = insert_user(&pool, "user").await;
    let att = insert_attachment(&pool, &storage, &owner, "local", "ready").await;
    set_attachment_policy(&pool, &att, "free", 0).await;
    download(&pool, &storage, &user, &att, "idem-audit")
        .await
        .unwrap();

    let audits: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE actor_id = ? AND action = 'download.authorize'",
        )
        .bind(&user)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE actor_id = ? AND action = 'download.authorize'",
        )
        .bind(&user)
        .fetch_one(p)
        .await
        .unwrap(),
    };
    assert_eq!(audits, 1);
    let outbox: i64 = match &pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE event_type = 'download.authorization_created.v1'")
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM outbox_events WHERE event_type = 'download.authorization_created.v1'")
                .fetch_one(p)
                .await
                .unwrap()
        }
    };
    assert_eq!(outbox, 1);
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn concurrent_downloads_of_free_attachment_both_succeed() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let user1 = insert_user(&pool, "u1").await;
    let user2 = insert_user(&pool, "u2").await;
    let att = insert_attachment(&pool, &storage, &owner, "local", "ready").await;
    set_attachment_policy(&pool, &att, "free", 0).await;
    let (r1, r2) = tokio::join!(
        download(&pool, &storage, &user1, &att, "idem-c1"),
        download(&pool, &storage, &user2, &att, "idem-c2"),
    );
    assert!(r1.is_ok() && r2.is_ok(), "免费并发下载都应成功");
    close_pool(&pool).await;
    cleanup(&dir);
}
