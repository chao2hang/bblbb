-- BBLBB board icon (MariaDB)
--
-- boards.icon：板块图标名（lucide 图标库的 kebab-case 图标名，如 `code`），
-- 由管理端在创建/编辑板块时从图标库（lucide）中选择；服务层校验
-- `[a-z0-9-]` 且 ≤64 字符（backend/src/boards/validation.rs ICON_MAX）。
-- NULL = 未设置图标（前台回退 slug 视觉映射/默认图标，
-- frontend/src/lib/board-visuals.ts）。

ALTER TABLE boards ADD COLUMN icon VARCHAR(64) NULL;
