//! M04-POSTS-05：发布前预检（P0）——发布前重新读取作者等级/账号状态/板块
//! 规则/附件状态/access policy（SQLite）。
//!
//! 覆盖：正常态通过；未验证邮箱/冷静期/封禁账号阻断；visibility 超等级阻断；
//! 板块不存在/停用/只读阻断；未知附件阻断；纯逻辑 policy 校验在
//! publish.rs 单测。

use std::path::{Path, PathBuf};

use bblbb_backend::content::posts::publish::{
    publish_preflight, PublishBlocked, PublishPreflightInput,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-ppl-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    bblbb_backend::authz::roles::seed_builtin_roles(&pool)
        .await
        .unwrap();
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

/// 插入作者；`level` 为等级、`verified` 是否已验证、`verified_ago_ms` 验证
/// 距今毫秒（>24h 冷静期通过）、`status` 账号状态。
async fn insert_author(
    pool: &DatabasePool,
    tag: &str,
    level: i64,
    verified: bool,
    verified_ago_ms: i64,
    status: &str,
) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', ?, ?, ?, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(status)
            .bind(level)
            .bind(if verified { 1 } else { 0 })
            .bind(if verified {
                Some(now - verified_ago_ms)
            } else {
                None
            })
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

async fn set_board_mode(pool: &DatabasePool, board_id: &str, active: bool, mode: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE boards SET is_active = ?, posting_mode = ? WHERE id = ?")
                .bind(if active { 1 } else { 0 })
                .bind(mode)
                .bind(board_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn input(author_id: &str, board_id: &str, visibility: Option<u32>) -> PublishPreflightInput {
    PublishPreflightInput {
        author_id: author_id.to_string(),
        board_id: board_id.to_string(),
        visibility_level: visibility,
        access_policy: "public".to_string(),
        min_level: None,
        currency_id: None,
        amount: None,
        attachment_ids: Vec::new(),
    }
}

/// 已过冷静期的有效作者。
async fn fresh_valid_author(pool: &DatabasePool) -> String {
    insert_author(pool, "ok", 5, true, 25 * 3600 * 1000, "active").await
}

#[tokio::test]
async fn preflight_passes_for_valid_state() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = fresh_valid_author(&pool).await;
    let r = publish_preflight(&pool, &input(&author, BOARD_ID, Some(1))).await;
    assert_eq!(r, Ok(()), "正常作者+活跃板块必须通过预检");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn preflight_blocks_unverified_and_cooldown_accounts() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    // 未验证邮箱
    let unverified = insert_author(&pool, "unv", 5, false, 0, "active").await;
    let r = publish_preflight(&pool, &input(&unverified, BOARD_ID, None)).await;
    assert!(matches!(r, Err(PublishBlocked::AccountUnavailable(msg)) if msg.contains("email")));
    // 冷静期内（刚验证 1 小时）
    let cooldown = insert_author(&pool, "cd", 5, true, 3600 * 1000, "active").await;
    let r = publish_preflight(&pool, &input(&cooldown, BOARD_ID, None)).await;
    assert!(matches!(r, Err(PublishBlocked::AccountUnavailable(msg)) if msg.contains("cooldown")));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn preflight_blocks_banned_account() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let banned = insert_author(&pool, "ban", 5, true, 25 * 3600 * 1000, "banned").await;
    let r = publish_preflight(&pool, &input(&banned, BOARD_ID, None)).await;
    assert!(
        matches!(r, Err(PublishBlocked::AccountUnavailable(_))),
        "封禁账号必须阻断: {r:?}"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn preflight_blocks_visibility_above_author_level() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    // 等级 2 作者申请 visibility 3 → 阻断（发布时重读等级）
    let author = insert_author(&pool, "lv", 2, true, 25 * 3600 * 1000, "active").await;
    let r = publish_preflight(&pool, &input(&author, BOARD_ID, Some(3))).await;
    assert_eq!(
        r,
        Err(PublishBlocked::VisibilityExceedsLevel {
            requested: 3,
            author_level: 2
        })
    );
    // 等级 3 作者申请 visibility 3 → 通过
    let author3 = insert_author(&pool, "lv3", 3, true, 25 * 3600 * 1000, "active").await;
    let r = publish_preflight(&pool, &input(&author3, BOARD_ID, Some(3))).await;
    assert_eq!(r, Ok(()));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn preflight_blocks_bad_board_state() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = fresh_valid_author(&pool).await;
    // 不存在
    let r = publish_preflight(
        &pool,
        &input(&author, "01911fd5-f999-7561-a2a5-3dd6434157f0", None),
    )
    .await;
    assert!(matches!(r, Err(PublishBlocked::BoardNotAcceptingPosts(_))));
    // 停用
    set_board_mode(&pool, BOARD_ID, false, "normal").await;
    let r = publish_preflight(&pool, &input(&author, BOARD_ID, None)).await;
    assert!(matches!(
        r,
        Err(PublishBlocked::BoardNotAcceptingPosts(msg)) if msg.contains("not active")
    ));
    // 只读
    set_board_mode(&pool, BOARD_ID, true, "readonly").await;
    let r = publish_preflight(&pool, &input(&author, BOARD_ID, None)).await;
    assert!(matches!(
        r,
        Err(PublishBlocked::BoardNotAcceptingPosts(msg)) if msg.contains("read-only")
    ));
    // 关闭
    set_board_mode(&pool, BOARD_ID, true, "closed").await;
    let r = publish_preflight(&pool, &input(&author, BOARD_ID, None)).await;
    assert!(matches!(
        r,
        Err(PublishBlocked::BoardNotAcceptingPosts(msg)) if msg.contains("closed")
    ));
    // 恢复正常
    set_board_mode(&pool, BOARD_ID, true, "normal").await;
    let r = publish_preflight(&pool, &input(&author, BOARD_ID, None)).await;
    assert_eq!(r, Ok(()));
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn preflight_rejects_attachments_before_m6() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = fresh_valid_author(&pool).await;
    let mut i = input(&author, BOARD_ID, None);
    i.attachment_ids = vec!["01911fd5-f000-0000-0000-000000000001".to_string()];
    // attachments 表 M6 才落地：有引用且表不存在 → 明确拒绝（不静默忽略）
    let r = publish_preflight(&pool, &i).await;
    assert!(matches!(
        r,
        Err(PublishBlocked::AttachmentNotAllowed(msg)) if msg.contains("not supported")
    ));
    close_pool(&pool).await;
    cleanup(&dir);
}
