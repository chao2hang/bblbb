-- BBLBB 标签禁用状态（M03-BOARDS-06）
-- tags.is_active=1 启用（默认）；0 禁用 → 移出公开投影（listTags），保留历史与
-- 既有关联（与 boards.is_active 停用语义一致，SCHEMA.md §6）。
ALTER TABLE tags ADD COLUMN is_active INTEGER NOT NULL DEFAULT 1;
