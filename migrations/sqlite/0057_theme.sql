-- BBLBB Themes & config plugin schema (M13-THEME / M13-PLUGIN, SQLite)
--
-- themes：数据型主题（closed token schema，不保存 CSS/JS/HTML/SVG/远程资源）。
-- theme_revisions：主题 Token 每次变更追加一条修订（revision 单调递增，
--   SSR/浏览器/缓存/用户偏好共享同一 revision）。
-- plugins：v1 配置型插件（manifest 版本化；capabilities/settings schema 封闭
--   白名单；安装默认 disabled）。
-- plugin_call_metrics：插件调用摘要（ok/error/timeout/repeat/stale/skipped +
--   policy_revision），异步记录，不阻塞核心论坛。
-- plugin_data：插件自身命名空间数据（配额由服务层校验）。

CREATE TABLE themes (
    name TEXT PRIMARY KEY NOT NULL,
    display_name TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'data' CHECK (kind IN ('data')),
    schema_version INTEGER NOT NULL DEFAULT 1,
    version TEXT NOT NULL,
    supports TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'disabled' CHECK (status IN ('active', 'disabled', 'corrupt')),
    is_default INTEGER NOT NULL DEFAULT 0,
    revision INTEGER NOT NULL DEFAULT 1,
    tokens_json TEXT NOT NULL,
    asset_meta_json TEXT NULL,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX themes_status_idx ON themes (status);

CREATE TABLE theme_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    theme_name TEXT NOT NULL,
    revision INTEGER NOT NULL,
    tokens_json TEXT NOT NULL,
    changed_by TEXT NOT NULL,
    reason TEXT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT theme_revisions_uq UNIQUE (theme_name, revision),
    CONSTRAINT theme_revisions_theme_fk FOREIGN KEY (theme_name) REFERENCES themes (name) ON DELETE CASCADE
);

CREATE INDEX theme_revisions_theme_idx ON theme_revisions (theme_name);

CREATE TABLE plugins (
    id TEXT PRIMARY KEY NOT NULL,
    plugin_id TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    schema_version INTEGER NOT NULL DEFAULT 1,
    supports TEXT NOT NULL,
    kind TEXT NOT NULL DEFAULT 'config' CHECK (kind IN ('config')),
    status TEXT NOT NULL DEFAULT 'disabled' CHECK (status IN ('disabled', 'enabled', 'error')),
    capabilities_json TEXT NOT NULL,
    subscriptions_json TEXT NOT NULL,
    settings_schema_json TEXT NOT NULL,
    settings_json TEXT NOT NULL DEFAULT '{}',
    policy_revision INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX plugins_status_idx ON plugins (status);

CREATE TABLE plugin_call_metrics (
    id TEXT PRIMARY KEY NOT NULL,
    plugin_id TEXT NOT NULL,
    event_type TEXT NOT NULL,
    result TEXT NOT NULL CHECK (result IN ('ok', 'error', 'timeout', 'repeat', 'stale', 'skipped')),
    error_class TEXT NULL,
    policy_revision INTEGER NOT NULL,
    latency_ms INTEGER NULL,
    occurred_at INTEGER NOT NULL,
    CONSTRAINT plugin_call_metrics_plugin_fk FOREIGN KEY (plugin_id) REFERENCES plugins (plugin_id) ON DELETE CASCADE
);

CREATE INDEX plugin_call_metrics_plugin_idx ON plugin_call_metrics (plugin_id, occurred_at);

CREATE TABLE plugin_data (
    plugin_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value_json TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (plugin_id, key),
    CONSTRAINT plugin_data_plugin_fk FOREIGN KEY (plugin_id) REFERENCES plugins (plugin_id) ON DELETE CASCADE
);
