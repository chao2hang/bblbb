-- BBLBB tags disabled state (M03-BOARDS-06)
-- is_active=1 enabled (default); 0 disabled → excluded from public projections
-- (listTags), history and existing links kept (mirrors boards.is_active).

ALTER TABLE tags ADD COLUMN is_active TINYINT NOT NULL DEFAULT 1;
