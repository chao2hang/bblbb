-- BBLBB platform governance base (P0 remediation, MySQL)
--
-- 1) bootstrap_tokens: one-time first-admin bootstrap token (SHA-256 hash only);
-- 2) feature_flags: persisted runtime feature flags (source of truth);
-- 3) shop_site_config: single-row shop site config (optimistic-lock version).

CREATE TABLE bootstrap_tokens (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    token_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    consumed_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    CONSTRAINT bootstrap_tokens_hash_uq UNIQUE (token_hash)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE INDEX bootstrap_tokens_created_idx ON bootstrap_tokens (created_at);

CREATE TABLE feature_flags (
    name VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    enabled TINYINT(1) NOT NULL DEFAULT 0,
    effective_at BIGINT NOT NULL DEFAULT 0,
    version BIGINT NOT NULL DEFAULT 1,
    updated_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT feature_flags_name_ck CHECK (name IN ('ai', 'video', 'download_billing', 'oidc', 'marketplace'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

INSERT INTO feature_flags (name, enabled, effective_at, version, updated_by, updated_at)
VALUES
    ('ai', 0, 0, 1, NULL, 1722816000000),
    ('video', 0, 0, 1, NULL, 1722816000000),
    ('download_billing', 0, 0, 1, NULL, 1722816000000),
    ('oidc', 0, 0, 1, NULL, 1722816000000),
    ('marketplace', 0, 0, 1, NULL, 1722816000000);

CREATE TABLE shop_site_config (
    id VARCHAR(16) CHARACTER SET ascii COLLATE ascii_bin NOT NULL PRIMARY KEY,
    enabled TINYINT(1) NOT NULL DEFAULT 1,
    max_quantity_per_order BIGINT NOT NULL DEFAULT 100,
    default_refund_policy VARCHAR(32) NOT NULL DEFAULT 'non_refundable',
    version BIGINT NOT NULL DEFAULT 1,
    updated_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT shop_site_config_singleton_ck CHECK (id = 'singleton'),
    CONSTRAINT shop_site_config_refund_ck CHECK (default_refund_policy IN ('non_refundable', 'compensation_only', 'full_refund'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

INSERT INTO shop_site_config (id, enabled, max_quantity_per_order, default_refund_policy, version, updated_by, updated_at)
VALUES ('singleton', 1, 100, 'non_refundable', 1, NULL, 1722816000000);
