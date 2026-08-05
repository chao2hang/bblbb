-- BBLBB delete/disable semantics migration (M03-SCHEMA-06)
-- 1) boards soft delete: ADD deleted_at (NULL = active; disable uses the
--    existing is_active=0 from 0003);
-- 2) complete the boards indexes documented in SCHEMA.md §6: (parent_id,
--    sort_order) for hierarchy ordering, (visibility, deleted_at) for
--    visibility filtering and soft-delete exclusion.
-- Application constraints (service layer, M03-AUTHZ/M03-BOARDS):
--   - roles/permissions: is_system=1 cannot be deleted/renamed; deleting a
--     non-system row cascades role_permissions/user_roles/board_roles/
--     board_role_assignments;
--   - boards: is_active=0 disables; deleted_at soft-deletes; hard delete is
--     forbidden while child boards exist (hierarchy integrity); delete
--     cascades board_roles/board_tags and other relations;
--   - assignments: expires_at NULL = permanent, expired = inactive
--     (M03-AUTHZ-03); granted_by is a soft reference, service layer sets it
--     NULL when the grantor is deleted.

ALTER TABLE boards ADD COLUMN deleted_at BIGINT NULL;

CREATE INDEX boards_parent_sort_idx ON boards (parent_id, sort_order);
CREATE INDEX boards_visibility_deleted_idx ON boards (visibility, deleted_at);
