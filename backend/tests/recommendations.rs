//! 推荐流集成测试（SQLite 迁移库）：端到端验证
//! 「排除本人/已互动 → 兴趣画像加分 → 冷启动兜底」的完整管线。
//!
//! 覆盖：
//! - 登录用户：本人/已点赞/已回复的帖子被排除；关注板块帖子排前且
//!   `reason = 你关注的板块`；`strategy = interest-v1`；
//! - 兴趣画像主导排序：关注板块低热度帖压过无关板块高热度帖；
//! - 冷启动（匿名）：`strategy = trending-fallback`，按热度+新鲜度排序。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::routes::recommendations::recommend_posts;
use serde_json::Value;
use sqlx::Either;

const NOW: i64 = 1_800_000_000_000;
const HOUR: i64 = 3_600_000;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-rec-{}", uuid::Uuid::now_v7()));
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

/// 绑定辅助：把字符串参数转为 Option 切片（exec 统一按可空绑定）。
fn binds<'a>(values: &[&'a str]) -> Vec<Option<&'a str>> {
    values.iter().map(|v| Some(*v)).collect()
}

async fn exec(pool: &DatabasePool, sql: &str, params: &[Option<&str>]) {
    match pool {
        Either::Left(p) => {
            let mut q = sqlx::query(sql);
            for b in params {
                q = q.bind(*b);
            }
            q.execute(p).await.unwrap();
        }
        Either::Right(p) => {
            let mut q = sqlx::query(sql);
            for b in params {
                q = q.bind(*b);
            }
            q.execute(p).await.unwrap();
        }
    }
}

/// 种子用户（dummy 凭据，仅作外键存在）。
async fn seed_user(pool: &DatabasePool) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis().to_string();
    let email = format!("{}@example.com", uuid::Uuid::now_v7().simple());
    let uname = uuid::Uuid::now_v7().simple().to_string();
    exec(
        pool,
        "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, created_at, updated_at)
         VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?)",
        &binds(&[&user_id, &uname, &email, &now, &now]),
    )
    .await;
    user_id
}

/// 种子板块。
async fn seed_board(pool: &DatabasePool, slug: &str, name: &str) -> String {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis().to_string();
    exec(
        pool,
        "INSERT INTO boards (id, slug, name, description, is_active, created_at, updated_at)
         VALUES (?, ?, ?, '', 1, ?, ?)",
        &binds(&[&board_id, slug, name, &now, &now]),
    )
    .await;
    board_id
}

/// 种子帖子（published；created_at 由调用方控制以验证新鲜度）。
async fn seed_post(
    pool: &DatabasePool,
    board_id: &str,
    author_id: &str,
    title: &str,
    created_at: i64,
    reply_count: i64,
    view_count: i64,
) -> String {
    let post_id = uuid::Uuid::now_v7().to_string();
    let created = created_at.to_string();
    let updated = created_at.to_string();
    let replies = reply_count.to_string();
    let views = view_count.to_string();
    exec(
        pool,
        "INSERT INTO posts (id, board_id, author_id, post_type, title, content, status, visibility,
                            reply_count, view_count, created_at, updated_at)
         VALUES (?, ?, ?, 'discussion', ?, '正文', 'published', 'public', ?, ?, ?, ?)",
        &binds(&[
            &post_id, board_id, author_id, title, &replies, &views, &created, &updated,
        ]),
    )
    .await;
    post_id
}

/// 种子点赞（post_reactions）。
async fn seed_like(pool: &DatabasePool, post_id: &str, user_id: &str) {
    let now = now_millis().to_string();
    exec(
        pool,
        "INSERT INTO post_reactions (post_id, user_id, reaction, created_at) VALUES (?, ?, 'like', ?)",
        &binds(&[post_id, user_id, &now]),
    )
    .await;
}

/// 种子评论（published）。
async fn seed_comment(pool: &DatabasePool, post_id: &str, author_id: &str) {
    let comment_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis().to_string();
    exec(
        pool,
        "INSERT INTO comments (id, post_id, author_id, content, status, created_at, updated_at)
         VALUES (?, ?, ?, '回复', 'published', ?, ?)",
        &binds(&[&comment_id, post_id, author_id, &now, &now]),
    )
    .await;
}

/// 种子板块关注。
async fn seed_board_follow(pool: &DatabasePool, user_id: &str, board_id: &str) {
    let now = now_millis().to_string();
    exec(
        pool,
        "INSERT INTO board_follows (user_id, board_id, created_at) VALUES (?, ?, ?)",
        &binds(&[user_id, board_id, &now]),
    )
    .await;
}

/// 种子标签 + 帖子关联（同名标签复用既有行——tags.name 唯一）。
async fn seed_tag(pool: &DatabasePool, post_id: &str, name: &str) {
    let tag_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis().to_string();
    exec(
        pool,
        "INSERT INTO tags (id, name, usage_count, created_at) VALUES (?, ?, 1, ?)
         ON CONFLICT(name) DO NOTHING",
        &binds(&[&tag_id, name, &now]),
    )
    .await;
    let existing: Option<String> = match pool {
        Either::Left(p) => {
            let r: (String,) = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
                .bind(name)
                .fetch_one(p)
                .await
                .unwrap();
            Some(r.0)
        }
        Either::Right(p) => {
            let r: (String,) = sqlx::query_as("SELECT id FROM tags WHERE name = ?")
                .bind(name)
                .fetch_one(p)
                .await
                .unwrap();
            Some(r.0)
        }
    };
    let tag_id = existing.unwrap();
    exec(
        pool,
        "INSERT INTO post_tags (post_id, tag_id) VALUES (?, ?)",
        &binds(&[post_id, &tag_id]),
    )
    .await;
}

/// 从推荐响应提取 (title, reason, score) 列表。
fn rows_of(items: &[Value]) -> Vec<(String, String, f64)> {
    items
        .iter()
        .map(|v| {
            (
                v["title"].as_str().unwrap_or_default().to_owned(),
                v["reason"].as_str().unwrap_or_default().to_owned(),
                v["score"].as_f64().unwrap_or_default(),
            )
        })
        .collect()
}

#[tokio::test]
async fn excludes_self_and_engaged_and_boosts_followed_board() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let viewer = seed_user(&pool).await;
    let alice = seed_user(&pool).await;
    let bob = seed_user(&pool).await;

    let followed_board = seed_board(&pool, "followed", "关注板块").await;
    let other_board = seed_board(&pool, "other", "无关板块").await;

    // 关注板块：alice 的帖子（viewer 未互动过，较新）。
    seed_post(
        &pool,
        &followed_board,
        &alice,
        "关注板块新帖",
        NOW - 2 * HOUR,
        1,
        10,
    )
    .await;
    // 无关板块：bob 的高热度帖（互动量远超前者）。
    seed_post(
        &pool,
        &other_board,
        &bob,
        "无关板块高热帖",
        NOW - 2 * HOUR,
        40,
        900,
    )
    .await;
    // 应被排除的三类：本人帖 / 已点赞 / 已回复。
    seed_post(
        &pool,
        &followed_board,
        &viewer,
        "我自己的帖",
        NOW - 3 * HOUR,
        0,
        0,
    )
    .await;
    let liked = seed_post(
        &pool,
        &followed_board,
        &alice,
        "我点赞过的帖",
        NOW - 4 * HOUR,
        0,
        0,
    )
    .await;
    seed_like(&pool, &liked, &viewer).await;
    let replied = seed_post(
        &pool,
        &followed_board,
        &alice,
        "我回复过的帖",
        NOW - 5 * HOUR,
        1,
        0,
    )
    .await;
    seed_comment(&pool, &replied, &viewer).await;

    seed_board_follow(&pool, &viewer, &followed_board).await;

    let (items, strategy) = recommend_posts(&pool, Some(&viewer), 10, NOW, "test")
        .await
        .unwrap();
    assert_eq!(strategy, "interest-v1");
    let rows = rows_of(&items);
    let titles: Vec<&str> = rows.iter().map(|(t, _, _)| t.as_str()).collect();
    assert!(!titles.contains(&"我自己的帖"), "本人帖必须被排除");
    assert!(!titles.contains(&"我点赞过的帖"), "已点赞帖必须被排除");
    assert!(!titles.contains(&"我回复过的帖"), "已回复帖必须被排除");
    assert_eq!(rows[0].0, "关注板块新帖");
    assert_eq!(rows[0].1, "你关注的板块");
    // 兴趣主导：无关板块高热帖排在关注板块帖之后。
    assert_eq!(rows[1].0, "无关板块高热帖");
    assert_eq!(rows[1].1, "社区热门");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn tag_match_boosts_and_reports_reason() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let viewer = seed_user(&pool).await;
    let alice = seed_user(&pool).await;
    let board = seed_board(&pool, "tags", "标签板块").await;

    // viewer 点赞过带 rust 标签的旧帖 → rust 成为兴趣标签。
    let engaged = seed_post(&pool, &board, &alice, "旧帖", NOW - 48 * HOUR, 1, 0).await;
    seed_tag(&pool, &engaged, "rust").await;
    seed_like(&pool, &engaged, &viewer).await;

    // 同板块新帖也打了 rust 标签 → 标签命中， reason = 相关标签。
    let candidate = seed_post(&pool, &board, &alice, "新帖", NOW - HOUR, 0, 0).await;
    seed_tag(&pool, &candidate, "rust").await;

    let (items, strategy) = recommend_posts(&pool, Some(&viewer), 10, NOW, "test")
        .await
        .unwrap();
    assert_eq!(strategy, "interest-v1");
    let rows = rows_of(&items);
    let hit = rows
        .iter()
        .find(|(t, _, _)| t == "新帖")
        .expect("新帖必须出现");
    assert_eq!(hit.1, "相关标签");
    // 已点赞的旧帖被排除（避免重复推荐）。
    assert!(!rows.iter().any(|(t, _, _)| t == "旧帖"));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn anonymous_cold_start_falls_back_to_trending() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let alice = seed_user(&pool).await;
    let board = seed_board(&pool, "trend", "热门板块").await;

    seed_post(&pool, &board, &alice, "高热新帖", NOW - 2 * HOUR, 30, 900).await;
    seed_post(&pool, &board, &alice, "低热旧帖", NOW - 96 * HOUR, 0, 2).await;

    let (items, strategy) = recommend_posts(&pool, None, 10, NOW, "test").await.unwrap();
    assert_eq!(strategy, "trending-fallback");
    let rows = rows_of(&items);
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].0, "高热新帖", "冷启动按热度+新鲜度排序");
    assert_eq!(rows[0].1, "社区热门");
    assert_eq!(rows[1].0, "低热旧帖");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn empty_database_returns_empty_not_error() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let (items, strategy) = recommend_posts(&pool, None, 8, NOW, "test").await.unwrap();
    assert!(items.is_empty());
    assert_eq!(strategy, "trending-fallback");
    close_pool(&pool).await;
    cleanup(&dir);
}
