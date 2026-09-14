-- BBLBB board icon backfill (MySQL)
--
-- 0005 种子板块建库早于 0071（boards.icon），种子数据的 icon 均为 NULL，
-- 前台/管理端只能回退 frontend/src/lib/board-visuals.ts 的 slug 写死映射，
-- 管理端 IconPicker 之外的位置无法感知板块真实图标。
--
-- 本迁移把种子板块的既有视觉持久化进 boards.icon，使 boards.icon 成为
-- 全站板块图标的唯一权威来源（M18）。
-- 仅回填 icon IS NULL 的行：管理员已设置的图标不被覆盖。
-- 图标名与 board-visuals.ts 的 slug 映射一致，回填后前台视觉不变。

UPDATE boards SET icon = 'message-circle' WHERE slug = 'general'  AND icon IS NULL;
UPDATE boards SET icon = 'code'           WHERE slug = 'tech'     AND icon IS NULL;
UPDATE boards SET icon = 'palette'        WHERE slug = 'creative' AND icon IS NULL;
UPDATE boards SET icon = 'search'         WHERE slug = 'help'     AND icon IS NULL;
UPDATE boards SET icon = 'book-open'      WHERE slug = 'news'     AND icon IS NULL;
