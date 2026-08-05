-- BBLBB MFA 迁移（M02-MFA-01）
-- 1) totp_credentials：TOTP enrollment——
--    id                 UUID 主键
--    user_id            用户（一个用户同一时刻至多一个启用中的 TOTP，
--                       由服务层保证；重复启用 = 撤销旧 + 新建）
--    encrypted_secret   加密后的 TOTP secret（AEAD 密文，可解密以验证 code；
--                       不可哈希——验证时需要原始 secret）
--    last_accepted_step 最近接受的 TOTP time-step（防重放：code 的 step 必须
--                       大于该值且在允许时间窗口内，M02-MFA-03）
--    created_at         签发时间（Unix 毫秒）
--    confirmed_at       确认启用时间（NULL = 未完成 enrollment）
--    revoked_at         撤销时间（NULL = 有效）
-- 2) mfa_recovery_codes：恢复码只存 SHA-256 hash（不存明文），
--    消费时原子标记（consumed_at 由 UPDATE WHERE consumed_at IS NULL 保证唯一）

CREATE TABLE totp_credentials (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    encrypted_secret MEDIUMTEXT NOT NULL,
    last_accepted_step BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    confirmed_at BIGINT NULL,
    revoked_at BIGINT NULL,
    PRIMARY KEY (id),
    KEY totp_credentials_user_idx (user_id),
    CONSTRAINT totp_credentials_user_fk
        FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;

CREATE TABLE mfa_recovery_codes (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    code_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY mfa_recovery_codes_hash_uq (code_hash),
    KEY mfa_recovery_codes_user_idx (user_id),
    CONSTRAINT mfa_recovery_codes_user_fk
        FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;
