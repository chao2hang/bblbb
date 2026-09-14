-- BBLBB Passkey 迁移（M02-MFA-PK，SQLite）
-- Passkey（WebAuthn/FIDO2）作为 MFA 第二步的第三选项，与 TOTP/恢复码共存。
-- 结构与 mysql/mariadb 同版本同语义；差异：TEXT 主键、BLOB 凭据字节、
-- INTEGER 布尔（见 0015 的方言约定）。

CREATE TABLE passkey_credentials (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    credential_id BLOB NOT NULL,
    credential_json TEXT NOT NULL,
    aaguid TEXT,
    backup_eligible INTEGER NOT NULL DEFAULT 0,
    backed_up INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at INTEGER,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX passkey_credentials_cred_uq
    ON passkey_credentials (credential_id);
CREATE INDEX passkey_credentials_user_idx
    ON passkey_credentials (user_id);

CREATE TABLE webauthn_challenges (
    id TEXT PRIMARY KEY NOT NULL,
    purpose TEXT NOT NULL,
    user_id TEXT,
    mfa_challenge_hash TEXT,
    state_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX webauthn_challenges_user_idx
    ON webauthn_challenges (user_id, purpose);
CREATE INDEX webauthn_challenges_mfa_hash_idx
    ON webauthn_challenges (mfa_challenge_hash, purpose);
