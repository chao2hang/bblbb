-- BBLBB drafts data model (M04-SCHEMA-03, MariaDB)
--
-- drafts: standalone draft resource (OpenAPI Draft, separate from posts) —
-- owner, optional board, article/discussion type, title, Markdown body,
-- visibility level / access policy, scheduled publish time, optimistic
-- version, soft delete.
--
-- Indexes: owner cursor list (owner_id, deleted_at, updated_at) and scheduled
-- publish job scan (scheduled_at).
--
-- Constraints: board is NULLable (drafts may be created without a board);
-- board delete sets draft board to NULL (does not cascade); owner delete
-- cascades.

CREATE TABLE drafts (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    owner_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
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
