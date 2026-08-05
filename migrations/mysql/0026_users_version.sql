-- BBLBB users optimistic-concurrency version column (M03-PROFILE-04)
-- Version source for the PATCH /api/v1/me If-Match header: every profile
-- update increments version; a stale version yields 409 version_conflict
-- (ERROR-CODES.md). Maps to OpenAPI ResourceMeta.version (minimum 1); default 1.

ALTER TABLE users ADD COLUMN version BIGINT NOT NULL DEFAULT 1;
