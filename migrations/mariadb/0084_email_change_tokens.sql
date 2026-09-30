-- 换绑邮箱（GA 用户请求：settings 显示验证状态 + 支持修改绑定邮箱）：
-- 申请时存新邮箱 + token（hash 校验 + enc1 密文供邮件投递时解密渲染确认链接）；
-- 新邮箱确认前不写 users.email_normalized（收件人经 params.to_email 覆盖定向新邮箱）。
CREATE TABLE email_change_tokens (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    new_email VARCHAR(320) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL,
    token_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    token_encrypted TEXT NULL,
    expires_at BIGINT NOT NULL,
    consumed_at BIGINT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    KEY email_change_tokens_user_idx (user_id),
    KEY email_change_tokens_hash_idx (token_hash),
    CONSTRAINT email_change_tokens_user_fk FOREIGN KEY (user_id) REFERENCES users (id)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;
