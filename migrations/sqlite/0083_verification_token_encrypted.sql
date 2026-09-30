-- 验证/重置 token 明文加密存储（GA P0-2 收尾）：同 mariadb/mysql。
ALTER TABLE email_verification_tokens ADD COLUMN token_encrypted TEXT NULL;
ALTER TABLE password_reset_tokens ADD COLUMN token_encrypted TEXT NULL;
