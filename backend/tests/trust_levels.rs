//! 信任等级（M20-TRUST）集成测试（SQLite + 领域服务 + 路由层）。
//!
//! 覆盖：行为统计幂等与服务端钳制（访问/进入话题/阅读楼层/阅读时长），
//! 累计口径晋升 0→1→2，TL3 滚动窗口晋升（含 25%/上限与点赞多样性），
//! TL3 宽限期与降级，TL4 手动授予（领域 + 管理路由 + 审计），以及
//! GET /me/trust-level、GET /admin/trust-levels 的鉴权与响应形状。
//!
//! 映射口径：话题=posts（统一内容）、帖子=comments 楼层、点赞=user_reactions
//! （reactions 服务唯一写入路径）、禁言/封禁=sanctions(mute/ban)。

use std::path::{Path, PathBuf};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::trust::{self, service, store};
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::Either;
use tower::ServiceExt;

mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-trust-{}", uuid::Uuid::now_v7()));
    let url = format!("sqlite://{}", dir.display());
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(
        &Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../migrations/sqlite"),
    )
    .unwrap();
    run_migrations(&pool, &files).await.unwrap();
    seed_builtin_roles(&pool).await.unwrap();
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

fn app_with(pool: DatabasePool) -> Router {
    build_router(AppConfig::default(), Some(pool))
}

/// 插入 active 用户（trust_level 默认 0）。
async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 1, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(format!("{username}@example.com"))
            .bind(now - 3600 * 1000)
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

async fn count(pool: &DatabasePool, sql: &str, arg: &str) -> i64 {
    match pool {
        Either::Left(p) => sqlx::query_scalar(sql)
            .bind(arg)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 直接插入一行统计/种子数据（SQLite 专用测试夹具）。
/// 池按值传入（内部 clone，廉价）；args 统一按引用绑定，值/引用均可传。
macro_rules! seed_exec {
    ($pool:expr, $sql:expr $(, $arg:expr)* $(,)?) => {
        match $pool.clone() {
            Either::Left(p) => {
                sqlx::query($sql)$( .bind(&$arg) )* .execute(&p).await.unwrap();
            }
            Either::Right(_) => panic!("SQLite only"),
        }
    };
}

fn day(offset_days: i64) -> String {
    store::utc_day_of(now_millis() - offset_days * 86_400_000)
}

async fn insert_post(pool: &DatabasePool, author_id: &str, created_at: i64) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    seed_exec!(
        pool,
        "INSERT INTO posts (id, board_id, author_id, title, content, status, visibility, created_at, updated_at, deleted_at)
         VALUES (?, ?, ?, '信任等级测试帖', '正文', 'published', 'public', ?, ?, NULL)",
        &id, BOARD_ID, author_id, created_at, created_at,
    );
    id
}

/// 插入楼层：floor 按 MAX+1 分配（comments_post_floor_uq 唯一约束）。
async fn insert_comment(
    pool: &DatabasePool,
    post_id: &str,
    author_id: &str,
    created_at: i64,
) -> String {
    let id = uuid::Uuid::now_v7().to_string();
    let floor: i64 = match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COALESCE(MAX(floor), 0) + 1 FROM comments WHERE post_id = ?")
                .bind(post_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    seed_exec!(
        pool,
        "INSERT INTO comments (id, post_id, author_id, content, status, floor, created_at, updated_at)
         VALUES (?, ?, ?, '楼层内容', 'published', ?, ?, ?)",
        &id, post_id, author_id, floor, created_at, created_at,
    );
    id
}

async fn insert_reaction(
    pool: &DatabasePool,
    actor_id: &str,
    target_type: &str,
    target_id: &str,
    created_at: i64,
) {
    seed_exec!(
        pool,
        "INSERT OR IGNORE INTO user_reactions (user_id, target_type, target_id, reaction, created_at)
         VALUES (?, ?, ?, 'like', ?)",
        actor_id, target_type, target_id, created_at,
    );
}

/// 直接把用户置为某信任等级（reason='seed'，事件可后续改时间）。
async fn seed_level(pool: &DatabasePool, user_id: &str, level: i64) {
    store::set_level(pool, user_id, None, level, "seed", Some("测试夹具"), None)
        .await
        .unwrap();
}

// ───────────────────────── 统计写入：幂等与钳制 ─────────────────────────

#[tokio::test]
async fn stats_idempotent_and_read_time_clamped() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "clamp").await;

    // 访问幂等：同日多次 → 一行。
    trust::on_session_active(&pool, &user).await;
    trust::on_session_active(&pool, &user).await;
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM trust_visits WHERE user_id = ?",
            &user
        )
        .await,
        1
    );

    // 进入话题幂等。
    let post = insert_post(&pool, &user, now_millis()).await;
    store::record_topic_view(&pool, &user, &post).await.unwrap();
    store::record_topic_view(&pool, &user, &post).await.unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM trust_topic_views WHERE user_id = ?",
            &user
        )
        .await,
        1
    );

    // 阅读楼层幂等。
    let comment = insert_comment(&pool, &post, &user, now_millis()).await;
    store::record_comment_reads(&pool, &user, &post, &[comment.clone()])
        .await
        .unwrap();
    store::record_comment_reads(&pool, &user, &post, &[comment])
        .await
        .unwrap();
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM trust_comment_reads WHERE user_id = ?",
            &user
        )
        .await,
        1
    );

    // 阅读时长：单请求钳 60s，按日累计 7200s 封顶。
    let (credited, total) = service::add_read_time(&pool, &user, 30).await.unwrap();
    assert_eq!((credited, total), (30, 30));
    for _ in 0..4 {
        service::add_read_time(&pool, &user, 60).await.unwrap();
    }
    let (_, total) = service::add_read_time(&pool, &user, 60).await.unwrap();
    assert_eq!(total, 330, "30 + 5×60 = 330");
    // 打满每日上限。
    for _ in 0..120 {
        store::add_read_time(&pool, &user, 60, &day(0))
            .await
            .unwrap();
    }
    let (_, total) = service::add_read_time(&pool, &user, 60).await.unwrap();
    assert_eq!(
        total,
        store::MAX_READ_SECONDS_PER_DAY,
        "每日阅读时长必须按 7200s 封顶"
    );

    // 非法心跳：seconds < 1 → Invalid。
    assert!(service::add_read_time(&pool, &user, 0).await.is_err());

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────── 累计口径晋升 0→1→2 ─────────────────────────

#[tokio::test]
async fn cumulative_promotion_0_to_1_then_2() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "cum").await;
    let other = insert_user(&pool, "other").await;

    // TL1 子集：5 话题 / 30 楼层 / 600s。
    let host_post = insert_post(&pool, &other, now_millis()).await;
    let mut views = 0;
    while views < 5 {
        let p = if views == 0 {
            host_post.clone()
        } else {
            insert_post(&pool, &other, now_millis()).await
        };
        seed_exec!(
            pool,
            "INSERT OR IGNORE INTO trust_topic_views (user_id, post_id, first_viewed_at) VALUES (?, ?, ?)",
            user, p, now_millis(),
        );
        views += 1;
    }
    for _ in 0..30 {
        let c = insert_comment(&pool, &host_post, &other, now_millis()).await;
        seed_exec!(
            pool,
            "INSERT OR IGNORE INTO trust_comment_reads (user_id, comment_id, post_id, first_read_at) VALUES (?, ?, ?, ?)",
            user, c, host_post, now_millis(),
        );
    }
    seed_exec!(
        pool,
        "INSERT INTO trust_read_time (user_id, read_day, seconds, updated_at) VALUES (?, ?, 600, ?)",
        user, day(0), now_millis(),
    );

    let ev = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        ev.changed && ev.to_level == 1 && ev.reason == Some("promotion"),
        "TL1 晋升: {ev:?}"
    );

    // 补齐 TL2：16 天访问 / 20 话题 / 100 楼层 / 3600s / 收发赞各 1 / 回复 3 话题。
    for i in 0..16 {
        seed_exec!(
            pool,
            "INSERT OR IGNORE INTO trust_visits (user_id, visit_day, first_seen_at) VALUES (?, ?, ?)",
            user, day(i), now_millis(),
        );
    }
    seed_exec!(
        pool,
        "INSERT INTO trust_read_time (user_id, read_day, seconds, updated_at) VALUES (?, ?, 3000, ?)",
        user, day(1), now_millis(),
    );
    // 回复 3 个不同话题。
    for _ in 0..3 {
        let p = insert_post(&pool, &other, now_millis()).await;
        insert_comment(&pool, &p, &user, now_millis()).await;
    }
    // 收赞 1（other 赞用户的帖子）+ 送赞 1（用户赞 other 的帖子）。
    let my_post = insert_post(&pool, &user, now_millis()).await;
    insert_reaction(&pool, &other, "post", &my_post, now_millis()).await;
    insert_reaction(&pool, &user, "post", &host_post, now_millis()).await;
    // 楼层读到 100。
    for _ in 0..70 {
        let c = insert_comment(&pool, &host_post, &other, now_millis()).await;
        seed_exec!(
            pool,
            "INSERT OR IGNORE INTO trust_comment_reads (user_id, comment_id, post_id, first_read_at) VALUES (?, ?, ?, ?)",
            user, c, host_post, now_millis(),
        );
    }
    // 话题进到 20。
    for _ in 0..15 {
        let p = insert_post(&pool, &other, now_millis()).await;
        seed_exec!(
            pool,
            "INSERT OR IGNORE INTO trust_topic_views (user_id, post_id, first_viewed_at) VALUES (?, ?, ?)",
            user, p, now_millis(),
        );
    }

    let ev = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        ev.changed && ev.from_level == 1 && ev.to_level == 2,
        "TL2 晋升: {ev:?}"
    );
    assert_eq!(
        count(&pool, "SELECT trust_level FROM users WHERE id = ?", &user).await,
        2
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────── TL3 滚动窗口晋升 ─────────────────────────

/// 窗口规模：40 他人话题 + 用户 10 回复 + 他人 30 楼层（窗口总楼层 40）；
/// 浏览 10（ceil(25%×40)=10）/ 阅读 10 / 访问 51 天（≥50）
/// / 收 20 赞（4 人 × 5 赞，≥4 用户、≥5 天）/ 送 30 赞（30 个不同目标，8 天）
/// / 无标记无处罚。
async fn seed_tl3_window(pool: &DatabasePool, user: &str, others: &[String]) {
    let now = now_millis();
    let mut posts = Vec::new();
    // 帖子作者分散到多个用户（送赞多样性按不同收赞人统计）。
    for (i, _) in (0..40).enumerate() {
        posts.push(insert_post(pool, &others[i % others.len()], now).await);
    }
    // 用户回复 10 个不同话题（计入窗口楼层）。
    for p in posts.iter().take(10) {
        insert_comment(pool, p, user, now).await;
    }
    // 他人楼层 30。
    for p in posts.iter().skip(10).take(30) {
        insert_comment(pool, p, &others[1], now).await;
    }
    // 浏览 12 个窗口期话题（窗口总帖数 = 40 + 收赞 2 帖 + 每点赞人独占 4 帖
    // = 46，ceil(25%×46)=12）。
    for p in posts.iter().take(12) {
        seed_exec!(
            pool,
            "INSERT OR IGNORE INTO trust_topic_views (user_id, post_id, first_viewed_at) VALUES (?, ?, ?)",
            user, p, now,
        );
    }
    // 阅读 12 个窗口期楼层（窗口总楼层 = 10 回复 + 30 他人 + 2 自帖首评
    // = 42，ceil(25%×42)=11）；楼层不限作者。
    for p in posts.iter().take(12) {
        let cid: String = match pool {
            Either::Left(pp) => {
                sqlx::query_scalar("SELECT id FROM comments WHERE post_id = ? LIMIT 1")
                    .bind(p)
                    .fetch_one(pp)
                    .await
                    .unwrap()
            }
            Either::Right(_) => panic!("SQLite only"),
        };
        seed_exec!(
            pool,
            "INSERT OR IGNORE INTO trust_comment_reads (user_id, comment_id, post_id, first_read_at) VALUES (?, ?, ?, ?)",
            user, cid, p, now,
        );
    }
    // 访问 51 天。
    for i in 0..51 {
        seed_exec!(
            pool,
            "INSERT OR IGNORE INTO trust_visits (user_id, visit_day, first_seen_at) VALUES (?, ?, ?)",
            user, day(i), now,
        );
    }
    // 收赞：4 个不同用户 × 5 赞 = 20（目标 8 个用户自己的内容，(li+k)%8 保证
    // 每个点赞人对同一目标只赞一次）；时间分布 5 天。
    let my_post_a = insert_post(pool, user, now).await;
    let my_post_b = insert_post(pool, user, now).await;
    let my_comment_a = insert_comment(pool, &my_post_a, user, now).await;
    let my_comment_b = insert_comment(pool, &my_post_b, user, now).await;
    let targets: [(&str, &String); 4] = [
        ("post", &my_post_a),
        ("post", &my_post_b),
        ("comment", &my_comment_a),
        ("comment", &my_comment_b),
    ];
    // 目标扩到 8 个：4 内容 × 2 份镜像不行（唯一键），改为 4 liker × 5 赞
    // 每人打 5 个不同 (类型, id)：4 内容 + 每人 1 个独占帖。
    let mut per_liker_extra = Vec::new();
    for (li, liker) in others.iter().take(4).enumerate() {
        let extra_post = insert_post(pool, user, now).await;
        per_liker_extra.push((liker.clone(), extra_post.clone()));
        for k in 0..4i64 {
            let (ttype, tid) = &targets[k as usize];
            insert_reaction(pool, liker, ttype, tid, now - (k % 5) * 86_400_000).await;
        }
        // 第 5 赞打独占帖（保证每人 5 赞且不与 4 内容重复）；天数固定为第 5 天，
        // 与内容赞的 4 天取并集恰好 5 天（要求 ceil(20/4)=5）。
        insert_reaction(pool, liker, "post", &extra_post, now - 4 * 86_400_000).await;
    }
    let _ = per_liker_extra;
    // 送赞：30 赞打 30 个不同窗口期帖子（0..30 不重复），时间分布 8 天。
    for k in 0..30i64 {
        let p = &posts[k as usize % posts.len()];
        insert_reaction(pool, user, "post", p, now - (k % 8) * 86_400_000).await;
    }
}

#[tokio::test]
async fn tl3_window_promotion() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "tl3").await;
    let mut other_ids = Vec::new();
    for i in 0..8 {
        other_ids.push(insert_user(&pool, &format!("o{i}")).await);
    }

    seed_level(&pool, &user, 2).await;
    seed_tl3_window(&pool, &user, &other_ids).await;

    let ev = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        ev.changed && ev.from_level == 2 && ev.to_level == 3 && ev.reason == Some("promotion"),
        "TL3 窗口晋升: {ev:?}"
    );

    // 达标后再次评估：稳定在 3。
    let ev2 = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(!ev2.changed && ev2.to_level == 3);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────── TL3 宽限与降级 ─────────────────────────

#[tokio::test]
async fn tl3_grace_protects_then_demotes() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let in_grace = insert_user(&pool, "grace").await;
    let past_grace = insert_user(&pool, "demote").await;

    // 宽限期内：3 级 + 零统计 → 不降级。
    seed_level(&pool, &in_grace, 3).await;
    let ev = service::evaluate_user(&pool, &in_grace).await.unwrap();
    assert!(!ev.changed && ev.to_level == 3, "宽限期内不降级: {ev:?}");

    // 宽限期已过：3 级 + 零统计 → 降回 2（成员）。
    seed_level(&pool, &past_grace, 3).await;
    seed_exec!(
        pool,
        "UPDATE trust_level_events SET created_at = ? WHERE user_id = ? AND to_level = 3",
        now_millis() - 20 * 86_400_000,
        past_grace,
    );
    let ev = service::evaluate_user(&pool, &past_grace).await.unwrap();
    assert!(
        ev.changed && ev.from_level == 3 && ev.to_level == 2 && ev.reason == Some("demotion"),
        "超宽限降级: {ev:?}"
    );
    assert_eq!(
        count(
            &pool,
            "SELECT trust_level FROM users WHERE id = ?",
            &past_grace
        )
        .await,
        2
    );

    // 降级事件落库。
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM trust_level_events WHERE user_id = ? AND reason = 'demotion' AND to_level = 2",
            &past_grace,
        ).await,
        1,
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────── TL4 手动授予 ─────────────────────────

#[tokio::test]
async fn manual_tl4_and_evaluation_freeze() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let admin = insert_user(&pool, "adm").await;
    let target = insert_user(&pool, "target").await;

    let ev = service::manual_set(&pool, &admin, &target, 4, "创始人授予")
        .await
        .unwrap();
    assert!(ev.changed && ev.to_level == 4);
    assert_eq!(
        count(&pool, "SELECT trust_level FROM users WHERE id = ?", &target).await,
        4
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM trust_level_events WHERE user_id = ? AND reason = 'manual' AND to_level = 4",
            &target,
        ).await,
        1,
    );
    // 自动评估不得改动 TL4。
    let ev2 = service::evaluate_user(&pool, &target).await.unwrap();
    assert!(!ev2.changed && ev2.to_level == 4);
    // 越界值 / 未知用户拒绝。
    assert!(service::manual_set(&pool, &admin, &target, 5, "x")
        .await
        .is_err());
    assert!(service::manual_set(
        &pool,
        &admin,
        "01920000-0000-7000-8000-000000000000",
        2,
        "x"
    )
    .await
    .is_err());

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────── HTTP 路由 ─────────────────────────

async fn session_csrf(app: &Router, session: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/auth/csrf")
                .header("cookie", session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    body["token"].as_str().unwrap().to_string()
}

async fn anon_get(app: &Router, uri: &str) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn authed(
    app: &Router,
    method: &str,
    uri: &str,
    session: &str,
    csrf: &str,
    body: Value,
) -> (StatusCode, Value) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .header("x-csrf-token", csrf)
                .header("cookie", session)
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn admin_ctx(app: &Router, pool: &DatabasePool) -> (String, String, String) {
    let admin_id = insert_user(pool, "admin").await;
    match pool {
        Either::Left(p) => {
            let role_id: String =
                sqlx::query_scalar("SELECT id FROM roles WHERE name = 'administrator'")
                    .fetch_one(p)
                    .await
                    .unwrap();
            sqlx::query(
                "INSERT OR IGNORE INTO user_roles (user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, NULL, ?, NULL)",
            )
            .bind(&admin_id)
            .bind(&role_id)
            .bind(now_millis() - 60_000)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    common::enroll_totp(pool, &admin_id).await;
    let session = common::direct_session_cookie(pool, &admin_id).await;
    let token = session.split('=').nth(1).unwrap().to_string();
    bblbb_backend::auth::session::mark_step_up(pool, &token)
        .await
        .unwrap();
    let csrf = session_csrf(app, &session).await;
    (admin_id, session, csrf)
}

#[tokio::test]
async fn me_trust_level_endpoint_auth_and_shape() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let user = insert_user(&pool, "me").await;
    let session = common::direct_session_cookie(&pool, &user).await;

    // 匿名 401。
    let (status, _) = anon_get(&app, "/api/v1/me/trust-level").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);

    // 登录：200 + 结构。
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri("/api/v1/me/trust-level")
                .header("cookie", &session)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(
        resp.headers()
            .get("cache-control")
            .and_then(|v| v.to_str().ok()),
        Some("private, no-store"),
        "进度视图必须 private, no-store"
    );
    let body: Value =
        serde_json::from_slice(&resp.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(body["level"], 0);
    assert_eq!(body["name"], "新用户");
    assert_eq!(body["next_level"]["level"], 1);
    assert_eq!(body["next_level"]["name"], "基本用户");
    let reqs = body["next_level"]["requirements"].as_array().unwrap();
    assert!(reqs
        .iter()
        .any(|r| r["key"] == "topics_entered" && r["required"] == 5));

    // 读帖心跳：合法 + 越界 + 多余字段。
    let csrf = session_csrf(&app, &session).await;
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/me/trust-level/read-time",
        &session,
        &csrf,
        json!({ "seconds": 45 }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["credited_seconds"], 45);
    assert_eq!(body["day_total_seconds"], 45);
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/me/trust-level/read-time",
        &session,
        &csrf,
        json!({ "seconds": 61 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/me/trust-level/read-time",
        &session,
        &csrf,
        json!({ "seconds": 5, "post_id": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "多余字段必须拒绝");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_trust_level_endpoints_authz_and_manual_set() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let target = insert_user(&pool, "t").await;
    let member = insert_user(&pool, "mem").await;
    let member_session = common::direct_session_cookie(&pool, &member).await;
    let member_csrf = session_csrf(&app, &member_session).await;

    // 匿名 401（POST 无会话）。
    let (status, _) = anon_get(&app, "/api/v1/admin/trust-levels").await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/admin/users/{target}/trust-level"))
                .header("content-type", "application/json")
                .body(Body::from(json!({ "level": 1, "reason": "x" }).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "匿名手动授予必须 401"
    );

    // member 403（无 level.manage）。
    let (status, _) = authed(
        &app,
        "GET",
        "/api/v1/admin/trust-levels",
        &member_session,
        &member_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{target}/trust-level"),
        &member_session,
        &member_csrf,
        json!({ "level": 4, "reason": "越权" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // admin GET /admin/trust-levels：5 级规则 + 计数字段。
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/trust-levels",
        &admin_session,
        &admin_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 5);
    assert_eq!(items[3]["name"], "活跃用户");
    assert_eq!(items[3]["requirements"]["window_days"], 100);

    // admin 手动授予 TL4：200 + 落库 + 审计。
    let (status, body) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{target}/trust-level"),
        &admin_session,
        &admin_csrf,
        json!({ "level": 4, "reason": "授予领导者" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["to_level"], 4);
    assert_eq!(
        count(&pool, "SELECT trust_level FROM users WHERE id = ?", &target).await,
        4
    );
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.trust_level.set' AND target_id = ?",
            &target,
        ).await,
        1,
        "手动授予必须写审计"
    );

    // 校验：越界等级 / 缺 reason / 未知用户。
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{target}/trust-level"),
        &admin_session,
        &admin_csrf,
        json!({ "level": 9, "reason": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    let (status, _) = authed(
        &app,
        "POST",
        &format!("/api/v1/admin/users/{target}/trust-level"),
        &admin_session,
        &admin_csrf,
        json!({ "level": 2 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "缺 reason 必须 400");
    let (status, _) = authed(
        &app,
        "POST",
        "/api/v1/admin/users/01920000-0000-7000-8000-000000000000/trust-level",
        &admin_session,
        &admin_csrf,
        json!({ "level": 2, "reason": "x" }),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────── 等级规则可配置化（2026-09）─────────────────────────

/// PATCH 请求（带 If-Match 头；authed 不支持自定义头，此处独立构造）。
async fn patch_with_if_match(
    app: &Router,
    uri: &str,
    session: &str,
    csrf: &str,
    if_match: Option<&str>,
    body: Value,
) -> (StatusCode, Value) {
    let mut builder = Request::builder()
        .method("PATCH")
        .uri(uri)
        .header("content-type", "application/json")
        .header("x-csrf-token", csrf)
        .header("cookie", session);
    if let Some(v) = if_match {
        builder = builder.header("if-match", v);
    }
    let resp = app
        .clone()
        .oneshot(builder.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

#[tokio::test]
async fn admin_trust_level_rule_patch_validation_and_audit() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let member = insert_user(&pool, "mem").await;
    let member_session = common::direct_session_cookie(&pool, &member).await;
    let member_csrf = session_csrf(&app, &member_session).await;

    // member 403（无 level.manage）。
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &member_session,
        &member_csrf,
        Some("1"),
        json!({ "name": "x", "is_enabled": true, "requirements": {}, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);

    // 缺 If-Match → 400。
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        None,
        json!({ "name": "x", "is_enabled": true, "requirements": {}, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);

    // 未知字段 / 未知阈值键 / 越界等级 → 400。
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({ "name": "x", "is_enabled": true, "requirements": {}, "reason": "r", "extra": 1 }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "未知字段必须 400");
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({ "name": "x", "is_enabled": true, "requirements": { "post_read": 3 }, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "未知阈值键必须 400");
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/0",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({ "name": "x", "is_enabled": true, "requirements": { "topics_entered": 1 }, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "TL0 不允许阈值条件");
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/4",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({ "name": "x", "is_enabled": true, "requirements": { "manual_only": false }, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "TL4 必须保持 manual_only");
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({ "name": "x", "is_enabled": true, "requirements": { "topics_entered": -1 }, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "负数阈值必须 400");

    // happy path：编辑 TL1（收紧阈值 + 改名）→ 200 + version 递增。
    let (status, body) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({
            "name": "基本用户（收紧）",
            "summary": "自定义摘要",
            "is_enabled": true,
            "requirements": { "topics_entered": 50, "posts_read": 30, "time_read_seconds": 600 },
            "reason": "收紧晋升门槛"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["item"]["version"], 2);
    assert_eq!(body["item"]["name"], "基本用户（收紧）");
    assert_eq!(body["item"]["requirements"]["topics_entered"], 50);

    // 旧版本再 PATCH → 409。
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({ "name": "again", "is_enabled": true, "requirements": {}, "reason": "r" }),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT);

    // 审计 + 列表反映新值。
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.trust_level_rules.update' AND target_id = '1'",
            "",
        ).await,
        1,
        "规则编辑必须写审计"
    );
    let (status, body) = authed(
        &app,
        "GET",
        "/api/v1/admin/trust-levels",
        &admin_session,
        &admin_csrf,
        Value::Null,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let item = body["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["level"] == 1)
        .unwrap();
    assert_eq!(item["name"], "基本用户（收紧）");
    assert_eq!(item["version"], 2);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_trust_level_rule_edit_affects_evaluation() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;
    let user = insert_user(&pool, "cfg").await;

    // 满足默认 TL1 条件（5 话题 / 30 楼层 / 600 秒）→ 评估晋升到 1。
    let now = now_millis();
    for _ in 0..5 {
        let post = insert_post(&pool, &user, now).await;
        store::record_topic_view(&pool, &user, &post).await.unwrap();
    }
    for _ in 0..30 {
        let post = insert_post(&pool, &user, now).await;
        let comment = insert_comment(&pool, &post, &user, now).await;
        store::record_comment_reads(&pool, &user, &post, &[comment])
            .await
            .unwrap();
    }
    // 单次心跳钳 60s：600 秒 = 10 × 60。
    for _ in 0..10 {
        store::add_read_time(&pool, &user, 60, &day(0))
            .await
            .unwrap();
    }
    let evaluation = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        evaluation.changed && evaluation.to_level == 1,
        "{evaluation:?}"
    );

    // 停用 TL1 → 重置回 0 再评估：停用级不参与晋升。
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({
            "name": "基本用户", "summary": Value::Null, "is_enabled": false,
            "requirements": { "topics_entered": 5, "posts_read": 30, "time_read_seconds": 600 },
            "reason": "停用 TL1"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    seed_level(&pool, &user, 0).await;
    let evaluation = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        !evaluation.changed && evaluation.to_level == 0,
        "{evaluation:?}"
    );

    // 启用但收紧到 100 话题 → 仍不晋升。
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("2"),
        json!({
            "name": "基本用户", "summary": Value::Null, "is_enabled": true,
            "requirements": { "topics_entered": 100, "posts_read": 30, "time_read_seconds": 600 },
            "reason": "收紧"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    seed_level(&pool, &user, 0).await;
    let evaluation = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        !evaluation.changed && evaluation.to_level == 0,
        "{evaluation:?}"
    );

    // 自定义条件调回 5 话题 → 晋升恢复（配置驱动引擎双向生效）。
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("3"),
        json!({
            "name": "基本用户", "summary": Value::Null, "is_enabled": true,
            "requirements": { "topics_entered": 5, "posts_read": 30, "time_read_seconds": 600 },
            "reason": "回调"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    seed_level(&pool, &user, 0).await;
    let evaluation = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        evaluation.changed && evaluation.to_level == 1,
        "{evaluation:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn admin_trust_level_rule_reset_and_disabled_tl3_no_demotion() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let (_admin_id, admin_session, admin_csrf) = admin_ctx(&app, &pool).await;

    // 编辑 TL1 后 reset：恢复内置默认（名称/阈值回种子值，version 递增）。
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/1",
        &admin_session,
        &admin_csrf,
        Some("1"),
        json!({
            "name": "基本用户改", "summary": "改", "is_enabled": true,
            "requirements": { "topics_entered": 99, "posts_read": 99, "time_read_seconds": 99 },
            "reason": "自定义"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let (status, body) = authed(
        &app,
        "POST",
        "/api/v1/admin/trust-levels/1/reset",
        &admin_session,
        &admin_csrf,
        json!({ "reason": "恢复默认" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["item"]["name"], "基本用户");
    assert_eq!(body["item"]["requirements"]["topics_entered"], 5);
    assert_eq!(body["item"]["is_enabled"], true);
    assert_eq!(body["item"]["version"], 3);
    assert_eq!(
        count(
            &pool,
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'admin.trust_level_rules.reset' AND target_id = '1'",
            "",
        ).await,
        1,
        "reset 必须写审计"
    );

    // TL3 停用 → 到达 TL3 超过宽限期的用户不再被自动降级。
    let user = insert_user(&pool, "t3").await;
    seed_level(&pool, &user, 3).await;
    seed_exec!(
        &pool,
        "UPDATE trust_level_events SET created_at = ? WHERE user_id = ? AND to_level = 3",
        now_millis() - 30 * 86_400_000,
        user,
    );
    // 先验证启用态：无窗口统计 → 目标 < 3 → 降级回 2。
    let evaluation = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        evaluation.changed && evaluation.to_level == 2,
        "{evaluation:?}"
    );

    // 停用 TL3 后重试：脱离自动管理，保持 3 级。
    let rows: Vec<(i64, i64)> = match &pool {
        Either::Left(p) => {
            sqlx::query_as("SELECT level, version FROM trust_level_rules WHERE level = 3")
                .fetch_all(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    let tl3_version = rows[0].1;
    seed_level(&pool, &user, 3).await;
    seed_exec!(
        &pool,
        "UPDATE trust_level_events SET created_at = ? WHERE user_id = ? AND to_level = 3",
        now_millis() - 30 * 86_400_000,
        user,
    );
    let (status, _) = patch_with_if_match(
        &app,
        "/api/v1/admin/trust-levels/3",
        &admin_session,
        &admin_csrf,
        Some(&tl3_version.to_string()),
        json!({
            "name": "活跃用户", "summary": Value::Null, "is_enabled": false,
            "requirements": { "window_days": 100, "visit_ratio": 0.5, "replied_topics_window": 10,
                "viewed_ratio": 0.25, "viewed_cap": 500, "read_ratio": 0.25, "read_cap": 20000,
                "likes_received_window": 20, "likes_given_window": 30,
                "like_distinct_user_divisor": 5, "like_distinct_day_divisor": 4,
                "max_flags": 5, "no_sanction_months": 6 },
            "reason": "停用 TL3 自动考核"
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let evaluation = service::evaluate_user(&pool, &user).await.unwrap();
    assert!(
        !evaluation.changed && evaluation.to_level == 3,
        "{evaluation:?}"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
