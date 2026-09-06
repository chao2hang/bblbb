
CREATE TABLE drafts (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    owner_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    post_type VARCHAR(16) NOT NULL DEFAULT 'discussion',
    title VARCHAR(240) NOT NULL,
    markdown MEDIUMTEXT NOT NULL,
    visibility_level INT NULL,
    access_policy VARCHAR(32) NULL,
    scheduled_at BIGINT NULL,
    version INT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    deleted_at BIGINT NULL,
    PRIMARY KEY (id),
    CONSTRAINT drafts_owner_fk FOREIGN KEY (owner_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT drafts_board_fk FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE SET NULL,
    CONSTRAINT drafts_post_type_ck CHECK (post_type IN ('article', 'discussion'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX drafts_owner_cursor_idx ON drafts (owner_id, deleted_at, updated_at);
CREATE INDEX drafts_scheduled_idx ON drafts (scheduled_at);
