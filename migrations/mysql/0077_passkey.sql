-- BBLBB Passkey 迁移（M02-MFA-PK）
-- Passkey（WebAuthn/FIDO2）作为 MFA 第二步的第三选项，与 TOTP/恢复码共存：
-- 第二步恰好一个因素通过即签发会话（OR 语义，纯增量，不影响既有 TOTP 路径）。
-- 1) passkey_credentials：WebAuthn 凭据——
--    id              UUID 主键
--    user_id         用户（一个用户可注册多把 Passkey，与 TOTP 互不影响；
--                    用户删除级联清理）
--    name            用户可读标签（如「MacBook 指纹」）
--    credential_id   WebAuthn credential id 原始字节（RP 内唯一，查找键）
--    credential_json webauthn-rs Passkey 结构序列化（公钥 / sign counter /
--                    transports 等；验证时需原始公钥，不可哈希。sign counter
--                    以该 JSON 内为准，update_credential 后整体回写）
--    aaguid          认证器型号 GUID（注册时上报，可为 NULL）
--    backup_eligible 注册时凭据是否声明支持多设备备份
--    backed_up       最近一次断言上报的备份状态（可随使用变化）
--    created_at      注册时间（Unix 毫秒）
--    last_used_at    最近一次断言成功时间（NULL = 尚未使用）
--    revoked_at      撤销时间（NULL = 有效）
-- 2) webauthn_challenges：WebAuthn challenge 状态（注册/登录断言共用）——
--    服务端生成的 challenge 以 webauthn-rs state 序列化保存（不信任客户端
--    回传 challenge），一次性消费 + 短期过期（与 mfa_login_challenges 同策略）
--    purpose            'registration' | 'authentication'
--    user_id            registration 绑定当前会话用户；authentication 为 NULL
--                       （authentication 绑定 mfa_challenge_hash）
--    mfa_challenge_hash authentication 绑定的 mfa_login_challenges.token_hash
--                       （把 WebAuthn challenge 锁定到具体一次两步登录）
--    state_json         webauthn-rs 注册/认证 state 序列化（含 challenge）
--    created_at         签发时间（Unix 毫秒）
--    expires_at         过期时间（Unix 毫秒，5 分钟）
--    consumed_at        消费时间（NULL = 未消费）

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
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_0900_as_cs;

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
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_0900_as_cs;
