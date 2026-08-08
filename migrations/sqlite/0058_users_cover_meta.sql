-- BBLBB profile cover metadata (M03-PROFILE, SQLite)
--
-- users.cover_attachment_id（0020）之外补充 cover 展示元数据：alt_text
-- （无障碍替代文本，≤300）与 position（封面定位，≤64）。与 mysql/mariadb
-- 同版本同结构（仅注释差异）。

ALTER TABLE users ADD COLUMN cover_alt_text TEXT;
ALTER TABLE users ADD COLUMN cover_position TEXT;
