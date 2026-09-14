-- BBLBB 平台治理底座（P0 整改，SQLite）
--
-- 1) bootstrap_tokens：首管理员一次性引导令牌（只存 SHA-256 哈希；
--    消费一次后永久失效，docs/OPERATIONS.md §15 / AUTHORIZATION.md §10）；
-- 2) feature_flags：可选能力运行时开关的持久化事实来源（替代
--    config.feature_flags() 进程内启动快照；admin API GET/PATCH + 审计）；
-- 3) shop_site_config：商城站点配置单行表（version 乐观并发；替代
--    admin_shop.rs 固定返回/丢弃 PATCH 的假持久化）。
--
-- 时间一律 Unix 毫秒（M01-DB-08）。

CREATE TABLE bootstrap_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    token_hash TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER,
    consumed_by TEXT,
    CONSTRAINT bootstrap_tokens_hash_uq UNIQUE (token_hash)
);

CREATE INDEX bootstrap_tokens_created_idx ON bootstrap_tokens (created_at);

CREATE TABLE feature_flags (
    name TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    effective_at INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    updated_by TEXT,
    updated_at INTEGER NOT NULL,
    CONSTRAINT feature_flags_name_ck CHECK (name IN ('ai', 'video', 'download_billing', 'oidc', 'marketplace'))
);

INSERT INTO feature_flags (name, enabled, effective_at, version, updated_by, updated_at)
VALUES
    ('ai', 0, 0, 1, NULL, 1722816000000),
    ('video', 0, 0, 1, NULL, 1722816000000),
    ('download_billing', 0, 0, 1, NULL, 1722816000000),
    ('oidc', 0, 0, 1, NULL, 1722816000000),
    ('marketplace', 0, 0, 1, NULL, 1722816000000);

CREATE TABLE shop_site_config (
    id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    max_quantity_per_order INTEGER NOT NULL DEFAULT 100,
    default_refund_policy TEXT NOT NULL DEFAULT 'non_refundable'
        CHECK (default_refund_policy IN ('non_refundable', 'compensation_only', 'full_refund')),
    version INTEGER NOT NULL DEFAULT 1,
    updated_by TEXT,
    updated_at INTEGER NOT NULL,
    CHECK (id = 'singleton')
);

INSERT INTO shop_site_config (id, enabled, max_quantity_per_order, default_refund_policy, version, updated_by, updated_at)
VALUES ('singleton', 1, 100, 'non_refundable', 1, NULL, 1722816000000);
