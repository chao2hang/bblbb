-- BBLBB achievement icon (MariaDB)
--
-- achievements.icon_path：成就图标文件名（storage_dir/achievements/ 目录内，
-- 见 backend/src/achievements/icon.rs）。不走 S3/附件域，直写本地磁盘；
-- NULL = 未上传图标（前台回退内置图标）。

ALTER TABLE achievements ADD COLUMN icon_path VARCHAR(255) NULL;
