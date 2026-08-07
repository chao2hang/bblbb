//! M04-VISIBILITY：统一评估 + grant 只读 + 写路径等级重检（SQLite）。
//!
//! 覆盖：
//! - M04-VISIBILITY-01：封闭枚举恰好 5 值，拒绝 named-user 可见性；
//! - M04-VISIBILITY-02：DB 版 evaluate（after_reply 作者/管理/reply grant；
//!   logged_in/level/匿名规则在 lib 单测）；
//! - M04-VISIBILITY-03：边界校验 + 422 problem shape；
//! - M04-VISIBILITY-04：草稿创建越级拒绝 + 发布前作者降级阻断 + 编辑前
//!   等级重检（Blocked(VisibilityExceedsLevel)）；
//! - M04-VISIBILITY-06：paid 只读有效 grant（无 grant 锁定 / 有 grant 解锁 /
//!   撤销后重新锁定 / 无行 fail-closed）。

use std::path::{Path, PathBuf};

use bblbb_backend::content::model::{ContentAccessPolicy, Post, PostContent, PostStatus, PostType};
use bblbb_backend::content::posts::command::{
    validate_draft_create, CreateDraftInput, PostCreateError,
};
use bblbb_backend::content::posts::publish::{
    publish_preflight, PublishBlocked, PublishPreflightInput,
};
use bblbb_backend::content::posts::service::{edit_post, EditPostInput};
use bblbb_backend::content::repository::{
    insert_access_policy, insert_post, save_post_content, set_post_access_policy,
};
use bblbb_backend::content::visibility::evaluate::{
    evaluate, post_grant_key, AccessContent, Actor, DbGrantLookup, EvaluateContext,
};
use bblbb_backend::content::visibility::validate::{validate_visibility_level, VisibilityError};
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::domain::posts::AccessPolicy;
use bblbb_backend::outbox::now_millis;
use sqlx::Either;

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

/// 已验证、过冷静期的作者（level 可指定）。
async fn insert_user(pool: &DatabasePool, tag: &str, level: i64) -> String {
    let user_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, level, email_verified, email_verified_at, created_at, updated_at)
                 VALUES (?, ?, ?, 'dummy', 'active', ?, 1, ?, ?, ?)",
            )
            .bind(&user_id)
            .bind(format!("{tag}_{}", uuid::Uuid::now_v7().simple()))
            .bind(format!("{tag}_{}@example.com", uuid::Uuid::now_v7().simple()))
            .bind(level)
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

async fn set_user_level(pool: &DatabasePool, user_id: &str, level: i64) {
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE users SET level = ?, updated_at = ? WHERE id = ?")
                .bind(level)
                .bind(now_millis())
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
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

async fn seed_content(pool: &DatabasePool, post_id: &str, body: &str) {
    save_post_content(
        pool,
        &PostContent {
            post_id: post_id.to_string(),
            body_markdown: body.to_string(),
            body_html: format!("<p>{body}</p>"),
            restricted_markdown: None,
            restricted_html: None,
            renderer_version: "v1".to_string(),
            excerpt: "excerpt".to_string(),
            updated_at: now_millis(),
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

#[allow(clippy::too_many_arguments)]
async fn insert_grant(
    pool: &DatabasePool,
    id: &str,
    user_id: &str,
    post_id: Option<&str>,
    comment_id: Option<&str>,
    policy_id: &str,
    source_kind: &str,
    grant_target_key: &str,
    revoked: bool,
) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO content_access_grants
                    (id, user_id, post_id, comment_id, policy_id, source_kind, source_id,
                     point_operation_id, grant_target_key, granted_at, revoked_at)
                 VALUES (?, ?, ?, ?, ?, ?, NULL, NULL, ?, ?, ?)",
            )
            .bind(id)
            .bind(user_id)
            .bind(post_id)
            .bind(comment_id)
            .bind(policy_id)
            .bind(source_kind)
            .bind(grant_target_key)
            .bind(now)
            .bind(if revoked { Some(now) } else { None })
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn revoke_grant(pool: &DatabasePool, id: &str) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE content_access_grants SET revoked_at = ? WHERE id = ?")
                .bind(now)
                .bind(id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn preflight_input(
    author_id: &str,
    board_id: &str,
    visibility: Option<u32>,
) -> PublishPreflightInput {
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

fn ctx<'a>(
    grants: &'a dyn bblbb_backend::content::visibility::evaluate::GrantLookup,
) -> EvaluateContext<'a> {
    EvaluateContext {
        grants,
        now: now_millis(),
        moderator_override: false,
    }
}

// ───────────────────────── M04-VISIBILITY-01：封闭枚举 ─────────────────────

#[test]
fn closed_enum_accepts_exactly_five_values() {
    for name in ["public", "logged_in", "after_reply", "level", "paid"] {
        assert_eq!(AccessPolicy::parse(name).map(|p| p.as_str()), Some(name));
    }
    assert_eq!(AccessPolicy::ALL.len(), 5);
}

#[test]
fn closed_enum_rejects_named_user_and_legacy_values() {
    for bad in [
        "private",
        "followers",
        "mentioned",
        "指定用户可见",
        "friends",
        "PUBLIC",
        "",
    ] {
        assert_eq!(AccessPolicy::parse(bad), None, "{bad:?} 必须被拒绝");
    }
}

// ───────────────────── M04-VISIBILITY-02：DB 版 evaluate ───────────────────

#[tokio::test]
async fn db_evaluate_after_reply_grant_author_and_moderator() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author_id = insert_user(&pool, "author", 5).await;
    let stranger = insert_user(&pool, "stranger", 5).await;
    let replier = insert_user(&pool, "replier", 1).await;

    let mut p = policy("p-reply", AccessPolicy::AfterReply, &author_id);
    p.reply_grant_persists = true;
    insert_access_policy(&pool, &p).await.unwrap();
    let post_id = uuid::Uuid::now_v7().to_string();
    seed_post(&pool, &post_id, &author_id, BOARD_ID).await;
    set_post_access_policy(&pool, &post_id, Some("p-reply"))
        .await
        .unwrap();

    let lookup = DbGrantLookup { pool: &pool };
    let content = AccessContent {
        grant_target_key: Some(&post_grant_key(&post_id)),
        author_id: Some(&author_id),
        policy: AccessPolicy::AfterReply,
        min_level: None,
        visibility_level: 1,
        author_level: 5,
    };

    // 无 grant 的非作者 → 锁定
    let g = evaluate(
        Some(&Actor {
            id: &stranger,
            level: 5,
            username: "stranger",
        }),
        &content,
        &ctx(&lookup),
    )
    .await;
    assert!(!g.unlocked);
    assert_eq!(g.reason, "after_reply");

    // 有效 reply grant → 解锁
    insert_grant(
        &pool,
        "g-reply",
        &replier,
        Some(&post_id),
        None,
        "p-reply",
        "reply",
        &post_grant_key(&post_id),
        false,
    )
    .await;
    let g = evaluate(
        Some(&Actor {
            id: &replier,
            level: 1,
            username: "replier",
        }),
        &content,
        &ctx(&lookup),
    )
    .await;
    assert!(g.unlocked, "有效 reply grant 必须解锁");

    // 作者自见（无 grant）
    let g = evaluate(
        Some(&Actor {
            id: &author_id,
            level: 5,
            username: "author",
        }),
        &content,
        &ctx(&lookup),
    )
    .await;
    assert!(g.unlocked, "作者始终能看自己内容");

    // 管理 override
    let g = evaluate(
        Some(&Actor {
            id: &stranger,
            level: 5,
            username: "stranger",
        }),
        &content,
        &EvaluateContext {
            grants: &lookup,
            now: now_millis(),
            moderator_override: true,
        },
    )
    .await;
    assert!(g.unlocked, "管理 override 必须解锁");

    // 匿名 → 锁定
    let g = evaluate(None, &content, &ctx(&lookup)).await;
    assert!(!g.unlocked, "匿名不得解锁 after_reply");

    // 撤销 grant → 重新锁定
    revoke_grant(&pool, "g-reply").await;
    let g = evaluate(
        Some(&Actor {
            id: &replier,
            level: 1,
            username: "replier",
        }),
        &content,
        &ctx(&lookup),
    )
    .await;
    assert!(!g.unlocked, "撤销后的 grant 不得解锁");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────── M04-VISIBILITY-06：paid 只读 ────────────────────

#[tokio::test]
async fn db_evaluate_paid_readonly_valid_grant() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author_id = insert_user(&pool, "author", 5).await;
    let buyer = insert_user(&pool, "buyer", 1).await;
    let paidbuyer = insert_user(&pool, "paidbuyer", 1).await;

    let mut p = policy("p-paid", AccessPolicy::Paid, &author_id);
    p.currency_id = Some("bcoin".to_string());
    p.amount = Some(100);
    insert_access_policy(&pool, &p).await.unwrap();
    let post_id = uuid::Uuid::now_v7().to_string();
    seed_post(&pool, &post_id, &author_id, BOARD_ID).await;
    set_post_access_policy(&pool, &post_id, Some("p-paid"))
        .await
        .unwrap();

    let lookup = DbGrantLookup { pool: &pool };
    let key = post_grant_key(&post_id);
    let content = AccessContent {
        grant_target_key: Some(&key),
        author_id: Some(&author_id),
        policy: AccessPolicy::Paid,
        min_level: None,
        visibility_level: 1,
        author_level: 5,
    };

    // 无 purchase grant → 锁定（作者本人也锁定：paid 只认 grant）
    for actor in [
        Some(Actor {
            id: &buyer,
            level: 1,
            username: "buyer",
        }),
        Some(Actor {
            id: &author_id,
            level: 5,
            username: "author",
        }),
        None,
    ] {
        let g = evaluate(actor.as_ref(), &content, &ctx(&lookup)).await;
        assert!(!g.unlocked, "无 purchase grant 必须锁定");
        assert_eq!(g.reason, "paid");
        assert_eq!(g.capabilities, &["purchase"]);
    }

    // 只有 reply grant → 仍锁定（paid 只认 purchase）
    insert_grant(
        &pool,
        "g-wrong-kind",
        &buyer,
        Some(&post_id),
        None,
        "p-paid",
        "reply",
        &key,
        false,
    )
    .await;
    let g = evaluate(
        Some(&Actor {
            id: &buyer,
            level: 1,
            username: "buyer",
        }),
        &content,
        &ctx(&lookup),
    )
    .await;
    assert!(!g.unlocked, "reply grant 不得解锁 paid 内容");

    // 有效 purchase grant → 解锁（paidbuyer 独立行，避开 UNIQUE(user_id,key)）
    insert_grant(
        &pool,
        "g-paid",
        &paidbuyer,
        Some(&post_id),
        None,
        "p-paid",
        "purchase",
        &key,
        false,
    )
    .await;
    let g = evaluate(
        Some(&Actor {
            id: &paidbuyer,
            level: 1,
            username: "paidbuyer",
        }),
        &content,
        &ctx(&lookup),
    )
    .await;
    assert!(g.unlocked, "有效 purchase grant 必须解锁");
    assert_eq!(g.reason, "paid");

    // 撤销 → 重新锁定
    revoke_grant(&pool, "g-paid").await;
    let g = evaluate(
        Some(&Actor {
            id: &paidbuyer,
            level: 1,
            username: "paidbuyer",
        }),
        &content,
        &ctx(&lookup),
    )
    .await;
    assert!(!g.unlocked, "撤销后的 purchase grant 不得解锁");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn db_evaluate_fail_closed_when_grants_row_missing() {
    // 策略行存在但 grants 表无任何行 → 仍走“未解锁”（fail-closed，非报错）。
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author_id = insert_user(&pool, "author", 5).await;
    let viewer = insert_user(&pool, "viewer", 5).await;

    let mut p = policy("p-paid2", AccessPolicy::Paid, &author_id);
    p.currency_id = Some("bcoin".to_string());
    p.amount = Some(100);
    insert_access_policy(&pool, &p).await.unwrap();
    let post_id = uuid::Uuid::now_v7().to_string();
    seed_post(&pool, &post_id, &author_id, BOARD_ID).await;
    set_post_access_policy(&pool, &post_id, Some("p-paid2"))
        .await
        .unwrap();

    let lookup = DbGrantLookup { pool: &pool };
    let content = AccessContent {
        grant_target_key: Some(&post_grant_key(&post_id)),
        author_id: Some(&author_id),
        policy: AccessPolicy::Paid,
        min_level: None,
        visibility_level: 1,
        author_level: 5,
    };
    let g = evaluate(
        Some(&Actor {
            id: &viewer,
            level: 5,
            username: "viewer",
        }),
        &content,
        &ctx(&lookup),
    )
    .await;
    assert!(!g.unlocked, "grants 表无行必须 fail-closed 锁定");
    assert_eq!(g.reason, "paid");

    close_pool(&pool).await;
    cleanup(&dir);
}

// ───────────────────────── M04-VISIBILITY-03：边界校验 ─────────────────────

#[test]
fn validate_level_boundaries() {
    assert_eq!(
        validate_visibility_level(Some(0), 5),
        Err(VisibilityError::Invalid)
    );
    assert_eq!(validate_visibility_level(Some(1), 5), Ok(1));
    assert_eq!(validate_visibility_level(Some(5), 5), Ok(5));
    assert_eq!(
        validate_visibility_level(Some(6), 5),
        Err(VisibilityError::ExceedsAuthorLevel {
            requested: 6,
            author_level: 5
        })
    );
}

#[tokio::test]
async fn visibility_exceeds_author_problem_is_422_with_stable_code() {
    use axum::http::StatusCode;
    use axum::response::IntoResponse;
    use http_body_util::BodyExt;

    let resp = bblbb_backend::error::AppError::visibility_level_exceeds_author(
        "visibility_level 4 exceeds author level 3",
        "req-v",
    )
    .into_response();
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let v: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(v["code"], "visibility_level_exceeds_author");
    assert_eq!(v["status"], 422);
}

// ───────────────── M04-VISIBILITY-04：写路径等级重检 ───────────────────────

#[test]
fn draft_create_rejects_visibility_above_author_level() {
    // validator 层证明 422-class 拒绝（路由接线由 master 后续落地）。
    let r = validate_draft_create(
        CreateDraftInput {
            post_type: "article".to_string(),
            title: "草稿".to_string(),
            markdown: "正文".to_string(),
            board_id: Some(BOARD_ID.to_string()),
            visibility_level: Some(4),
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: "draft-req-000000001".to_string(),
        },
        3, // author_level
        now_millis(),
    );
    assert_eq!(
        r.unwrap_err(),
        PostCreateError::VisibilityExceedsAuthorLevel {
            requested: 4,
            author_level: 3
        }
    );

    // 作者等级内合法
    let r = validate_draft_create(
        CreateDraftInput {
            post_type: "article".to_string(),
            title: "草稿".to_string(),
            markdown: "正文".to_string(),
            board_id: Some(BOARD_ID.to_string()),
            visibility_level: Some(3),
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: "draft-req-000000002".to_string(),
        },
        3,
        now_millis(),
    );
    assert!(r.is_ok(), "visibility_level = author_level 必须合法");
}

#[tokio::test]
async fn publish_preflight_rejects_after_author_downgrade() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "downgrade", 5).await;
    let i = preflight_input(&author, BOARD_ID, Some(5));

    // 等级 5 时 visibility 5 通过
    let r = publish_preflight(&pool, &i).await;
    assert_eq!(r, Ok(()), "等级足够时预检必须通过: {r:?}");

    // 作者降级到 3 → 同一发布请求被阻断（发布时重读等级）
    set_user_level(&pool, &author, 3).await;
    let r = publish_preflight(&pool, &i).await;
    assert_eq!(
        r,
        Err(PublishBlocked::VisibilityExceedsLevel {
            requested: 5,
            author_level: 3
        })
    );

    // 降级后降低到当前等级 → 恢复通过
    let i3 = preflight_input(&author, BOARD_ID, Some(3));
    let r = publish_preflight(&pool, &i3).await;
    assert_eq!(r, Ok(()));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn edit_post_blocks_when_author_below_effective_visibility() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "editor", 5).await;

    // 帖子挂 level-4 策略（有效可见等级 4）
    let mut p = policy("p-level-edit", AccessPolicy::Level, &author);
    p.min_level = Some(4);
    insert_access_policy(&pool, &p).await.unwrap();
    let post_id = uuid::Uuid::now_v7().to_string();
    seed_post(&pool, &post_id, &author, BOARD_ID).await;
    seed_content(&pool, &post_id, "正文").await;
    set_post_access_policy(&pool, &post_id, Some("p-level-edit"))
        .await
        .unwrap();

    // 作者降级到 3（低于有效可见等级 4）→ 编辑被阻断
    set_user_level(&pool, &author, 3).await;
    let r = edit_post(
        &pool,
        &post_id,
        &author,
        &EditPostInput {
            title: Some("新标题".to_string()),
            markdown: Some("新正文".to_string()),
            expected_version: 1,
            change_reason: None,
        },
        now_millis(),
    )
    .await;
    assert_eq!(
        r.unwrap_err(),
        bblbb_backend::content::posts::service::PublishError::Blocked(
            PublishBlocked::VisibilityExceedsLevel {
                requested: 4,
                author_level: 3
            }
        ),
        "作者低于帖子有效可见等级时编辑必须被阻断"
    );

    // 版本不变（阻断发生在写路径之前）
    let v: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar("SELECT version FROM posts WHERE id = ?")
            .bind(&post_id)
            .fetch_one(p)
            .await
            .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(v, 1, "被阻断的编辑不得递增版本");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn edit_post_allows_when_author_level_sufficient() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let author = insert_user(&pool, "editor2", 5).await;

    let mut p = policy("p-level-edit2", AccessPolicy::Level, &author);
    p.min_level = Some(4);
    insert_access_policy(&pool, &p).await.unwrap();
    let post_id = uuid::Uuid::now_v7().to_string();
    seed_post(&pool, &post_id, &author, BOARD_ID).await;
    seed_content(&pool, &post_id, "正文").await;
    set_post_access_policy(&pool, &post_id, Some("p-level-edit2"))
        .await
        .unwrap();

    // 等级 5 ≥ 有效可见等级 4 → 编辑成功
    let r = edit_post(
        &pool,
        &post_id,
        &author,
        &EditPostInput {
            title: Some("新标题".to_string()),
            markdown: Some("新正文".to_string()),
            expected_version: 1,
            change_reason: None,
        },
        now_millis(),
    )
    .await;
    let post = r.expect("等级足够时编辑必须成功");
    assert_eq!(post.version, 2);

    // public 策略 + 等级 1 作者 → 编辑成功（有效可见等级 1）
    let pubp = policy("p-pub-edit", AccessPolicy::Public, &author);
    insert_access_policy(&pool, &pubp).await.unwrap();
    let pub_post_id = uuid::Uuid::now_v7().to_string();
    seed_post(&pool, &pub_post_id, &author, BOARD_ID).await;
    seed_content(&pool, &pub_post_id, "pub 正文").await;
    set_post_access_policy(&pool, &pub_post_id, Some("p-pub-edit"))
        .await
        .unwrap();
    set_user_level(&pool, &author, 1).await;
    let r = edit_post(
        &pool,
        &pub_post_id,
        &author,
        &EditPostInput {
            title: Some("t".to_string()),
            markdown: None,
            expected_version: 1,
            change_reason: None,
        },
        now_millis(),
    )
    .await;
    assert!(r.is_ok(), "public 策略下等级 1 作者编辑必须成功: {r:?}");

    close_pool(&pool).await;
    cleanup(&dir);
}
