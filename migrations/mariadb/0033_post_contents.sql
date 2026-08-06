-- BBLBB post contents and revisions data model (M04-SCHEMA-02, MariaDB)
--
-- 1) post_contents: current post body (1:1 with posts) — original Markdown,
--    backend-cleaned public HTML, renderer version (M04-MARKDOWN-05 upgrade
--    triggers re-render), safe excerpt (public, never truncated from hidden
--    body, M04-MARKDOWN-06);
-- 2) post_revisions: immutable revision snapshots (edit history,
--    M04-POSTS-08) — body and restricted-body snapshots, renderer version,
--    change_reason, the post.version this snapshot represents, created_at.
--
-- Constraints: post_contents is 1:1 with posts (post_id PK + cascade delete);
-- revisions are UNIQUE (post_id, version); restricted-body columns are NULLable.

CREATE TABLE post_contents (
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    body_markdown MEDIUMTEXT NOT NULL,
    body_html MEDIUMTEXT NOT NULL,
    restricted_markdown MEDIUMTEXT NULL,
    restricted_html MEDIUMTEXT NULL,
    renderer_version VARCHAR(32) NOT NULL,
    excerpt TEXT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (post_id),
    CONSTRAINT post_contents_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE post_revisions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    editor_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    body_markdown MEDIUMTEXT NOT NULL,
    body_html MEDIUMTEXT NOT NULL,
    restricted_markdown MEDIUMTEXT NULL,
    restricted_html MEDIUMTEXT NULL,
    renderer_version VARCHAR(32) NOT NULL,
    change_reason TEXT NULL,
    version INT NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY post_revisions_post_version_uq (post_id, version),
    CONSTRAINT post_revisions_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    CONSTRAINT post_revisions_editor_fk FOREIGN KEY (editor_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX post_revisions_post_idx ON post_revisions (post_id, version);
CREATE INDEX post_revisions_editor_idx ON post_revisions (editor_id);
