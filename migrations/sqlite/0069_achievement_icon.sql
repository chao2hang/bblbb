-- BBLBB achievement icon (SQLite)
--
-- 成就图标（后台配置上传）：achievements.icon_path 保存图标文件名
-- （storage_dir/achievements/ 目录内，见 backend/src/achievements/icon.rs）。
-- 成就图标为运营配置的小型静态资源，刻意不走 S3/附件域：
-- 直写本地磁盘（admin 上传/删除），经 GET /api/v1/achievements/{code}/icon 公开读取。
-- NULL = 未上传图标（前台回退内置图标）。

ALTER TABLE achievements ADD COLUMN icon_path TEXT NULL;
