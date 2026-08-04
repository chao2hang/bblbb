-- BBLBB identity migration: email verification and password reset tokens (MariaDB)
-- Same as MySQL migration

ALTER TABLE users
    ADD COLUMN email_verified TINYINT NOT NULL DEFAULT 0,
    ADD COLUMN display_name VARCHAR(100),
    ADD COLUMN bio TEXT,
    ADD COLUMN timezone VARCHAR(50) NOT NULL DEFAULT 'UTC';

CREATE TABLE email_verification_tokens (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    token_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY email_verification_tokens_hash_uq (token_hash),
    KEY email_verification_tokens_user_idx (user_id),
    CONSTRAINT email_verification_tokens_user_fk
        FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;

CREATE TABLE password_reset_tokens (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    token_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY password_reset_tokens_hash_uq (token_hash),
    KEY password_reset_tokens_user_idx (user_id),
    CONSTRAINT password_reset_tokens_user_fk
        FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;
