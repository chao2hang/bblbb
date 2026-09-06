
ALTER TABLE boards ADD COLUMN deleted_at BIGINT NULL;

CREATE INDEX boards_parent_sort_idx ON boards (parent_id, sort_order);
CREATE INDEX boards_visibility_deleted_idx ON boards (visibility, deleted_at);
