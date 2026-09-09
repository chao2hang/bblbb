-- BBLBB storage settings persistence (MySQL)
ALTER TABLE site_settings ADD COLUMN storage_backend VARCHAR(16) NOT NULL DEFAULT 'local';
ALTER TABLE site_settings ADD COLUMN storage_local_path VARCHAR(512) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_upload_max_bytes BIGINT NOT NULL DEFAULT 20971520;
ALTER TABLE site_settings ADD COLUMN storage_s3_endpoint VARCHAR(512) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_region VARCHAR(64) NOT NULL DEFAULT 'us-east-1';
ALTER TABLE site_settings ADD COLUMN storage_s3_bucket VARCHAR(128) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_access_key_id VARCHAR(128) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_secret_access_key VARCHAR(256) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_path_style TINYINT NOT NULL DEFAULT 0;
ALTER TABLE site_settings ADD COLUMN storage_s3_public_base_url VARCHAR(512) NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_signed_url_ttl INT NOT NULL DEFAULT 300;
