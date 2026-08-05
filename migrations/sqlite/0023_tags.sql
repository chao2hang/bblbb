-- BBLBB 标签数据模型迁移（M03-SCHEMA-05）
-- 1) tag_groups：标签分组（slug 全局唯一，sort_order 排序）；
-- 2) tags 演进（0003 骨架：id/name/usage_count/created_at + name 唯一）：
--    新增 group_id（软引用 tag_groups，ALTER 不能带 FK，见 SCHEMA.md）、
--    slug（可空，非空时全局唯一；存量行为 NULL，服务层写入时必填）、
--    description 与 color；usage_count 保留为可重建缓存（listTags 已读取）；
-- 3) board_tags：板块启用的标签关联（复合主键 (board_id, tag_id)，删板块/
--    标签级联；与 SCHEMA-04 board_roles 的"板块启用角色"对称）。

CREATE TABLE tag_groups (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    slug TEXT NOT NULL UNIQUE,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL
);

ALTER TABLE tags ADD COLUMN group_id TEXT;
ALTER TABLE tags ADD COLUMN slug TEXT;
ALTER TABLE tags ADD COLUMN description TEXT NOT NULL DEFAULT '';
ALTER TABLE tags ADD COLUMN color TEXT;

CREATE UNIQUE INDEX tags_slug_uq ON tags (slug);
CREATE INDEX tags_group_id_idx ON tags (group_id);

CREATE TABLE board_tags (
    board_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (board_id, tag_id),
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags (id) ON DELETE CASCADE
);
