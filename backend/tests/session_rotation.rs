//! M02-SESSION-04：Session 旋转（防 fixation）——登录/权限提升/改密/高风险
//! 重认证后撤销旧 token 并签发新 token；旧 session 记录旋转原因与版本。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::session::{create_session, rotate_session};
use bblbb_backend::auth::token::hash_token;
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
    let dir = std::env::temp_dir().join(format!("bblbb-rotate-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
            )
            .bind(&user_id)
            .bind("rotate_user")
            .bind("rotate@example.com")
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

/// 查询 token 对应 session 的状态：返回 (revoked_at, revoke_reason, version, token_hash)。
async fn session_state(
    pool: &DatabasePool,
    token: &str,
) -> (Option<i64>, Option<String>, i64, String) {
    let token_hash = hash_token(token);
    match pool {
        Either::Left(p) => sqlx::query_as(
            "SELECT revoked_at, revoke_reason, version, token_hash FROM user_sessions WHERE token_hash = ?",
        )
        .bind(&token_hash)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn session_count(pool: &DatabasePool) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT COUNT(*) FROM user_sessions")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 旋转：旧 token 立即失效（revoked + reason + version+1），新 token 有效。
#[tokio::test]
async fn rotate_invalidates_old_token_and_issues_new() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool).await;
    let old_token = create_session(&pool, &user_id).await.unwrap();
    assert_eq!(session_count(&pool).await, 1);

    let new_token = rotate_session(&pool, &old_token, "login")
        .await
        .expect("旋转必须成功");
    assert_ne!(old_token, new_token, "旋转必须签发全新 token");

    // 旧 session：撤销 + 原因 + version 1
    let (revoked, reason, version, _) = session_state(&pool, &old_token).await;
    assert!(revoked.is_some(), "旧 session 必须撤销（防 fixation）");
    assert_eq!(reason.as_deref(), Some("login"));
    assert_eq!(version, 1, "旋转后旧 session version +1");

    // 新 session：未撤销、version 0（全新 session 行）
    let (revoked, _, version, new_hash) = session_state(&pool, &new_token).await;
    assert!(revoked.is_none(), "新 session 必须有效");
    assert_eq!(version, 0);
    assert_ne!(new_hash, hash_token(&old_token));

    assert_eq!(session_count(&pool).await, 2, "旋转保留旧行并新增新行");

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 无效/已撤销 token 旋转必须报错（不签发新 session）。
#[tokio::test]
async fn rotate_unknown_token_returns_error() {
    let (pool, dir) = pool_with_migrations().await;
    let err = rotate_session(&pool, "no-such-token", "login")
        .await
        .unwrap_err();
    assert!(matches!(err, sqlx::Error::RowNotFound));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 两次登录（create_session）签发不同 token：每次登录都是全新 session，
/// 攻击者预设的 token 无法复用（fixation 防护的基础）。
#[tokio::test]
async fn each_login_issues_fresh_session_token() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool).await;

    let token_a = create_session(&pool, &user_id).await.unwrap();
    let token_b = create_session(&pool, &user_id).await.unwrap();
    assert_ne!(token_a, token_b, "每次登录必须签发全新 token");

    let (revoked_a, ..) = session_state(&pool, &token_a).await;
    let (revoked_b, ..) = session_state(&pool, &token_b).await;
    assert!(revoked_a.is_none());
    assert!(revoked_b.is_none(), "两个独立 session 都有效");

    close_pool(&pool).await;
    cleanup(&dir);
}
