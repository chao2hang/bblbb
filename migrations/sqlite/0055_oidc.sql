-- BBLBB OIDC Provider schema (M11-OIDC, SQLite)
--
-- oauth_clients（Public/Confidential；secret 只存 hash；redirect/post-logout
-- URI JSON；status active/disabled；版本化）。
-- oauth_consents（逐 Client × 逐 scope；撤销保留记录）。
-- oauth_authorization_codes（高熵 code 只存 hash；PKCE S256；一次性+过期；
-- state/request hash 绑定）。
-- oauth_token_families + oauth_tokens（opaque access/refresh token 只存 hash；
-- refresh rotation 按 family；revoke_reason/usage 时间戳）。
-- oauth_signing_keys（加密私钥密文 + 公开 JWK JSON；active/retiring；
-- 轮换审计）。
-- oauth_interactions（consent/授权交互：request hash 绑定 + pending/approved/denied）。

CREATE TABLE oauth_clients (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    client_type TEXT NOT NULL CHECK (client_type IN ('public', 'confidential')),
    client_id TEXT NOT NULL,
    client_secret_hash TEXT NULL,
    redirect_uris_json TEXT NOT NULL,
    post_logout_uris_json TEXT NULL,
    scopes_json TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'disabled')),
    version INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_by TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    CONSTRAINT oauth_clients_client_id_uq UNIQUE (client_id)
);

CREATE TABLE oauth_consents (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    client_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    granted_at INTEGER NOT NULL,
    revoked_at INTEGER NULL,
    revoke_reason TEXT NULL,
    CONSTRAINT oauth_consents_user_client_scope_uq UNIQUE (user_id, client_id, scope),
    CONSTRAINT oauth_consents_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT oauth_consents_client_fk FOREIGN KEY (client_id) REFERENCES oauth_clients (id) ON DELETE CASCADE
);

CREATE INDEX oauth_consents_user_idx ON oauth_consents (user_id, granted_at);

CREATE TABLE oauth_authorization_codes (
    id TEXT PRIMARY KEY NOT NULL,
    code_hash TEXT NOT NULL,
    client_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    scope TEXT NOT NULL,
    nonce TEXT NULL,
    state_hash TEXT NULL,
    request_hash TEXT NULL,
    code_challenge TEXT NULL,
    code_challenge_method TEXT NOT NULL DEFAULT 'S256',
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT oauth_codes_hash_uq UNIQUE (code_hash),
    CONSTRAINT oauth_codes_client_fk FOREIGN KEY (client_id) REFERENCES oauth_clients (id) ON DELETE CASCADE
);

CREATE INDEX oauth_codes_expiry_idx ON oauth_authorization_codes (expires_at);

CREATE TABLE oauth_token_families (
    id TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    revoked_at INTEGER NULL,
    revoke_reason TEXT NULL,
    CONSTRAINT oauth_families_client_fk FOREIGN KEY (client_id) REFERENCES oauth_clients (id) ON DELETE CASCADE
);

CREATE TABLE oauth_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    family_id TEXT NOT NULL,
    access_token_hash TEXT NOT NULL,
    refresh_token_hash TEXT NULL,
    id_token_jti TEXT NULL,
    client_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    scope TEXT NOT NULL,
    issued_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    revoked_at INTEGER NULL,
    revoke_reason TEXT NULL,
    last_used_at INTEGER NULL,
    CONSTRAINT oauth_tokens_access_hash_uq UNIQUE (access_token_hash),
    CONSTRAINT oauth_tokens_refresh_hash_uq UNIQUE (refresh_token_hash),
    CONSTRAINT oauth_tokens_family_fk FOREIGN KEY (family_id) REFERENCES oauth_token_families (id) ON DELETE CASCADE
);

CREATE INDEX oauth_tokens_family_idx ON oauth_tokens (family_id, issued_at);
CREATE INDEX oauth_tokens_client_idx ON oauth_tokens (client_id, user_id);

CREATE TABLE oauth_signing_keys (
    kid TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'retiring')),
    private_key_ciphertext TEXT NOT NULL,
    public_jwk_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    retired_at INTEGER NULL,
    key_audit_json TEXT NULL
);

CREATE TABLE oauth_interactions (
    id TEXT PRIMARY KEY NOT NULL,
    client_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    request_hash TEXT NOT NULL,
    redirect_uri TEXT NOT NULL,
    scope TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'approved', 'denied')),
    decision_at INTEGER NULL,
    expires_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    CONSTRAINT oauth_interactions_client_fk FOREIGN KEY (client_id) REFERENCES oauth_clients (id) ON DELETE CASCADE
);

CREATE INDEX oauth_interactions_user_idx ON oauth_interactions (user_id, status, created_at);
