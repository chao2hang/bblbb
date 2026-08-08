-- BBLBB remember-me session flag (M02-UX-03, MariaDB)
--
-- mfa_login_challenges 增加 remember：两步登录第一步把「记住我」写入
-- challenge，第二步签发会话时按该标志使用延长会话期限（30 天绝对 / 7 天
-- 空闲 vs 默认 7 天绝对 / 30 分钟空闲）。与 sqlite/mysql 同版本同结构
-- （仅注释差异）。

ALTER TABLE mfa_login_challenges ADD COLUMN remember TINYINT NOT NULL DEFAULT 0;
