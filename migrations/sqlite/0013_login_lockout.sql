-- BBLBB 登录锁定迁移（M02-SESSION-03）
-- users 增加连续失败计数与登录锁定截止：
--   failed_login_count  连续登录失败次数（成功登录重置为 0）
--   locked_until        登录锁定截止（Unix 毫秒，NULL = 未锁定）
-- SECURITY.md §16：每账号连续失败 5 次短时锁定（10 分钟）

ALTER TABLE users ADD COLUMN failed_login_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN locked_until INTEGER;
