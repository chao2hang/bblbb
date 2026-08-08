//! M13-PLUGIN 集成测试（SQLite 真库 + 全量迁移 + 线上服务函数）。
//!
//! 覆盖：安装/启停/settings/卸载生命周期、closed settings schema 校验、
//! 危险 URL/代码内容拒绝、capability 越权拒绝、policy_revision 乐观锁、
//! 事件解析不消费禁用插件、调用摘要指标（repeat/old-version 安全降级）、
//! 无 JS/离线路径不阻塞（record_call 非阻塞）。

use std::path::Path;

use bblbb_backend::plugins::{
    install_plugin, list_plugin_metrics, list_plugins, put_plugin_data, record_call,
    resolve_plugins_for_event, set_plugin_status, uninstall_plugin, update_plugin_settings,
    PluginError,
};
use serde_json::{json, Value};

mod support;

use support::{cleanup, close_pool, insert_user, sqlite_pool_with_migrations};

fn valid_package() -> Value {
    json!({
        "schema_version": 1,
        "id": "welcome-reward",
        "name": "新用户欢迎奖励",
        "version": "1.0.0",
        "supports": ">=1.0 <2.0",
        "kind": "config",
        "subscriptions": ["user.verified.v1", "post.published.v1"],
        "capabilities": ["notification.create", "points.award"],
        "settings_schema": {
            "type": "object",
            "properties": {
                "amount": { "type": "integer", "minimum": 0, "maximum": 1000 },
                "welcome_message": { "type": "string", "minLength": 1, "maxLength": 200 }
            },
            "required": ["amount"],
            "additionalProperties": false
        }
    })
}

// ─────────────────────────── M13-PLUGIN-01/04/06 ──────────────────────────

#[tokio::test]
async fn install_then_configure_then_enable_then_disable_then_uninstall() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let admin = insert_user(&pool, "admin").await;

    // 安装（默认 disabled；policy_revision=1）
    let plugin = install_plugin(&pool, &valid_package(), &admin)
        .await
        .expect("install");
    assert_eq!(plugin.status, "disabled");
    assert_eq!(plugin.policy_revision, 1);
    assert_eq!(plugin.plugin_id, "welcome-reward");
    assert!(plugin.capabilities.contains(&"points.award".to_string()));

    // 重复安装 → 冲突
    let err = install_plugin(&pool, &valid_package(), &admin)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "plugin_conflict");

    // 未知 capability / 未知事件 / 代码 kind → 拒绝（M13-PLUGIN-01/04）
    let mut overreach = valid_package();
    overreach["id"] = json!("overreach");
    overreach["capabilities"] = json!(["admin.manage", "moderation.sanction"]);
    let err = install_plugin(&pool, &overreach, &admin).await.unwrap_err();
    assert_eq!(err.code(), "plugin_invalid");
    assert!(err.message().contains("unknown capability"));

    let mut unknown_event = valid_package();
    unknown_event["id"] = json!("unknown-event");
    unknown_event["subscriptions"] = json!(["secrets.leak.v1"]);
    assert_eq!(
        install_plugin(&pool, &unknown_event, &admin)
            .await
            .unwrap_err()
            .code(),
        "plugin_invalid"
    );

    let mut wasm = valid_package();
    wasm["id"] = json!("wasm-plugin");
    wasm["kind"] = json!("wasm");
    let err = install_plugin(&pool, &wasm, &admin).await.unwrap_err();
    assert_eq!(err.code(), "plugin_invalid");
    assert!(err.message().contains("v2"), "代码/WASM 必须指向 v2 研究项");

    // settings 校验：缺 required → 拒绝；超上限 → 拒绝；危险 URL/代码 → 拒绝
    let err = update_plugin_settings(&pool, "welcome-reward", &json!({}), &admin, "x", 1)
        .await
        .unwrap_err();
    assert_eq!(err.code(), "plugin_invalid");

    let err = update_plugin_settings(
        &pool,
        "welcome-reward",
        &json!({ "amount": 99999 }),
        &admin,
        "x",
        1,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "plugin_invalid");

    let err = update_plugin_settings(
        &pool,
        "welcome-reward",
        &json!({ "amount": 10, "welcome_message": "http://169.254.169.254/ssrf" }),
        &admin,
        "x",
        1,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "plugin_invalid");

    let err = update_plugin_settings(
        &pool,
        "welcome-reward",
        &json!({ "amount": 10, "welcome_message": "<script>eval(1)</script>" }),
        &admin,
        "x",
        1,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "plugin_invalid");

    // 正确 settings → policy_revision 1→2
    let updated = update_plugin_settings(
        &pool,
        "welcome-reward",
        &json!({ "amount": 100, "welcome_message": "欢迎加入" }),
        &admin,
        "configure",
        1,
    )
    .await
    .unwrap();
    assert_eq!(updated.policy_revision, 2);

    // 过期 revision → 冲突
    let err = update_plugin_settings(
        &pool,
        "welcome-reward",
        &json!({ "amount": 1 }),
        &admin,
        "x",
        1,
    )
    .await
    .unwrap_err();
    assert_eq!(err.code(), "plugin_conflict");

    // 启用 → 3；停用 → 4
    let enabled = set_plugin_status(&pool, "welcome-reward", "enabled", &admin, "go live", 2)
        .await
        .unwrap();
    assert_eq!(enabled.status, "enabled");
    assert_eq!(enabled.policy_revision, 3);
    let disabled = set_plugin_status(&pool, "welcome-reward", "disabled", &admin, "halt", 3)
        .await
        .unwrap();
    assert_eq!(disabled.status, "disabled");
    assert_eq!(disabled.policy_revision, 4);

    // enabled 状态下不可卸载
    set_plugin_status(&pool, "welcome-reward", "enabled", &admin, "x", 4)
        .await
        .unwrap();
    let err = uninstall_plugin(&pool, "welcome-reward", &admin, "x")
        .await
        .unwrap_err();
    assert_eq!(err.code(), "plugin_conflict");
    set_plugin_status(&pool, "welcome-reward", "disabled", &admin, "x", 5)
        .await
        .unwrap();
    uninstall_plugin(&pool, "welcome-reward", &admin, "uninstall")
        .await
        .unwrap();
    let plugins = list_plugins(&pool).await.unwrap();
    assert!(plugins.is_empty(), "卸载后列表为空");

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-PLUGIN-02/03 ─────────────────────────────

#[tokio::test]
async fn plugin_cannot_reach_core_tables_or_secrets() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let admin = insert_user(&pool, "admin").await;
    install_plugin(&pool, &valid_package(), &admin)
        .await
        .unwrap();

    // plugin_data 只能写自身命名空间：其它插件名/核心表名 key 拒绝。
    let err = put_plugin_data(&pool, "welcome-reward", "__system__", &json!(1))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "plugin_invalid");
    put_plugin_data(&pool, "welcome-reward", "counter", &json!(42))
        .await
        .expect("own namespace write");
    // 配额：超过 key 数上限 → 冲突（写满）。
    for i in 0..64 {
        let _ = put_plugin_data(&pool, "welcome-reward", &format!("k{i}"), &json!(i)).await;
    }
    let err = put_plugin_data(&pool, "welcome-reward", "overflow", &json!(1))
        .await
        .unwrap_err();
    assert_eq!(err.code(), "plugin_conflict", "超过 key 配额必须拒绝");

    cleanup(&dir);
    close_pool(&pool).await;
}

// ─────────────────────────── M13-PLUGIN-05/06/07 ──────────────────────────

#[tokio::test]
async fn disabled_plugins_do_not_consume_events_and_calls_are_recorded() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let admin = insert_user(&pool, "admin").await;
    install_plugin(&pool, &valid_package(), &admin)
        .await
        .unwrap();

    // disabled：不消费事件
    let resolved = resolve_plugins_for_event(&pool, "user.verified.v1")
        .await
        .unwrap();
    assert!(resolved.is_empty(), "disabled 插件不得消费新事件");

    // enabled：订阅匹配的事件可解析
    set_plugin_status(&pool, "welcome-reward", "enabled", &admin, "x", 1)
        .await
        .unwrap();
    let resolved = resolve_plugins_for_event(&pool, "user.verified.v1")
        .await
        .unwrap();
    assert_eq!(resolved.len(), 1);
    // 未订阅事件不解析
    let resolved = resolve_plugins_for_event(&pool, "reaction.created.v1")
        .await
        .unwrap();
    assert!(resolved.is_empty());

    // 调用摘要：ok + error + timeout + repeat + stale（旧版本）全部记录；
    // 记录失败不阻塞（记录函数本身不返回 Result）。
    record_call(
        &pool,
        "welcome-reward",
        "user.verified.v1",
        "ok",
        None,
        3,
        Some(12),
    )
    .await;
    record_call(
        &pool,
        "welcome-reward",
        "user.verified.v1",
        "error",
        Some("provider_timeout"),
        3,
        Some(5000),
    )
    .await;
    record_call(
        &pool,
        "welcome-reward",
        "user.verified.v1",
        "timeout",
        Some("timeout"),
        3,
        None,
    )
    .await;
    record_call(
        &pool,
        "welcome-reward",
        "user.verified.v1",
        "repeat",
        Some("idempotency_reuse"),
        3,
        None,
    )
    .await;
    // 旧版本策略（policy_revision=1 已过期）→ stale，安全降级不执行动作。
    record_call(
        &pool,
        "welcome-reward",
        "user.verified.v1",
        "stale",
        None,
        1,
        None,
    )
    .await;
    // 未知 result → 丢弃（不落库）
    record_call(
        &pool,
        "welcome-reward",
        "user.verified.v1",
        "hacked",
        None,
        3,
        None,
    )
    .await;

    let metrics = list_plugin_metrics(&pool, "welcome-reward", 100)
        .await
        .unwrap();
    assert_eq!(metrics.len(), 5, "只记录 5 条合法摘要");
    let results: Vec<&str> = metrics
        .iter()
        .filter_map(|m| m["result"].as_str())
        .collect();
    for expected in ["ok", "error", "timeout", "repeat", "stale"] {
        assert!(results.contains(&expected), "缺 result={expected}");
    }
    // metrics 不含 Secret/正文（脱敏投影）
    for m in &metrics {
        let s = m.to_string();
        assert!(!s.contains("welcome_message"));
        assert!(!s.contains("password"));
        assert!(!s.contains("token"));
    }

    cleanup(&dir);
    close_pool(&pool).await;
}

#[tokio::test]
async fn version_range_and_schema_rejects_are_stable() {
    let (pool, dir) = sqlite_pool_with_migrations().await;
    let admin = insert_user(&pool, "admin").await;
    let mut pkg = valid_package();
    pkg["id"] = json!("old-version");
    pkg["supports"] = json!(">=2.0");
    let err = install_plugin(&pool, &pkg, &admin).await.unwrap_err();
    assert!(matches!(err, PluginError::Incompatible(_)));

    let mut pkg = valid_package();
    pkg["id"] = json!("bad-schema");
    pkg["settings_schema"]["properties"]["x"]["pattern"] = json!(".*");
    let err = install_plugin(&pool, &pkg, &admin).await.unwrap_err();
    assert_eq!(
        err.code(),
        "plugin_invalid",
        "schema 必须封闭（拒绝未知键）"
    );

    let _dir = Path::new(&dir);
    cleanup(&dir);
    close_pool(&pool).await;
}
