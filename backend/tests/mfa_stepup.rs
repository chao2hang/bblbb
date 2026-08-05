//! M02-MFA-07：近期认证 / step-up。
//! - `create_session` 签发会话即写入 `auth_verified_at`（完整认证时间）；
//! - `step_up_required` 纯函数：None（从未完整认证）→ 需要；超过窗口 → 需要；
//! - `mark_step_up` 刷新会话的 `auth_verified_at`，无效会话返回 RowNotFound；
//! - `is_step_up_required_for_session` 端到端（含撤销/过期 fail-closed）。

use std::path::{Path, PathBuf};

use bblbb_backend::auth::session::{create_session, revoke_session};
use bblbb_backend::auth::{
    is_step_up_required_for_session, mark_step_up, step_up_required, DEFAULT_STEP_UP_WINDOW_SECS,
};
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
    let dir = std::env::temp_dir().join(format!("bblbb-stepup-{}", uuid::Uuid::now_v7()));
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
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

/// 读取会话的 auth_verified_at（Unix 毫秒，NULL → None）。
async fn auth_verified_at(pool: &DatabasePool, session_token: &str) -> Option<i64> {
    let token_hash = bblbb_backend::auth::hash_token(session_token);
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT auth_verified_at FROM user_sessions WHERE token_hash = ?")
                .bind(token_hash)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 把会话的 auth_verified_at 回溯到（now - offset_ms）毫秒。
async fn backdate_verification(pool: &DatabasePool, session_token: &str, offset_ms: i64) {
    let token_hash = bblbb_backend::auth::hash_token(session_token);
    let backdated = now_millis() - offset_ms;
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE user_sessions SET auth_verified_at = ? WHERE token_hash = ?")
                .bind(backdated)
                .bind(token_hash)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

// ─────────────────────────── 纯函数：step_up_required ───────────────────────────

/// None（从未完整认证）→ 需要 step-up（fail closed）。
#[tokio::test]
async fn step_up_required_when_never_verified() {
    assert!(step_up_required(None, 1_000_000, 300));
}

/// 认证时间距今恰好在窗口内 → 不需要 step-up（含边界：等于窗口不触发）。
#[tokio::test]
async fn step_up_required_false_within_window() {
    let now_secs = 1_000_000u64;
    let window_secs = 300u64;
    // 恰好 300 秒前认证：窗口边界，不触发
    assert!(!step_up_required(
        Some(((now_secs - 300) * 1000) as i64),
        now_secs,
        window_secs
    ));
    // 刚认证
    assert!(!step_up_required(
        Some((now_secs * 1000) as i64),
        now_secs,
        window_secs
    ));
}

/// 认证时间超过窗口 → 需要 step-up。
#[tokio::test]
async fn step_up_required_true_after_window() {
    let now_secs = 1_000_000u64;
    let window_secs = 300u64;
    assert!(step_up_required(
        Some((now_secs.saturating_sub(301) * 1000) as i64),
        now_secs,
        window_secs
    ));
    // 很久以前认证
    assert!(step_up_required(
        Some(((now_secs - 7 * 24 * 3600) * 1000) as i64),
        now_secs,
        window_secs
    ));
}

// ─────────────────────────── create_session 写入 auth_verified_at ───────────────────────────

/// 签发会话即写入 auth_verified_at（≈ now）。
#[tokio::test]
async fn create_session_sets_auth_verified_at() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "alice").await;

    let before = now_millis();
    let token = create_session(&pool, &user_id).await.unwrap();
    let after = now_millis();

    let verified = auth_verified_at(&pool, &token).await;
    assert!(
        verified.is_some(),
        "create_session 必须写入 auth_verified_at"
    );
    let v = verified.unwrap();
    assert!(
        v >= before && v <= after,
        "auth_verified_at 必须约为签发时刻: {v} not in [{before}, {after}]"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── mark_step_up ───────────────────────────

/// mark_step_up 刷新 auth_verified_at → 不再要求 step-up。
#[tokio::test]
async fn mark_step_up_refreshes_and_clears_requirement() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "bob").await;
    let token = create_session(&pool, &user_id).await.unwrap();

    // 回溯到超过窗口 → 需要 step-up
    let window_ms = DEFAULT_STEP_UP_WINDOW_SECS as i64 * 1000;
    backdate_verification(&pool, &token, window_ms + 60_000).await;
    assert!(
        is_step_up_required_for_session(&pool, &token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap()
    );

    // mark_step_up → 刷新 → 不再要求
    mark_step_up(&pool, &token).await.unwrap();
    assert!(
        !is_step_up_required_for_session(&pool, &token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap()
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// mark_step_up 对无效/未知会话 → RowNotFound。
#[tokio::test]
async fn mark_step_up_on_unknown_session_row_not_found() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "carol").await;
    let token = create_session(&pool, &user_id).await.unwrap();

    let err = mark_step_up(&pool, "not-a-real-token").await.unwrap_err();
    assert!(
        matches!(err, sqlx::Error::RowNotFound),
        "未知会话必须返回 RowNotFound: {err:?}"
    );

    // 撤销后再 mark_step_up 同样 RowNotFound（fail closed）
    revoke_session(&pool, &token).await.unwrap();
    let err2 = mark_step_up(&pool, &token).await.unwrap_err();
    assert!(
        matches!(err2, sqlx::Error::RowNotFound),
        "已撤销会话 mark_step_up 必须 RowNotFound: {err2:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ─────────────────────────── is_step_up_required_for_session 端到端 ───────────────────────────

/// 新建会话在窗口内 → 不需要；回溯超过窗口 → 需要。
#[tokio::test]
async fn session_requires_step_up_after_window_only() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "dave").await;
    let token = create_session(&pool, &user_id).await.unwrap();

    // 刚签发：在窗口内
    assert!(
        !is_step_up_required_for_session(&pool, &token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap()
    );

    // 回溯 1 分钟（窗口内）→ 仍不要求
    backdate_verification(&pool, &token, 60_000).await;
    assert!(
        !is_step_up_required_for_session(&pool, &token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap()
    );

    // 回溯到窗口外 → 要求
    backdate_verification(&pool, &token, 400_000).await;
    assert!(
        is_step_up_required_for_session(&pool, &token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap()
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 会话被撤销 → 一律需要 step-up（fail closed）。
#[tokio::test]
async fn revoked_session_always_requires_step_up() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "erin").await;
    let token = create_session(&pool, &user_id).await.unwrap();

    revoke_session(&pool, &token).await.unwrap();
    assert!(
        is_step_up_required_for_session(&pool, &token, DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap(),
        "已撤销会话必须要求 step-up"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 未知 token → 需要 step-up（fail closed）。
#[tokio::test]
async fn unknown_token_requires_step_up() {
    let (pool, dir) = pool_with_migrations().await;
    let user_id = insert_user(&pool, "frank").await;
    let _token = create_session(&pool, &user_id).await.unwrap();

    assert!(
        is_step_up_required_for_session(&pool, "bogus-token", DEFAULT_STEP_UP_WINDOW_SECS)
            .await
            .unwrap()
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
