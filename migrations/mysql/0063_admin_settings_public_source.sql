-- BBLBB 系统设置公开源字段（管理台系统设置页原型对齐，MySQL）
--
-- site_settings.public_source：公开源（RSS / API）地址——站点对外宣称的
-- http(s):// 公开访问源（原型 prototype/pages/admin-settings.html 的
-- data-sys-source 字段）。应用层校验：非空时必须是合法 http(s) URL 且
-- ≤200 字符（PATCH 时必填）。单例行种子为原型默认值 https://bblbb.local，
-- 部署后应由管理员改为真实公开源。

ALTER TABLE site_settings ADD COLUMN public_source VARCHAR(200) NOT NULL DEFAULT '';

UPDATE site_settings SET public_source = 'https://bblbb.local' WHERE id = 'singleton';
