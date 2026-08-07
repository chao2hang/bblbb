-- BBLBB storage + download (M06-SCHEMA, MariaDB)
--
-- attachments: owner/backend/object_key/size/hash/media metadata/status/
--   revision/retention. status pending/processing/ready/quarantined/deleted.
--   quota_bytes_charged records the bytes that count against the owner quota
--   (variant/recompute drift guard). S3 signed-url expires_at is a transient
--   response/audit attribute, never stored as attachment lifetime.
-- attachment_links: polymorphic stable references (avatar/cover/post/...).
-- user_quota_counters + quota_policy_revisions: reserved/charged/released bytes
--   and versioned per-level policy.
-- download_billing_policies: site/board/attachment scope, mode disabled/free/
--   fixed/inherit/forced_free/forced_paid, price + limits + version.
-- download_authorizations: per (user, attachment) authorization; charged_amount
--   + currency snapshot; URL re-signed per request, never stored.
-- download_idempotency_records: (scope, user_id, idempotency_key) UNIQUE for
--   replay/no-double-charge.

CREATE TABLE attachments (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    owner_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    storage_backend VARCHAR(16) NOT NULL,
    storage_key VARCHAR(512) NOT NULL,
    original_name VARCHAR(255) NULL,
    media_type VARCHAR(64) NOT NULL,
    size_bytes BIGINT NOT NULL,
    sha256 VARCHAR(64) NOT NULL,
    width INT NULL,
    height INT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    quota_bytes_charged BIGINT NOT NULL DEFAULT 0,
    is_public TINYINT NOT NULL DEFAULT 0,
    ref_count INT NOT NULL DEFAULT 0,
    processing_version INT NOT NULL DEFAULT 0,
    processing_error VARCHAR(255) NULL,
    created_at BIGINT NOT NULL,
    deleted_at BIGINT NULL,
    PRIMARY KEY (id),
    CONSTRAINT attachments_status_ck CHECK (status IN ('pending', 'processing', 'ready', 'quarantined', 'deleted')),
    CONSTRAINT attachments_backend_ck CHECK (storage_backend IN ('local', 's3')),
    CONSTRAINT attachments_owner_fk FOREIGN KEY (owner_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX attachments_owner_status_idx ON attachments (owner_id, status, created_at);
CREATE INDEX attachments_backend_key_idx ON attachments (storage_backend, storage_key);

CREATE TABLE attachment_links (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    attachment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    target_type VARCHAR(32) NOT NULL,
    target_id VARCHAR(64) NOT NULL,
    purpose VARCHAR(32) NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT attachment_links_attachment_fk FOREIGN KEY (attachment_id) REFERENCES attachments (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX attachment_links_target_idx ON attachment_links (target_type, target_id);
CREATE INDEX attachment_links_attachment_idx ON attachment_links (attachment_id);

CREATE TABLE user_quota_counters (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    bytes_reserved BIGINT NOT NULL DEFAULT 0,
    bytes_charged BIGINT NOT NULL DEFAULT 0,
    bytes_released BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (user_id),
    CONSTRAINT user_quota_counters_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT user_quota_counters_reserved_ck CHECK (bytes_reserved >= 0),
    CONSTRAINT user_quota_counters_charged_ck CHECK (bytes_charged >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE quota_policy_revisions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    level INT NOT NULL,
    single_file_max_bytes BIGINT NOT NULL,
    total_bytes BIGINT NOT NULL,
    daily_upload_bytes BIGINT NOT NULL,
    retention_days INT NOT NULL DEFAULT 30,
    policy_version INT NOT NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT quota_policy_revisions_created_by_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX quota_policy_revisions_level_idx ON quota_policy_revisions (level, policy_version);

CREATE TABLE download_billing_policies (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    scope_type VARCHAR(16) NOT NULL,
    scope_id VARCHAR(64) NULL,
    mode VARCHAR(16) NOT NULL,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    amount BIGINT NOT NULL DEFAULT 0,
    authorization_ttl_seconds BIGINT NOT NULL DEFAULT 3600,
    daily_user_limit BIGINT NULL,
    single_charge_limit BIGINT NULL,
    attachment_revenue_limit BIGINT NULL,
    grace_on_disable TINYINT NOT NULL DEFAULT 1,
    version INT NOT NULL DEFAULT 1,
    is_enabled TINYINT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT download_policies_scope_ck CHECK (scope_type IN ('site', 'board', 'attachment')),
    CONSTRAINT download_policies_mode_ck CHECK (mode IN ('disabled', 'free', 'fixed', 'inherit', 'forced_free', 'forced_paid')),
    CONSTRAINT download_policies_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT download_policies_amount_ck CHECK (amount >= 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX download_policies_scope_idx ON download_billing_policies (scope_type, scope_id, is_enabled);

CREATE TABLE download_authorizations (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    attachment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    policy_version INT NOT NULL,
    point_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    charged_amount BIGINT NOT NULL DEFAULT 0,
    currency_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    valid_from BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT download_auth_status_ck CHECK (status IN ('active', 'expired', 'revoked')),
    CONSTRAINT download_auth_attachment_fk FOREIGN KEY (attachment_id) REFERENCES attachments (id) ON DELETE CASCADE,
    CONSTRAINT download_auth_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT download_auth_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX download_auth_user_lookup_idx ON download_authorizations (user_id, attachment_id, status, expires_at);
CREATE UNIQUE INDEX download_auth_operation_uq ON download_authorizations (point_operation_id);

CREATE TABLE download_idempotency_records (
    scope VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    idempotency_key VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    request_hash VARCHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    authorization_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    response_status VARCHAR(32) NOT NULL,
    created_at BIGINT NOT NULL,
    completed_at BIGINT NULL,
    PRIMARY KEY (scope, user_id, idempotency_key),
    CONSTRAINT download_idem_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
