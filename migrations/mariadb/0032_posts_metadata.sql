
ALTER TABLE posts
    ADD COLUMN post_type VARCHAR(16) NOT NULL DEFAULT 'discussion',
    ADD COLUMN slug VARCHAR(200) NULL,
    ADD COLUMN excerpt TEXT NULL,
    ADD COLUMN version INT NOT NULL DEFAULT 1,
    ADD COLUMN scheduled_at BIGINT NULL,
    ADD COLUMN published_at BIGINT NULL,
    ADD COLUMN pinned_at BIGINT NULL,
    ADD COLUMN featured_at BIGINT NULL,
    ADD COLUMN closed_at BIGINT NULL,
    ADD COLUMN canonical_url TEXT NULL,
    ADD COLUMN seo_title VARCHAR(200) NULL,
    ADD COLUMN seo_description TEXT NULL,
    ADD COLUMN last_reply_id CHAR(36) NULL,
    ADD COLUMN deleted_at BIGINT NULL,
    ADD CONSTRAINT posts_post_type_ck CHECK (post_type IN ('article', 'discussion'));

CREATE UNIQUE INDEX posts_board_slug_uq ON posts (board_id, slug);
CREATE INDEX posts_board_status_idx ON posts (board_id, status, pinned_at, last_reply_at);
CREATE INDEX posts_author_status_idx ON posts (author_id, status, created_at);
CREATE INDEX posts_type_status_published_idx ON posts (post_type, status, published_at);
CREATE INDEX posts_scheduled_idx ON posts (scheduled_at);
