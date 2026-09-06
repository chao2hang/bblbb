
CREATE TABLE mfa_login_challenges (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    token_hash VARCHAR(64) NOT NULL UNIQUE,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX mfa_login_challenges_user_idx ON mfa_login_challenges (user_id);
