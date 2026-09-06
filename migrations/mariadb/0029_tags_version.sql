
ALTER TABLE tags ADD COLUMN updated_at BIGINT NOT NULL DEFAULT 0;
UPDATE tags SET updated_at = created_at WHERE updated_at = 0;
