-- 验证/重置 token 明文加密存储（GA P0-2 收尾）：
-- DB 仍不存明文（token_hash 校验用途不变）；新增密文列（enc1:，settings key
-- 加密）供邮件投递时解密渲染一次性链接。历史行 NULL = 无法投递（token 过期作废）。
ALTER TABLE email_verification_tokens
    ADD COLUMN token_encrypted TEXT NULL;
ALTER TABLE password_reset_tokens
    ADD COLUMN token_encrypted TEXT NULL;
