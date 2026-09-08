-- BBLBB storage settings persistence (SQLite)
--
-- 在 site_settings 表增加在线存储配置列：
-- - storage_backend: 存储后端类型（'local' 或 's3'）
-- - storage_local_path: 本地存储目录
-- - storage_upload_max_bytes: 上传大小限制（字节）
-- - storage_s3_endpoint: S3 服务端点
-- - storage_s3_region: S3 区域（如 us-east-1 或 auto）
-- - storage_s3_bucket: S3 存储桶名称
-- - storage_s3_access_key_id: S3 访问密钥 ID
-- - storage_s3_secret_access_key: S3 访问私钥
-- - storage_s3_path_style: 是否启用 path-style 寻址（0/1）
-- - storage_s3_public_base_url: 公网访问路径（CDN 或自建网关）
-- - storage_s3_signed_url_ttl: 签名 URL TTL（秒）

ALTER TABLE site_settings ADD COLUMN storage_backend TEXT NOT NULL DEFAULT 'local';
ALTER TABLE site_settings ADD COLUMN storage_local_path TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_upload_max_bytes INTEGER NOT NULL DEFAULT 20971520;
ALTER TABLE site_settings ADD COLUMN storage_s3_endpoint TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_region TEXT NOT NULL DEFAULT 'us-east-1';
ALTER TABLE site_settings ADD COLUMN storage_s3_bucket TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_access_key_id TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_secret_access_key TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_path_style INTEGER NOT NULL DEFAULT 0;
ALTER TABLE site_settings ADD COLUMN storage_s3_public_base_url TEXT NOT NULL DEFAULT '';
ALTER TABLE site_settings ADD COLUMN storage_s3_signed_url_ttl INTEGER NOT NULL DEFAULT 300;
