-- BBLBB AI Gateway schema (M09-SCHEMA, SQLite)
--
-- ai_providers（Provider 与用途策略/预算）、ai_consents（逐次同意，
-- (user,provider,purpose) 唯一）、ai_tasks（异步任务幂等 + 状态机：
-- queued/running/retry_wait/succeeded/cancelled/dead）、ai_suggestions
-- （格式化/SEO/标签/审核建议，schema_version + base_revision 防旧覆盖新）。
-- Secret 不落库（仅 secret_configured + secret_ref 引用受保护 Secret Store）。

CREATE TABLE ai_providers (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    adapter_type TEXT NOT NULL CHECK (adapter_type IN ('openai_compatible', 'anthropic', 'custom')),
    base_url TEXT NOT NULL,
    api_type TEXT NOT NULL,
    default_model TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'disabled' CHECK (status IN ('enabled', 'disabled')),
    secret_configured INTEGER NOT NULL DEFAULT 0,
    secret_ref TEXT NULL,
    timeout_ms INTEGER NOT NULL DEFAULT 15000,
    max_input_tokens INTEGER NOT NULL DEFAULT 8000,
    max_output_tokens INTEGER NOT NULL DEFAULT 2000,
    max_concurrency INTEGER NOT NULL DEFAULT 4,
    data_mode TEXT NOT NULL DEFAULT 'redacted'
        CHECK (data_mode IN ('disabled', 'metadata_only', 'redacted', 'full_with_consent')),
    retention_days INTEGER NULL,
    training_disclosure INTEGER NOT NULL DEFAULT 0,
    region TEXT NULL,
    purpose_budgets_json TEXT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT ai_providers_name_uq UNIQUE (name),
    CONSTRAINT ai_providers_timeout_ck CHECK (timeout_ms > 0)
);

CREATE TABLE ai_consents (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    purpose TEXT NOT NULL CHECK (purpose IN ('formatting', 'moderation', 'seo', 'tagging')),
    data_mode TEXT NOT NULL CHECK (data_mode IN ('full_with_consent')),
    disclosure_version INTEGER NOT NULL,
    disclosure_hash TEXT NOT NULL,
    disclosure_text TEXT NOT NULL,
    scope TEXT NOT NULL DEFAULT 'per_task',
    granted_at INTEGER NOT NULL,
    revoked_at INTEGER NULL,
    revoke_reason TEXT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT ai_consents_user_provider_purpose_uq UNIQUE (user_id, provider_id, purpose),
    CONSTRAINT ai_consents_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT ai_consents_provider_fk FOREIGN KEY (provider_id) REFERENCES ai_providers (id) ON DELETE CASCADE,
    CONSTRAINT ai_consents_version_ck CHECK (disclosure_version >= 1)
);

CREATE INDEX ai_consents_user_idx ON ai_consents (user_id, granted_at);

CREATE TABLE ai_tasks (
    id TEXT PRIMARY KEY NOT NULL,
    task_type TEXT NOT NULL CHECK (task_type IN ('formatting', 'moderation', 'seo', 'tagging')),
    target_type TEXT NOT NULL CHECK (target_type IN ('draft', 'post')),
    target_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    content_revision INTEGER NOT NULL,
    policy_version INTEGER NOT NULL,
    consent_id TEXT NULL,
    status TEXT NOT NULL DEFAULT 'queued'
        CHECK (status IN ('queued', 'running', 'retry_wait', 'succeeded', 'cancelled', 'dead')),
    attempt INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    error_class TEXT NULL,
    error_message_safe TEXT NULL,
    input_hash TEXT NULL,
    output_hash TEXT NULL,
    budget_reserved_tokens INTEGER NOT NULL DEFAULT 0,
    idempotency_key TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    result_json TEXT NULL,
    started_at INTEGER NULL,
    finished_at INTEGER NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT ai_tasks_target_key_uq UNIQUE (task_type, target_type, target_id, content_revision, idempotency_key),
    CONSTRAINT ai_tasks_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT ai_tasks_provider_fk FOREIGN KEY (provider_id) REFERENCES ai_providers (id) ON DELETE RESTRICT,
    CONSTRAINT ai_tasks_revision_ck CHECK (content_revision >= 0)
);

CREATE INDEX ai_tasks_user_status_idx ON ai_tasks (user_id, status, created_at);
CREATE INDEX ai_tasks_provider_status_idx ON ai_tasks (provider_id, status);

CREATE TABLE ai_suggestions (
    id TEXT PRIMARY KEY NOT NULL,
    task_id TEXT NOT NULL,
    suggestion_type TEXT NOT NULL CHECK (suggestion_type IN ('formatting', 'seo', 'tagging', 'moderation')),
    target_type TEXT NOT NULL CHECK (target_type IN ('draft', 'post')),
    target_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    schema_version INTEGER NOT NULL,
    base_revision INTEGER NOT NULL,
    payload_json TEXT NOT NULL,
    decision TEXT NOT NULL DEFAULT 'pending' CHECK (decision IN ('pending', 'accepted', 'rejected', 'stale')),
    accepted_fields_json TEXT NULL,
    accepted_at INTEGER NULL,
    accepted_by TEXT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT ai_suggestions_task_fk FOREIGN KEY (task_id) REFERENCES ai_tasks (id) ON DELETE CASCADE,
    CONSTRAINT ai_suggestions_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT ai_suggestions_schema_version_ck CHECK (schema_version >= 1)
);

CREATE INDEX ai_suggestions_target_idx ON ai_suggestions (target_type, target_id, decision);
