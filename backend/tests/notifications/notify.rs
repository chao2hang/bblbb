//! M05-NOTIFY-09：通知创建/去重、列表/已读、偏好、权限复查、邮件 Job
//! 重试/死信/重放与日志安全测试（SQLite 全量 + 跨库 #[ignore]）。

use std::path::{Path, PathBuf};

use bblbb_backend::db::migrate::{read_migration_files, run_migrations};
use bblbb_backend::db::pool::create_pool;
use bblbb_backend::db::DatabasePool;
use bblbb_backend::email::service as email;
use bblbb_backend::email::service::{deliver_email_job, enqueue_email, sanitize_log, RecordingSender};
use bblbb_backend::jobs::classify::ProviderError;
use bblbb_backend::jobs::worker::claim_batch;
use bblbb_backend::notifications::model::NotificationCategory;
use bblbb_backend::notifications::service as notify;
use bblbb_backend::notifications::templates::{TemplateKey, FORBIDDEN_NOTIFICATION_PARAMS};
use bblbb_backend::outbox::now_millis;
use serde_json::Value;
use sqlx::Either;

#[path = "../common/mod.rs"]
mod common;

const BOARD_ID: &str = "01911fd5-f000-7561-a2a5-3dd6434157f0"; // seeded 'general'

async fn sqlite_pool_with_migrations() -> (DatabasePool, PathBuf) {
    let dir = std::env::temp_dir().join(format!("bblbb-notify-{}", uuid::Uuid::now_v7()));
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

async fn insert_user(pool: &DatabasePool, tag: &str) -> String {
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
            .bind(now - 30 * 86_400 * 1000)
            .bind(now - 30 * 86_400 * 1000)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    user_id
}

async fn insert_post(pool: &DatabasePool, author_id: &str) -> String {
    let post_id = uuid::Uuid::now_v7().to_string();
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO posts (id, board_id, author_id, title, content, created_at, updated_at)
                 VALUES (?, ?, ?, '通知测试帖', '正文', ?, ?)",
            )
            .bind(&post_id)
            .bind(BOARD_ID)
            .bind(author_id)
            .bind(now)
            .bind(now)
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    post_id
}

async fn set_post_status(pool: &DatabasePool, post_id: &str, status: &str) {
    let now = now_millis();
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE posts SET status = ?, updated_at = ? WHERE id = ?")
                .bind(status)
                .bind(now)
                .bind(post_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn job_status(pool: &DatabasePool, job_id: &str) -> (String, i64) {
    match pool {
        Either::Left(p) => sqlx::query_as::<_, (String, i64)>(
            "SELECT status, attempts FROM jobs WHERE id = ?",
        )
        .bind(job_id)
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    }
}

/// 把 retry_wait 的 available_at 拨回过去，使 claim_batch 立即可领。
async fn make_available(pool: &DatabasePool, job_id: &str) {
    let past = now_millis() - 60_000;
    match pool {
        Either::Left(p) => {
            sqlx::query("UPDATE jobs SET available_at = ? WHERE id = ?")
                .bind(past)
                .bind(job_id)
                .execute(p)
                .await
                .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

async fn count_notifications(pool: &DatabasePool, user_id: &str) -> i64 {
    match pool {
        Either::Left(p) => {
            sqlx::query_scalar("SELECT COUNT(*) FROM notifications WHERE user_id = ?")
                .bind(user_id)
                .fetch_one(p)
                .await
                .unwrap()
        }
        Either::Right(_) => panic!("SQLite only"),
    }
}

fn params(pairs: &[(&str, &str)]) -> serde_json::Map<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), Value::String(v.to_string())))
        .collect()
}

#[tokio::test]
async fn create_notification_validates_and_dedups() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    // 合法创建：模板键 + 安全参数
    let r = notify::create_notification(
        &pool,
        notify::CreateNotificationInput {
            user_id: user.clone(),
            category: NotificationCategory::Activity,
            template_key: TemplateKey::ReplyCreated,
            r#type: None,
            resource_type: Some("post".to_string()),
            resource_id: Some("p1".to_string()),
            params: params(&[("actor_name", "小明")]),
        },
        now,
    )
    .await
    .unwrap();
    assert!(r.inserted);
    assert_eq!(r.notification.r#type, "reply");
    assert_eq!(r.notification.category, NotificationCategory::Activity);
    assert!(r.notification.title.contains("回复"));

    // 去重：同 (收件人, 模板, 资源) 不重复插入（重放幂等）
    let r2 = notify::create_notification(
        &pool,
        notify::CreateNotificationInput {
            user_id: user.clone(),
            category: NotificationCategory::Activity,
            template_key: TemplateKey::ReplyCreated,
            r#type: None,
            resource_type: Some("post".to_string()),
            resource_id: Some("p1".to_string()),
            params: params(&[("actor_name", "小明")]),
        },
        now,
    )
    .await
    .unwrap();
    assert!(!r2.inserted, "同键重放不得重复插入");
    assert_eq!(count_notifications(&pool, &user).await, 1);

    // 未知模板键：TemplateKey 枚举保证合法；email 路径的字符串解析另测
    assert!(bblbb_backend::notifications::templates::is_known_template(
        TemplateKey::SecurityNotice.as_str()
    ));
    assert!(!bblbb_backend::notifications::templates::is_known_template("unknown.template"));

    // 禁止参数：隐藏正文 / 内部 note
    for forbidden in FORBIDDEN_NOTIFICATION_PARAMS {
        let err = notify::create_notification(
            &pool,
            notify::CreateNotificationInput {
                user_id: user.clone(),
                category: NotificationCategory::Moderation,
                template_key: TemplateKey::ModerationAction,
                r#type: None,
                resource_type: None,
                resource_id: None,
                params: params(&[(forbidden, "隐藏正文/内部note")]),
            },
            now,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, notify::NotifyError::Invalid(_)),
            "must reject {forbidden}"
        );
    }

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn list_cursor_and_read_flows() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    for i in 0..5 {
        notify::create_notification(
            &pool,
            notify::CreateNotificationInput {
                user_id: user.clone(),
                category: NotificationCategory::System,
                template_key: TemplateKey::LevelUp,
                r#type: None,
                resource_type: None,
                resource_id: None,
                params: params(&[("level", &format!("{i}"))]),
            },
            now + i,
        )
        .await
        .unwrap();
    }

    // 分页：limit=2 → has_more
    let (page1, has_more) = notify::list_notifications(&pool, &user, 2, false, None)
        .await
        .unwrap();
    assert_eq!(page1.len(), 2);
    assert!(has_more);
    let cursor = page1.last().unwrap().id.clone();
    let (page2, has_more2) = notify::list_notifications(&pool, &user, 2, false, Some(&cursor))
        .await
        .unwrap();
    assert_eq!(page2.len(), 2);
    assert!(has_more2);
    // 翻页不重复
    for n in &page2 {
        assert!(page1.iter().all(|x| x.id != n.id));
    }

    // 未读计数 + 单条已读
    assert_eq!(notify::unread_count(&pool, &user).await.unwrap(), 5);
    let first_id = page1[0].id.clone();
    assert!(notify::mark_read(&pool, &user, &first_id, now + 1000).await.unwrap());
    assert_eq!(notify::unread_count(&pool, &user).await.unwrap(), 4);
    // 重复已读幂等
    assert!(!notify::mark_read(&pool, &user, &first_id, now + 2000).await.unwrap());

    // 批量已读
    let updated = notify::mark_all_read(&pool, &user, now + 3000).await.unwrap();
    assert_eq!(updated, 4);
    assert_eq!(notify::unread_count(&pool, &user).await.unwrap(), 0);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn preferences_security_never_fully_disabled() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    // security 类别不可全关
    let err = notify::set_preference(
        &pool,
        &user,
        NotificationCategory::Security,
        false,
        false,
        false,
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, notify::NotifyError::Invalid(_)));

    // 普通类别允许全关
    notify::set_preference(
        &pool,
        &user,
        NotificationCategory::Activity,
        false,
        false,
        false,
        now,
    )
    .await
    .unwrap();

    // 缺行类别返回默认全开
    let prefs = notify::get_preferences(&pool, &user).await.unwrap();
    assert_eq!(prefs.len(), 5);
    let security = prefs
        .iter()
        .find(|p| p.category == NotificationCategory::Security)
        .unwrap();
    assert!(security.email_enabled && security.in_app_enabled && security.push_enabled);
    let activity = prefs
        .iter()
        .find(|p| p.category == NotificationCategory::Activity)
        .unwrap();
    assert!(!activity.email_enabled && !activity.in_app_enabled && !activity.push_enabled);

    // 更新后读取
    notify::set_preference(
        &pool,
        &user,
        NotificationCategory::Security,
        true,
        false,
        false,
        now + 1000,
    )
    .await
    .unwrap();
    let prefs = notify::get_preferences(&pool, &user).await.unwrap();
    let security = prefs
        .iter()
        .find(|p| p.category == NotificationCategory::Security)
        .unwrap();
    assert!(security.email_enabled && !security.in_app_enabled && !security.push_enabled);

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn permission_recheck_hides_unavailable_content() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let post = insert_post(&pool, &user).await;
    let now = now_millis();

    notify::create_notification(
        &pool,
        notify::CreateNotificationInput {
            user_id: user.clone(),
            category: NotificationCategory::Activity,
            template_key: TemplateKey::ReplyCreated,
            r#type: None,
            resource_type: Some("post".to_string()),
            resource_id: Some(post.clone()),
            params: params(&[("actor_name", "小明")]),
        },
        now,
    )
    .await
    .unwrap();
    let (items, _) = notify::list_notifications(&pool, &user, 10, false, None)
        .await
        .unwrap();
    let projected = notify::project_list(&pool, items).await.unwrap();
    assert_eq!(projected.len(), 1);
    assert_eq!(projected[0]["unavailable"], Value::Bool(false));
    assert_eq!(projected[0]["title"], "有新回复");

    // 隐藏后：只显示安全失效状态，不泄漏标题/正文/链接
    set_post_status(&pool, &post, "hidden").await;
    let (items, _) = notify::list_notifications(&pool, &user, 10, false, None)
        .await
        .unwrap();
    let projected = notify::project_list(&pool, items).await.unwrap();
    assert_eq!(projected[0]["unavailable"], Value::Bool(true));
    assert_eq!(projected[0]["title"], "内容不可用");
    assert_eq!(projected[0]["body"], "相关内容已被隐藏或删除");
    assert_eq!(projected[0]["link"], Value::Null);

    // 恢复后恢复正常
    set_post_status(&pool, &post, "published").await;
    let (items, _) = notify::list_notifications(&pool, &user, 10, false, None)
        .await
        .unwrap();
    let projected = notify::project_list(&pool, items).await.unwrap();
    assert_eq!(projected[0]["unavailable"], Value::Bool(false));

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn email_job_transient_retry_permanent_dead_replay() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    // 临时失败 → retry_wait + 退避
    let job_id = enqueue_email(
        &pool,
        &user,
        TemplateKey::SecurityNotice,
        params(&[("kind", "new_device")]),
        None,
        None,
        now,
    )
    .await
    .unwrap();
    let sender = RecordingSender::default();
    sender
        .failures
        .lock()
        .unwrap()
        .push(ProviderError::Smtp { code: 450 });
    let claimed = claim_batch(&pool, "w1", email::EMAIL_QUEUE, 10, 60_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    deliver_email_job(&pool, "w1", &job_id, &sender).await.unwrap_err();
    let (status, attempts) = job_status(&pool, &job_id).await;
    assert_eq!(status, "retry_wait");
    assert_eq!(attempts, 1);

    // 临时失败耗尽前再次领取 → 成功
    make_available(&pool, &job_id).await;
    let claimed = claim_batch(&pool, "w1", email::EMAIL_QUEUE, 10, 60_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    deliver_email_job(&pool, "w1", &job_id, &sender).await.unwrap();
    let (status, _) = job_status(&pool, &job_id).await;
    assert_eq!(status, "succeeded");
    {
        let calls = sender.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert!(calls[0].0.ends_with("@example.com"), "投递到完整邮箱");
    }

    // 永久失败 → dead；管理员重放 → queued
    let job_id2 = enqueue_email(
        &pool,
        &user,
        TemplateKey::SanctionApplied,
        params(&[("kind", "禁言"), ("expires_hint", "3 天")]),
        None,
        None,
        now,
    )
    .await
    .unwrap();
    let sender2 = RecordingSender::default();
    sender2
        .failures
        .lock()
        .unwrap()
        .push(ProviderError::Smtp { code: 550 });
    let claimed = claim_batch(&pool, "w1", email::EMAIL_QUEUE, 10, 60_000)
        .await
        .unwrap();
    assert_eq!(claimed.len(), 1);
    deliver_email_job(&pool, "w1", &job_id2, &sender2)
        .await
        .unwrap_err();
    let (status, _) = job_status(&pool, &job_id2).await;
    assert_eq!(status, "dead");

    assert!(email::replay_email_job(&pool, &job_id2).await.unwrap());
    let (status, attempts) = job_status(&pool, &job_id2).await;
    assert_eq!(status, "queued");
    assert_eq!(attempts, 0, "重放重置 attempts");

    close_pool(&pool).await;
    cleanup(&dir);
}

#[tokio::test]
async fn email_payload_and_log_sanitization() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "u").await;
    let now = now_millis();

    // payload 禁止明文 token（M01-JOBS-12）：长 URL-safe base64 被拒
    let token = bblbb_backend::auth::token::generate_token();
    let mut bad_params = params(&[("kind", "new_device")]);
    bad_params.insert("link".to_string(), Value::String(token.clone()));
    let err = enqueue_email(
        &pool,
        &user,
        TemplateKey::SecurityNotice,
        bad_params,
        None,
        None,
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, email::EmailError::Invalid(_)), "{err:?}");

    // 去重：同 (收件人, 模板, 资源) 只入队一条
    let id = enqueue_email(
        &pool,
        &user,
        TemplateKey::SecurityNotice,
        params(&[("kind", "new_device")]),
        None,
        None,
        now,
    )
    .await
    .unwrap();
    let err = enqueue_email(
        &pool,
        &user,
        TemplateKey::SecurityNotice,
        params(&[("kind", "new_device")]),
        None,
        None,
        now,
    )
    .await
    .unwrap_err();
    assert!(matches!(err, email::EmailError::Invalid(_)));
    let _ = id;

    // 日志安全：完整邮箱、正文、token、Provider 响应不进入日志文本
    let detail = format!("smtp rejected (code 550) token={token}");
    let log = sanitize_log("alice@example.com", "有处罚", &detail);
    assert!(!log.contains("alice@example.com"), "完整邮箱不得入日志: {log}");
    assert!(log.contains("a***@example.com"));
    assert!(!log.contains(&token), "token 必须脱敏");
    assert!(!log.contains("正文"), "正文不得入日志");
    assert!(log.contains("[REDACTED]"));

    close_pool(&pool).await;
    cleanup(&dir);
}
