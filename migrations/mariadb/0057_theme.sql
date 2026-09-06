
CREATE TABLE themes (
    name VARCHAR(64) PRIMARY KEY NOT NULL,
    display_name VARCHAR(120) NOT NULL,
    kind VARCHAR(16) NOT NULL DEFAULT 'data',
    schema_version BIGINT NOT NULL DEFAULT 1,
    version VARCHAR(32) NOT NULL,
    supports VARCHAR(32) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'disabled',
    is_default BIGINT NOT NULL DEFAULT 0,
    revision BIGINT NOT NULL DEFAULT 1,
    tokens_json TEXT NOT NULL,
    asset_meta_json TEXT NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT themes_kind_ck CHECK (kind IN ('data')),
    CONSTRAINT themes_status_ck CHECK (status IN ('active', 'disabled', 'corrupt'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX themes_status_idx ON themes (status);

CREATE TABLE theme_revisions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci PRIMARY KEY NOT NULL,
    theme_name VARCHAR(64) NOT NULL,
    revision BIGINT NOT NULL,
    tokens_json TEXT NOT NULL,
    changed_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    reason VARCHAR(500) NULL,
    created_at BIGINT NOT NULL,
    CONSTRAINT theme_revisions_uq UNIQUE (theme_name, revision),
    CONSTRAINT theme_revisions_theme_fk FOREIGN KEY (theme_name) REFERENCES themes (name) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX theme_revisions_theme_idx ON theme_revisions (theme_name);

CREATE TABLE plugins (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci PRIMARY KEY NOT NULL,
    plugin_id VARCHAR(64) NOT NULL,
    name VARCHAR(120) NOT NULL,
    version VARCHAR(32) NOT NULL,
    schema_version BIGINT NOT NULL DEFAULT 1,
    supports VARCHAR(32) NOT NULL,
    kind VARCHAR(16) NOT NULL DEFAULT 'config',
    status VARCHAR(16) NOT NULL DEFAULT 'disabled',
    capabilities_json TEXT NOT NULL,
    subscriptions_json TEXT NOT NULL,
    settings_schema_json TEXT NOT NULL,
    settings_json TEXT NOT NULL DEFAULT '{}',
    policy_revision BIGINT NOT NULL DEFAULT 1,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT plugins_kind_ck CHECK (kind IN ('config')),
    CONSTRAINT plugins_status_ck CHECK (status IN ('disabled', 'enabled', 'error')),
    CONSTRAINT plugins_plugin_id_uq UNIQUE (plugin_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX plugins_status_idx ON plugins (status);

CREATE TABLE plugin_call_metrics (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci PRIMARY KEY NOT NULL,
    plugin_id VARCHAR(64) NOT NULL,
    event_type VARCHAR(64) NOT NULL,
    result VARCHAR(16) NOT NULL,
    error_class VARCHAR(64) NULL,
    policy_revision BIGINT NOT NULL,
    latency_ms BIGINT NULL,
    occurred_at BIGINT NOT NULL,
    CONSTRAINT plugin_call_metrics_result_ck CHECK (result IN ('ok', 'error', 'timeout', 'repeat', 'stale', 'skipped')),
    CONSTRAINT plugin_call_metrics_plugin_fk FOREIGN KEY (plugin_id) REFERENCES plugins (plugin_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX plugin_call_metrics_plugin_idx ON plugin_call_metrics (plugin_id, occurred_at);

CREATE TABLE plugin_data (
    plugin_id VARCHAR(64) NOT NULL,
    key VARCHAR(128) NOT NULL,
    value_json TEXT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (plugin_id, key),
    CONSTRAINT plugin_data_plugin_fk FOREIGN KEY (plugin_id) REFERENCES plugins (plugin_id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;
