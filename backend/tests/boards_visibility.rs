//! M03-BOARDS-03：板块可见性（public/members/restricted/hidden）统一授权门
//! （真实 DB，经 authz 聚合 + 账号状态门）。
//!
//! 纯函数（VisibilityDeny → 状态码、HIDDEN_READ_PERMISSIONS 注册）在
//! visibility.rs 单测覆盖；本文件锁定真实门语义：
//! - public 匿名可见；members 需有效登录；restricted 需本板块生效角色；
//!   hidden 仅管理权限可读（404 防存在性泄漏）；
//! - banned 状态门优先（members/restricted/hidden 一律拒）；
//! - 列表投影过滤：hidden 不进入公开列表，members/restricted 对匿名隐藏；
//! - 过期板块角色不授予 restricted 可见性。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::decision::BoardVisibility;
use bblbb_backend::authz::roles::seed_builtin_roles;
use bblbb_backend::boards::{board_read_gate, filter_visible_board_ids, VisibilityDeny};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

mod common;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-vis-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let username = format!("{tag}_{}", &uuid::Uuid::now_v7().simple().to_string()[..10]);
    let email = format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple());
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy-hash', 'active', ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(&username)
            .bind(&email)
            .bind(Some(now - 2 * 86_400_000))
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

async fn set_banned(pool: &DatabasePool, user_id: &str) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET status = 'banned' WHERE id = ?")
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn insert_board(pool: &DatabasePool, slug: &str, visibility: &str) -> String {
    let board_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO boards (id, slug, name, description, visibility, created_at, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&board_id)
            .bind(slug)
            .bind(slug)
            .bind(slug)
            .bind(visibility)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    board_id
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

async fn assign_global_role(pool: &DatabasePool, user_id: &str, role_name: &str) {
    let role_id = role_id_by_name(pool, role_name).await;
    let now = now_millis();
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

/// 启用板块角色并授予用户（expires_at 可空 = 永久；board_roles 幂等）。
async fn grant_board_role(
    pool: &DatabasePool,
    board_id: &str,
    user_id: &str,
    expires_at: Option<i64>,
) {
    let role_id = role_id_by_name(pool, "board_moderator").await;
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT OR IGNORE INTO board_roles (board_id, role_id, granted_at) VALUES (?, ?, ?)",
            )
            .bind(board_id)
            .bind(&role_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO board_role_assignments (id, board_id, user_id, role_id, granted_by, granted_at, expires_at)
                 VALUES (?, ?, ?, ?, NULL, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(board_id)
            .bind(user_id)
            .bind(&role_id)
            .bind(now - 60_000)
            .bind(expires_at)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn gate(
    pool: &DatabasePool,
    board_id: &str,
    visibility: &str,
    actor: Option<&str>,
) -> (bool, Option<VisibilityDeny>) {
    let visibility = BoardVisibility::parse(visibility).unwrap();
    let access = board_read_gate(pool, board_id, visibility, actor)
        .await
        .expect("门判定必须成功");
    (access.visible, access.deny)
}

/// public 板块匿名可见。
#[tokio::test]
async fn public_board_visible_to_everyone() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let board = insert_board(&pool, "open-forum", "public").await;
    let member = insert_user(&pool, "mem").await;

    assert_eq!(
        gate(&pool, &board, "public", None).await,
        (true, None),
        "匿名可见"
    );
    assert_eq!(
        gate(&pool, &board, "public", Some(&member)).await,
        (true, None)
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

/// members 板块：匿名 401；有效成员可见；banned 拒绝（状态门优先）。
#[tokio::test]
async fn members_board_requires_login() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let board = insert_board(&pool, "member-lounge", "members").await;
    let member = insert_user(&pool, "mem").await;
    let banned = insert_user(&pool, "ban").await;
    set_banned(&pool, &banned).await;

    assert_eq!(
        gate(&pool, &board, "members", None).await,
        (false, Some(VisibilityDeny::Unauthenticated))
    );
    assert_eq!(
        gate(&pool, &board, "members", Some(&member)).await,
        (true, None)
    );
    assert_eq!(
        gate(&pool, &board, "members", Some(&banned)).await,
        (false, Some(VisibilityDeny::AccountNotAllowed)),
        "banned 状态门优先"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

/// restricted 板块：匿名 401；无角色成员 403；生效角色可见；过期角色不可见。
#[tokio::test]
async fn restricted_board_requires_board_role() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let board = insert_board(&pool, "mods-only", "restricted").await;
    let outsider = insert_user(&pool, "out").await;
    let insider = insert_user(&pool, "in").await;
    let expired = insert_user(&pool, "exp").await;
    let admin = insert_user(&pool, "adm").await;
    assign_global_role(&pool, &admin, "administrator").await;
    common::enroll_totp(&pool, &admin).await; // M02-MFA-05：管理员必须完成 TOTP

    assert_eq!(
        gate(&pool, &board, "restricted", None).await,
        (false, Some(VisibilityDeny::Unauthenticated))
    );
    assert_eq!(
        gate(&pool, &board, "restricted", Some(&outsider)).await,
        (false, Some(VisibilityDeny::NotBoardMember))
    );

    grant_board_role(&pool, &board, &insider, None).await;
    common::enroll_totp(&pool, &insider).await; // M02-MFA-05：板块版主必须完成 TOTP
    assert_eq!(
        gate(&pool, &board, "restricted", Some(&insider)).await,
        (true, None),
        "本板块生效角色可见"
    );

    grant_board_role(&pool, &board, &expired, Some(now_millis() - 1)).await;
    assert_eq!(
        gate(&pool, &board, "restricted", Some(&expired)).await,
        (false, Some(VisibilityDeny::NotBoardMember)),
        "过期板块角色不授予可见性"
    );

    assert_eq!(
        gate(&pool, &board, "restricted", Some(&admin)).await,
        (true, None),
        "管理员经 post.moderate 全局/管理通道可见"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

/// hidden 板块：匿名 401；普通成员 404（不泄漏存在性）；全局版主/管理员可见。
#[tokio::test]
async fn hidden_board_requires_management() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let board = insert_board(&pool, "secret", "hidden").await;
    let member = insert_user(&pool, "mem").await;
    let gmod = insert_user(&pool, "gmod").await;
    let admin = insert_user(&pool, "adm").await;
    assign_global_role(&pool, &gmod, "global_moderator").await;
    assign_global_role(&pool, &admin, "administrator").await;
    common::enroll_totp(&pool, &gmod).await; // M02-MFA-05：版主/管理员必须完成 TOTP
    common::enroll_totp(&pool, &admin).await;

    assert_eq!(
        gate(&pool, &board, "hidden", None).await,
        (false, Some(VisibilityDeny::MissingPermission)),
        "hidden 匿名也 404（防存在性推断，M03-BOARDS-08）"
    );
    assert_eq!(
        gate(&pool, &board, "hidden", Some(&member)).await,
        (false, Some(VisibilityDeny::MissingPermission)),
        "普通成员无管理权限 → 404 语义"
    );
    assert_eq!(
        gate(&pool, &board, "hidden", Some(&gmod)).await,
        (true, None),
        "全局版主 post.moderate 可见"
    );
    assert_eq!(
        gate(&pool, &board, "hidden", Some(&admin)).await,
        (true, None),
        "管理员 board.manage 可见"
    );
    close_pool(&pool).await;
    cleanup(&dir);
}

/// 列表投影过滤：匿名只见 public；成员见 public+members（+有角色的 restricted）；
/// 管理员见全部（含 hidden）。
#[tokio::test]
async fn list_filter_respects_visibility() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let pub_b = insert_board(&pool, "pub", "public").await;
    let mem_b = insert_board(&pool, "mem", "members").await;
    let res_b = insert_board(&pool, "res", "restricted").await;
    let hid_b = insert_board(&pool, "hid", "hidden").await;

    let member = insert_user(&pool, "mem").await;
    let admin = insert_user(&pool, "adm").await;
    assign_global_role(&pool, &admin, "administrator").await;
    common::enroll_totp(&pool, &admin).await; // M02-MFA-05：管理员必须完成 TOTP
    grant_board_role(&pool, &res_b, &member, None).await;
    common::enroll_totp(&pool, &member).await; // M02-MFA-05：板块版主（elevated）必须完成 TOTP

    let all = vec![
        (pub_b.clone(), BoardVisibility::Public),
        (mem_b.clone(), BoardVisibility::Members),
        (res_b.clone(), BoardVisibility::Restricted),
        (hid_b.clone(), BoardVisibility::Hidden),
    ];

    // 匿名：只 public
    let anon = filter_visible_board_ids(&pool, &all, None).await.unwrap();
    assert_eq!(anon, vec![pub_b.clone()]);

    // 成员（有 restricted 角色）：public + members + restricted
    let member_visible = filter_visible_board_ids(&pool, &all, Some(&member))
        .await
        .unwrap();
    assert_eq!(
        member_visible,
        vec![pub_b.clone(), mem_b.clone(), res_b.clone()],
        "hidden 不进入成员列表"
    );

    // 管理员：全部
    let admin_visible = filter_visible_board_ids(&pool, &all, Some(&admin))
        .await
        .unwrap();
    assert_eq!(admin_visible, vec![pub_b, mem_b, res_b, hid_b]);
    close_pool(&pool).await;
    cleanup(&dir);
}
