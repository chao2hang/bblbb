//! M03-SCHEMA-01：用户资料/隐私/偏好/等级缓存/profile revision 迁移契约——
//! - `users` 增加资料与等级缓存列（level 默认 1、avatar/signature/时间戳）；
//! - `user_preferences`：展示偏好（时区/语言/主题/通知 JSON）；
//! - `user_privacy`：隐私设置（邮箱/资料可见范围，默认最保守 + CHECK 约束）；
//! - `profile_revisions`：资料每次变更追加一条（revision 递增、UNIQUE(user_id,
//!   revision)、FK 级联）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations/sqlite";

fn migrations_dir() -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(MIGRATIONS_ROOT)
}

async fn pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-profile-{}", uuid::Uuid::now_v7()));
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

async fn table_columns(pool: &DatabasePool, table: &str) -> Vec<String> {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info('{table}')"))
                .fetch_all(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
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

/// users 新列契约：资料字段 + 等级缓存列全部存在。
#[tokio::test]
async fn users_gains_profile_and_level_columns() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "users").await;

    for required in [
        "level",                // 等级缓存（可重建，真实来源 M7 经验账户）
        "level_updated_at",     // 等级缓存刷新时间（NULL = 未计算）
        "avatar_attachment_id", // 头像附件引用（软引用，attachments 表 M6 落地）
        "signature",            // 个人签名
        "last_login_at",        // 最近登录时间
        "delete_requested_at",  // 注销申请时间
        "deleted_at",           // 硬删除时间（匿名化后）
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "users 缺少列 {required}，实际: {columns:?}"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 新用户 level 默认 1（等级缓存尚未计算）。
#[tokio::test]
async fn level_defaults_to_one() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "level").await;
    match &pool {
        Either::Left(p) => {
            let level: i64 = sqlx::query_scalar("SELECT level FROM users WHERE id = ?")
                .bind(&user_id)
                .fetch_one(p)
                .await
                .unwrap();
            assert_eq!(level, 1, "新用户 level 必须默认 1");
            let updated: Option<i64> =
                sqlx::query_scalar("SELECT level_updated_at FROM users WHERE id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert!(updated.is_none(), "未计算的等级缓存必须为 NULL");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// user_preferences 契约：展示偏好列 + FK 级联。
#[tokio::test]
async fn preferences_schema_and_cascade() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "user_preferences").await;
    for required in [
        "user_id",
        "timezone",          // IANA 时区，默认 UTC
        "locale",            // 语言，默认 zh-CN
        "theme_name",        // 已安装主题名（可空）
        "notification_json", // 通知偏好 JSON
        "updated_at",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "user_preferences 缺少列 {required}，实际: {columns:?}"
        );
    }

    let user_id = insert_user(&pool, "pref").await;
    match &pool {
        Either::Left(p) => {
            // 首访时应用创建偏好行（仅 user_id），默认值生效
            sqlx::query("INSERT INTO user_preferences (user_id, updated_at) VALUES (?, ?)")
                .bind(&user_id)
                .bind(now_millis())
                .execute(p)
                .await
                .unwrap();
            let (tz, locale): (String, String) =
                sqlx::query_as("SELECT timezone, locale FROM user_preferences WHERE user_id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(tz, "UTC");
            assert_eq!(locale, "zh-CN");

            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
            let left: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM user_preferences WHERE user_id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(left, 0, "用户删除必须级联清理 user_preferences");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// user_privacy 契约：默认最保守（邮箱不可见）+ CHECK 拒绝非法取值。
#[tokio::test]
async fn privacy_defaults_and_check_constraint() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "user_privacy").await;
    for required in [
        "user_id",
        "email_visible_to",   // everyone/registered/nobody，默认 nobody
        "profile_visible_to", // everyone/registered/nobody，默认 everyone
        "updated_at",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "user_privacy 缺少列 {required}，实际: {columns:?}"
        );
    }

    let user_id = insert_user(&pool, "priv").await;
    match &pool {
        Either::Left(p) => {
            // 首访时应用创建隐私行（仅 user_id），默认值生效
            sqlx::query("INSERT INTO user_privacy (user_id, updated_at) VALUES (?, ?)")
                .bind(&user_id)
                .bind(now_millis())
                .execute(p)
                .await
                .unwrap();
            let (email_vis, profile_vis): (String, String) = sqlx::query_as(
                "SELECT email_visible_to, profile_visible_to FROM user_privacy WHERE user_id = ?",
            )
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(email_vis, "nobody", "邮箱可见范围默认必须最保守");
            assert_eq!(profile_vis, "everyone");

            // CHECK 约束：非法取值必须被拒绝
            let invalid = sqlx::query(
                "UPDATE user_privacy SET email_visible_to = 'everyone-and-their-dog' WHERE user_id = ?",
            )
            .bind(&user_id)
            .execute(p)
            .await
            .unwrap_err();
            assert!(
                matches!(invalid, sqlx::Error::Database(ref e) if e.is_check_violation()),
                "user_privacy CHECK 约束必须生效: {invalid}"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// profile_revisions 契约：revision 递增唯一 + FK 级联。
#[tokio::test]
async fn profile_revisions_schema_unique_and_cascade() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "profile_revisions").await;
    for required in [
        "id",
        "user_id",
        "revision",      // 每次变更递增
        "changes_json",  // 本次变更的字段/值
        "actor_user_id", // 本人或管理员
        "created_at",
    ] {
        assert!(
            columns.iter().any(|c| c == required),
            "profile_revisions 缺少列 {required}，实际: {columns:?}"
        );
    }

    let user_id = insert_user(&pool, "rev").await;
    match &pool {
        Either::Left(p) => {
            let insert = |revision: i64| {
                sqlx::query(
                    "INSERT INTO profile_revisions (id, user_id, revision, changes_json, created_at)
                     VALUES (?, ?, ?, '{}', ?)",
                )
                .bind(uuid::Uuid::now_v7().to_string())
                .bind(&user_id)
                .bind(revision)
                .bind(now_millis())
            };
            insert(1).execute(p).await.unwrap();
            // UNIQUE(user_id, revision)：同 revision 二次插入必须失败
            let dup = insert(1).execute(p).await.unwrap_err();
            assert!(
                matches!(dup, sqlx::Error::Database(ref e) if e.is_unique_violation()),
                "profile_revisions UNIQUE(user_id, revision) 必须生效: {dup}"
            );
            insert(2).execute(p).await.unwrap();

            sqlx::query("DELETE FROM users WHERE id = ?")
                .bind(&user_id)
                .execute(p)
                .await
                .unwrap();
            let left: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM profile_revisions WHERE user_id = ?")
                    .bind(&user_id)
                    .fetch_one(p)
                    .await
                    .unwrap();
            assert_eq!(left, 0, "用户删除必须级联清理 profile_revisions");
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}

/// M03-SCHEMA-02：头像/Cover 只存附件 UUID（软引用），URL/签名 URL 禁止入库。
///
/// avatar_attachment_id / cover_attachment_id 为 TEXT 软引用（attachments 表
/// M6 落地后补 FK）；"禁止保存远程 URL/签名 URL"由 M3-PROFILE 服务层校验
/// （ProfileCoverSet.attachment_id format: uuid）——DB 层不做跨库 URL 判定，
/// 此处验证列存在、UUID 可往返、NULL 为默认（未设置）。
#[tokio::test]
async fn avatar_cover_reference_only_stores_attachment_uuid() {
    let (pool, dir) = pool_with_migrations().await;
    let columns = table_columns(&pool, "users").await;
    for required in ["avatar_attachment_id", "cover_attachment_id"] {
        assert!(
            columns.iter().any(|c| c == required),
            "users 缺少附件引用列 {required}，实际: {columns:?}"
        );
    }

    let user_id = insert_user(&pool, "attach").await;
    match &pool {
        Either::Left(p) => {
            let avatar_id = uuid::Uuid::now_v7().to_string();
            let cover_id = uuid::Uuid::now_v7().to_string();
            sqlx::query(
                "UPDATE users SET avatar_attachment_id = ?, cover_attachment_id = ? WHERE id = ?",
            )
            .bind(&avatar_id)
            .bind(&cover_id)
            .bind(&user_id)
            .execute(p)
            .await
            .unwrap();
            let (avatar, cover): (Option<String>, Option<String>) = sqlx::query_as(
                "SELECT avatar_attachment_id, cover_attachment_id FROM users WHERE id = ?",
            )
            .bind(&user_id)
            .fetch_one(p)
            .await
            .unwrap();
            assert_eq!(avatar.as_deref(), Some(avatar_id.as_str()));
            assert_eq!(cover.as_deref(), Some(cover_id.as_str()));

            // 未设置时为 NULL（默认）
            let user2 = insert_user(&pool, "attach2").await;
            let (avatar2, cover2): (Option<String>, Option<String>) = sqlx::query_as(
                "SELECT avatar_attachment_id, cover_attachment_id FROM users WHERE id = ?",
            )
            .bind(&user2)
            .fetch_one(p)
            .await
            .unwrap();
            assert!(
                avatar2.is_none() && cover2.is_none(),
                "未设置附件引用必须为 NULL"
            );
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    close_pool(&pool).await;
    cleanup(&dir);
}
