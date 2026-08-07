-- BBLBB storage + download (M06-SCHEMA, SQLite)
--
-- 与 mysql/mariadb 同版本同结构：attachments（对象存储元数据/状态机）、
-- attachment_links（多态稳定引用）、user_quota_counters + quota_policy_revisions
-- （reserved/charged/released 字节与版本化等级配额）、download_billing_policies
-- （site/board/attachment 作用域下载计费）、download_authorizations（授权 + 扣费
-- 快照）与 download_idempotency_records（幂等防重复扣费）。

CREATE TABLE attachments (
    id TEXT PRIMARY KEY NOT NULL,
    owner_id TEXT NOT NULL,
    storage_backend TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    original_name TEXT NULL,
    media_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    sha256 TEXT NOT NULL,
    width INTEGER NULL,
    height INTEGER NULL,
    status TEXT NOT NULL DEFAULT 'pending',
    quota_bytes_charged INTEGER NOT NULL DEFAULT 0,
    is_public INTEGER NOT NULL DEFAULT 0,
    ref_count INTEGER NOT NULL DEFAULT 0,
    processing_version INTEGER NOT NULL DEFAULT 0,
    processing_error TEXT NULL,
    created_at INTEGER NOT NULL,
    deleted_at INTEGER NULL,
    CONSTRAINT attachments_status_ck CHECK (status IN ('pending', 'processing', 'ready', 'quarantined', 'deleted')),
    CONSTRAINT attachments_backend_ck CHECK (storage_backend IN ('local', 's3')),
    CONSTRAINT attachments_owner_fk FOREIGN KEY (owner_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX attachments_owner_status_idx ON attachments (owner_id, status, created_at);
CREATE INDEX attachments_backend_key_idx ON attachments (storage_backend, storage_key);

CREATE TABLE attachment_links (
    id TEXT PRIMARY KEY NOT NULL,
    attachment_id TEXT NOT NULL,
    target_type TEXT NOT NULL,
    target_id TEXT NOT NULL,
    purpose TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT attachment_links_attachment_fk FOREIGN KEY (attachment_id) REFERENCES attachments (id) ON DELETE CASCADE
);

CREATE INDEX attachment_links_target_idx ON attachment_links (target_type, target_id);
CREATE INDEX attachment_links_attachment_idx ON attachment_links (attachment_id);

CREATE TABLE user_quota_counters (
    user_id TEXT NOT NULL,
    bytes_reserved INTEGER NOT NULL DEFAULT 0,
    bytes_charged INTEGER NOT NULL DEFAULT 0,
    bytes_released INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id),
    CONSTRAINT user_quota_counters_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT user_quota_counters_reserved_ck CHECK (bytes_reserved >= 0),
    CONSTRAINT user_quota_counters_charged_ck CHECK (bytes_charged >= 0)
);

CREATE TABLE quota_policy_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    level INTEGER NOT NULL,
    single_file_max_bytes INTEGER NOT NULL,
    total_bytes INTEGER NOT NULL,
    daily_upload_bytes INTEGER NOT NULL,
    retention_days INTEGER NOT NULL DEFAULT 30,
    policy_version INTEGER NOT NULL,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT quota_policy_revisions_created_by_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
);

CREATE INDEX quota_policy_revisions_level_idx ON quota_policy_revisions (level, policy_version);

CREATE TABLE download_billing_policies (
    id TEXT PRIMARY KEY NOT NULL,
    scope_type TEXT NOT NULL,
    scope_id TEXT NULL,
    mode TEXT NOT NULL,
    currency_id TEXT NULL,
    amount INTEGER NOT NULL DEFAULT 0,
    authorization_ttl_seconds INTEGER NOT NULL DEFAULT 3600,
    daily_user_limit INTEGER NULL,
    single_charge_limit INTEGER NULL,
    attachment_revenue_limit INTEGER NULL,
    grace_on_disable INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT download_policies_scope_ck CHECK (scope_type IN ('site', 'board', 'attachment')),
    CONSTRAINT download_policies_mode_ck CHECK (mode IN ('disabled', 'free', 'fixed', 'inherit', 'forced_free', 'forced_paid')),
    CONSTRAINT download_policies_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT,
    CONSTRAINT download_policies_amount_ck CHECK (amount >= 0)
);

CREATE INDEX download_policies_scope_idx ON download_billing_policies (scope_type, scope_id, is_enabled);

CREATE TABLE download_authorizations (
    id TEXT PRIMARY KEY NOT NULL,
    attachment_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    policy_version INTEGER NOT NULL,
    point_operation_id TEXT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    charged_amount INTEGER NOT NULL DEFAULT 0,
    currency_id TEXT NULL,
    valid_from INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT download_auth_status_ck CHECK (status IN ('active', 'expired', 'revoked')),
    CONSTRAINT download_auth_attachment_fk FOREIGN KEY (attachment_id) REFERENCES attachments (id) ON DELETE CASCADE,
    CONSTRAINT download_auth_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT download_auth_currency_fk FOREIGN KEY (currency_id) REFERENCES currencies (id) ON DELETE RESTRICT
);

CREATE INDEX download_auth_user_lookup_idx ON download_authorizations (user_id, attachment_id, status, expires_at);
CREATE UNIQUE INDEX download_auth_operation_uq ON download_authorizations (point_operation_id);

CREATE TABLE download_idempotency_records (
    scope TEXT NOT NULL,
    user_id TEXT NOT NULL,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    authorization_id TEXT NULL,
    response_status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    completed_at INTEGER NULL,
    PRIMARY KEY (scope, user_id, idempotency_key),
    CONSTRAINT download_idem_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);
