-- BBLBB tags data model migration (M03-SCHEMA-05)
-- 1) tag_groups: tag groups (globally unique slug, sort_order ordering);
-- 2) tags evolution (0003 skeleton: id/name/usage_count/created_at + unique
--    name): ADD group_id (soft reference to tag_groups; ALTER cannot carry an
--    FK, see SCHEMA.md), slug (nullable, globally unique when non-NULL; NULL
--    for legacy rows, required by the service layer on write), description and
--    color; usage_count stays as the rebuildable cache (read by listTags);
-- 3) board_tags: tags enabled for a board (composite PK (board_id, tag_id),
--    cascade on board/tag delete; symmetric to SCHEMA-04 board_roles).

CREATE TABLE tag_groups (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    sort_order INT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY tag_groups_slug_uq (slug)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;

ALTER TABLE tags ADD COLUMN group_id VARCHAR(36) NULL;
ALTER TABLE tags ADD COLUMN slug VARCHAR(100) NULL;
ALTER TABLE tags ADD COLUMN description VARCHAR(500) NOT NULL DEFAULT '';
ALTER TABLE tags ADD COLUMN color VARCHAR(16) NULL;
ALTER TABLE tags ADD UNIQUE KEY tags_slug_uq (slug);
CREATE INDEX tags_group_id_idx ON tags (group_id);

CREATE TABLE board_tags (
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    tag_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    PRIMARY KEY (board_id, tag_id),
    CONSTRAINT board_tags_board_fk FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    CONSTRAINT board_tags_tag_fk FOREIGN KEY (tag_id) REFERENCES tags (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_bin;
