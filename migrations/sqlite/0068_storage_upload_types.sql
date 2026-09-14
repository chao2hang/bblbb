-- BBLBB storage upload types policy (SQLite)
--
-- 在 site_settings 增加站点上传类型策略（M06-UPLOAD-05 白名单的类目开关）：
-- - storage_allowed_upload_types: 启用的上传类型类目 CSV（image/pdf/text/office/av）。
--   空串 = 全部类目启用（缺省基线）。未知类目在读取时忽略；
--   解析为空时回退全部启用（能力白名单始终是安全下限）。

ALTER TABLE site_settings ADD COLUMN storage_allowed_upload_types TEXT NOT NULL DEFAULT '';
