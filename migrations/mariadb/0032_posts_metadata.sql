-- BBLBB posts metadata expansion (M04-SCHEMA-01, MariaDB)
--
-- posts gains: article/discussion type, board-unique slug, optimistic concurrency
-- version, scheduled/published/pinned/featured/closed timestamps, SEO fields,
-- deleted_at and last_reply cache key.
--
-- Compatibility: content/content_format/visibility/pinned/last_reply_by remain
-- as 0003 skeleton columns (collected into post_contents/access_policy when
-- M04-POSTS replaces the skeleton routes); status CHECK stays at the 0003 value
-- set ('locked' is a legacy value, new code uses closed_at; pending_review/
-- rejected status values land with the M04-POSTS migration).

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

-- Board-unique slug (articles must have one; drafts may be NULL, multiple NULLs
-- do not collide).
CREATE UNIQUE INDEX posts_board_slug_uq ON posts (board_id, slug);
CREATE INDEX posts_board_status_idx ON posts (board_id, status, pinned_at, last_reply_at);
CREATE INDEX posts_author_status_idx ON posts (author_id, status, created_at);
CREATE INDEX posts_type_status_published_idx ON posts (post_type, status, published_at);
CREATE INDEX posts_scheduled_idx ON posts (scheduled_at);
