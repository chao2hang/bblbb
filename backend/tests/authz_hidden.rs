//! M03-AUTHZ-07：隐藏内容显式管理投影读取测试——
//! 权限（moderation.review/post.moderate）+ 显式理由 + 不可删除审计；
//! 任何缺失一律默认拒绝。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::decision::{DenyReason, AUTHZ_POLICY_VERSION};
use bblbb_backend::authz::hidden::{
    hidden_read_to_error, require_hidden_read, HiddenReadError, HIDDEN_READ_REASON_MAX,
};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authz-hid-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
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

async fn insert_user(pool: &DatabasePool, tag: &str, verified_at: Option<i64>) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let email = format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple());
    let now = bblbb_backend::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy-hash', 'active', ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(&email)
            .bind(verified_at)
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

async fn role_id_by_name(pool: &DatabasePool, name: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM roles WHERE name = ?")
            .bind(name)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 全局角色 assignment（user_roles），granted_at 在过去（已生效）。
async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id = role_id_by_name(pool, role_name).await;
    let now = bblbb_backend::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(user_id)
            .bind(&role_id)
            .bind(now - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn count_audit(pool: &DatabasePool, action: &str, reason: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM audit_logs WHERE action = ? AND reason = ?",
        )
        .bind(action)
        .bind(reason)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 全局版主 + 理由 → 放行并写审计（不可删除，含 reason + policy_version）。
#[tokio::test]
async fn moderator_with_reason_reads_hidden_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let now = bblbb_backend::outbox::now_millis();
    let moderator = insert_user(&pool, "mod", Some(now - 86_400_000)).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;

    require_hidden_read(
        &pool,
        &moderator,
        "moderation.review",
        "post",
        "post-1",
        "举报复核：核验隐藏帖子存在性",
        "req-1",
    )
    .await
    .expect("全局版主 + 理由必须放行");

    assert_eq!(
        count_audit(
            &pool,
            "moderation.read_hidden",
            "举报复核：核验隐藏帖子存在性"
        )
        .await,
        1,
        "隐藏读取必须写审计"
    );
    // 审计携带 policy_version
    let policy: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT policy_version FROM audit_logs WHERE action = 'moderation.read_hidden' LIMIT 1",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(policy.as_deref(), Some(AUTHZ_POLICY_VERSION));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 无理由 → MissingReason（先于权限，防存在性探测），不写审计。
#[tokio::test]
async fn missing_reason_is_rejected_without_audit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let now = bblbb_backend::outbox::now_millis();
    let moderator = insert_user(&pool, "nore", Some(now - 86_400_000)).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;

    assert_eq!(
        require_hidden_read(
            &pool,
            &moderator,
            "moderation.review",
            "post",
            "post-1",
            "   ",
            "req-2",
        )
        .await,
        Err(HiddenReadError::MissingReason)
    );
    assert_eq!(
        count_audit(&pool, "moderation.read_hidden", "   ").await,
        0,
        "无理由不得写审计"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 理由超长 → ReasonTooLong。
#[tokio::test]
async fn overlong_reason_is_rejected() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let now = bblbb_backend::outbox::now_millis();
    let moderator = insert_user(&pool, "long", Some(now - 86_400_000)).await;
    assign_global_role(&pool, &moderator, "global_moderator").await;

    let long = "x".repeat(HIDDEN_READ_REASON_MAX + 1);
    assert_eq!(
        require_hidden_read(
            &pool,
            &moderator,
            "moderation.review",
            "post",
            "post-1",
            &long,
            "req-3",
        )
        .await,
        Err(HiddenReadError::ReasonTooLong)
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 无权限（member）→ Denied(MissingPermission)，不写审计。
#[tokio::test]
async fn member_without_moderation_permission_is_denied() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    seed_builtin_roles(&pool).await.unwrap();
    let now = bblbb_backend::outbox::now_millis();
    let member = insert_user(&pool, "mem", Some(now - 86_400_000)).await;

    assert_eq!(
        require_hidden_read(
            &pool,
            &member,
            "moderation.review",
            "post",
            "post-1",
            "想看隐藏内容",
            "req-4",
        )
        .await,
        Err(HiddenReadError::Denied(DenyReason::MissingPermission))
    );
    assert_eq!(
        count_audit(&pool, "moderation.read_hidden", "想看隐藏内容").await,
        0
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 错误映射：理由类 → 400；拒绝 → 403。
#[tokio::test]
async fn hidden_read_error_maps_to_http() {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    let bad = hidden_read_to_error(HiddenReadError::MissingReason, "req-5").into_response();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    let denied = hidden_read_to_error(
        HiddenReadError::Denied(DenyReason::MissingPermission),
        "req-6",
    )
    .into_response();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
}
