//! M05-SCHEMA-07：moderation 数据约束测试。
//!
//! SQLite 本地全量跑通；MySQL/MariaDB 以 `BBLBB_TEST_MYSQL_URL` +
//! `#[ignore]`（CI mysql-family 任务以 `--ignored` 分别运行）验证三库
//! 等价，模式同 `schema_fixture.rs`。
//!
//! 断言内容：
//! 1. 非法状态/原因码/目标类型被 CHECK 拒绝（reports、moderation_cases、
//!    moderation_actions、sanctions、appeals、appeal_decisions）；
//! 2. 举报去重窗口：同 (reporter, target, reason) 同窗口内唯一，
//!    下一窗口允许重新举报（模型层 + DB UNIQUE 双重覆盖）；
//! 3. 动作修订只追加：revision 严格递增、历史不可覆盖；
//! 4. 处罚到期边界与板块范围/期限/撤销一致性；
//! 5. 处罚撤销只追加：每处罚至多一条 reversal；
//! 6. 申诉每处罚至多一条 + 利益冲突 reviewer 校验。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::moderation::model::*;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

const MIGRATIONS_ROOT: &str = "../migrations";
const WINDOW: i64 = REPORT_DEDUP_WINDOW_MS;

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
    let dir = std::env::temp_dir().join(format!("bblbb-mod-{}", uuid::Uuid::now_v7()));
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
        ViolationKind::Check => {
            let is_mariadb_check = db.code().as_deref() == Some("4025");
            assert!(
                db.is_check_violation() || is_mariadb_check,
                "{ctx}: 期望 CHECK 违例，实际 {err}"
            );
        }
    }
}

// ─────────────────────────── 数据准备助手 ───────────────────────────

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

async fn insert_post(pool: &DatabasePool, board_id: &str, author_id: &str) -> String {
    let post_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO posts (id, board_id, author_id, title, content, created_at, updated_at)
         VALUES (?, ?, ?, 'fixture post', 'fixture body', ?, ?)",
        &post_id,
        board_id,
        author_id,
        now,
        now
    );
    post_id
}

async fn insert_case(pool: &DatabasePool, created_by: &str) -> String {
    let case_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    exec!(
        pool,
        "INSERT INTO moderation_cases (id, title, status, priority, created_by, created_at, updated_at)
         VALUES (?, ?, 'open', 'normal', ?, ?, ?)",
        &case_id,
        "fixture case",
        created_by,
        now,
        now
    );
    case_id
}

// ─────────────────────────── 纯模型规则 ───────────────────────────

#[test]
fn report_status_transition_rules() {
    use ReportStatus as S;
    // 合法迁移（STATE-MACHINES.md §3）
    assert!(S::Open.can_transition_to(S::Triaged));
    assert!(S::Open.can_transition_to(S::Investigating));
    assert!(S::Open.can_transition_to(S::Rejected));
    assert!(S::Triaged.can_transition_to(S::Investigating));
    assert!(S::Triaged.can_transition_to(S::Resolved));
    assert!(S::Investigating.can_transition_to(S::Resolved));
    assert!(S::Investigating.can_transition_to(S::Rejected));
    assert!(S::Resolved.can_transition_to(S::Reopened));
    assert!(S::Rejected.can_transition_to(S::Reopened));
    assert!(S::Reopened.can_transition_to(S::Triaged));
    // 非法迁移
    assert!(!S::Open.can_transition_to(S::Open));
    assert!(!S::Resolved.can_transition_to(S::Open));
    assert!(!S::Resolved.can_transition_to(S::Rejected));
    assert!(!S::Rejected.can_transition_to(S::Resolved));
    assert!(!S::Withdrawn.can_transition_to(S::Reopened));
    // 字面值往返
    for v in S::ALL {
        assert_eq!(S::parse(v.as_str()), Some(v));
    }
    assert_eq!(S::parse("bogus"), None);
}

#[test]
fn case_status_transition_rules() {
    use CaseStatus as S;
    assert!(S::Open.can_transition_to(S::Triaged));
    assert!(S::Open.can_transition_to(S::Investigating));
    assert!(S::Triaged.can_transition_to(S::Resolved));
    assert!(S::Investigating.can_transition_to(S::Rejected));
    assert!(S::Resolved.can_transition_to(S::Reopened));
    assert!(S::Rejected.can_transition_to(S::Reopened));
    assert!(!S::Open.can_transition_to(S::Reopened));
    assert!(!S::Resolved.can_transition_to(S::Rejected));
    for v in S::ALL {
        assert_eq!(S::parse(v.as_str()), Some(v));
    }
    assert_eq!(S::parse("withdrawn"), None, "案件不含 withdrawn");
}

#[test]
fn appeal_status_transition_rules() {
    use AppealStatus as S;
    assert!(S::Submitted.can_transition_to(S::Reviewing));
    assert!(S::Submitted.can_transition_to(S::Withdrawn));
    assert!(S::Reviewing.can_transition_to(S::Upheld));
    assert!(S::Reviewing.can_transition_to(S::PartiallyUpheld));
    assert!(S::Reviewing.can_transition_to(S::Rejected));
    assert!(S::Reviewing.can_transition_to(S::Withdrawn));
    assert!(!S::Upheld.can_transition_to(S::Reviewing));
    assert!(!S::Rejected.can_transition_to(S::Submitted));
    assert!(!S::Submitted.can_transition_to(S::Upheld));
    for v in S::ALL {
        assert_eq!(S::parse(v.as_str()), Some(v));
    }
}

#[test]
fn report_dedup_key_and_window_helpers() {
    let reporter = "u-reporter";
    let target = "post-123";
    let key = Report::build_dedup_key(
        reporter,
        ReportTargetType::Post,
        target,
        ReportReasonCode::Spam,
    );
    assert_eq!(key, "u-reporter|post|post-123|spam");

    // 同一组合归一化为同一键；不同 reason 产生不同键
    let other = Report::build_dedup_key(
        reporter,
        ReportTargetType::Post,
        target,
        ReportReasonCode::Nsfw,
    );
    assert_ne!(key, other);

    // 锚定窗口：同一窗口内 created_at 共享同一 dedup_until
    let now = now_millis();
    let w1 = Report::dedup_window_end(now, WINDOW);
    let w2 = Report::dedup_window_end(now + 1000, WINDOW);
    assert_eq!(w1, w2, "同一锚定窗口内 dedup_until 一致");
    assert!(w1 > now, "窗口终点在未来");
    assert!(Report::is_within_dedup_window(w1, now), "窗口内仍去重");
    assert!(
        !Report::is_within_dedup_window(w1, w1),
        "窗口终点半开：到期即允许重报"
    );

    // 下一窗口允许重新举报
    let w3 = Report::dedup_window_end(now + WINDOW + 1, WINDOW);
    assert!(w3 > w1);
}

#[test]
fn sanction_model_rules() {
    // 板块范围
    assert!(Sanction::validate_board_scope(SanctionKind::BoardMute, Some("board-1")).is_ok());
    assert!(Sanction::validate_board_scope(SanctionKind::BoardMute, None).is_err());
    assert!(Sanction::validate_board_scope(SanctionKind::Mute, None).is_ok());
    assert!(Sanction::validate_board_scope(SanctionKind::Mute, Some("board-1")).is_err());
    assert!(Sanction::validate_board_scope(SanctionKind::Ban, None).is_ok());

    // 期限
    assert!(Sanction::validate_timeline(100, Some(200)).is_ok());
    assert!(Sanction::validate_timeline(100, None).is_ok());
    assert!(Sanction::validate_timeline(200, Some(200)).is_err());
    assert!(Sanction::validate_timeline(200, Some(100)).is_err());

    // 撤销一致性
    assert!(Sanction::validate_revoked(SanctionStatus::Revoked, Some(1), Some("u")).is_ok());
    assert!(Sanction::validate_revoked(SanctionStatus::Revoked, None, Some("u")).is_err());
    assert!(Sanction::validate_revoked(SanctionStatus::Revoked, Some(1), None).is_err());
    assert!(Sanction::validate_revoked(SanctionStatus::Active, None, None).is_ok());

    // 到期边界（ends_at 半开：now == ends_at 即到期）
    let s = Sanction {
        id: "s1".into(),
        user_id: "u".into(),
        board_id: None,
        kind: SanctionKind::Mute,
        status: SanctionStatus::Active,
        reason: None,
        starts_at: 100,
        ends_at: Some(200),
        created_by: "u".into(),
        created_at: 100,
        revoked_at: None,
        revoked_by: None,
        revoke_reason: None,
    };
    assert!(s.is_active_at(100), "starts_at 边界含端点");
    assert!(s.is_active_at(199));
    assert!(!s.is_active_at(200), "ends_at 边界半开：恰等于即到期");
    assert!(s.is_expired_at(200));
    assert!(!s.is_expired_at(199));

    // 永久处罚永不到期
    let permanent = Sanction {
        ends_at: None,
        ..s.clone()
    };
    assert!(permanent.is_active_at(10_000));
    assert!(!permanent.is_expired_at(10_000));

    // status 非 active 即使时间在窗口内也不算生效
    let revoked = Sanction {
        status: SanctionStatus::Revoked,
        ..s.clone()
    };
    assert!(!revoked.is_active_at(150));
    // scheduled 同理
    let scheduled = Sanction {
        status: SanctionStatus::Scheduled,
        ..s
    };
    assert!(!scheduled.is_active_at(150));
}

#[test]
fn revision_append_only_rule() {
    assert!(ModerationActionRevision::validate_revision(0, 1).is_ok());
    assert!(ModerationActionRevision::validate_revision(3, 4).is_ok());
    assert!(
        ModerationActionRevision::validate_revision(1, 1).is_err(),
        "同号修订拒绝"
    );
    assert!(
        ModerationActionRevision::validate_revision(2, 1).is_err(),
        "回退修订拒绝"
    );
}

#[test]
fn appeal_reviewer_conflict_rule() {
    assert!(AppealDecision::validate_reviewer("appellant", "reviewer", None).is_ok());
    assert!(
        AppealDecision::validate_reviewer("appellant", "appellant", None).is_err(),
        "审查者不得是申诉人本人"
    );
    assert!(
        AppealDecision::validate_reviewer("appellant", "reviewer", Some("我是处罚签发人")).is_ok()
    );
    assert!(
        AppealDecision::validate_reviewer("appellant", "reviewer", Some("  ")).is_err(),
        "声明冲突必须填写理由"
    );
}

// ─────────────────────────── 数据库约束流 ───────────────────────────

/// 三数据库等价约束流（M05-SCHEMA-07 的 DB 断言）。
async fn moderation_schema_flow(pool: &DatabasePool) {
    let now = now_millis();
    let reporter = insert_user(pool, "rep").await;
    let offender = insert_user(pool, "off").await;
    let moderator = insert_user(pool, "mod").await;
    let board = insert_board(pool, "mod-board").await;
    let post = insert_post(pool, &board, &offender).await;
    let case_id = insert_case(pool, &moderator).await;

    // ── 举报：非法 status / reason_code / target_type 拒绝 ──
    let dedup_key = Report::build_dedup_key(
        &reporter,
        ReportTargetType::Post,
        &post,
        ReportReasonCode::Spam,
    );
    let window_end = now + WINDOW;

    let err = expect_err!(
        pool,
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason_code, status, report_dedup_key, dedup_until, created_at, updated_at)
         VALUES (?, ?, 'post', ?, 'spam', 'bogus', ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &reporter,
        &post,
        &dedup_key,
        window_end,
        now,
        now
    );
    assert_violation(&err, ViolationKind::Check, "reports.status CHECK");

    let err = expect_err!(
        pool,
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason_code, status, report_dedup_key, dedup_until, created_at, updated_at)
         VALUES (?, ?, 'post', ?, 'bogus', 'open', ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &reporter,
        &post,
        &dedup_key,
        window_end,
        now,
        now
    );
    assert_violation(&err, ViolationKind::Check, "reports.reason_code CHECK");

    let err = expect_err!(
        pool,
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason_code, status, report_dedup_key, dedup_until, created_at, updated_at)
         VALUES (?, ?, 'bogus', ?, 'spam', 'open', ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &reporter,
        &post,
        &dedup_key,
        window_end,
        now,
        now
    );
    assert_violation(&err, ViolationKind::Check, "reports.target_type CHECK");

    // 不存在的举报者 → FK 拒绝
    let ghost = uuid::Uuid::now_v7().to_string();
    let err = expect_err!(
        pool,
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason_code, status, report_dedup_key, dedup_until, created_at, updated_at)
         VALUES (?, ?, 'post', ?, 'spam', 'open', ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &ghost,
        &post,
        &dedup_key,
        window_end,
        now,
        now
    );
    assert_violation(&err, ViolationKind::ForeignKey, "reports.reporter_id FK");

    // ── 举报去重窗口：同窗口同键唯一，下一窗口允许 ──
    let report_id = uuid::Uuid::now_v7().to_string();
    exec!(
        pool,
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason_code, status, report_dedup_key, dedup_until, created_at, updated_at)
         VALUES (?, ?, 'post', ?, 'spam', 'open', ?, ?, ?, ?)",
        &report_id,
        &reporter,
        &post,
        &dedup_key,
        window_end,
        now,
        now
    );
    let err = expect_err!(
        pool,
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason_code, status, report_dedup_key, dedup_until, created_at, updated_at)
         VALUES (?, ?, 'post', ?, 'spam', 'open', ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &reporter,
        &post,
        &dedup_key,
        window_end,
        now,
        now
    );
    assert_violation(&err, ViolationKind::Unique, "reports 同窗口重复举报唯一性");

    // 下一窗口（不同 dedup_until）允许重新举报
    let next_window = window_end + WINDOW;
    exec!(
        pool,
        "INSERT INTO reports (id, reporter_id, target_type, target_id, reason_code, status, report_dedup_key, dedup_until, created_at, updated_at)
         VALUES (?, ?, 'post', ?, 'spam', 'open', ?, ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &reporter,
        &post,
        &dedup_key,
        next_window,
        now + WINDOW + 1,
        now + WINDOW + 1
    );

    // ── 案件：状态/优先级 CHECK、case_reports 关联 ──
    let err = expect_err!(
        pool,
        "INSERT INTO moderation_cases (id, title, status, priority, created_by, created_at, updated_at)
         VALUES (?, 'bad case', 'bogus', 'normal', ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &moderator,
        now,
        now
    );
    assert_violation(&err, ViolationKind::Check, "moderation_cases.status CHECK");

    let err = expect_err!(
        pool,
        "INSERT INTO moderation_cases (id, title, status, priority, created_by, created_at, updated_at)
         VALUES (?, 'bad case', 'open', 'bogus', ?, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &moderator,
        now,
        now
    );
    assert_violation(
        &err,
        ViolationKind::Check,
        "moderation_cases.priority CHECK",
    );

    exec!(
        pool,
        "INSERT INTO case_reports (case_id, report_id, added_by, added_at) VALUES (?, ?, ?, ?)",
        &case_id,
        &report_id,
        &moderator,
        now
    );

    // case_assignments 只追加（释放记 released_at）
    exec!(
        pool,
        "INSERT INTO case_assignments (id, case_id, assignee_id, assigned_by, assigned_at, released_at, note)
         VALUES (?, ?, ?, ?, ?, NULL, NULL)",
        uuid::Uuid::now_v7().to_string(),
        &case_id,
        &moderator,
        &moderator,
        now
    );

    // moderation_notes 内部备注
    exec!(
        pool,
        "INSERT INTO moderation_notes (id, case_id, author_id, body, created_at, updated_at)
         VALUES (?, ?, ?, 'internal note body', ?, NULL)",
        uuid::Uuid::now_v7().to_string(),
        &case_id,
        &moderator,
        now
    );

    // ── 审核动作：action/target_type CHECK、修订只追加 ──
    let err = expect_err!(
        pool,
        "INSERT INTO moderation_actions (id, case_id, actor_id, action, target_type, target_id, reason, metadata_json, created_at)
         VALUES (?, ?, ?, 'bogus', NULL, NULL, NULL, NULL, ?)",
        uuid::Uuid::now_v7().to_string(),
        &case_id,
        &moderator,
        now
    );
    assert_violation(
        &err,
        ViolationKind::Check,
        "moderation_actions.action CHECK",
    );

    let err = expect_err!(
        pool,
        "INSERT INTO moderation_actions (id, case_id, actor_id, action, target_type, target_id, reason, metadata_json, created_at)
         VALUES (?, ?, ?, 'resolve', 'bogus', NULL, NULL, NULL, ?)",
        uuid::Uuid::now_v7().to_string(),
        &case_id,
        &moderator,
        now
    );
    assert_violation(
        &err,
        ViolationKind::Check,
        "moderation_actions.target_type CHECK",
    );

    let action_id = uuid::Uuid::now_v7().to_string();
    exec!(
        pool,
        "INSERT INTO moderation_actions (id, case_id, actor_id, action, target_type, target_id, reason, metadata_json, created_at)
         VALUES (?, ?, ?, 'hide_content', 'post', ?, 'fixture reason', '{\"k\":\"v\"}', ?)",
        &action_id,
        &case_id,
        &moderator,
        &post,
        now
    );

    // 修订只追加：rev1 成功、同号唯一拒绝、rev2 成功
    let snapshot = r#"{"action":"hide_content","reason":"fixture reason"}"#;
    exec!(
        pool,
        "INSERT INTO moderation_action_revisions (id, action_id, revision, snapshot_json, change_reason, created_by, created_at)
         VALUES (?, ?, 1, ?, NULL, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &action_id,
        snapshot,
        &moderator,
        now
    );
    let err = expect_err!(
        pool,
        "INSERT INTO moderation_action_revisions (id, action_id, revision, snapshot_json, change_reason, created_by, created_at)
         VALUES (?, ?, 1, ?, NULL, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &action_id,
        snapshot,
        &moderator,
        now
    );
    assert_violation(
        &err,
        ViolationKind::Unique,
        "revision (action_id, revision) 唯一",
    );
    exec!(
        pool,
        "INSERT INTO moderation_action_revisions (id, action_id, revision, snapshot_json, change_reason, created_by, created_at)
         VALUES (?, ?, 2, ?, 'correction', ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &action_id,
        snapshot,
        &moderator,
        now
    );

    // ── 处罚：板块范围/期限/撤销一致性 CHECK ──
    let err = expect_err!(
        pool,
        "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at, revoked_at, revoked_by, revoke_reason)
         VALUES (?, ?, NULL, 'board_mute', 'active', NULL, ?, NULL, ?, ?, NULL, NULL, NULL)",
        uuid::Uuid::now_v7().to_string(),
        &offender,
        now - 1000,
        &moderator,
        now
    );
    assert_violation(&err, ViolationKind::Check, "board_mute 必须带 board_id");

    let err = expect_err!(
        pool,
        "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at, revoked_at, revoked_by, revoke_reason)
         VALUES (?, ?, ?, 'mute', 'active', NULL, ?, NULL, ?, ?, NULL, NULL, NULL)",
        uuid::Uuid::now_v7().to_string(),
        &offender,
        &board,
        now - 1000,
        &moderator,
        now
    );
    assert_violation(&err, ViolationKind::Check, "非 board_mute 拒绝板块范围");

    let err = expect_err!(
        pool,
        "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at, revoked_at, revoked_by, revoke_reason)
         VALUES (?, ?, NULL, 'mute', 'active', NULL, 100, 100, ?, ?, NULL, NULL, NULL)",
        uuid::Uuid::now_v7().to_string(),
        &offender,
        &moderator,
        now
    );
    assert_violation(&err, ViolationKind::Check, "ends_at 必须晚于 starts_at");

    let err = expect_err!(
        pool,
        "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at, revoked_at, revoked_by, revoke_reason)
         VALUES (?, ?, NULL, 'mute', 'revoked', NULL, ?, NULL, ?, ?, NULL, NULL, NULL)",
        uuid::Uuid::now_v7().to_string(),
        &offender,
        now - 1000,
        &moderator,
        now
    );
    assert_violation(
        &err,
        ViolationKind::Check,
        "revoked 必须带 revoked_at/revoked_by",
    );

    // 合法：board_mute 带板块；ban 全局无板块；mute 有时限
    let board_mute_id = uuid::Uuid::now_v7().to_string();
    exec!(
        pool,
        "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at, revoked_at, revoked_by, revoke_reason)
         VALUES (?, ?, ?, 'board_mute', 'active', NULL, ?, ?, ?, ?, NULL, NULL, NULL)",
        &board_mute_id,
        &offender,
        &board,
        now - 1000,
        now + 3_600_000,
        &moderator,
        now
    );
    let ban_id = uuid::Uuid::now_v7().to_string();
    exec!(
        pool,
        "INSERT INTO sanctions (id, user_id, board_id, kind, status, reason, starts_at, ends_at, created_by, created_at, revoked_at, revoked_by, revoke_reason)
         VALUES (?, ?, NULL, 'ban', 'active', '永久封禁', ?, NULL, ?, ?, NULL, NULL, NULL)",
        &ban_id,
        &offender,
        now - 1000,
        &moderator,
        now
    );

    // ── 处罚撤销只追加：每处罚至多一条 reversal ──
    exec!(
        pool,
        "INSERT INTO sanction_reversals (id, sanction_id, reversed_by, reason, reversed_at) VALUES (?, ?, ?, '误判', ?)",
        uuid::Uuid::now_v7().to_string(),
        &ban_id,
        &moderator,
        now
    );
    let err = expect_err!(
        pool,
        "INSERT INTO sanction_reversals (id, sanction_id, reversed_by, reason, reversed_at) VALUES (?, ?, ?, '再次撤销', ?)",
        uuid::Uuid::now_v7().to_string(),
        &ban_id,
        &moderator,
        now
    );
    assert_violation(&err, ViolationKind::Unique, "每处罚至多一条 reversal");

    // ── 申诉：状态 CHECK、每处罚至多一条、利益冲突字段 ──
    let err = expect_err!(
        pool,
        "INSERT INTO appeals (id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at)
         VALUES (?, ?, ?, '申诉正文', 'bogus', NULL, NULL, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &ban_id,
        &offender,
        now,
        now
    );
    assert_violation(&err, ViolationKind::Check, "appeals.status CHECK");

    let appeal_id = uuid::Uuid::now_v7().to_string();
    exec!(
        pool,
        "INSERT INTO appeals (id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at)
         VALUES (?, ?, ?, '申诉正文', 'submitted', NULL, NULL, ?, ?)",
        &appeal_id,
        &ban_id,
        &offender,
        now,
        now
    );
    let err = expect_err!(
        pool,
        "INSERT INTO appeals (id, sanction_id, user_id, message, status, reviewed_by, decided_at, submitted_at, updated_at)
         VALUES (?, ?, ?, '二次申诉', 'submitted', NULL, NULL, ?, ?)",
        uuid::Uuid::now_v7().to_string(),
        &ban_id,
        &offender,
        now,
        now
    );
    assert_violation(&err, ViolationKind::Unique, "每处罚至多一条申诉");

    // 决定记录：decision CHECK + 利益冲突声明字段可写
    let err = expect_err!(
        pool,
        "INSERT INTO appeal_decisions (id, appeal_id, reviewer_id, decision, decision_note, conflict_of_interest, created_at)
         VALUES (?, ?, ?, 'bogus', NULL, NULL, ?)",
        uuid::Uuid::now_v7().to_string(),
        &appeal_id,
        &moderator,
        now
    );
    assert_violation(
        &err,
        ViolationKind::Check,
        "appeal_decisions.decision CHECK",
    );

    exec!(
        pool,
        "INSERT INTO appeal_decisions (id, appeal_id, reviewer_id, decision, decision_note, conflict_of_interest, created_at)
         VALUES (?, ?, ?, 'rejected', '证据不足', NULL, ?)",
        uuid::Uuid::now_v7().to_string(),
        &appeal_id,
        &moderator,
        now
    );
}

// ─────────────────────────── 三数据库入口 ───────────────────────────

#[tokio::test]
async fn sqlite_moderation_schema() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    moderation_schema_flow(&pool).await;
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mysql_moderation_schema() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mysql")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    moderation_schema_flow(&pool).await;
    close_pool(&pool).await;
}

#[tokio::test]
#[ignore = "需要 BBLBB_TEST_MYSQL_URL（CI mysql-family 任务，--ignored 运行）"]
async fn mariadb_moderation_schema() {
    let url = std::env::var("BBLBB_TEST_MYSQL_URL").expect("BBLBB_TEST_MYSQL_URL 未设置");
    let pool = create_pool(&url).await.unwrap();
    let files = read_migration_files(&migrations_dir("mariadb")).unwrap();
    run_migrations(&pool, &files).await.unwrap();
    moderation_schema_flow(&pool).await;
    close_pool(&pool).await;
}
