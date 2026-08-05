//! M03-AUTHZ-01：`resource.action` 权限注册表集成测试——
//! 数据库未知权限名拒绝（permissions 表）、已知权限放行、缺失只报告。
//!
//! 注册表唯一事实来源：`backend/src/authz/mod.rs::PERMISSION_REGISTRY`
//! （68 项，取自 docs/PERMISSION-MATRIX.md §2-8 + 附录）。

use std::path::{Path, PathBuf};

use bblbb_backend::authz::{
    is_registered, verify_db_permissions, DbPermissionError, PERMISSION_REGISTRY,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-authz-{}", uuid::Uuid::now_v7()));
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

/// 插入一条权限（name/risk_level/is_system/description）。
async fn insert_permission(
    pool: &DatabasePool,
    name: &str,
    risk: &str,
    is_system: bool,
    description: &str,
) {
    let now = bblbb_backend::outbox::now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO permissions (id, name, description, risk_level, is_system, created_at)
                 VALUES (?, ?, ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(name)
            .bind(description)
            .bind(risk)
            .bind(is_system as i64)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn permission_names(pool: &DatabasePool) -> Vec<String> {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT name FROM permissions")
            .fetch_all(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 空表（尚未种子）→ Ok，全部注册权限报告为缺失（种子由 AUTHZ-02 落地）。
#[tokio::test]
async fn empty_db_reports_missing_but_is_ok() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    let check = verify_db_permissions(&pool).await.expect("空表必须 Ok");
    assert_eq!(check.known_in_db, 0);
    assert_eq!(check.missing_from_db.len(), PERMISSION_REGISTRY.len());

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 数据库只含已注册权限 → Ok，known 计数正确。
#[tokio::test]
async fn db_with_only_registered_permissions_passes() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    for name in ["post.read", "post.create", "user.manage", "storage.manage"] {
        let risk = match name {
            "user.manage" => "system",
            "storage.manage" => "sensitive",
            _ => "normal",
        };
        insert_permission(&pool, name, risk, risk == "system", "test").await;
    }

    let check = verify_db_permissions(&pool)
        .await
        .expect("已知权限必须放行");
    assert_eq!(check.known_in_db, 4);
    assert!(is_registered("post.read"));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 数据库含未知权限名 → 拒绝并列出未知名（本叶核心契约）。
#[tokio::test]
async fn unknown_db_permissions_are_rejected() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    insert_permission(&pool, "post.read", "normal", false, "known").await;
    insert_permission(&pool, "ban.hammer", "system", true, "注入的未知权限").await;
    insert_permission(&pool, "everyone.manage", "normal", false, "注入的未知权限").await;

    let err = verify_db_permissions(&pool)
        .await
        .expect_err("未知权限必须拒绝");
    match err {
        DbPermissionError::UnknownPermissions(mut unknown) => {
            unknown.sort();
            assert_eq!(
                unknown,
                vec!["ban.hammer".to_string(), "everyone.manage".to_string()],
                "必须精确列出全部未知权限名"
            );
        }
        DbPermissionError::Database(e) => panic!("不应是数据库错误: {e}"),
    }

    // 修复：删除未知行后恢复 Ok
    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM permissions WHERE name IN ('ban.hammer', 'everyone.manage')")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let check = verify_db_permissions(&pool)
        .await
        .expect("删除未知行后必须 Ok");
    assert_eq!(check.known_in_db, 1);

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 注册表名称集合必须与数据库真实种子（未来 AUTHZ-02）保持一致：此处断言
/// 注册表自洽 + 全部名称可被 DB CHECK（risk_level 枚举）接受。
#[tokio::test]
async fn registry_names_are_db_insertable() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    // 全量注册表名（含 system/sensitive 风险）可插入且不违反 permissions CHECK
    for p in PERMISSION_REGISTRY {
        insert_permission(
            &pool,
            p.name,
            p.risk_level.as_str(),
            p.is_system,
            p.description,
        )
        .await;
    }
    let names = permission_names(&pool).await;
    assert_eq!(names.len(), PERMISSION_REGISTRY.len());
    let check = verify_db_permissions(&pool)
        .await
        .expect("全量注册表必须全部放行");
    assert_eq!(check.known_in_db, PERMISSION_REGISTRY.len());
    assert!(check.missing_from_db.is_empty());

    close_pool(&pool).await;
    cleanup(&dir);
}
