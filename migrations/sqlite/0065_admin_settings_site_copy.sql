-- BBLBB 系统设置站点文案（全站文案统一，SQLite）
--
-- site_settings 增加站点文案列（空串默认 = 前端内置通用文案兜底）：
-- - site_description: 站点描述（layout meta description、登录/注册说明兜底）
-- - login_eyebrow: 登录页眉题（默认渲染 WELCOME BACK）
-- - login_title: 登录页标题（默认渲染「登录 {站点名称}」）
-- - login_subtitle: 登录页说明文案
-- - register_eyebrow: 注册页眉题（默认渲染 JOIN {站点名称}）
-- - register_title: 注册页标题（默认渲染「创建账号」）
-- - register_subtitle: 注册页说明文案
-- 公开只读投影见 GET /api/v1/site（不含 SMTP/开关等运营字段）。

ALTER TABLE site_settings ADD COLUMN site_description TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN login_eyebrow TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN login_title TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN login_subtitle TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN register_eyebrow TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN register_title TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN register_subtitle TEXT NOT NULL DEFAULT '';
