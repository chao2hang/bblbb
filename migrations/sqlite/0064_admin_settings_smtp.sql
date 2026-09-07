-- BBLBB 系统设置 SMTP 邮件配置（管理台系统设置扩展，SQLite）
--
-- site_settings 增加 SMTP 发件配置列：
-- - smtp_enabled: 是否启用 SMTP 发件（0=关闭，1=启用）
-- - smtp_host: SMTP 服务器主机名
-- - smtp_port: SMTP 服务器端口（默认 587）
-- - smtp_user: SMTP 认证用户名
-- - smtp_pass: SMTP 认证密码/授权码
-- - smtp_from_email: 发件人邮箱
-- - smtp_from_name: 发件人显示名称
-- - smtp_encryption: 加密模式（'none' | 'starttls' | 'tls'，默认 'starttls'）

ALTER TABLE site_settings ADD COLUMN smtp_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE site_settings ADD COLUMN smtp_host TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN smtp_port INTEGER NOT NULL DEFAULT 587;
ALTER TABLE site_settings ADD COLUMN smtp_user TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN smtp_pass TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN smtp_from_email TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN smtp_from_name TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN smtp_encryption TEXT NOT NULL DEFAULT 'starttls';
