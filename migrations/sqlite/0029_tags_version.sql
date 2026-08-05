-- BBLBB 标签乐观并发版本（M03-BOARDS-07）
-- tags.updated_at（Unix 毫秒）作为 If-Match 版本（与 boards.updated_at 语义
-- 一致）；存量行以 created_at 初始化（从未更新）。
ALTER TABLE tags ADD COLUMN updated_at INTEGER NOT NULL DEFAULT 0;
UPDATE tags SET updated_at = created_at WHERE updated_at = 0;
