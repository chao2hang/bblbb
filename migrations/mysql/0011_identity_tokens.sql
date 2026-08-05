-- BBLBB 身份迁移扩展（M02-IDENTITY-01）
-- users 增加 email_verified_at（邮箱验证时间，Unix 毫秒，可空）。
-- 0002_identity 已提供 email_verified（bool）与 verification/reset token 表；
-- email_verified_at 是权威字段：pending 账号验证成功后写入，
-- 由 email_verified 布尔可推导，二者保持一致。

ALTER TABLE users ADD COLUMN email_verified_at BIGINT NULL;
