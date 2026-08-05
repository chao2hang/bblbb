-- BBLBB 删除/停用语义迁移（M03-SCHEMA-06）
-- 1) boards 软删除：新增 deleted_at（NULL=活跃；停用另用 is_active=0，0003 已有）；
-- 2) 补齐 SCHEMA.md §6 记录的 boards 索引：(parent_id, sort_order) 层级排序、
--    (visibility, deleted_at) 可见性过滤与软删排除。
-- 应用约束（服务层，M03-AUTHZ/M03-BOARDS 落地）：
--   - roles/permissions：is_system=1 不可删除/改名；非系统删除级联清理
--     role_permissions/user_roles/board_roles/board_role_assignments；
--   - boards：is_active=0 停用；deleted_at 软删除；存在子板块时禁止硬删除
--     （层级完整性）；删除级联 board_roles/board_tags 等关联；
--   - assignments：expires_at 可空=永久，过期按未生效处理（M03-AUTHZ-03）；
--     granted_by 为软引用，授予人删除时由服务层置 NULL。

ALTER TABLE boards ADD COLUMN deleted_at INTEGER;

CREATE INDEX boards_parent_sort_idx ON boards (parent_id, sort_order);
CREATE INDEX boards_visibility_deleted_idx ON boards (visibility, deleted_at);
