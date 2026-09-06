
CREATE TABLE content_access_grants (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    comment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    policy_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    source_kind VARCHAR(16) NOT NULL,
    source_id VARCHAR(64) NULL,
    point_operation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    grant_target_key VARCHAR(128) NOT NULL,
    granted_at BIGINT NOT NULL,
    revoked_at BIGINT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY content_access_grants_user_target_uq (user_id, grant_target_key),
    CONSTRAINT content_access_grants_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT content_access_grants_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    CONSTRAINT content_access_grants_comment_fk FOREIGN KEY (comment_id) REFERENCES comments (id) ON DELETE CASCADE,
    CONSTRAINT content_access_grants_policy_fk FOREIGN KEY (policy_id) REFERENCES content_access_policies (id) ON DELETE CASCADE,
    CONSTRAINT content_access_grants_source_kind_ck CHECK (source_kind IN ('reply', 'purchase', 'moderator', 'import'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX content_access_grants_user_idx ON content_access_grants (user_id);
CREATE INDEX content_access_grants_target_idx ON content_access_grants (grant_target_key);
