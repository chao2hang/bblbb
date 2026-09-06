
ALTER TABLE posts ADD COLUMN cover_attachment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL;
ALTER TABLE post_tags ADD COLUMN created_at BIGINT NOT NULL DEFAULT 0;

CREATE TABLE post_attachments (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    attachment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    kind VARCHAR(16) NOT NULL DEFAULT 'gallery',
    position INT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT post_attachments_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    CONSTRAINT post_attachments_kind_ck CHECK (kind IN ('cover', 'gallery'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX post_attachments_post_idx ON post_attachments (post_id, position);
