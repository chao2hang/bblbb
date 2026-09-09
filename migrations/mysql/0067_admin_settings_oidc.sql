-- BBLBB admin settings OIDC columns (MySQL)
ALTER TABLE site_settings ADD COLUMN google_auth_enabled TINYINT NOT NULL DEFAULT 0;
ALTER TABLE site_settings ADD COLUMN google_client_id VARCHAR(256) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN google_client_secret VARCHAR(512) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN github_auth_enabled TINYINT NOT NULL DEFAULT 0;
ALTER TABLE site_settings ADD COLUMN github_client_id VARCHAR(256) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN github_client_secret VARCHAR(512) NOT NULL DEFAULT '';
