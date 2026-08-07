-- BBLBB AI Gateway schema (M09-SCHEMA, MySQL)
--
-- ai_providers（Provider 与用途策略/预算）、ai_consents（逐次同意，
-- (user,provider,purpose) 唯一）、ai_tasks（异步任务幂等 + 状态机）、
-- ai_suggestions（schema_version + base_revision 防旧覆盖新）。
-- Secret 不落库（仅 secret_configured + secret_ref）。

CREATE TABLE ai_providers (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    name VARCHAR(120) NOT NULL,
    adapter_type VARCHAR(24) NOT NULL,
    base_url VARCHAR(512) NOT NULL,
    api_type VARCHAR(64) NOT NULL,
    default_model VARCHAR(128) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'disabled',
    secret_configured TINYINT(1) NOT NULL DEFAULT 0,
    secret_ref VARCHAR(255) NULL,
    timeout_ms BIGINT NOT NULL DEFAULT 15000,
    max_input_tokens BIGINT NOT NULL DEFAULT 8000,
    max_output_tokens BIGINT NOT NULL DEFAULT 2000,
    max_concurrency BIGINT NOT NULL DEFAULT 4,
    data_mode VARCHAR(24) NOT NULL DEFAULT 'redacted',
    retention_days BIGINT NULL,
    training_disclosure TINYINT(1) NOT NULL DEFAULT 0,
    region VARCHAR(64) NULL,
    purpose_budgets_json TEXT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT ai_providers_name_uq UNIQUE (name),
    CONSTRAINT ai_providers_adapter_ck CHECK (adapter_type IN ('openai_compatible', 'anthropic', 'custom')),
    CONSTRAINT ai_providers_status_ck CHECK (status IN ('enabled', 'disabled')),
    CONSTRAINT ai_providers_mode_ck CHECK (data_mode IN ('disabled', 'metadata_only', 'redacted', 'full_with_consent')),
    CONSTRAINT ai_providers_timeout_ck CHECK (timeout_ms > 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE ai_consents (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    provider_id VARCHAR(36) NOT NULL,
    purpose VARCHAR(24) NOT NULL,
    data_mode VARCHAR(24) NOT NULL,
    disclosure_version BIGINT NOT NULL,
    disclosure_hash VARCHAR(64) NOT NULL,
    disclosure_text TEXT NOT NULL,
    scope VARCHAR(24) NOT NULL DEFAULT 'per_task',
    granted_at BIGINT NOT NULL,
    revoked_at BIGINT NULL,
    revoke_reason VARCHAR(500) NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT ai_consents_user_provider_purpose_uq UNIQUE (user_id, provider_id, purpose),
    CONSTRAINT ai_consents_purpose_ck CHECK (purpose IN ('formatting', 'moderation', 'seo', 'tagging')),
    CONSTRAINT ai_consents_mode_ck CHECK (data_mode IN ('full_with_consent')),
    CONSTRAINT ai_consents_version_ck CHECK (disclosure_version >= 1),
    CONSTRAINT ai_consents_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT ai_consents_provider_fk FOREIGN KEY (provider_id) REFERENCES ai_providers (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX ai_consents_user_idx ON ai_consents (user_id, granted_at);

CREATE TABLE ai_tasks (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    task_type VARCHAR(24) NOT NULL,
    target_type VARCHAR(16) NOT NULL,
    target_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    provider_id VARCHAR(36) NOT NULL,
    content_revision BIGINT NOT NULL,
    policy_version BIGINT NOT NULL,
    consent_id VARCHAR(36) NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'queued',
    attempt BIGINT NOT NULL DEFAULT 0,
    max_attempts BIGINT NOT NULL DEFAULT 3,
    error_class VARCHAR(64) NULL,
    error_message_safe VARCHAR(500) NULL,
    input_hash VARCHAR(64) NULL,
    output_hash VARCHAR(64) NULL,
    budget_reserved_tokens BIGINT NOT NULL DEFAULT 0,
    idempotency_key VARCHAR(128) NOT NULL,
    request_hash VARCHAR(64) NOT NULL,
    result_json TEXT NULL,
    started_at BIGINT NULL,
    finished_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT ai_tasks_target_key_uq UNIQUE (task_type, target_type, target_id, content_revision, idempotency_key),
    CONSTRAINT ai_tasks_type_ck CHECK (task_type IN ('formatting', 'moderation', 'seo', 'tagging')),
    CONSTRAINT ai_tasks_target_ck CHECK (target_type IN ('draft', 'post')),
    CONSTRAINT ai_tasks_status_ck CHECK (status IN ('queued', 'running', 'retry_wait', 'succeeded', 'cancelled', 'dead')),
    CONSTRAINT ai_tasks_revision_ck CHECK (content_revision >= 0),
    CONSTRAINT ai_tasks_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT ai_tasks_provider_fk FOREIGN KEY (provider_id) REFERENCES ai_providers (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX ai_tasks_user_status_idx ON ai_tasks (user_id, status, created_at);
CREATE INDEX ai_tasks_provider_status_idx ON ai_tasks (provider_id, status);

CREATE TABLE ai_suggestions (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    task_id VARCHAR(36) NOT NULL,
    suggestion_type VARCHAR(24) NOT NULL,
    target_type VARCHAR(16) NOT NULL,
    target_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    schema_version BIGINT NOT NULL,
    base_revision BIGINT NOT NULL,
    payload_json TEXT NOT NULL,
    decision VARCHAR(16) NOT NULL DEFAULT 'pending',
    accepted_fields_json TEXT NULL,
    accepted_at BIGINT NULL,
    accepted_by VARCHAR(36) NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT ai_suggestions_type_ck CHECK (suggestion_type IN ('formatting', 'seo', 'tagging', 'moderation')),
    CONSTRAINT ai_suggestions_target_ck CHECK (target_type IN ('draft', 'post')),
    CONSTRAINT ai_suggestions_decision_ck CHECK (decision IN ('pending', 'accepted', 'rejected', 'stale')),
    CONSTRAINT ai_suggestions_schema_version_ck CHECK (schema_version >= 1),
    CONSTRAINT ai_suggestions_task_fk FOREIGN KEY (task_id) REFERENCES ai_tasks (id) ON DELETE CASCADE,
    CONSTRAINT ai_suggestions_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX ai_suggestions_target_idx ON ai_suggestions (target_type, target_id, decision);
