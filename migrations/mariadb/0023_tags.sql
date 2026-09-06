
CREATE TABLE tag_groups (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    name VARCHAR(100) NOT NULL,
    slug VARCHAR(100) NOT NULL,
    sort_order INT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY tag_groups_slug_uq (slug)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;

ALTER TABLE tags ADD COLUMN group_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL;
ALTER TABLE tags ADD COLUMN slug VARCHAR(100) NULL;
ALTER TABLE tags ADD COLUMN description VARCHAR(500) NOT NULL DEFAULT '';
ALTER TABLE tags ADD COLUMN color VARCHAR(16) NULL;
ALTER TABLE tags ADD UNIQUE KEY tags_slug_uq (slug);
CREATE INDEX tags_group_id_idx ON tags (group_id);

CREATE TABLE board_tags (
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    tag_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    PRIMARY KEY (board_id, tag_id),
    CONSTRAINT board_tags_board_fk FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    CONSTRAINT board_tags_tag_fk FOREIGN KEY (tag_id) REFERENCES tags (id) ON DELETE CASCADE
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_general_ci;
