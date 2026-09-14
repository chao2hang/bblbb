-- BBLBB Passkey 迁移（M02-MFA-PK，MariaDB；与 mysql/0077 同结构，
-- 排序规则差异见 0015：MariaDB 不支持 utf8mb4_0900_as_cs）

CREATE TABLE passkey_credentials (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    name VARCHAR(64) NOT NULL,
    credential_id VARBINARY(1024) NOT NULL,
    credential_json MEDIUMTEXT NOT NULL,
    aaguid CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    backup_eligible TINYINT NOT NULL DEFAULT 0,
    backed_up TINYINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    last_used_at BIGINT NULL,
    revoked_at BIGINT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY passkey_credentials_cred_uq (credential_id),
    KEY passkey_credentials_user_idx (user_id),
    CONSTRAINT passkey_credentials_user_fk
        FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;

CREATE TABLE webauthn_challenges (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    purpose VARCHAR(16) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    mfa_challenge_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    state_json MEDIUMTEXT NOT NULL,
    created_at BIGINT NOT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    PRIMARY KEY (id),
    KEY webauthn_challenges_user_idx (user_id, purpose),
    KEY webauthn_challenges_mfa_hash_idx (mfa_challenge_hash, purpose),
    CONSTRAINT webauthn_challenges_user_fk
        FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;
