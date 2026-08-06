//! M04-SCHEMA-06：content_access_policies 结构与仓储契约（SQLite）。
//!
//! - kind 封闭枚举 public/logged_in/after_reply/level/paid（CHECK）；
//! - 字段组合校验（level 需 min_level；paid 需 currency_id+amount 且金额为正）；
//! - posts.access_policy_id 可空外键，策略删除置空回退 public；
//! - policy_version 默认 1。

use std::path::{Path, PathBuf};

use bblbb_backend::content::model::{ContentAccessPolicy, Post, PostStatus, PostType};
use bblbb_backend::content::repository::{
    get_access_policy, insert_access_policy, insert_post, set_post_access_policy,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::domain::posts::AccessPolicy;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-cap-{}", uuid::Uuid::now_v7()));
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
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
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

async fn board_id_by_slug(pool: &DatabasePool, slug: &str) -> String {
    match pool {
        Either::Left(p) => sqlx::query_scalar("SELECT id FROM boards WHERE slug = ?")
            .bind(slug)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn seed_post(pool: &DatabasePool, id: &str, author: &str, board: &str) {
    let now = now_millis();
    insert_post(
        pool,
        &Post {
            id: id.to_string(),
            board_id: board.to_string(),
            author_id: author.to_string(),
            post_type: PostType::Article,
            slug: Some(format!("slug-{id}")),
            title: format!("post {id}"),
            status: PostStatus::Published,
            version: 1,
            scheduled_at: None,
            published_at: Some(now),
            pinned_at: None,
            featured_at: None,
            closed_at: None,
            canonical_url: None,
            seo_title: None,
            seo_description: None,
            view_count: 0,
            reply_count: 0,
            last_reply_id: None,
            last_reply_at: None,
            created_at: now,
            updated_at: now,
            deleted_at: None,
        },
    )
    .await
    .unwrap();
}

fn policy(id: &str, kind: AccessPolicy, creator: &str) -> ContentAccessPolicy {
    ContentAccessPolicy {
        id: id.to_string(),
        kind,
        min_level: None,
        currency_id: None,
        amount: None,
        reply_grant_persists: false,
        policy_version: 1,
        created_by: creator.to_string(),
        created_at: now_millis(),
    }
}

/// 各 kind 往返 + 结构校验规则。
#[tokio::test]
async fn policy_kinds_roundtrip_and_validate() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let creator = insert_user(&pool, "creator").await;

    for (id, kind) in [
        ("p-public", AccessPolicy::Public),
        ("p-logged", AccessPolicy::LoggedIn),
        ("p-reply", AccessPolicy::AfterReply),
    ] {
        insert_access_policy(&pool, &policy(id, kind, &creator))
            .await
            .unwrap();
        let got = get_access_policy(&pool, id)
            .await
            .unwrap()
            .expect("必须读到");
        assert_eq!(got.kind, kind);
        assert_eq!(got.policy_version, 1);
        assert!(got.validate().is_ok());
    }

    // level 需 min_level
    let mut level = policy("p-level", AccessPolicy::Level, &creator);
    assert_eq!(level.validate(), Err("level 策略必须指定 min_level"));
    level.min_level = Some(3);
    assert!(level.validate().is_ok());
    insert_access_policy(&pool, &level).await.unwrap();

    // paid 需 currency_id+amount 且金额为正
    let mut paid = policy("p-paid", AccessPolicy::Paid, &creator);
    assert_eq!(
        paid.validate(),
        Err("paid 策略必须指定 currency_id 与 amount")
    );
    paid.currency_id = Some("bcoin".to_string());
    paid.amount = Some(0);
    assert_eq!(paid.validate(), Err("amount 必须为正"));
    paid.amount = Some(100);
    assert!(paid.validate().is_ok());
    insert_access_policy(&pool, &paid).await.unwrap();

    close_pool(&pool).await;
    cleanup(&dir);
}

/// kind CHECK 拒绝未知值；posts.access_policy_id 往返 + 清除 + 删除置空。
#[tokio::test]
async fn policy_kind_check_and_post_reference() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let creator = insert_user(&pool, "creator").await;
    let general = board_id_by_slug(&pool, "general").await;
    seed_post(&pool, "p1", &creator, &general).await;

    // 非法 kind → CHECK 拒绝
    let r = match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO content_access_policies (id, kind, created_by, created_at)
                 VALUES ('p-bad', 'bogus', ?, ?)",
            )
            .bind(&creator)
            .bind(now_millis())
            .execute(p)
            .await
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(r.is_err(), "未知 kind 必须被 CHECK 拒绝");

    // 帖子关联策略 + 读取 + 清除
    let mut level = policy("p-level", AccessPolicy::Level, &creator);
    level.min_level = Some(2);
    insert_access_policy(&pool, &level).await.unwrap();
    set_post_access_policy(&pool, "p1", Some("p-level"))
        .await
        .unwrap();
    let pid: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT access_policy_id FROM posts WHERE id = 'p1'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(pid.as_deref(), Some("p-level"));

    // 删除策略 → 帖子 access_policy_id 置空（回退 public）
    match &pool {
        Either::Left(p) => {
            sqlx::query("DELETE FROM content_access_policies WHERE id = 'p-level'")
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let pid: Option<String> = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT access_policy_id FROM posts WHERE id = 'p1'")
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert!(pid.is_none(), "策略删除后帖子引用必须置空");

    close_pool(&pool).await;
    cleanup(&dir);
}
