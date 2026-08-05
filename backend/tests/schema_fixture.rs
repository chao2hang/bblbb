//! M03-SCHEMA-07：三数据库 schema 等价 Fixture——同一套数据操作在
//! SQLite/MySQL/MariaDB 上断言行为一致：
//! 1. 唯一性：tag_groups.slug、tags.slug（非空）、board_roles 复合主键、
//!    board_role_assignments UNIQUE(board,user,role)、role_permissions 复合
//!    主键、user_roles 复合主键、post_tags 复合主键——三库统一拒绝重复；
//! 2. 过期 assignment：expires_at 可空=永久，过期行按未生效过滤但仍保留
//!    （软失效而非物理删除），活跃谓词三库一致；
//! 3. 非法状态：boards.visibility/posting_mode、permissions.risk_level、
//!    user_privacy.email_visible_to、posts.status 的 CHECK 约束三库统一拒绝；
//! 4. 外键完整性：board_tags/role_permissions/board_role_assignments 引用
//!    不存在的行三库统一拒绝。
//!
//! - SQLite：本地始终运行（临时文件 + 迁移）；
//! - MySQL 8 / MariaDB 10.11：`BBLBB_TEST_MYSQL_URL` 环境变量 + `#[ignore]`
//!   （CI mysql-family 任务以 `cargo test --test schema_fixture -- --ignored`
//!   分别对两个数据库运行，见 .github/workflows/ci.yml）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
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

/// 标量 COUNT 查询（i64）。
macro_rules! scalar {
    ($pool:expr, $sql:expr, $($arg:expr),* $(,)?) => {{
        match $pool {
            Either::Left(p) => {
                scalar!(@chain sqlx::query_scalar::<_, i64>($sql), $($arg),*)
                    .fetch_one(p).await.unwrap()
            }
            Either::Right(p) => {
                scalar!(@chain sqlx::query_scalar::<_, i64>($sql), $($arg),*)
                    .fetch_one(p).await.unwrap()
            }
        }
    }};
    (@chain $q:expr,) => { $q };
    (@chain $q:expr, $a:expr $(, $rest:expr)*) => {
        scalar!(@chain $q.bind($a), $($rest),*)
    };
}

fn migrations_dir(engine: &str) -> PathBuf {
    let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
    Path::new(&manifest).join(format!("{MIGRATIONS_ROOT}/{engine}"))
}

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-sf-{}", uuid::Uuid::now_v7()));
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

// ─────────────────────────── 数据准备助手 ───────────────────────────

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, created_at, updated_at)
         VALUES (?, ?, ?, 'dummy', 'active', ?, ?)",
        &user_id,
        format!("{tag}_user"),
        format!("{tag}@example.com"),
        now,
        now
    );
    user_id
}

async fn insert_board(pool: &DatabasePool, slug: &str) -> String {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO boards (id, slug, name, created_at, updated_at) VALUES (?, ?, ?, ?, ?)",
        &board_id,
        slug,
        slug,
        now,
        now
    );
    board_id
}

async fn insert_role(pool: &DatabasePool, name: &str, is_system: bool) -> String {
    let role_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO roles (id, name, display_name, is_system, created_at, updated_at)
         VALUES (?, ?, ?, ?, ?, ?)",
        &role_id,
        name,
        name,
        is_system as i64,
        now,
        now
    );
    role_id
}

async fn insert_permission(pool: &DatabasePool, name: &str) -> String {
    let permission_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO permissions (id, name, created_at) VALUES (?, ?, ?)",
        &permission_id,
        name,
        now
    );
    permission_id
}

async fn insert_tag(pool: &DatabasePool, name: &str) -> String {
    let tag_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO tags (id, name, created_at) VALUES (?, ?, ?)",
        &tag_id,
        name,
        now
    );
    tag_id
}

async fn insert_tag_group(pool: &DatabasePool, name: &str, slug: &str) -> String {
    let group_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO tag_groups (id, name, slug, created_at) VALUES (?, ?, ?, ?)",
        &group_id,
        name,
        slug,
        now
    );
    group_id
}

async fn insert_post(pool: &DatabasePool, board_id: &str, author_id: &str, title: &str) -> String {
    let post_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO posts (id, board_id, author_id, title, content, created_at, updated_at)
         VALUES (?, ?, ?, ?, 'fixture body', ?, ?)",
        &post_id,
        board_id,
        author_id,
        title,
        now,
        now
    );
    post_id
}

// ─────────────────────────── 约束错误分类 ───────────────────────────

#[derive(Clone, Copy)]
enum ViolationKind {
    Unique,
    ForeignKey,
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
        ViolationKind::ForeignKey => {
            assert!(
                db.is_foreign_key_violation(),
                "{ctx}: 期望外键违例，实际 {err}"
            );
        }
        // MySQL 8 用 3819、MariaDB 用 4025；SQLite 用 SQLITE_CONSTRAINT_CHECK。
        // is_check_violation() 覆盖 SQLite/MySQL 8；MariaDB 的 4025 需按 code 判断。
        ViolationKind::Check => {
            let is_mariadb_check = db.code().as_deref() == Some("4025");
            assert!(
                db.is_check_violation() || is_mariadb_check,
                "{ctx}: 期望 CHECK 违例，实际 {err}"
            );
        }
    }
}

// ─────────────────────────── 共享行为流 ───────────────────────────

/// 三数据库 schema 等价 Fixture（M03-SCHEMA-07）。
async fn schema_fixture_flow(pool: &DatabasePool) {
    let now = now_millis();

    // ── 1. 数据准备 ──
    let user_id = insert_user(pool, "fix").await;
    let board_a = insert_board(pool, "fixture-a").await;
    let board_b = insert_board(pool, "fixture-b").await;
    let role_a = insert_role(pool, "fixture_role_a", false).await;
    let role_b = insert_role(pool, "fixture_role_b", false).await;
    let perm_a = insert_permission(pool, "fixture.perm_a").await;
    let tag_a = insert_tag(pool, "fixture_tag_a").await;
    let tag_b = insert_tag(pool, "fixture_tag_b").await;
    let _group_a = insert_tag_group(pool, "fixture_group", "fixture-group").await;
    let post_a = insert_post(pool, &board_a, &user_id, "fixture post").await;

    // ── 2. 唯一性（三库统一拒绝重复） ──

    // tag_groups.slug 全局唯一
    let err = expect_err!(
        pool,
        "INSERT INTO tag_groups (id, name, slug, created_at) VALUES (?, ?, 'fixture-group', ?)",
        uuid::Uuid::now_v7().to_string(),
        "fixture_group_dup",
        now
    );
    assert_violation(&err, ViolationKind::Unique, "tag_groups.slug 唯一性");

    // tags.slug 非空时全局唯一
    exec!(
        pool,
        "UPDATE tags SET slug = 'fixture-tag' WHERE id = ?",
        &tag_a
    );
    let err = expect_err!(
        pool,
        "UPDATE tags SET slug = 'fixture-tag' WHERE id = ?",
        &tag_b
    );
    assert_violation(&err, ViolationKind::Unique, "tags.slug 非空唯一性");

    // board_roles 复合主键 (board_id, role_id)
    exec!(
        pool,
        "INSERT INTO board_roles (board_id, role_id, granted_at) VALUES (?, ?, ?)",
        &board_a,
        &role_a,
        now
    );
    let err = expect_err!(
        pool,
        "INSERT INTO board_roles (board_id, role_id, granted_at) VALUES (?, ?, ?)",
        &board_a,
        &role_a,
        now
    );
    assert_violation(&err, ViolationKind::Unique, "board_roles 复合主键");

    // board_role_assignments UNIQUE(board_id, user_id, role_id)
    exec!(
        pool,
        "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_at)
         VALUES (?, ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &board_a,
        &user_id,
        &role_a,
        now
    );
    let err = expect_err!(
        pool,
        "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_at)
         VALUES (?, ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &board_a,
        &user_id,
        &role_a,
        now
    );
    assert_violation(
        &err,
        ViolationKind::Unique,
        "board_role_assignments UNIQUE(board,user,role)",
    );

    // role_permissions 复合主键
    exec!(
        pool,
        "INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
        &role_a,
        &perm_a
    );
    let err = expect_err!(
        pool,
        "INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
        &role_a,
        &perm_a
    );
    assert_violation(&err, ViolationKind::Unique, "role_permissions 复合主键");

    // user_roles 复合主键
    exec!(
        pool,
        "INSERT INTO user_roles (user_id, role_id, granted_at) VALUES (?, ?, ?)",
        &user_id,
        &role_a,
        now
    );
    let err = expect_err!(
        pool,
        "INSERT INTO user_roles (user_id, role_id, granted_at) VALUES (?, ?, ?)",
        &user_id,
        &role_a,
        now
    );
    assert_violation(&err, ViolationKind::Unique, "user_roles 复合主键");

    // post_tags 复合主键
    exec!(
        pool,
        "INSERT INTO post_tags (post_id, tag_id) VALUES (?, ?)",
        &post_a,
        &tag_a
    );
    let err = expect_err!(
        pool,
        "INSERT INTO post_tags (post_id, tag_id) VALUES (?, ?)",
        &post_a,
        &tag_a
    );
    assert_violation(&err, ViolationKind::Unique, "post_tags 复合主键");

    // ── 3. 过期 assignment：expires_at 过去=未生效但保留；NULL=永久 ──
    // role_a 的 assignment 已在上一步插入（expires_at NULL=永久）；
    // 再给 role_b 一条过期 assignment。
    exec!(
        pool,
        "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_at, expires_at)
         VALUES (?, ?, ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &board_a,
        &user_id,
        &role_b,
        now,
        now - 3_600_000
    );

    // 活跃谓词（M03-AUTHZ-03）：expires_at IS NULL 或未过期
    let active = scalar!(
        pool,
        "SELECT COUNT(*) FROM board_role_assignments
         WHERE board_id = ? AND user_id = ? AND (expires_at IS NULL OR expires_at > ?)",
        &board_a,
        &user_id,
        now
    );
    assert_eq!(
        active, 1,
        "过期 assignment 必须按未生效过滤，仅保留永久 assignment"
    );

    // 过期行仍保留（软失效而非物理删除，供审计/恢复）
    let expired = scalar!(
        pool,
        "SELECT COUNT(*) FROM board_role_assignments
         WHERE board_id = ? AND user_id = ? AND expires_at IS NOT NULL AND expires_at <= ?",
        &board_a,
        &user_id,
        now
    );
    assert_eq!(expired, 1, "过期 assignment 行必须保留（软失效）");

    // ── 4. 非法状态（CHECK 约束三库统一拒绝） ──
    let err = expect_err!(
        pool,
        "UPDATE boards SET visibility = 'top-secret' WHERE id = ?",
        &board_a
    );
    assert_violation(&err, ViolationKind::Check, "boards.visibility CHECK");

    let err = expect_err!(
        pool,
        "UPDATE boards SET posting_mode = 'n/a' WHERE id = ?",
        &board_a
    );
    assert_violation(&err, ViolationKind::Check, "boards.posting_mode CHECK");

    let err = expect_err!(
        pool,
        "UPDATE permissions SET risk_level = 'admin' WHERE id = ?",
        &perm_a
    );
    assert_violation(&err, ViolationKind::Check, "permissions.risk_level CHECK");

    // user_privacy 行首访惰性创建：先插入，再写非法值
    exec!(
        pool,
        "INSERT INTO user_privacy (user_id, updated_at) VALUES (?, ?)",
        &user_id,
        now
    );
    let err = expect_err!(
        pool,
        "UPDATE user_privacy SET email_visible_to = 'enemies' WHERE user_id = ?",
        &user_id
    );
    assert_violation(
        &err,
        ViolationKind::Check,
        "user_privacy.email_visible_to CHECK",
    );

    let err = expect_err!(
        pool,
        "UPDATE posts SET status = 'silly' WHERE id = ?",
        &post_a
    );
    assert_violation(&err, ViolationKind::Check, "posts.status CHECK");

    // ── 5. 外键完整性（三库统一拒绝悬空引用） ──
    let ghost = uuid::Uuid::now_v7().to_string();

    let err = expect_err!(
        pool,
        "INSERT INTO board_tags (board_id, tag_id) VALUES (?, ?)",
        &ghost,
        &tag_a
    );
    assert_violation(&err, ViolationKind::ForeignKey, "board_tags.board_id FK");

    let err = expect_err!(
        pool,
        "INSERT INTO role_permissions (role_id, permission_id) VALUES (?, ?)",
        &role_a,
        &ghost
    );
    assert_violation(
        &err,
        ViolationKind::ForeignKey,
        "role_permissions.permission_id FK",
    );

    let err = expect_err!(
        pool,
        "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_at)
         VALUES (?, ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &board_a,
        &ghost,
        &role_a,
        now
    );
    assert_violation(
        &err,
        ViolationKind::ForeignKey,
        "board_role_assignments.user_id FK",
    );

    // board_b 留作交叉断言：干净板块无任何关联（未被前面的唯一性/FK 测试污染）
    let left = scalar!(
        pool,
        "SELECT COUNT(*) FROM board_tags WHERE board_id = ?",
        &board_b
    );
    assert_eq!(left, 0, "board_b 不应产生任何 board_tags 关联");
}

// ─────────────────────────── 三数据库入口 ───────────────────────────

#[tokio::test]
async fn sqlite_schema_fixture() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    schema_fixture_flow(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_schema_fixture() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mysql")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    schema_fixture_flow(&pool).await;
    close_pool(&pool).await;
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mariadb_schema_fixture() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mariadb")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    schema_fixture_flow(&pool).await;
    close_pool(&pool).await;
}
