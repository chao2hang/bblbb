-- BBLBB users 乐观并发版本列（M03-PROFILE-04）
-- PATCH /api/v1/me 的 If-Match 版本来源：每次资料更新 version+1，
-- 版本过期 → 409 version_conflict（ERROR-CODES.md）。与 OpenAPI
-- ResourceMeta.version（minimum 1）对应；默认 1。

ALTER TABLE users ADD COLUMN version INTEGER NOT NULL DEFAULT 1;
