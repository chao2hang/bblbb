-- BBLBB OIDC Provider schema (M11-OIDC, MySQL)
--
-- oauth_clients / oauth_consents / oauth_authorization_codes /
-- oauth_token_families + oauth_tokens / oauth_signing_keys / oauth_interactions。
-- 高熵 code/token 只存 hash；scope/redirect/client type 封闭约束；密钥加密保存。

CREATE TABLE oauth_clients (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    name VARCHAR(120) NOT NULL,
    client_type VARCHAR(16) NOT NULL,
    client_id VARCHAR(128) NOT NULL,
    client_secret_hash VARCHAR(128) NULL,
    redirect_uris_json TEXT NOT NULL,
    post_logout_uris_json TEXT NULL,
    scopes_json TEXT NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    version BIGINT NOT NULL DEFAULT 1,
    created_by VARCHAR(36) NOT NULL,
    created_at BIGINT NOT NULL,
    updated_by VARCHAR(36) NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT oauth_clients_type_ck CHECK (client_type IN ('public', 'confidential')),
    CONSTRAINT oauth_clients_status_ck CHECK (status IN ('active', 'disabled')),
    CONSTRAINT oauth_clients_client_id_uq UNIQUE (client_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE oauth_consents (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    client_id VARCHAR(36) NOT NULL,
    scope VARCHAR(64) NOT NULL,
    granted_at BIGINT NOT NULL,
    revoked_at BIGINT NULL,
    revoke_reason VARCHAR(500) NULL,
    CONSTRAINT oauth_consents_user_client_scope_uq UNIQUE (user_id, client_id, scope),
    CONSTRAINT oauth_consents_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT oauth_consents_client_fk FOREIGN KEY (client_id) REFERENCES oauth_clients (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX oauth_consents_user_idx ON oauth_consents (user_id, granted_at);

CREATE TABLE oauth_authorization_codes (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    code_hash VARCHAR(128) NOT NULL,
    client_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    redirect_uri VARCHAR(2048) NOT NULL,
    scope VARCHAR(255) NOT NULL,
    nonce VARCHAR(255) NULL,
    state_hash VARCHAR(128) NULL,
    request_hash VARCHAR(128) NULL,
    code_challenge VARCHAR(128) NULL,
    code_challenge_method VARCHAR(16) NOT NULL DEFAULT 'S256',
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    CONSTRAINT oauth_codes_hash_uq UNIQUE (code_hash),
    CONSTRAINT oauth_codes_method_ck CHECK (code_challenge_method IN ('S256')),
    CONSTRAINT oauth_codes_client_fk FOREIGN KEY (client_id) REFERENCES oauth_clients (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX oauth_codes_expiry_idx ON oauth_authorization_codes (expires_at);

CREATE TABLE oauth_token_families (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    client_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    scope VARCHAR(255) NOT NULL,
    created_at BIGINT NOT NULL,
    revoked_at BIGINT NULL,
    revoke_reason VARCHAR(500) NULL,
    CONSTRAINT oauth_families_client_fk FOREIGN KEY (client_id) REFERENCES oauth_clients (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE oauth_tokens (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    family_id VARCHAR(36) NOT NULL,
    access_token_hash VARCHAR(128) NOT NULL,
    refresh_token_hash VARCHAR(128) NULL,
    id_token_jti VARCHAR(128) NULL,
    client_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    scope VARCHAR(255) NOT NULL,
    issued_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    revoked_at BIGINT NULL,
    revoke_reason VARCHAR(500) NULL,
    last_used_at BIGINT NULL,
    CONSTRAINT oauth_tokens_access_hash_uq UNIQUE (access_token_hash),
    CONSTRAINT oauth_tokens_refresh_hash_uq UNIQUE (refresh_token_hash),
    CONSTRAINT oauth_tokens_family_fk FOREIGN KEY (family_id) REFERENCES oauth_token_families (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX oauth_tokens_family_idx ON oauth_tokens (family_id, issued_at);
CREATE INDEX oauth_tokens_client_idx ON oauth_tokens (client_id, user_id);

CREATE TABLE oauth_signing_keys (
    kid VARCHAR(128) PRIMARY KEY NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    private_key_ciphertext TEXT NOT NULL,
    public_jwk_json TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    retired_at BIGINT NULL,
    key_audit_json TEXT NULL,
    CONSTRAINT oauth_keys_status_ck CHECK (status IN ('active', 'retiring'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE TABLE oauth_interactions (
    id VARCHAR(36) PRIMARY KEY NOT NULL,
    client_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    request_hash VARCHAR(128) NOT NULL,
    redirect_uri VARCHAR(2048) NOT NULL,
    scope VARCHAR(255) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'pending',
    decision_at BIGINT NULL,
    expires_at BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    CONSTRAINT oauth_interactions_status_ck CHECK (status IN ('pending', 'approved', 'denied')),
    CONSTRAINT oauth_interactions_client_fk FOREIGN KEY (client_id) REFERENCES oauth_clients (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_unicode_ci;

CREATE INDEX oauth_interactions_user_idx ON oauth_interactions (user_id, status, created_at);
