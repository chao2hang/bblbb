//! M04-VISIBILITY-05：after_reply 回复 grant 创建与撤销（冻结规则）。
//!
//! 覆盖：
//! 1. 用户在 after_reply 主题发布有效回复 → 落一条 reply grant（
//!    grant_target_key=`post:{id}`、source_kind='reply'、revoked_at NULL）；
//! 2. 同一用户多回复只持有一条 grant（UNIQUE(user_id, grant_target_key) 忽略）；
//! 3. 回复删除后 `reply_grant_persists=0` → grant 撤销（revoked_at 置位）；
//! 4. 回复删除后 `reply_grant_persists=1` → grant 保留；
//! 5. 非 after_reply 策略（public/logged_in/level/paid）回复 → 不写 grant；
//! 6. evaluate 端到端：grant 持有者解锁 after_reply，无 grant 者不解锁。

use std::path::{Path, PathBuf};

use sqlx::Either;

use bblbb_backend::content::comments::service::{
    create_comment, soft_delete_comment, CreateCommentInput,
};
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::service::publish_new_post;
use bblbb_backend::content::visibility::evaluate::{
    evaluate, post_grant_key, AccessContent, Actor, DbGrantLookup, EvaluateContext,
};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::domain::posts::AccessPolicy;
use bblbb_backend::outbox::now_millis;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-vis-{}", uuid::Uuid::now_v7()));
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

async fn insert_author(pool: &DatabasePool, tag: &str) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', 5, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(now - 25 * 3600 * 1000)
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

/// 直接经服务层发布一篇公开帖子，返回 post_id。
async fn publish(pool: &DatabasePool, author_id: &str, title: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "discussion".to_string(),
            title: title.to_string(),
            markdown: format!("正文 {title}"),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("grants-{}-{}", title, uuid::Uuid::now_v7().simple()),
        },
        5,
        now_millis(),
    )
    .unwrap();
    let published = publish_new_post(pool, &cmd, author_id, now_millis())
        .await
        .unwrap();
    published.post.id
}

/// 为主题附加 after_reply 策略行并设置 posts.access_policy_id。
async fn attach_after_reply_policy(
    pool: &DatabasePool,
    post_id: &str,
    creator_id: &str,
    reply_grant_persists: i64,
) -> String {
    let policy_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO content_access_policies
                 (id, kind, min_level, currency_id, amount, reply_grant_persists, policy_version, created_by, created_at)
                 VALUES (?, 'after_reply', NULL, NULL, NULL, ?, 1, ?, ?)",
            )
            .bind(&policy_id)
            .bind(reply_grant_persists)
            .bind(creator_id)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query("UPDATE posts SET access_policy_id = ? WHERE id = ?")
                .bind(&policy_id)
                .bind(post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    policy_id
}

async fn count_grants(pool: &DatabasePool, user_id: &str, post_id: &str) -> (i64, i64) {
    let key = post_grant_key(post_id);
    let (total, active): (i64, i64) = match pool {
        Either::Left(p) => {
            let total = sqlx::query_scalar(
                "SELECT COUNT(*) FROM content_access_grants WHERE user_id = ? AND grant_target_key = ? AND source_kind = 'reply'",
            )
            .bind(user_id)
            .bind(&key)
            .fetch_one(p)
            .await
            .unwrap();
            let active = sqlx::query_scalar(
                "SELECT COUNT(*) FROM content_access_grants WHERE user_id = ? AND grant_target_key = ? AND source_kind = 'reply' AND revoked_at IS NULL",
            )
            .bind(user_id)
            .bind(&key)
            .fetch_one(p)
            .await
            .unwrap();
            (total, active)
        }
        Either::Right(_) => panic!("SQLite only"),
    };
    (total, active)
}

async fn reply(pool: &DatabasePool, post_id: &str, author_id: &str) -> String {
    let comment_id = uuid::Uuid::now_v7().to_string();
    create_comment(
        pool,
        &CreateCommentInput {
            comment_id: comment_id.clone(),
            post_id,
            author_id,
            parent_id: None,
            markdown: "这是一条回复",
            now: now_millis(),
        },
    )
    .await
    .unwrap();
    comment_id
}

#[tokio::test]
async fn after_reply_grant_created_on_valid_reply_and_grants_access() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "owner").await;
    let replier = insert_author(&pool, "replyer").await;
    let post_id = publish(&pool, &author, "after_reply 主题").await;
    attach_after_reply_policy(&pool, &post_id, &author, 0).await;

    // 回复前：无 grant，评估锁定
    let (total, active) = count_grants(&pool, &replier, &post_id).await;
    assert_eq!((total, active), (0, 0), "回复前不得存在 grant");

    // 发布有效回复 → grant 落库
    let comment_id = reply(&pool, &post_id, &replier).await;
    let (total, active) = count_grants(&pool, &replier, &post_id).await;
    assert_eq!((total, active), (1, 1), "有效回复后必须落一条有效 grant");

    // evaluate 端到端：grant 持有者解锁
    let key = post_grant_key(&post_id);
    let lookup = DbGrantLookup { pool: &pool };
    let ctx = EvaluateContext {
        grants: &lookup,
        now: now_millis(),
        moderator_override: false,
    };
    let content = AccessContent {
        grant_target_key: Some(&key),
        author_id: Some(&author),
        policy: AccessPolicy::AfterReply,
        min_level: None,
        visibility_level: 1,
        author_level: 5,
    };
    let replier_actor = Actor {
        id: &replier,
        level: 1,
        username: "replyer_x",
    };
    let grant = evaluate(Some(&replier_actor), &content, &ctx).await;
    assert!(grant.unlocked, "grant 持有者必须解锁 after_reply");

    // 无 grant 的第三方不解锁
    let stranger = insert_author(&pool, "stranger").await;
    let stranger_actor = Actor {
        id: &stranger,
        level: 5,
        username: "stranger_x",
    };
    let grant = evaluate(Some(&stranger_actor), &content, &ctx).await;
    assert!(!grant.unlocked, "无 grant 第三方必须锁定");

    let _ = comment_id;
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn multiple_replies_create_single_grant() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "owner2").await;
    let replier = insert_author(&pool, "replyer2").await;
    let post_id = publish(&pool, &author, "after_reply 主题二").await;
    attach_after_reply_policy(&pool, &post_id, &author, 0).await;

    reply(&pool, &post_id, &replier).await;
    reply(&pool, &post_id, &replier).await;
    reply(&pool, &post_id, &replier).await;

    let (total, active) = count_grants(&pool, &replier, &post_id).await;
    assert_eq!((total, active), (1, 1), "同一用户多回复只持有一条 grant");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn reply_delete_revokes_grant_when_not_persistent() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "owner3").await;
    let replier = insert_author(&pool, "replyer3").await;
    let post_id = publish(&pool, &author, "after_reply 主题三").await;
    attach_after_reply_policy(&pool, &post_id, &author, 0).await;

    let comment_id = reply(&pool, &post_id, &replier).await;
    let (_, active) = count_grants(&pool, &replier, &post_id).await;
    assert_eq!(active, 1, "回复后 grant 生效");

    // 回复删除（reply_grant_persists=0）→ grant 撤销
    let deleted = soft_delete_comment(&pool, &comment_id, now_millis())
        .await
        .unwrap();
    assert!(deleted, "删除必须成功");
    let (total, active) = count_grants(&pool, &replier, &post_id).await;
    assert_eq!(total, 1, "grant 行保留（撤销不删行）");
    assert_eq!(active, 0, "reply_grant_persists=0 回复删除后授权撤销");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn reply_delete_keeps_grant_when_persistent() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "owner4").await;
    let replier = insert_author(&pool, "replyer4").await;
    let post_id = publish(&pool, &author, "after_reply 主题四").await;
    attach_after_reply_policy(&pool, &post_id, &author, 1).await;

    let comment_id = reply(&pool, &post_id, &replier).await;
    let deleted = soft_delete_comment(&pool, &comment_id, now_millis())
        .await
        .unwrap();
    assert!(deleted, "删除必须成功");

    let (total, active) = count_grants(&pool, &replier, &post_id).await;
    assert_eq!(total, 1);
    assert_eq!(active, 1, "reply_grant_persists=1 回复删除后授权保留");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn non_after_reply_policies_do_not_create_grants() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "owner5").await;
    let replier = insert_author(&pool, "replyer5").await;
    // public 策略主题（未附加 after_reply 行）回复 → 不写 grant
    let post_id = publish(&pool, &author, "public 主题").await;
    reply(&pool, &post_id, &replier).await;
    let (total, active) = count_grants(&pool, &replier, &post_id).await;
    assert_eq!((total, active), (0, 0), "public 策略不得写 grant");
    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn paid_policy_does_not_grant_on_reply() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_author(&pool, "owner6").await;
    let replier = insert_author(&pool, "replyer6").await;
    let post_id = publish(&pool, &author, "paid 主题").await;

    // paid 策略行（M7 才创建 purchase grant；回复不得产生 reply grant）
    let policy_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO content_access_policies
                 (id, kind, min_level, currency_id, amount, reply_grant_persists, policy_version, created_by, created_at)
                 VALUES (?, 'paid', NULL, 'coin', 100, 0, 1, ?, ?)",
            )
            .bind(&policy_id)
            .bind(&author)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query("UPDATE posts SET access_policy_id = ? WHERE id = ?")
                .bind(&policy_id)
                .bind(&post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }

    reply(&pool, &post_id, &replier).await;
    let (total, active) = count_grants(&pool, &replier, &post_id).await;
    assert_eq!((total, active), (0, 0), "paid 策略回复不得写 reply grant");
    close_pool(&pool).await;
    cleanup(&dir);
}
