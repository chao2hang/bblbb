-- BBLBB comments metadata expansion (M04-SCHEMA-04, MariaDB)
--
-- comments gains: quoted_comment_id (quote reply; SET NULL on delete keeps the
-- deleted-placeholder semantics), optimistic concurrency version, soft-delete
-- time.
--
-- Compatibility: content/content_format remain 0003 skeleton columns (collected
-- when M04-COMMENTS replaces the skeleton); floor is the post-scoped floor
-- number (existing column; uniqueness constraint lands with M04-SCHEMA-07);
-- status CHECK stays at the 0003 value set (published/hidden/deleted; pending
-- review state lands with the M04-POSTS migration).

ALTER TABLE comments
    ADD COLUMN quoted_comment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    ADD COLUMN version INT NOT NULL DEFAULT 1,
    ADD COLUMN deleted_at BIGINT NULL,
    ADD CONSTRAINT comments_quoted_fk FOREIGN KEY (quoted_comment_id) REFERENCES comments (id) ON DELETE SET NULL;

CREATE INDEX comments_quoted_idx ON comments (quoted_comment_id);
