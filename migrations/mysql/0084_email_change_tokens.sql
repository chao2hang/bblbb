-- 换绑邮箱：同 mariadb/mysql（MySQL 8 无 fk 兼容差异，索引改为内联 KEY 同义）。
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
