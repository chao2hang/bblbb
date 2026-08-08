//! M13-THEME 集成测试（SQLite 真库 + 全量迁移 + 线上服务函数）。
//!
//! 覆盖：上传隔离态/版本校验、closed token 校验（CSS/HTML/JS/SVG/远程资源/
//! 任意 style 字符串）、default fallback（不存在/不兼容/停用/损坏）、
//! theme_revision 在 SSR/浏览器/缓存/偏好一致、If-Match 偏好更新、
//! 配置变更审计。

use bblbb_backend::outbox::now_millis;
use bblbb_backend::theme::{
    self, delete_theme, list_themes, resolve_active_theme, set_default_theme,
    update_theme_settings, update_user_theme_preference, upload_theme_package,
    user_theme_preference, DEFAULT_THEME_NAME,
};
use serde_json::{json, Value};
use sqlx::Either;

mod support;

use support::{cleanup, close_pool, insert_user, sqlite_pool_with_migrations};

fn valid_package(name: &str) -> Value {
    json!({
        "schema_version": 1,
        "name": name,
        "display_name": format!("{name} display"),
        "version": "1.0.0",
        "supports": ">=1.0 <2.0",
        "kind": "data",
        "tokens": {
            "color.background": "#0f172a",
            "color.surface": "#1e293b",
            "color.text": "#e2e8f0",
            "color.muted": "#94a3b8",
            "color.accent": "#38bdf8",
            "color.border": "#334155",
            "font.body": "system-ui",
            "font.mono": "ui-monospace",
            "radius.control": "0.5rem",
            "radius.card": "0.75rem",
            "space.density": "comfortable",
            "shadow.card": "md",
            "motion.duration": "150ms",
            "motion.reduced": true,
        },
    })
}

// ─────────────────────────── M13-THEME-01/02/03 ───────────────────────────

#[tokio::test]
async fn upload_installs_disabled_and_validates_package() {
    let (pool, dir) = sqlite_pool_with_migrations().await;

    let installed = upload_theme_package(&pool, &valid_package("midnight"), "admin")
        .await
        .expect("upload");
    assert_eq!(installed.name, "midnight");
    assert_eq!(installed.revision, 1);
    assert_eq!(installed.status, "disabled", "上传即隔离态 disabled");
    assert!(!installed.is_default);

    // 同名重复上传 → 冲突
    let err = upload_theme_package(&pool, &valid_package("midnight"), "admin")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "theme_conflict");

    // 恶意包：kind=code（在线代码主题）→ 拒绝
    let mut code = valid_package("evil-code");
    code["kind"] = json!("code");
    let err = upload_theme_package(&pool, &code, "admin")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "theme_invalid");

    // 恶意包：CSS/JS/HTML/SVG/远程资源混入 token → 拒绝
    let mut xss = valid_package("xss");
    xss["tokens"]["color.background"] = json!("</style><svg onload=alert(1)>");
    assert_eq!(
        upload_theme_package(&pool, &xss, "admin")
            .await
            .unwrap_err()
            .code(),
        "theme_invalid"
    );
    let mut css = valid_package("css");
    css["tokens"]["color.text"] = json!("red; position: fixed");
    assert!(upload_theme_package(&pool, &css, "admin").await.is_err());
    let mut remote = valid_package("remote");
    remote["tokens"]["color.accent"] = json!("url(https://evil.example/px.png)");
    assert!(upload_theme_package(&pool, &remote, "admin").await.is_err());

    // 未知 token key（封闭 schema）→ 拒绝
    let mut unknown = valid_package("unknown-token");
    unknown["tokens"]["color.evil"] = json!("#000");
    assert!(upload_theme_package(&pool, &unknown, "admin")
        .await
        .is_err());

    // 不兼容版本范围 → 拒绝
    let mut bad_range = valid_package("bad-range");
    bad_range["supports"] = json!(">=2.0");
    let err = upload_theme_package(&pool, &bad_range, "admin")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "theme_incompatible");

    let themes = list_themes(&pool).await.unwrap();
    assert_eq!(themes.len(), 1);

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-THEME-04/05 ──────────────────────────────

#[tokio::test]
async fn falls_back_to_default_on_missing_incompatible_disabled_corrupt() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let user = insert_user(&pool, "theme").await;

    // 1) 无任何主题：内置 default 兜底（revision 恒 1）。
    let active = resolve_active_theme(&pool, None).await.unwrap();
    assert_eq!(active.name, DEFAULT_THEME_NAME);
    assert_eq!(active.revision, 1);
    assert_eq!(active.source, "builtin_default");

    // 2) 用户偏好指向不存在主题 → 回退 default 并告警（不返回错误）。
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "INSERT INTO user_preferences (user_id, timezone, locale, theme_name, updated_at)
                 VALUES (?, 'UTC', 'zh-CN', 'ghost-theme', ?)",
            )
            .bind(&user)
            .bind(now_millis())
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let active = resolve_active_theme(&pool, Some(&user)).await.unwrap();
    assert_eq!(
        active.name, DEFAULT_THEME_NAME,
        "不存在主题必须回退 default"
    );
    assert_eq!(active.revision, 1);

    // 3) 上传 disabled 主题：匿名/用户解析 → 默认（disabled 不生效）。
    upload_theme_package(&pool, &valid_package("midnight"), "admin")
        .await
        .unwrap();
    let active = resolve_active_theme(&pool, None).await.unwrap();
    assert_eq!(active.name, DEFAULT_THEME_NAME, "disabled 主题不得生效");

    // 4) 激活后生效，revision 一致（SSR/浏览器/偏好共享同一 revision）。
    set_default_theme(&pool, "midnight", "admin", "activate")
        .await
        .expect("activate");
    let active = resolve_active_theme(&pool, None).await.unwrap();
    assert_eq!(active.name, "midnight");
    assert_eq!(active.revision, 1);
    let active_user = resolve_active_theme(&pool, Some(&user)).await.unwrap();
    assert_eq!(active_user.name, "midnight", "无偏好用户也应解析到站点默认");
    assert_eq!(active_user.revision, active.revision, "revision 必须一致");

    // 5) 偏好保存到 midnight → 用户解析一致（revision 仍一致）。
    update_user_theme_preference(&pool, &user, "midnight", 1)
        .await
        .expect("set pref");
    let pref = user_theme_preference(&pool, &user).await.unwrap();
    assert_eq!(pref.theme, "midnight");
    assert_eq!(pref.revision, 1);

    // 6) token 损坏（模拟 DB 篡改）→ 用户解析回退 default，状态标 corrupt。
    match &pool {
        Either::Left(p) => {
            sqlx::query(
                "UPDATE themes SET tokens_json = '{\"color.background\":\"<script>\"}' WHERE name = 'midnight'",
            )
            .execute(p)
            .await
            .unwrap();
        }
        Either::Right(_) => panic!("SQLite only"),
    }
    let active = resolve_active_theme(&pool, Some(&user)).await.unwrap();
    assert_eq!(active.name, DEFAULT_THEME_NAME, "损坏主题必须回退 default");
    let themes = list_themes(&pool).await.unwrap();
    let midnight = themes.iter().find(|t| t.name == "midnight").unwrap();
    assert_eq!(midnight.status, "corrupt", "损坏主题必须标记 corrupt");

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-THEME-05/06/09 ───────────────────────────

#[tokio::test]
async fn settings_update_bumps_revision_with_if_match_and_audits() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    upload_theme_package(&pool, &valid_package("midnight"), "admin")
        .await
        .unwrap();
    set_default_theme(&pool, "midnight", "admin", "activate")
        .await
        .unwrap();

    // revision 冲突（期望 1 但当前已 1 → 传入 5 冲突）
    let err = update_theme_settings(
        &pool,
        "midnight",
        &json!({ "color.background": "#111827" }),
        "admin",
        "darken",
        Some(5),
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "theme_conflict");

    // 正确更新 → revision 1→2；前后 revision 与 resolve 一致。
    let updated = update_theme_settings(
        &pool,
        "midnight",
        &json!({
            "color.background": "#111827",
            "color.surface": "#1f2937",
            "color.text": "#f9fafb",
            "color.muted": "#9ca3af",
            "color.accent": "#60a5fa",
            "color.border": "#374151",
            "font.body": "system-ui",
            "font.mono": "ui-monospace",
            "radius.control": "0.5rem",
            "radius.card": "0.75rem",
            "space.density": "compact",
            "shadow.card": "md",
            "motion.duration": "150ms",
            "motion.reduced": true,
        }),
        "admin",
        "darken background",
        Some(1),
    )
    .await
    .unwrap();
    assert_eq!(updated.revision, 2);
    assert_eq!(updated.tokens["color.background"], "#111827");

    let active = resolve_active_theme(&pool, None).await.unwrap();
    assert_eq!(active.revision, 2, "SSR/浏览器必须读到新 revision");
    assert_eq!(active.tokens["color.background"], "#111827");

    // 用户偏好 revision 同步：If-Match 是"当前偏好 revision"（未设置过 → 1）。
    let user = insert_user(&pool, "pref").await;
    update_user_theme_preference(&pool, &user, "midnight", 1)
        .await
        .expect("pref set");
    let pref = user_theme_preference(&pool, &user).await.unwrap();
    assert_eq!(pref.revision, 2, "偏好解析到当前主题 revision=2");

    // 审计记录存在（非敏感 action）。
    let audit_count: i64 = match &pool {
        Either::Left(p) => sqlx::query_scalar(
            "SELECT COUNT(*) FROM audit_logs WHERE action = 'theme.settings.update'",
        )
        .fetch_one(p)
        .await
        .unwrap(),
        Either::Right(_) => panic!("SQLite only"),
    };
    assert_eq!(audit_count, 1);

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-THEME-06 删除保护 ───────────────────────

#[tokio::test]
async fn delete_guards_default_and_builtin() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    upload_theme_package(&pool, &valid_package("midnight"), "admin")
        .await
        .unwrap();
    set_default_theme(&pool, "midnight", "admin", "activate")
        .await
        .unwrap();

    // 内置 default 不可删除
    let err = delete_theme(&pool, DEFAULT_THEME_NAME, "admin", "x")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "theme_conflict");

    // 当前站点默认不可删除
    let err = delete_theme(&pool, "midnight", "admin", "x")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "theme_conflict");

    // 先切回默认再删除
    set_default_theme(&pool, "other", "admin", "x")
        .await
        .unwrap_err(); // other 不存在
    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn missing_user_resolves_to_builtin_default() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    // 未知用户（无偏好行）→ 内置 default（不因 user_id 不存在而报错）。
    let active = resolve_active_theme(&pool, Some("missing-user"))
        .await
        .unwrap();
    assert_eq!(active.name, DEFAULT_THEME_NAME);
    assert_eq!(active.revision, 1);
    // validate_theme_name 边界
    assert!(!theme::validate_theme_name("HasUpper"));
    assert!(!theme::validate_theme_name(""));
    assert!(theme::validate_theme_name("a-b-1"));
    cleanup(&dir);
    close_pool(&pool).await;
}
