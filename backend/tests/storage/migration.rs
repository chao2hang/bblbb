//! M06-MIGRATION：迁移 manifest、预演、复制校验与回滚测试（SQLite + local）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::storage::migration::{build_manifest, dry_run, rollback, run_migration};
use bblbb_backend::storage::model::StorageBackend;
use bblbb_backend::storage::StorageService;
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

async fn setup() -> (DatabasePool, PathBuf, StorageService) {
    let dir = std::env::temp_dir().join(format!("bblbb-mig-{}", uuid::Uuid::now_v7()));
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

/// 造一个带真实本地对象的附件。
async fn insert_attachment(
    pool: &DatabasePool,
    storage: &StorageService,
    owner_id: &str,
) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let key = format!("u/{owner_id}/{}/file.bin", uuid::Uuid::now_v7());
    let data = format!("migration-payload-{id}").into_bytes();
    let sha = hex::encode({
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&data);
        h.finalize().to_vec()
    });
    let adapter = storage
        .adapter(StorageBackend::Local)
        .expect("local adapter");
    adapter.write_object(&key, &data, None).await.unwrap();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO attachments
                     (id, owner_id, storage_backend, storage_key, original_name, media_type, size_bytes, sha256, status, quota_bytes_charged, is_public, ref_count, processing_version, created_at)
                 VALUES (?, ?, 'local', ?, 'f.bin', 'application/octet-stream', ?, ?, 'ready', 14, 0, 0, 0, ?)",
            )
            .bind(&id)
            .bind(owner_id)
            .bind(&key)
            .bind(data.len() as i64)
            .bind(&sha)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(p) => {
            sqlx::query(
                "INSERT INTO attachments
                     (id, owner_id, storage_backend, storage_key, original_name, media_type, size_bytes, sha256, status, quota_bytes_charged, is_public, ref_count, processing_version, created_at)
                 VALUES (?, ?, 'local', ?, 'f.bin', 'application/octet-stream', ?, ?, 'ready', 14, 0, 0, 0, ?)",
            )
            .bind(&id)
            .bind(owner_id)
            .bind(&key)
            .bind(data.len() as i64)
            .bind(&sha)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
    }
    id
}

#[tokio::test]
async fn manifest_builds_from_local_attachments() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    insert_attachment(&pool, &storage, &owner).await;
    insert_attachment(&pool, &storage, &owner).await;
    let manifest = build_manifest(&pool, StorageBackend::Local).await.unwrap();
    assert_eq!(manifest.len(), 2, "本地附件应进入 manifest");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn dry_run_detects_missing_source_object() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    let a1 = insert_attachment(&pool, &storage, &owner).await;
    let a2 = insert_attachment(&pool, &storage, &owner).await;
    let manifest = build_manifest(&pool, StorageBackend::Local).await.unwrap();
    // (a) 模拟源对象缺失：删掉 a1 的真实文件 → dry_run 报 not_found。
    let key = manifest
        .iter()
        .find(|e| e.attachment_id == a1)
        .unwrap()
        .object_key
        .clone();
    let adapter = storage.adapter(StorageBackend::Local).unwrap();
    adapter.delete_object(&key).await.unwrap();
    let err = dry_run(&storage, &manifest).await.unwrap_err();
    assert_eq!(err.code(), "not_found", "源对象缺失应报 not_found");
    // (b) a2 源对象仍存在，但 manifest 里 size 不符 → storage_hash_mismatch。
    let mut m: Vec<_> = build_manifest(&pool, StorageBackend::Local)
        .await
        .unwrap()
        .into_iter()
        .filter(|e| e.attachment_id == a2)
        .collect();
    m[0].size_bytes = 0; // 触发 size mismatch
    let err2 = dry_run(&storage, &m).await.unwrap_err();
    assert_eq!(err2.code(), "storage_hash_mismatch");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn run_migration_copies_and_marks_verified() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    insert_attachment(&pool, &storage, &owner).await;
    let manifest = build_manifest(&pool, StorageBackend::Local).await.unwrap();
    // local→local：复制到同一后端（key 相同，幂等跳过），验证报告。
    let report = run_migration(&pool, &storage, StorageBackend::Local, &manifest)
        .await
        .unwrap();
    assert_eq!(report.total, 1);
    assert!(report.verified >= 1, "应校验通过");
    assert!(report.failed.is_empty());
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn rollback_restores_backend_metadata() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    insert_attachment(&pool, &storage, &owner).await;
    let manifest = build_manifest(&pool, StorageBackend::Local).await.unwrap();
    let report = rollback(&pool, &storage, StorageBackend::Local, &manifest)
        .await
        .unwrap();
    assert_eq!(report.verified, 1);
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn repeated_migration_is_idempotent() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner").await;
    insert_attachment(&pool, &storage, &owner).await;
    let manifest = build_manifest(&pool, StorageBackend::Local).await.unwrap();
    let r1 = run_migration(&pool, &storage, StorageBackend::Local, &manifest)
        .await
        .unwrap();
    let r2 = run_migration(&pool, &storage, StorageBackend::Local, &manifest)
        .await
        .unwrap();
    assert_eq!(r1.verified, 1);
    assert_eq!(r2.verified, 1, "重复运行应幂等跳过");
    close_pool(&pool).await;
    cleanup(&dir);
}
