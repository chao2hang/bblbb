
CREATE TABLE totp_credentials (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    encrypted_secret MEDIUMTEXT NOT NULL,
    last_accepted_step BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    confirmed_at BIGINT NULL,
    revoked_at BIGINT NULL,
    PRIMARY KEY (id),
    KEY totp_credentials_user_idx (user_id),
    CONSTRAINT totp_credentials_user_fk
        FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;

CREATE TABLE mfa_recovery_codes (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    code_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY mfa_recovery_codes_hash_uq (code_hash),
    KEY mfa_recovery_codes_user_idx (user_id),
    CONSTRAINT mfa_recovery_codes_user_fk
        FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;
