-- BBLBB storage upload types policy (MariaDB)
--
-- site_settings.storage_allowed_upload_types：启用的上传类型类目 CSV
-- （image/pdf/text/office/av）。空串 = 全部启用。

ALTER TABLE site_settings ADD COLUMN storage_allowed_upload_types VARCHAR(128) NOT NULL DEFAULT '';
