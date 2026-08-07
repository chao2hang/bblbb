//! M05-SCHEMA-07：notifications 数据约束测试。
//!
//! SQLite 本地全量跑通；MySQL/MariaDB 以 `BBLBB_TEST_MYSQL_URL` +
//! `#[ignore]`（CI mysql-family 任务以 `--ignored` 分别运行）验证三库
//! 等价，模式同 `schema_fixture.rs`。
//!
//! 断言内容：
//! 1. notifications.category CHECK（0045 追加列）；
//! 2. 投递去重键唯一：同 (user_id, delivery_dedup_key) 至多一条，
//!    NULL 不去重（三库一致）；
//! 3. notification_preferences：「安全通知不可被普通偏好全关」CHECK、
//!    (user_id, category) 主键唯一；
//! 4. 模型层：类别枚举往返、去重键构造、安全偏好校验。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::notifications::model::*;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations";

/// 执行 INSERT/UPDATE（期望成功）。
macro_rules! exec {
    ($pool:expr, $sql:expr, $($arg:expr),* $(,)?) => {{
        match $pool {
            Either::Left(p) => {
                exec!(@chain sqlx::query($sql), $($arg),*).execute(p).await.unwrap();
            }
            Either::Right(p) => {
                exec!(@chain sqlx::query($sql), $($arg),*).execute(p).await.unwrap();
            }
        }
    }};
    (@chain $q:expr,) => { $q };
    (@chain $q:expr, $a:expr $(, $rest:expr)*) => {
        exec!(@chain $q.bind($a), $($rest),*)
    };
}

/// 执行 INSERT/UPDATE（期望被约束拒绝），返回数据库错误。
macro_rules! expect_err {
    ($pool:expr, $sql:expr, $($arg:expr),* $(,)?) => {{
        match $pool {
            Either::Left(p) => exec!(@chain sqlx::query($sql), $($arg),*).execute(p).await.unwrap_err(),
            Either::Right(p) => exec!(@chain sqlx::query($sql), $($arg),*).execute(p).await.unwrap_err(),
        }
    }};
    (@chain $q:expr,) => { $q };
    (@chain $q:expr, $a:expr $(, $rest:expr)*) => {
        expect_err!(@chain $q.bind($a), $($rest),*)
    };
}

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("{MIGRATIONS_ROOT}/{engine}"))
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-notify-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("sqlite")).unwrap();
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

#[derive(Clone, Copy)]
enum ViolationKind {
    Unique,
    Check,
}

/// 断言 sqlx 错误为指定类别的约束拒绝（三库统一）。
fn assert_violation(err: &sqlx::Error, kind: ViolationKind, ctx: &str) {
    let db = match err {
        sqlx::Error::Database(e) => e,
        other => panic!("{ctx}: 期望数据库约束错误，实际 {other}"),
    };
    match kind {
        ViolationKind::Unique => {
            assert!(
                db.is_unique_violation(),
                "{ctx}: 期望唯一性违例，实际 {err}"
            );
        }
        // MySQL 8 用 3819、MariaDB 用 4025；SQLite 用 SQLITE_CONSTRAINT_CHECK。
        ViolationKind::Check => {
            let is_mariadb_check = db.code().as_deref() == Some("4025");
            assert!(
                db.is_check_violation() || is_mariadb_check,
                "{ctx}: 期望 CHECK 违例，实际 {err}"
            );
        }
    }
}

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    let uniq = uuid::Uuid::now_v7().simple().to_string();
    exec!(
        pool,
        "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
         VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
        &user_id,
        format!("{tag}_{uniq}"),
        format!("{tag}_{uniq}@example.com"),
        now,
        now
    );
    user_id
}

/// 插入一条通知（`delivery_dedup_key` 为 None 表示不去重）。
async fn insert_notification(
    pool: &DatabasePool,
    user_id: &str,
    dedup_key: Option<&str>,
    category: &str,
) {
    let id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO notifications (id, user_id, type, title, body, link, is_read, created_at, read_at, security_kind, template_key, resource_type, resource_id, delivery_dedup_key, category)
         VALUES (?, ?, 'system', ?, NULL, NULL, 0, ?, NULL, NULL, NULL, NULL, NULL, ?, ?)",
        &id,
        user_id,
        "fixture title",
        now,
        dedup_key,
        category
    );
}

// ─────────────────────────── 纯模型规则 ───────────────────────────

#[test]
fn notification_category_enum() {
    use NotificationCategory as C;
    assert_eq!(C::Activity.as_str(), "activity");
    assert_eq!(C::Security.as_str(), "security");
    assert!(C::Security.is_security());
    assert!(!C::Digest.is_security());
    for v in C::ALL {
        assert_eq!(C::parse(v.as_str()), Some(v));
    }
    assert_eq!(C::parse("bogus"), None);
}

#[test]
fn delivery_dedup_key_builder() {
    let key = Notification::build_delivery_dedup_key("u1", "post", "post-1");
    assert_eq!(key, "u1|post|post-1");
    let other = Notification::build_delivery_dedup_key("u1", "post", "post-2");
    assert_ne!(key, other);
    let other_user = Notification::build_delivery_dedup_key("u2", "post", "post-1");
    assert_ne!(key, other_user);
}

#[test]
fn security_preference_cannot_be_disabled() {
    // security 全关 → 拒绝
    assert!(
        NotificationPreference::validate(NotificationCategory::Security, false, false, false)
            .is_err()
    );
    // security 至少一渠道开 → 允许
    assert!(
        NotificationPreference::validate(NotificationCategory::Security, false, true, false)
            .is_ok()
    );
    // 普通类别全关 → 允许
    assert!(
        NotificationPreference::validate(NotificationCategory::Activity, false, false, false)
            .is_ok()
    );
    assert!(
        NotificationPreference::validate(NotificationCategory::Digest, false, false, false).is_ok()
    );
    assert!(NotificationPreference::validate(
        NotificationCategory::Moderation,
        false,
        false,
        false
    )
    .is_ok());

    let pref = NotificationPreference {
        user_id: "u1".into(),
        category: NotificationCategory::Security,
        email_enabled: false,
        in_app_enabled: true,
        push_enabled: false,
        updated_at: 0,
    };
    assert!(!pref.is_category_fully_disabled());
    let closed = NotificationPreference {
        in_app_enabled: false,
        ..pref.clone()
    };
    assert!(closed.is_category_fully_disabled());
}

// ─────────────────────────── 数据库约束流 ───────────────────────────

/// 三数据库等价约束流（M05-SCHEMA-07 的 DB 断言）。
async fn notifications_schema_flow(pool: &DatabasePool) {
    let user = insert_user(pool, "nuser").await;
    let now = now_millis();

    // ── notifications.category CHECK（0045 追加列）──
    let err = expect_err!(
        pool,
        "INSERT INTO notifications (id, user_id, type, title, created_at, category)
         VALUES (?, ?, 'system', 't', ?, 'bogus')",
        uuid::Uuid::now_v7().to_string(),
        &user,
        now
    );
    assert_violation(&err, ViolationKind::Check, "notifications.category CHECK");

    // ── 投递去重：同 (user_id, delivery_dedup_key) 唯一 ──
    let dedup = Notification::build_delivery_dedup_key(&user, "post", "post-1");
    insert_notification(pool, &user, Some(&dedup), "activity").await;
    let err = expect_err!(
        pool,
        "INSERT INTO notifications (id, user_id, type, title, body, link, is_read, created_at, read_at, security_kind, template_key, resource_type, resource_id, delivery_dedup_key, category)
         VALUES (?, ?, 'system', 'dup', NULL, NULL, 0, ?, NULL, NULL, NULL, NULL, NULL, ?, 'activity')",
        uuid::Uuid::now_v7().to_string(),
        &user,
        now,
        &dedup
    );
    assert_violation(
        &err,
        ViolationKind::Unique,
        "notifications (user_id, delivery_dedup_key) 唯一",
    );

    // 不同用户同 key → 允许（key 含 user_id）
    let other_user = insert_user(pool, "nother").await;
    insert_notification(pool, &other_user, Some(&dedup), "activity").await;

    // NULL dedup key 不去重：多条均可插入
    insert_notification(pool, &user, None, "moderation").await;
    insert_notification(pool, &user, None, "moderation").await;

    // ── notification_preferences：安全通知不可全关 ──
    let err = expect_err!(
        pool,
        "INSERT INTO notification_preferences (user_id, category, email_enabled, in_app_enabled, push_enabled, updated_at)
         VALUES (?, 'security', 0, 0, 0, ?)",
        &user,
        now
    );
    assert_violation(
        &err,
        ViolationKind::Check,
        "security 类别不可全关（notification_preferences CHECK）",
    );

    // 普通类别全关 → 允许
    exec!(
        pool,
        "INSERT INTO notification_preferences (user_id, category, email_enabled, in_app_enabled, push_enabled, updated_at)
         VALUES (?, 'activity', 0, 0, 0, ?)",
        &user,
        now
    );
    // security 保留一渠道 → 允许
    exec!(
        pool,
        "INSERT INTO notification_preferences (user_id, category, email_enabled, in_app_enabled, push_enabled, updated_at)
         VALUES (?, 'security', 0, 0, 1, ?)",
        &user,
        now
    );

    // (user_id, category) 主键唯一
    let err = expect_err!(
        pool,
        "INSERT INTO notification_preferences (user_id, category, email_enabled, in_app_enabled, push_enabled, updated_at)
         VALUES (?, 'activity', 1, 1, 1, ?)",
        &user,
        now
    );
    assert_violation(&err, ViolationKind::Unique, "(user_id, category) 主键唯一");
}

// ─────────────────────────── 三数据库入口 ───────────────────────────

#[tokio::test]
async fn sqlite_notifications_schema() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    notifications_schema_flow(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_notifications_schema() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mysql")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    notifications_schema_flow(&pool).await;
    close_pool(&pool).await;
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mariadb_notifications_schema() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mariadb")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    notifications_schema_flow(&pool).await;
    close_pool(&pool).await;
}
