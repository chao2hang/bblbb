-- BBLBB two-step login MFA challenge migration (MariaDB)
-- For users with TOTP enabled: after password verification a one-time
-- challenge token is issued (SHA-256 hash only, 5-minute expiry); the
-- second login step POST /api/v1/auth/login/mfa submits a TOTP code or
-- recovery code to complete login; atomic consumption prevents replay.

CREATE TABLE mfa_login_challenges (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    user_id VARCHAR(36) NOT NULL,
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX mfa_login_challenges_user_idx ON mfa_login_challenges (user_id);
