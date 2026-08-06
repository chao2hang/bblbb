-- BBLBB post relations data model (M04-SCHEMA-05, MariaDB)
--
-- 1) posts.cover_attachment_id: cover attachment reference (stores only the
--    attachment UUID; FK lands with the attachments table in M6; remote/signed
--    URLs are forbidden, matching users.cover_attachment_id);
-- 2) post_tags.created_at: tag-link creation time (tag list ordering);
-- 3) post_attachments: post attachment references (gallery/cover) with
--    position ordering; FK to attachments lands in M6.
--
-- The quote-reply association (quoted_comment_id) landed in M04-SCHEMA-04.

ALTER TABLE posts ADD COLUMN cover_attachment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL;
ALTER TABLE post_tags ADD COLUMN created_at BIGINT NOT NULL DEFAULT 0;

CREATE TABLE post_attachments (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    attachment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    kind VARCHAR(16) NOT NULL DEFAULT 'gallery',
    position INT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT post_attachments_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    CONSTRAINT post_attachments_kind_ck CHECK (kind IN ('cover', 'gallery'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX post_attachments_post_idx ON post_attachments (post_id, position);
