-- BBLBB content access grants (M04-VISIBILITY-05/06, MariaDB)
--
-- content_access_grants: per-user access grants to posts/comments —
-- 1) after_reply (M04-VISIBILITY-05): a reply grant is written after the user
--    posts a valid visible reply in the topic (reply_grant_persists rule from
--    the frozen spec decides whether it survives deletion/penalty);
-- 2) paid (M04-VISIBILITY-06): a purchase grant is written by M7 after ledger
--    debit succeeds (this milestone only READS grants; debit/grant creation is
--    atomic in M7);
-- 3) moderator/import: explicit admin viewing and data migration.
--
-- Constraints: exactly one of post_id/comment_id (normalized via
-- grant_target_key to avoid cross-DB NULL uniqueness differences); at most one
-- grant per (user_id, grant_target_key), so duplicate requests never double-charge.

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
