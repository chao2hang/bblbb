-- BBLBB avatar/Cover stable attachment_id reference migration (MySQL)
-- users gains cover_attachment_id (aligned with avatar_attachment_id from 0019).
-- Avatar and Cover store only attachment UUIDs (FK lands with the attachments
-- table in M6); remote or signed URLs are forbidden -- format and source
-- validation live in the M3-PROFILE service layer
-- (ProfileCoverSet.attachment_id format: uuid), no cross-DB URL judgment in the DB.

ALTER TABLE users ADD COLUMN cover_attachment_id VARCHAR(36) NULL;
