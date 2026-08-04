-- BBLBB schema skeleton only, targeting MySQL 8.0+. Expand via new migrations.
CREATE TABLE users (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    username_normalized VARCHAR(255) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    email_normalized VARCHAR(320) CHARACTER SET utf8mb4 COLLATE utf8mb4_bin NOT NULL,
    password_hash VARCHAR(255) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    status VARCHAR(32) CHARACTER SET ascii COLLATE ascii_bin NOT NULL DEFAULT 'pending',
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY users_username_normalized_uq (username_normalized),
    UNIQUE KEY users_email_normalized_uq (email_normalized),
    CONSTRAINT users_status_ck CHECK (status IN ('pending', 'active', 'restricted', 'banned', 'pending_delete', 'deleted'))
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;

CREATE TABLE user_sessions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    token_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    csrf_secret_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    last_seen_at BIGINT NOT NULL,
    idle_expires_at BIGINT NOT NULL,
    absolute_expires_at BIGINT NOT NULL,
    revoked_at BIGINT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY user_sessions_token_hash_uq (token_hash),
    KEY user_sessions_user_id_idx (user_id),
    CONSTRAINT user_sessions_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;
