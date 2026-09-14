//! M04-POSTS-07：详情/列表/板块列表/作者列表——cursor/ETag/Cache-Control（SQLite）。
//!
//! 覆盖：列表 keyset 分页（has_more/next_cursor）；Cache-Control+ETag 头；
//! 详情投影（body_html/author/access_summary）；404；板块列表过滤；作者过滤。

use std::path::{Path, PathBuf};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use bblbb_backend::content::posts::command::{validate_post_create, CreatePostInput};
use bblbb_backend::content::posts::service::publish_new_post;
use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::outbox::now_millis;
use bblbb_backend::{build_router, AppConfig};
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::Either;
use tower::ServiceExt;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-prd-{}", uuid::Uuid::now_v7()));
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
                "INSERT INTO users (id, username_normalized, email_normalized, password_hash, status, trust_level, email_verified, email_verified_at, created_at, updated_at)
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

/// 直接经服务层发布一篇帖子，返回 post_id。
async fn publish(pool: &DatabasePool, author_id: &str, title: &str) -> String {
    let cmd = validate_post_create(
        CreatePostInput {
            post_type: "article".to_string(),
            title: title.to_string(),
            markdown: format!("正文 {title}"),
            board_id: BOARD_ID.to_string(),
            visibility_level: None,
            access_policy: "public".to_string(),
            scheduled_at: None,
            client_request_id: format!("read-{}-{}", title, uuid::Uuid::now_v7().simple()),
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

fn app_with(pool: DatabasePool) -> axum::Router {
    build_router(AppConfig::default(), Some(pool))
}

async fn get(app: &axum::Router, uri: &str) -> (StatusCode, Value, axum::http::HeaderMap) {
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
    let headers = resp.headers().clone();
    let bytes = resp.into_body().collect().await.unwrap().to_bytes();
    let value: Value = serde_json::from_slice(&bytes).unwrap();
    (status, value, headers)
}

#[tokio::test]
async fn list_posts_cursor_pagination_with_headers() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "alice").await;
    // 三帖（created_at 递增，latest 排序 → p3 最新）
    let p1 = publish(&pool, &author, "帖一").await;
    let p2 = publish(&pool, &author, "帖二").await;
    let p3 = publish(&pool, &author, "帖三").await;

    let (status, body, headers) = get(&app, "/api/v1/posts?limit=2").await;
    assert_eq!(status, StatusCode::OK, "列表必须 200");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "第一页两条");
    assert_eq!(items[0]["id"], Value::String(p3.clone()), "最新帖在最前");
    assert_eq!(items[1]["id"], Value::String(p2.clone()));
    assert_eq!(body["page"]["has_more"], true, "还有更多");
    let cursor = body["page"]["next_cursor"].as_str().unwrap().to_string();
    assert!(!cursor.is_empty(), "has_more 必须有 next_cursor");
    // 响应头
    assert!(
        headers.get("cache-control").is_some(),
        "必须带 Cache-Control"
    );
    assert!(headers.get("etag").is_some(), "必须带 ETag");

    // 第二页
    let (_, body2, _) = get(&app, &format!("/api/v1/posts?limit=2&after={cursor}")).await;
    let items2 = body2["items"].as_array().unwrap();
    assert_eq!(items2.len(), 1, "第二页一条");
    assert_eq!(items2[0]["id"], Value::String(p1.clone()));
    assert_eq!(body2["page"]["has_more"], false, "末页无更多");
    assert!(body2["page"]["next_cursor"].is_null(), "末页无 next_cursor");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn get_post_detail_returns_projection() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "bob").await;
    let post_id = publish(&pool, &author, "详情帖").await;

    let (status, body, headers) = get(&app, &format!("/api/v1/posts/{post_id}")).await;
    assert_eq!(status, StatusCode::OK, "详情必须 200");
    assert_eq!(body["id"], Value::String(post_id.clone()));
    assert_eq!(body["title"], "详情帖");
    assert_eq!(body["status"], "published");
    assert!(
        body["author"]["id"] == Value::String(author.clone()),
        "作者投影"
    );
    assert!(
        body["body_html"].as_str().unwrap().contains("正文 详情帖"),
        "body_html 必须可见: {}",
        body["body_html"]
    );
    assert_eq!(body["access_summary"]["policy"], "public");
    assert!(
        headers.get("cache-control").is_some(),
        "必须带 Cache-Control"
    );
    assert!(headers.get("etag").is_some(), "必须带 ETag");

    // 不存在 → 404
    let (status, _, _) = get(&app, &format!("/api/v1/posts/{}", uuid::Uuid::now_v7())).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "不存在必须 404");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn list_board_posts_filters_by_board() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "carol").await;
    publish(&pool, &author, "板块帖").await;

    let (status, body, _) = get(&app, "/api/v1/boards/general/posts").await;
    assert_eq!(status, StatusCode::OK, "板块列表必须 200");
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 1, "板块帖一条");
    assert_eq!(items[0]["title"], "板块帖");

    // 未知板块 → 404
    let (status, _, _) = get(&app, "/api/v1/boards/no-such-board/posts").await;
    assert_eq!(status, StatusCode::NOT_FOUND, "未知板块必须 404");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn list_posts_author_filter() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let alice = insert_author(&pool, "alice2").await;
    let bob = insert_author(&pool, "bob2").await;
    publish(&pool, &alice, "A1").await;
    publish(&pool, &alice, "A2").await;
    publish(&pool, &bob, "B1").await;

    let (_, body, _) = get(&app, &format!("/api/v1/posts?author_id={alice}")).await;
    let items = body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2, "作者列表只含该作者帖子");
    for it in items {
        assert_eq!(
            it["author"]["id"],
            Value::String(alice.clone()),
            "作者过滤生效"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 直接插入一条评论（测试参与者预览用；status/floor 显式给定）。
async fn insert_comment(
    pool: &DatabasePool,
    post_id: &str,
    author_id: &str,
    floor: i64,
    status: &str,
) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO comments (id, post_id, author_id, parent_id, content, content_format, status, floor, created_at, updated_at)
                 VALUES (?, ?, ?, NULL, '回复内容', 'markdown', ?, ?, ?, ?)",
            )
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(post_id)
            .bind(author_id)
            .bind(status)
            .bind(floor)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 列表投影含参与者预览：已发布回复的不同作者（不含楼主、不含隐藏/删除
/// 回复），按首评楼层排序，每帖 ≤2 个；无回复帖子 participants 为空数组。
#[tokio::test]
async fn list_posts_includes_participant_preview() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let alice = insert_author(&pool, "part_alice").await;
    let bob = insert_author(&pool, "part_bob").await;
    let carol = insert_author(&pool, "part_carol").await;
    let dave = insert_author(&pool, "part_dave").await;
    let eve = insert_author(&pool, "part_eve").await;
    let frank = insert_author(&pool, "part_frank").await;
    let grace = insert_author(&pool, "part_grace").await;
    let heidi = insert_author(&pool, "part_heidi").await;

    let with_replies = publish(&pool, &alice, "参与者帖").await;
    // bob 抢首评（两条只算一个参与者），carol/eve/frank/grace/heidi 依次回复；
    // 楼主自评（floor 4）与隐藏回复（dave, floor 5）都不进参与者；上限 5 个（heidi 截断）。
    insert_comment(&pool, &with_replies, &bob, 1, "published").await;
    insert_comment(&pool, &with_replies, &bob, 2, "published").await;
    insert_comment(&pool, &with_replies, &carol, 3, "published").await;
    insert_comment(&pool, &with_replies, &alice, 4, "published").await;
    insert_comment(&pool, &with_replies, &dave, 5, "hidden").await;
    insert_comment(&pool, &with_replies, &eve, 6, "published").await;
    insert_comment(&pool, &with_replies, &frank, 7, "published").await;
    insert_comment(&pool, &with_replies, &grace, 8, "published").await;
    insert_comment(&pool, &with_replies, &heidi, 9, "published").await;
    let no_replies = publish(&pool, &alice, "无回复帖").await;

    let (status, body, _) = get(&app, "/api/v1/posts?limit=10").await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();
    let row = items
        .iter()
        .find(|it| it["id"] == Value::String(with_replies.clone()))
        .expect("列表含参与者帖");
    let participants = row["participants"].as_array().expect("participants 数组");
    assert_eq!(participants.len(), 5, "每帖最多 5 个参与者预览");
    assert_eq!(
        participants[0]["id"],
        Value::String(bob.clone()),
        "首评在前"
    );
    assert_eq!(participants[1]["id"], Value::String(carol.clone()));
    assert_eq!(participants[2]["id"], Value::String(eve.clone()));
    assert_eq!(participants[3]["id"], Value::String(frank.clone()));
    assert_eq!(participants[4]["id"], Value::String(grace.clone()));
    for p in participants {
        assert_ne!(p["id"], Value::String(alice.clone()), "楼主不算参与者");
        assert_ne!(p["id"], Value::String(dave.clone()), "隐藏回复不算参与者");
        assert!(p["username"].is_string(), "参与者含公开 username 投影");
        assert!(p["display_name"].is_null() || p["display_name"].is_string());
    }

    let empty = items
        .iter()
        .find(|it| it["id"] == Value::String(no_replies.clone()))
        .expect("列表含无回复帖");
    assert_eq!(
        empty["participants"].as_array().unwrap().len(),
        0,
        "无回复帖 participants 为空数组"
    );

    // 板块列表投影同样带参与者预览。
    let (_, body, _) = get(&app, "/api/v1/boards/general/posts").await;
    let items = body["items"].as_array().unwrap();
    let row = items
        .iter()
        .find(|it| it["id"] == Value::String(with_replies.clone()))
        .expect("板块列表含参与者帖");
    let participants = row["participants"]
        .as_array()
        .expect("板块 participants 数组");
    assert_eq!(participants.len(), 5, "板块列表参与者预览一致");
    assert_eq!(participants[0]["id"], Value::String(bob.clone()));

    close_pool(&pool).await;
    cleanup(&dir);
}

/// 为用户插入 published 头像框商品 + 已装备权益，并把装配槽位指向该权益
/// （与 equip/rebuild_presentation 的真实落库语义一致：槽位存权益 id）。
async fn equip_gold_frame(pool: &DatabasePool, user_id: &str, tag: &str) {
    let now = now_millis();
    let suffix = uuid::Uuid::now_v7().simple().to_string();
    let product_id = format!("prod-frame-{tag}-{suffix}");
    let order_id = uuid::Uuid::now_v7().to_string();
    let ent_id = uuid::Uuid::now_v7().to_string();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO shop_products (id, kind, status, slug, title, presentation_tokens_json, \
                 slot, currency_id, unit_price, created_by, created_at, updated_at) \
                 VALUES (?, 'cosmetic_avatar', 'published', ?, '测试鎏金之环', '[\"avatar.frame.gold_ring\"]', \
                 'avatar_frame', '01911fd5-0047-0000-0000-000000000002', 100, ?, ?, ?)",
            )
            .bind(&product_id)
            .bind(format!("frame-{tag}-{suffix}"))
            .bind(user_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO shop_orders (id, user_id, product_id, product_version, quantity, \
                 currency_id, unit_price, total_amount, point_operation_id, status, \
                 idempotency_key, request_hash, created_at, updated_at) \
                 VALUES (?, ?, ?, 1, 1, '01911fd5-0047-0000-0000-000000000002', 100, 100, ?, \
                 'succeeded', ?, 'dummy_hash', ?, ?)",
            )
            .bind(&order_id)
            .bind(user_id)
            .bind(&product_id)
            .bind(uuid::Uuid::now_v7().to_string())
            .bind(format!("idem-{suffix}"))
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO user_entitlements (id, user_id, product_id, order_id, status, \
                 quantity, remaining_quantity, valid_from, expires_at, equipped_at, \
                 created_at, updated_at) \
                 VALUES (?, ?, ?, ?, 'equipped', 1, 1, ?, NULL, ?, ?, ?)",
            )
            .bind(&ent_id)
            .bind(user_id)
            .bind(&product_id)
            .bind(&order_id)
            .bind(now)
            .bind(now)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query(
                "INSERT INTO user_presentations (user_id, avatar_frame_id, version, updated_at, created_at) \
                 VALUES (?, ?, 1, ?, ?) \
                 ON CONFLICT(user_id) DO UPDATE SET \
                 avatar_frame_id = excluded.avatar_frame_id, updated_at = excluded.updated_at",
            )
            .bind(user_id)
            .bind(&ent_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 为用户插入一个公开就绪的 PNG 头像附件，并写回 users.avatar_attachment_id。
async fn set_uploaded_avatar(pool: &DatabasePool, user_id: &str, attachment_id: &str) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO attachments (id, owner_id, storage_backend, storage_key, media_type, \
                 size_bytes, sha256, status, is_public, created_at) \
                 VALUES (?, ?, 'local', ?, 'image/png', 64, 'dummy', 'ready', 1, ?)",
            )
            .bind(attachment_id)
            .bind(user_id)
            .bind(format!("avatars/{attachment_id}.png"))
            .bind(now)
            .execute(p)
            .await
            .unwrap();
            sqlx::query("UPDATE users SET avatar_attachment_id = ? WHERE id = ?")
                .bind(attachment_id)
                .bind(user_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 列表装扮投影：作者（楼主）与参与者装备头像框后，列表行的
/// author.presentation_tokens / participants[].presentation_tokens 携带
/// 服务端编译的公开 Token（avatar_frame=gold_ring）；无装备行不出该键。
#[tokio::test]
async fn list_posts_projects_equipped_avatar_frame_tokens() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let app = app_with(pool.clone());
    let author = insert_author(&pool, "frame_author").await;
    let replier = insert_author(&pool, "frame_replier").await;
    let plain_author = insert_author(&pool, "plain_author").await;

    equip_gold_frame(&pool, &author, "fa").await;
    equip_gold_frame(&pool, &replier, "fr").await;
    set_uploaded_avatar(&pool, &author, "att-author-avatar").await;
    set_uploaded_avatar(&pool, &replier, "att-replier-avatar").await;

    let with_frame = publish(&pool, &author, "金框帖").await;
    insert_comment(&pool, &with_frame, &replier, 1, "published").await;
    let plain_post = publish(&pool, &plain_author, "无装扮帖").await;

    let (status, body, _) = get(&app, "/api/v1/posts?limit=10").await;
    assert_eq!(status, StatusCode::OK);
    let items = body["items"].as_array().unwrap();

    let row = items
        .iter()
        .find(|it| it["id"] == Value::String(with_frame.clone()))
        .expect("列表含金框帖");
    assert_eq!(
        row["author"]["presentation_tokens"]["avatar_frame"],
        Value::String("gold_ring".into()),
        "楼主已装备头像框随列表行下发"
    );
    assert_eq!(
        row["author"]["avatar_attachment_id"],
        Value::String("att-author-avatar".into()),
        "楼主上传头像附件引用随列表行下发"
    );
    let participants = row["participants"].as_array().unwrap();
    assert_eq!(participants.len(), 1);
    assert_eq!(
        participants[0]["presentation_tokens"]["avatar_frame"],
        Value::String("gold_ring".into()),
        "参与者已装备头像框随列表行下发"
    );
    assert_eq!(
        participants[0]["avatar_attachment_id"],
        Value::String("att-replier-avatar".into()),
        "参与者上传头像附件引用随列表行下发"
    );

    let plain = items
        .iter()
        .find(|it| it["id"] == Value::String(plain_post.clone()))
        .expect("列表含无装扮帖");
    assert!(
        plain["author"].get("presentation_tokens").is_none(),
        "无装扮作者不出 presentation_tokens 键"
    );
    assert!(
        plain["author"].get("avatar_attachment_id").is_none(),
        "未上传头像的作者不出 avatar_attachment_id 键"
    );
    assert!(
        plain["participants"]
            .as_array()
            .unwrap()
            .iter()
            .all(|p| p.get("presentation_tokens").is_none()),
        "无装扮参与者不出 presentation_tokens 键"
    );

    // 板块列表投影与首页列表同构（作者装扮同样随行下发）。
    let (_, body, _) = get(&app, "/api/v1/boards/general/posts").await;
    let items = body["items"].as_array().unwrap();
    let row = items
        .iter()
        .find(|it| it["id"] == Value::String(with_frame.clone()))
        .expect("板块列表含金框帖");
    assert_eq!(
        row["author"]["presentation_tokens"]["avatar_frame"],
        Value::String("gold_ring".into()),
        "板块列表楼主头像框投影一致"
    );
    assert_eq!(
        row["author"]["avatar_attachment_id"],
        Value::String("att-author-avatar".into()),
        "板块列表楼主上传头像投影一致"
    );
    let participants = row["participants"].as_array().unwrap();
    assert_eq!(
        participants[0]["presentation_tokens"]["avatar_frame"],
        Value::String("gold_ring".into()),
        "板块列表参与者头像框投影一致"
    );
    assert_eq!(
        participants[0]["avatar_attachment_id"],
        Value::String("att-replier-avatar".into()),
        "板块列表参与者上传头像投影一致"
    );

    close_pool(&pool).await;
    cleanup(&dir);
}
