
CREATE TABLE comment_revisions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    comment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    editor_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    body_markdown MEDIUMTEXT NOT NULL,
    body_html MEDIUMTEXT NOT NULL,
    renderer_version VARCHAR(32) NOT NULL,
    change_reason TEXT NULL,
    version INT NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY comment_revisions_comment_version_uq (comment_id, version),
    CONSTRAINT comment_revisions_comment_fk FOREIGN KEY (comment_id) REFERENCES comments (id) ON DELETE CASCADE,
    CONSTRAINT comment_revisions_editor_fk FOREIGN KEY (editor_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX comment_revisions_comment_idx ON comment_revisions (comment_id, version);
CREATE INDEX comment_revisions_editor_idx ON comment_revisions (editor_id);
