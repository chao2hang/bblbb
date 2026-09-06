
ALTER TABLE comments
    ADD COLUMN quoted_comment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    ADD COLUMN version INT NOT NULL DEFAULT 1,
    ADD COLUMN deleted_at BIGINT NULL,
    ADD CONSTRAINT comments_quoted_fk FOREIGN KEY (quoted_comment_id) REFERENCES comments (id) ON DELETE SET NULL;

CREATE INDEX comments_quoted_idx ON comments (quoted_comment_id);
