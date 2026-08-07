//! M06-QUOTA 真实测试：等级策略版本化、reserved/charged/released 口径、
//! 每日上限、引用完整性、保留期清理与孤儿回收。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::storage::model::{AttachmentStatus, QuotaCounters, StorageBackend};
use bblbb_backend::storage::quota::{
    self, get_counters, get_policy_for_level, get_policy_revisions, link_attachment,
    purge_expired_deleted, release_charged, release_reserved, reserve_bytes, sweep_orphans,
    unlink_attachment, update_level_quota, verify_reference_candidate, DEFAULT_RETENTION_DAYS,
    SITE_SINGLE_FILE_HARD_LIMIT_BYTES, SITE_TOTAL_HARD_LIMIT_BYTES,
};
use bblbb_backend::storage::upload::{
    self, CompleteOutcome, CreateAttachmentInput, NoopVirusScan, ScanVerdict, VirusScan,
};
use bblbb_backend::storage::{StorageError, StorageService};
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

async fn setup() -> (DatabasePool, PathBuf, StorageService) {
    let dir = std::env::temp_dir().join(format!("bblbb-quota-{}", uuid::Uuid::now_v7()));
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

/// 走完整 happy path（create → stream → complete → ready）。
async fn upload_ready(
    pool: &DatabasePool,
    storage: &StorageService,
    user_id: &str,
    data: &[u8],
) -> bblbb_backend::storage::model::AttachmentRecord {
    let created = upload::create_attachment(
        pool,
        storage,
        user_id,
        CreateAttachmentInput {
            owner_id: user_id.to_string(),
            original_name: Some("note.txt".to_string()),
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

async fn counters(pool: &DatabasePool, user_id: &str) -> QuotaCounters {
    get_counters(pool, user_id).await.unwrap()
}

async fn set_user_level(pool: &DatabasePool, user_id: &str, level: i64) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET level = ? WHERE id = ?")
                .bind(level)
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

// ────────────────────────── 策略版本化（M06-QUOTA-01/02）───────────────

#[tokio::test]
async fn default_policy_is_seeded_lazily_and_stable() {
    let (pool, dir, _storage) = setup().await;
    let actor = insert_user(&pool, "admin", 1).await;

    let policy = get_policy_for_level(&pool, 1, &actor).await.unwrap();
    assert_eq!(policy.policy_version, 1);
    let (single, total, daily, retention) = quota::default_policy_for_level(1);
    assert_eq!(policy.single_file_max_bytes, single);
    assert_eq!(policy.total_bytes, total);
    assert_eq!(policy.daily_upload_bytes, daily);
    assert_eq!(policy.retention_days, retention);
    assert_eq!(policy.retention_days, DEFAULT_RETENTION_DAYS);

    // 幂等：再次读取同一版本，不重复 seed
    let again = get_policy_for_level(&pool, 1, &actor).await.unwrap();
    assert_eq!(again.policy_version, 1);
    let revisions = get_policy_revisions(&pool, 1).await.unwrap();
    assert_eq!(revisions.len(), 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn update_level_quota_creates_new_version_never_touches_old_rows() {
    let (pool, dir, _storage) = setup().await;
    let actor = insert_user(&pool, "admin", 1).await;

    let v1 = get_policy_for_level(&pool, 1, &actor).await.unwrap();
    let v2 = update_level_quota(
        &pool,
        1,
        4 * 1024 * 1024,
        300 * 1024 * 1024,
        60 * 1024 * 1024,
        14,
        1,
        &actor,
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(v2.policy_version, 2);
    assert_eq!(v2.total_bytes, 300 * 1024 * 1024);
    assert_eq!(v2.retention_days, 14);

    // 旧行保留（历史），最新生效的是 v2
    let revisions = get_policy_revisions(&pool, 1).await.unwrap();
    assert_eq!(revisions.len(), 2);
    assert_eq!(revisions[0].policy_version, v1.policy_version);
    let latest = get_policy_for_level(&pool, 1, &actor).await.unwrap();
    assert_eq!(latest.policy_version, 2);
    assert_eq!(latest.total_bytes, 300 * 1024 * 1024);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn update_level_quota_rejects_stale_if_match_and_bad_values() {
    let (pool, dir, _storage) = setup().await;
    let actor = insert_user(&pool, "admin", 1).await;
    get_policy_for_level(&pool, 1, &actor).await.unwrap();

    // If-Match 过期版本 → Conflict（版本冲突，M06-QUOTA-02）
    let err = update_level_quota(
        &pool,
        1,
        1024,
        1024,
        1024,
        30,
        99, // stale
        &actor,
        now_millis(),
    )
    .await
    .unwrap_err();
    assert!(matches!(err, StorageError::Conflict(_)), "got {err:?}");

    // 单文件上限 > 总容量 → Invalid
    let err = update_level_quota(&pool, 1, 1024, 100, 100, 30, 1, &actor, now_millis())
        .await
        .unwrap_err();
    assert_eq!(err.code(), "invalid_storage_request");

    // 总容量 > 站点硬上限 → Invalid
    let err = update_level_quota(
        &pool,
        1,
        SITE_SINGLE_FILE_HARD_LIMIT_BYTES,
        SITE_TOTAL_HARD_LIMIT_BYTES + 1,
        SITE_TOTAL_HARD_LIMIT_BYTES,
        30,
        1,
        &actor,
        now_millis(),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "invalid_storage_request");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── reserved/charged/released（M06-QUOTA-04/05）──

#[tokio::test]
async fn reserve_charge_release_roundtrip_matches_net_accounting() {
    let (pool, dir, _storage) = setup().await;
    let user = insert_user(&pool, "user", 1).await;
    let policy = get_policy_for_level(&pool, 1, &user).await.unwrap();

    // reserve 100
    reserve_bytes(&pool, &user, 100, &policy, now_millis())
        .await
        .unwrap();
    let c = counters(&pool, &user).await;
    assert_eq!(
        (c.bytes_reserved, c.bytes_charged, c.bytes_released),
        (100, 0, 0)
    );

    // complete 结算：charged += 100、released += 100、reserved -= 100（净效果 0）
    quota::charge_reserved(&pool, &user, 100, 100, now_millis())
        .await
        .unwrap();
    let c = counters(&pool, &user).await;
    assert_eq!(
        (c.bytes_reserved, c.bytes_charged, c.bytes_released),
        (0, 100, 100)
    );

    // quarantined 回滚：reserved 提前释放（released += 100、reserved -= 100）
    reserve_bytes(&pool, &user, 100, &policy, now_millis())
        .await
        .unwrap();
    release_reserved(&pool, &user, 100, now_millis())
        .await
        .unwrap();
    let c = counters(&pool, &user).await;
    assert_eq!(
        (c.bytes_reserved, c.bytes_charged, c.bytes_released),
        (0, 100, 200)
    );

    // 物理删除：released += charged、charged -= charged
    release_charged(&pool, &user, 100, now_millis())
        .await
        .unwrap();
    let c = counters(&pool, &user).await;
    assert_eq!(
        (c.bytes_reserved, c.bytes_charged, c.bytes_released),
        (0, 0, 300)
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn release_operations_clamp_to_zero_never_negative() {
    let (pool, dir, _storage) = setup().await;
    let user = insert_user(&pool, "user", 1).await;

    // 没有预留却释放：reserved 被钳制为 0，不出现负数
    release_reserved(&pool, &user, 50, now_millis())
        .await
        .unwrap();
    let c = counters(&pool, &user).await;
    assert_eq!(c.bytes_reserved, 0);
    assert_eq!(c.bytes_released, 50);

    // 没有 charged 却释放：charged 被钳制为 0
    release_charged(&pool, &user, 30, now_millis())
        .await
        .unwrap();
    let c = counters(&pool, &user).await;
    assert_eq!(c.bytes_charged, 0);
    assert_eq!(c.bytes_released, 80);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn reserve_rejects_when_total_or_daily_exhausted() {
    let (pool, dir, storage) = setup().await;
    let user = insert_user(&pool, "user", 1).await;
    let actor = insert_user(&pool, "admin", 1).await;
    get_policy_for_level(&pool, 1, &actor).await.unwrap();

    // Phase A：total 收紧到 100 字节（daily 同 100）→ 第二次 create 总容量超卖
    update_level_quota(&pool, 1, 100, 100, 100, 30, 1, &actor, now_millis())
        .await
        .unwrap();
    create_fixture(&pool, &storage, &user, 60).await.unwrap();
    let err = create_fixture(&pool, &storage, &user, 60)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "quota_exceeded", "总容量超卖必须拒绝");
    // 释放第一个附件的预留
    let first_id = upload::list_attachments_for_owner(&pool, &user, 10)
        .await
        .unwrap()
        .into_iter()
        .find(|a| a.size_bytes == 60)
        .unwrap()
        .id;
    upload::delete_attachment(&pool, &user, &first_id, now_millis())
        .await
        .unwrap();

    // Phase B：daily 收紧到 100 字节（total 放宽）→ 第二次 create 每日超限
    update_level_quota(&pool, 1, 500, 500, 100, 30, 2, &actor, now_millis())
        .await
        .unwrap();
    create_fixture(&pool, &storage, &user, 60).await.unwrap();
    let err = create_fixture(&pool, &storage, &user, 60)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "quota_exceeded", "每日上传量超限必须拒绝");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 造一个指定大小的 create 声明（不传输）。
async fn create_fixture(
    pool: &DatabasePool,
    storage: &StorageService,
    user: &str,
    size: i64,
) -> Result<bblbb_backend::storage::model::AttachmentRecord, bblbb_backend::storage::StorageError> {
    upload::create_attachment(
        pool,
        storage,
        user,
        CreateAttachmentInput {
            owner_id: user.to_string(),
            original_name: Some("f.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: size,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .map(|outcome| outcome.attachment)
}

// ────────────────────────── 引用完整性（M06-QUOTA-06/07）───────────────

#[tokio::test]
async fn verify_reference_candidate_requires_own_ready_attachment() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let other = insert_user(&pool, "other", 1).await;

    // pending 附件不可作为 Cover/引用目标
    let pending = upload::create_attachment(
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
    let err = verify_reference_candidate(&pool, &owner, &pending.attachment.id)
        .await
        .unwrap_err();
    assert_eq!(
        err.code(),
        "storage_state_error",
        "未 ready 禁止关联公开内容"
    );

    // ready 后：本人通过
    upload::stream_upload(
        &pool,
        &storage,
        &pending.attachment.id,
        &owner,
        b"hello world",
        None,
    )
    .await
    .unwrap();
    upload::complete_attachment(
        &pool,
        &storage,
        &pending.attachment.id,
        &owner,
        &NoopVirusScan,
        now_millis(),
    )
    .await
    .unwrap();
    verify_reference_candidate(&pool, &owner, &pending.attachment.id)
        .await
        .unwrap();

    // 他人引用 → Forbidden
    let err = verify_reference_candidate(&pool, &other, &pending.attachment.id)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "storage_forbidden");

    // 不存在 → NotFound
    let err = verify_reference_candidate(&pool, &owner, "missing-id")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "not_found");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn link_and_unlink_update_refcount_and_are_balanced() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let attachment = upload_ready(&pool, &storage, &owner, b"hello world").await;
    let now = now_millis();

    link_attachment(&pool, &attachment.id, "post", "post-1", "cover", now)
        .await
        .unwrap();
    let ref_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT ref_count FROM attachments WHERE id = ?")
            .bind(&attachment.id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(ref_count, 1, "link 必须递增 ref_count");

    // 移除引用只解除引用，不删附件（M06-QUOTA-07）
    unlink_attachment(&pool, &attachment.id, "post", "post-1")
        .await
        .unwrap();
    let ref_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT ref_count FROM attachments WHERE id = ?")
            .bind(&attachment.id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(ref_count, 0);
    let exists: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM attachments WHERE id = ?")
            .bind(&attachment.id)
            .fetch_optional(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(exists.is_some(), "unlink 不得删除附件");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── 保留期 / 孤儿（M06-QUOTA-09/10）────────────

#[tokio::test]
async fn purge_releases_charged_only_when_unreferenced_and_object_present() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let now = now_millis();
    let old = now - 40 * 86_400_000;
    let attachment = upload_ready(&pool, &storage, &owner, b"hello world").await;
    assert_eq!(counters(&pool, &owner).await.bytes_charged, 11);

    // 软删除并过保留期；对象仍存在
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
    let summary = purge_expired_deleted(&pool, &storage, now).await.unwrap();
    assert_eq!(summary.purged, 1);
    assert_eq!(summary.released_bytes, 11);
    let c = counters(&pool, &owner).await;
    assert_eq!(c.bytes_charged, 0, "物理删除后必须释放 charged");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn purge_skips_referenced_attachments() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let now = now_millis();
    let old = now - 40 * 86_400_000;
    let attachment = upload_ready(&pool, &storage, &owner, b"hello world").await;

    // 直接构造：deleted + 过期 + 仍有引用
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
                 VALUES (?, ?, 'post', 'p-9', 'cover', ?)",
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

    let summary = purge_expired_deleted(&pool, &storage, now).await.unwrap();
    assert_eq!(summary.purged, 0);
    assert_eq!(summary.skipped_referenced, 1, "有引用不得清理");
    assert_eq!(counters(&pool, &owner).await.bytes_charged, 11);

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
async fn sweep_orphans_mark_and_sweep_never_deletes_in_use_objects() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    let now = now_millis();
    let adapter = storage.adapter(StorageBackend::Local).unwrap();

    // 在用对象：与 attachments 行绑定（create 后 key 存在行中）
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

    // 超期孤儿：应清理
    let stale_key = format!("u/{owner}/{}/orphan.bin", uuid_v7_at(now - 3 * 86_400_000));
    adapter
        .write_object(&stale_key, b"orphan", None)
        .await
        .unwrap();

    // 未绑定但仍在宽限期：应保留
    let recent_key = format!("u/{owner}/{}/recent.bin", uuid_v7_at(now - 60_000));
    adapter
        .write_object(&recent_key, b"orphan", None)
        .await
        .unwrap();

    let purged = sweep_orphans(&pool, &storage, now).await.unwrap();
    assert_eq!(purged, 1);

    assert!(!adapter.head_object(&stale_key).await.unwrap().exists);
    assert!(adapter.head_object(&recent_key).await.unwrap().exists);
    assert!(
        adapter
            .head_object(&in_use.attachment.storage_key)
            .await
            .unwrap()
            .exists
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── 等级联动（M06-QUOTA-03）────────────────────

#[tokio::test]
async fn policy_follows_current_level_and_quotas_are_per_user() {
    let (pool, dir, storage) = setup().await;
    let user = insert_user(&pool, "user", 1).await;
    let actor = insert_user(&pool, "admin", 1).await;

    // 升级到等级 3（默认总容量 500 MiB）
    set_user_level(&pool, &user, 3).await;
    let p3 = get_policy_for_level(&pool, 3, &user).await.unwrap();
    let (_, total3, _, _) = quota::default_policy_for_level(3);
    assert_eq!(p3.total_bytes, total3);
    assert_eq!(p3.level, 3);

    // create 走等级 3 策略：500 MiB 单文件内可创建
    let created = upload::create_attachment(
        &pool,
        &storage,
        &user,
        CreateAttachmentInput {
            owner_id: user.clone(),
            original_name: Some("big.txt".to_string()),
            media_type: "text/plain".to_string(),
            size_bytes: 10 * 1024 * 1024,
            is_public: false,
        },
        now_millis(),
    )
    .await
    .unwrap();
    assert_eq!(created.attachment.status, AttachmentStatus::Pending);

    // 配额按用户隔离：另一个用户不受影响
    let other = insert_user(&pool, "other", 1).await;
    assert_eq!(counters(&pool, &other).await.bytes_reserved, 0);
    assert_eq!(
        counters(&pool, &user).await.bytes_reserved,
        10 * 1024 * 1024
    );

    // 管理员收紧等级 3（降级处罚场景）
    update_level_quota(
        &pool,
        3,
        1024,
        1024,
        1024,
        30,
        p3.policy_version,
        &actor,
        now_millis(),
    )
    .await
    .unwrap();
    // 已预留的容量在 complete 时会被重检拒绝（upload.rs 已覆盖），此处验证新策略生效
    let p3_new = get_policy_for_level(&pool, 3, &actor).await.unwrap();
    assert_eq!(p3_new.policy_version, p3.policy_version + 1);
    assert_eq!(p3_new.total_bytes, 1024);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ────────────────────────── 病毒扫描占位（与 upload 共用）──────────────

struct InfectedScan;
impl VirusScan for InfectedScan {
    fn scan(&self, _data: &[u8]) -> ScanVerdict {
        ScanVerdict::Infected
    }
}

#[tokio::test]
async fn quarantine_rolls_back_reserved_without_touching_charged() {
    let (pool, dir, storage) = setup().await;
    let owner = insert_user(&pool, "owner", 1).await;
    // 先有一个 ready 附件（charged=11）
    let ready = upload_ready(&pool, &storage, &owner, b"hello world").await;
    assert_eq!(ready.status, AttachmentStatus::Ready);

    // 第二个附件：病毒命中 → quarantined
    let created = upload::create_attachment(
        &pool,
        &storage,
        &owner,
        CreateAttachmentInput {
            owner_id: owner.clone(),
            original_name: Some("bad.txt".to_string()),
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

    let c = counters(&pool, &owner).await;
    assert_eq!(c.bytes_charged, 11, "隔离不得影响已 ready 的 charged");
    assert_eq!(c.bytes_reserved, 0, "隔离必须回滚新增的 reserved");

    close_pool(&pool).await;
    cleanup(&dir);
}
