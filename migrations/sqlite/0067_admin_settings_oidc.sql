-- BBLBB admin settings OIDC columns (SQLite)
--
-- 在 site_settings 表增加第三方登录（OIDC）配置列：
-- - google_auth_enabled: 是否启用 Google 登录（0/1）
-- - google_client_id: Google OAuth Client ID
-- - google_client_secret: Google OAuth Client Secret
-- - github_auth_enabled: 是否启用 GitHub 登录（0/1）
-- - github_client_id: GitHub OAuth Client ID
-- - github_client_secret: GitHub OAuth Client Secret
--
-- 说明：GET /api/v1/admin/settings 与 /api/v1/site 的读取投影
-- （admin_ext::load_site_settings）已引用这些列；本迁移补齐列定义，
-- 使管理后台设置页与登录页第三方登录入口正常工作。

ALTER TABLE site_settings ADD COLUMN google_auth_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE site_settings ADD COLUMN google_client_id TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN google_client_secret TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN github_auth_enabled INTEGER NOT NULL DEFAULT 0;
ALTER TABLE site_settings ADD COLUMN github_client_id TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN github_client_secret TEXT NOT NULL DEFAULT '';
