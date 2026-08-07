//! M06-UPLOAD 真实测试：两阶段上传（create → stream → complete）、内容安全、
//! 隔离回滚、幂等、软删除/保留期清理、孤儿回收与并发预留。
//!
//! SQLite 全量迁移 + local 适配器（真实文件落盘）；服务层直调
//! （upload::create_attachment / stream_upload / complete_attachment / delete_attachment）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::storage::model::{AttachmentStatus, QuotaCounters, StorageBackend};
use bblbb_backend::storage::quota::{self, get_counters, get_policy_for_level, update_level_quota};
use bblbb_backend::storage::upload::{
    self, scan_for_safety, CompleteOutcome, CreateAttachmentInput, NoopVirusScan, ScanVerdict,
    UploadTransport, VirusScan,
};
use bblbb_backend::storage::StorageService;
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

async fn setup() -> (DatabasePool, PathBuf, StorageService) {
    let dir = std::env::temp_dir().join(format!("bblbb-upload-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(&dir).unwrap();
    // macOS 的 /var /tmp 是符号链接，本地适配器会阻断符号链接路径；
    // canonicalize 解析到真实路径（/private/...），避免误伤。
    let dir = std::fs::canonicalize(&dir).unwrap();
    let db_file = dir.join("test.sqlite");
    let url = format!("sqlite://{}", db_file.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
    let storage_dir = dir.join("uploads");
    let storage = StorageService::local_only(storage_dir).unwrap();
    (pool, dir, storage)
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_file(dir.join("test.sqlite"));
    let _ = std::fs::remove_file(dir.join("test.sqlite-wal"));
    let _ = std::fs::remove_file(dir.join("test.sqlite-shm"));
    let _ = std::fs::remove_dir_all(dir.join("uploads"));
    let _ = std::fs::remove_dir_all(dir);
}

async fn close_pool(pool: &DatabasePool) {
    match pool {
        Either::Left(p) => p.close().await,
        Either::Right(p) => p.close().await,
    }
}

async fn insert_user(pool: &DatabasePool, tag: &str, level: i64) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(level)
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

/// 读取单个 i64 标量（SQLite）。
async fn scalar_sqlite(pool: &DatabasePool, sql: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql).fetch_one(p).await.unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn counters_async(pool: &DatabasePool, user_id: &str) -> QuotaCounters {
    get_counters(pool, user_id).await.unwrap()
}

/// 走完整 happy path：create → stream → complete（text/plain）。
async fn upload_ready(
    pool: &DatabasePool,
    storage: &StorageService,
    user_id: &str,
    filename: &str,
    data: &[u8],
) -> bblbb_backend::storage::model::AttachmentRecord {
    let created = upload::create_attachment(
        pool,
        storage,
        user_id,
        CreateAttachmentInput {
            owner_id: user_id.to_string(),
            original_name: Some(filename.to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: data.len() as i64,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    upload::stream_upload(
        pool,
        storage,
        &created.attachment.id,
        user_id,
        data,
        Some("text/plain"),
    )
    .await
    .unwrap();
    let outcome = upload::complete_attachment(
        pool,
        storage,
        &created.attachment.id,
        user_id,
        &NoopVirusScan,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(outcome, CompleteOutcome::Ready);
    upload::load_attachment(pool, &created.attachment.id)
        .await
        .unwrap()
        .unwrap()
}

// ────────────────────────── create 阶段（M06-UPLOAD-01/02）──────────────

#[tokio::test]
async fn create_reserves_quota_and_returns_stream_transport() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;

    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("note.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 11,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();

    assert_eq!(created.attachment.owner_id, owner);
    assert_eq!(created.attachment.status, AttachmentStatus::Pending);
    assert_eq!(created.attachment.size_bytes, 11);
    assert!(
        created.attachment.sha256.is_empty(),
        "未 complete 不泄漏 hash"
    );
    assert_eq!(created.transport, UploadTransport::Stream);
    assert!(
        created
            .attachment
            .storage_key
            .starts_with(&format!("u/{owner}/")),
        "object key 必须带 owner 前缀，got {}",
        created.attachment.storage_key
    );
    assert!(bblbb_backend::storage::model::is_safe_key(
        &created.attachment.storage_key
    ));

    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_reserved, 11, "create 阶段必须预留容量");
    assert_eq!(counters.bytes_charged, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn create_rejects_bad_input_and_over_limit_sizes() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;

    // size <= 0
    let err = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: None,
            media_type: "text/plain".to_string(),
            size_bytes: 0,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "invalid_storage_request");

    // 不在白名单的媒体类型
    let err = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: None,
            media_type: "text/html".to_string(),
            size_bytes: 10,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "invalid_storage_request");

    // 超过等级单文件上限（level 1 默认 2 MiB）
    let err = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: None,
            media_type: "text/plain".to_string(),
            size_bytes: 3 * 1024 * 1024,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "quota_exceeded");

    // 失败不得留下 reserved
    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_reserved, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn stream_upload_rejects_wrong_owner_size_and_content_type() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let other = insert_user(&pool, "other", 1).await;

    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("note.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 11,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();

    // 他人上传 → Forbidden
    let err = upload::stream_upload(
        &pool,
        &storage,
        &created.attachment.id,
        &other,
        b"hello world",
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "storage_forbidden");

    // 大小与 create 声明不一致 → Verification
    let err = upload::stream_upload(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        b"hello",
        None,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "storage_verification_failed");

    // Content-Type 与声明不一致 → Verification（octet-stream 放行）
    let err = upload::stream_upload(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        b"hello world",
        Some("image/jpeg"),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "storage_verification_failed");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── complete（M06-UPLOAD-04/05/08）──────────────

#[tokio::test]
async fn complete_happy_path_readies_attachment_with_hash_audit_outbox() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let attachment = upload_ready(&pool, &storage, &owner, "note.txt", b"hello world").await;

    assert_eq!(attachment.status, AttachmentStatus::Ready);
    let expected_sha = hex::encode({
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(b"hello world");
        h.finalize().to_vec()
    });
    assert_eq!(attachment.sha256, expected_sha);
    assert_eq!(
        attachment.processing_version, 1,
        "finalize 成功推进一次 version"
    );

    // 容量结算：reserved 归零、charged == size、released 记录流转
    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_reserved, 0);
    assert_eq!(counters.bytes_charged, 11);
    assert_eq!(counters.bytes_released, 11, "reserved 释放计入 released");

    // 对象已落盘且内容正确
    let adapter = storage.adapter(StorageBackend::Local).unwrap();
    let obj = adapter.read_object(&attachment.storage_key).await.unwrap();
    assert_eq!(obj, b"hello world");

    // 审计 + Outbox
    let audits: i64 = scalar_sqlite(
        &pool,
        "SELECT COUNT(*) FROM audit_logs WHERE action = 'attachment.complete'",
    )
    .await;
    assert_eq!(audits, 1, "complete 必须写审计");
    let events: i64 = scalar_sqlite(
        &pool,
        "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'attachment.ready.v1'",
    )
    .await;
    assert_eq!(events, 1, "complete 必须发 attachment.ready.v1 Outbox 事件");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn complete_is_idempotent_and_never_double_charges() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let attachment = upload_ready(&pool, &storage, &owner, "note.txt", b"hello world").await;

    // 第二次 complete：ready 重放成功，不重复结算/发事件
    let outcome = upload::complete_attachment(
        &pool,
        &storage,
        &attachment.id,
        &owner,
        &NoopVirusScan,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(outcome, CompleteOutcome::Ready);

    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_charged, 11, "ready 重放不得重复计费");
    let events: i64 = scalar_sqlite(
        &pool,
        "SELECT COUNT(*) FROM outbox_events WHERE event_type = 'attachment.ready.v1'",
    )
    .await;
    assert_eq!(events, 1, "ready 重放不得重复发事件");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn complete_quarantines_on_head_size_mismatch_and_rolls_back_reserved() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;

    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("note.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 11,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    // 绕过 stream_upload 直接写一个大小不符的对象（模拟 S3 直传被篡改）
    let adapter = storage.adapter(StorageBackend::Local).unwrap();
    adapter
        .write_object(&created.attachment.storage_key, b"short", None)
        .await
        .unwrap();

    let err = upload::complete_attachment(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        &NoopVirusScan,
        now_millis(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "storage_verification_failed");

    let attachment = upload::load_attachment(&pool, &created.attachment.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attachment.status, AttachmentStatus::Quarantined);
    assert!(attachment.processing_error.is_some());
    assert!(attachment.processing_version >= 1);

    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_reserved, 0, "隔离必须回滚 reserved");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn complete_quarantines_dangerous_html_and_dangerous_extension() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;

    // 内容为 HTML（polyglot/宏文档类攻击面）→ quarantined
    let html = b"<html><body>hi</body></html>";
    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("page.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: html.len() as i64,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    upload::stream_upload(&pool, &storage, &created.attachment.id, &owner, html, None)
        .await
        .unwrap();
    let outcome = upload::complete_attachment(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        &NoopVirusScan,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(outcome, CompleteOutcome::Quarantined);
    let attachment = upload::load_attachment(&pool, &created.attachment.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attachment.status, AttachmentStatus::Quarantined);
    assert!(
        attachment
            .processing_error
            .as_deref()
            .unwrap()
            .contains("blocked"),
        "processing_error 必须是安全摘要，got {:?}",
        attachment.processing_error
    );

    // 扩展名欺骗（.svg 不在 jpeg 合法扩展内且属于危险扩展）→ quarantined
    let png_bytes = b"hello world";
    let created2 = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("avatar.svg".to_string()),
            media_type: "image/jpeg".to_string(),
            size_bytes: png_bytes.len() as i64,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    upload::stream_upload(
        &pool,
        &storage,
        &created2.attachment.id,
        &owner,
        png_bytes,
        None,
    )
    .await
    .unwrap();
    let outcome2 = upload::complete_attachment(
        &pool,
        &storage,
        &created2.attachment.id,
        &owner,
        &NoopVirusScan,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(outcome2, CompleteOutcome::Quarantined);

    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_reserved, 0);
    assert_eq!(counters.bytes_charged, 0, "隔离附件不占 charged 容量");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 确定性病毒扫描 mock（M06-UPLOAD-05）。
struct InfectedScan;
impl VirusScan for InfectedScan {
    fn scan(&self, _data: &[u8]) -> ScanVerdict {
        ScanVerdict::Infected
    }
}

#[tokio::test]
async fn complete_quarantines_on_virus_scan_mock() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;

    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("note.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 11,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    upload::stream_upload(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        b"hello world",
        None,
    )
    .await
    .unwrap();
    let outcome = upload::complete_attachment(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        &InfectedScan,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(outcome, CompleteOutcome::Quarantined);
    let attachment = upload::load_attachment(&pool, &created.attachment.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attachment.status, AttachmentStatus::Quarantined);

    // quarantined 不可再 complete（状态机非法迁移防护）
    let err = upload::complete_attachment(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        &InfectedScan,
        now_millis(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "storage_state_error");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn complete_quarantines_when_policy_downgraded_below_commitment() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;

    // create 时等级 1 默认总容量 100 MiB，预留 11 字节成功
    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("note.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 11,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    upload::stream_upload(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        b"hello world",
        None,
    )
    .await
    .unwrap();

    // 管理员把等级 1 总容量压到 8 字节（当前版本 1 → 新版本 2）
    let policy = update_level_quota(&pool, 1, 8, 8, 8, 30, 1, &owner, now_millis())
        .await
        .unwrap();
    assert_eq!(policy.policy_version, 2);

    // complete 重检（M06-QUOTA-03）：committed 11 > total 8 → 拒绝且不超卖
    let err = upload::complete_attachment(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        &NoopVirusScan,
        now_millis(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "quota_exceeded");
    let attachment = upload::load_attachment(&pool, &created.attachment.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attachment.status, AttachmentStatus::Quarantined);
    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_reserved, 0, "降级拒绝后必须回滚 reserved");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── EXIF/GPS 剥离（M06-UPLOAD-09）──────────────

/// 最小 JPEG（SOI + APP1-Exif + SOF0 + EOI）。
fn jpeg_with_exif(width: u16, height: u16) -> Vec<u8> {
    let mut out = vec![0xFF, 0xD8];
    let exif_payload = b"Exif\0\0GPSLatitude=1.0;GPSLongitude=2.0";
    out.extend_from_slice(&0xFFE1u16.to_be_bytes());
    out.extend_from_slice(&((exif_payload.len() + 2) as u16).to_be_bytes());
    out.extend_from_slice(exif_payload);
    out.extend_from_slice(&0xFFC0u16.to_be_bytes());
    out.extend_from_slice(&17u16.to_be_bytes()); // SOF0 段长
    out.push(8); // precision
    out.extend_from_slice(&height.to_be_bytes());
    out.extend_from_slice(&width.to_be_bytes());
    out.push(3); // components
    out.extend_from_slice(&[1, 0x11, 0, 2, 0x11, 0, 3, 0x11, 0]);
    out.extend_from_slice(&0xFFD9u16.to_be_bytes());
    out
}

#[tokio::test]
async fn complete_strips_exif_rewrites_object_and_recomputes_sha256() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let jpeg = jpeg_with_exif(3, 2);

    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("photo.jpg".to_string()),
            media_type: "image/jpeg".to_string(),
            size_bytes: jpeg.len() as i64,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    upload::stream_upload(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        &jpeg,
        Some("image/jpeg"),
    )
    .await
    .unwrap();
    let outcome = upload::complete_attachment(
        &pool,
        &storage,
        &created.attachment.id,
        &owner,
        &NoopVirusScan,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(outcome, CompleteOutcome::Ready);

    let attachment = upload::load_attachment(&pool, &created.attachment.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(attachment.width, Some(3));
    assert_eq!(attachment.height, Some(2));

    // 对象被重写：Exif 段已剥离
    let adapter = storage.adapter(StorageBackend::Local).unwrap();
    let rewritten = adapter.read_object(&attachment.storage_key).await.unwrap();
    assert!(
        !rewritten.windows(6).any(|w| w == b"Exif\0\0"),
        "对象不得再含 Exif"
    );

    // sha256 与重写后的二进制一致（M06-UPLOAD-09 重算 hash）
    let expected = hex::encode({
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&rewritten);
        h.finalize().to_vec()
    });
    assert_eq!(attachment.sha256, expected);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── 删除 / 保留期 / 清理（M06-QUOTA-09/10）──────

#[tokio::test]
async fn delete_soft_deletes_and_releases_reserved_for_incomplete_upload() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;

    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("note.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 11,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(counters_async(&pool, &owner).await.bytes_reserved, 11);

    let _deleted = upload::delete_attachment(&pool, &owner, &created.attachment.id, now_millis())
        .await
        .unwrap();
    // delete_attachment 返回加载时的快照；重新读取确认落库状态
    let deleted = upload::load_attachment(&pool, &created.attachment.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(deleted.status, AttachmentStatus::Deleted);
    assert!(deleted.deleted_at.is_some());

    // 行仍在（30 天保留），引用清空
    let row: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM attachments WHERE id = ?")
            .bind(&created.attachment.id)
            .fetch_optional(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(row.as_deref(), Some(created.attachment.id.as_str()));

    // 未结算的预留立即回滚
    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_reserved, 0);
    assert_eq!(counters.bytes_charged, 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn purge_after_retention_releases_charged_and_removes_object() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let attachment = upload_ready(&pool, &storage, &owner, "note.txt", b"hello world").await;
    assert_eq!(counters_async(&pool, &owner).await.bytes_charged, 11);

    // 模拟软删除且已过 30 天保留期
    let old = now_millis() - 40 * 86_400_000;
    match &pool {
        Either::Left(p) => {
            sqlx::query("UPDATE attachments SET status = 'deleted', deleted_at = ? WHERE id = ?")
                .bind(old)
                .bind(&attachment.id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let summary = quota::purge_expired_deleted(&pool, &storage, now_millis())
        .await
        .unwrap();
    assert_eq!(summary.purged, 1);
    assert_eq!(summary.released_bytes, 11);

    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_charged, 0, "物理删除后释放 charged");
    assert_eq!(
        counters.bytes_released,
        11 + 11,
        "reserved 释放 + charged 释放"
    );

    // 对象物理删除
    let adapter = storage.adapter(StorageBackend::Local).unwrap();
    let head = adapter.head_object(&attachment.storage_key).await.unwrap();
    assert!(!head.exists, "保留期满必须物理删除对象");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn purge_skips_deleted_attachments_still_referenced() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let attachment = upload_ready(&pool, &storage, &owner, "note.txt", b"hello world").await;

    // 直接插入一条已删除 + 仍有引用的行（ref_count=1，attachment_links 一行）
    let now = now_millis();
    let old = now - 40 * 86_400_000;
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE attachments SET status = 'deleted', deleted_at = ?, ref_count = 1 WHERE id = ?",
            )
            .bind(old)
            .bind(&attachment.id)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO attachment_links (id, attachment_id, target_type, target_id, purpose, created_at)
                 VALUES (?, ?, 'post', 'p-1', 'cover', ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(&attachment.id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    let summary = quota::purge_expired_deleted(&pool, &storage, now)
        .await
        .unwrap();
    assert_eq!(summary.purged, 0);
    assert_eq!(summary.skipped_referenced, 1);
    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_charged, 11, "有引用不得释放容量");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn reap_stale_uploads_cleans_interrupted_transfers() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;

    // 中断上传：只有 pending 行，从未传输
    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("note.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 11,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(counters_async(&pool, &owner).await.bytes_reserved, 11);

    // 24h 后清理
    let later = now_millis() + 2 * 86_400_000;
    let reaped = upload::reap_stale_uploads(&pool, &storage, later)
        .await
        .unwrap();
    assert_eq!(reaped, 1);

    let counters = counters_async(&pool, &owner).await;
    assert_eq!(counters.bytes_reserved, 0, "清理必须回滚 reserved");
    let row: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM attachments WHERE id = ?")
            .bind(&created.attachment.id)
            .fetch_optional(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(row.is_none(), "中断上传清理后行应删除");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 构造指定时间戳的 UUIDv7（孤儿 age 判定用）。
fn uuid_v7_at(ms: i64) -> String {
    let rand = uuid::Uuid::now_v7();
    let mut bytes = [0u8; 16];
    let ts = (ms as u64) & 0xFFFF_FFFF_FFFF;
    bytes[0] = (ts >> 40) as u8;
    bytes[1] = (ts >> 32) as u8;
    bytes[2] = (ts >> 24) as u8;
    bytes[3] = (ts >> 16) as u8;
    bytes[4] = (ts >> 8) as u8;
    bytes[5] = ts as u8;
    bytes[6] = 0x70 | (rand.as_bytes()[6] & 0x0F);
    bytes[7] = rand.as_bytes()[7];
    bytes[8] = 0x80 | (rand.as_bytes()[8] & 0x3F);
    bytes[9..].copy_from_slice(&rand.as_bytes()[9..]);
    uuid::Uuid::from_bytes(bytes).to_string()
}

#[tokio::test]
async fn sweep_orphans_removes_stale_but_keeps_recent_and_in_use() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let now = now_millis();

    let adapter = storage.adapter(StorageBackend::Local).unwrap();
    // 1) 在用对象：进入 attachments 行（create 已落盘行，key 由服务生成）
    let in_use = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("inuse.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 11,
            is_public: false,
        },
        now,
    )
    .await
    .unwrap();
    adapter
        .write_object(&in_use.attachment.storage_key, b"hello world", None)
        .await
        .unwrap();

    // 2) 近期孤儿（宽限期内）：不被清理
    let recent_key = format!("u/{owner}/{}/orphan-recent.bin", uuid_v7_at(now));
    adapter
        .write_object(&recent_key, b"orphan", None)
        .await
        .unwrap();

    // 3) 超期孤儿（>24h）：应被清理
    let stale_key = format!(
        "u/{owner}/{}/orphan-stale.bin",
        uuid_v7_at(now - 3 * 86_400_000)
    );
    adapter
        .write_object(&stale_key, b"orphan", None)
        .await
        .unwrap();

    let purged = quota::sweep_orphans(&pool, &storage, now).await.unwrap();
    assert_eq!(purged, 1, "只清理超期孤儿");

    let head = adapter.head_object(&stale_key).await.unwrap();
    assert!(!head.exists);
    let head_recent = adapter.head_object(&recent_key).await.unwrap();
    assert!(head_recent.exists, "宽限期内的孤儿不得清理");
    let head_in_use = adapter
        .head_object(&in_use.attachment.storage_key)
        .await
        .unwrap();
    assert!(head_in_use.exists, "在用对象不得误删");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── 并发（M06-QUOTA-05）────────────────────────

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_create_reserve_only_one_succeeds_at_capacity() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    // 先把等级 1 收窄到恰好 2 MiB（single == total == daily）
    let total = 2 * 1024 * 1024;
    get_policy_for_level(&pool, 1, &owner).await.unwrap();
    update_level_quota(&pool, 1, total, total, total, 30, 1, &owner, now_millis())
        .await
        .unwrap();
    let now = now_millis();

    let mk = |name: &'static str| CreateAttachmentInput {
        owner_id: owner.clone(),
        original_name: Some(name.to_string()),
        media_type: "text/plain".to_string(),
        size_bytes: total,
        is_public: false,
    };

    let fut1 = upload::create_attachment(&pool, &storage, &owner, mk("a.bin"), now);
    let fut2 = upload::create_attachment(&pool, &storage, &owner, mk("b.bin"), now);
    let (r1, r2) = tokio::join!(fut1, fut2);

    let successes = r1.is_ok() as i32 + r2.is_ok() as i32;
    let quota_errors = (matches!(&r1, Err(e) if e.code() == "quota_exceeded") as i32)
        + (matches!(&r2, Err(e) if e.code() == "quota_exceeded") as i32);
    assert_eq!(successes, 1, "容量满时只有一个 create 成功");
    assert_eq!(quota_errors, 1, "另一个必须稳定返回 quota_exceeded");

    let counters = get_counters(&pool, &owner).await.unwrap();
    assert_eq!(counters.bytes_reserved, total, "成功方保留满额，无超卖");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── 单元级内容安全（M06-UPLOAD-06）─────────────

#[tokio::test]
async fn scan_for_safety_rejects_svg_polyglot_and_mime_spoofing() {
    // 默认拒绝 SVG
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script/></svg>";
    let err = scan_for_safety(svg, "image/jpeg", Some("x.jpg"), &NoopVirusScan).unwrap_err();
    assert!(err.summary().contains("svg"), "{}", err.summary());

    // 扩展名/内容欺骗：声明 jpeg、实为 PNG
    let png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x02\x00\x00\x00\x00\x00\x00\x00\x00IEND\xaeB\x60\x82";
    let err = scan_for_safety(png, "image/jpeg", Some("x.jpg"), &NoopVirusScan).unwrap_err();
    assert_eq!(
        err,
        upload::ScanError::TypeMismatch {
            declared: "image/jpeg".to_string(),
            detected: "image/png".to_string(),
        }
    );

    // 可执行 shebang
    let script = b"#!/bin/sh\necho pwned";
    let err = scan_for_safety(script, "text/plain", Some("x.txt"), &NoopVirusScan).unwrap_err();
    assert!(err.summary().contains("executable"), "{}", err.summary());

    // 正常文本放行
    let ok = scan_for_safety(b"hello world", "text/plain", Some("x.txt"), &NoopVirusScan).unwrap();
    assert_eq!(ok.sha256.len(), 64);
}
