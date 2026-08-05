-- BBLBB tags optimistic concurrency version (M03-BOARDS-07)
-- tags.updated_at (Unix ms) is the If-Match version (mirrors boards.updated_at);
-- existing rows are initialised to created_at (never updated).

ALTER TABLE tags ADD COLUMN updated_at BIGINT NOT NULL DEFAULT 0;
UPDATE tags SET updated_at = created_at WHERE updated_at = 0;
