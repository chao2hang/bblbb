
CREATE TABLE content_access_policies (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    kind VARCHAR(16) NOT NULL,
    min_level INT NULL,
    currency_id VARCHAR(16) NULL,
    amount INT NULL,
    reply_grant_persists TINYINT NOT NULL DEFAULT 0,
    policy_version INT NOT NULL DEFAULT 1,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT cap_kind_ck CHECK (kind IN ('public', 'logged_in', 'after_reply', 'level', 'paid')),
    CONSTRAINT cap_creator_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

ALTER TABLE posts ADD COLUMN access_policy_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    ADD CONSTRAINT posts_access_policy_fk FOREIGN KEY (access_policy_id) REFERENCES content_access_policies (id) ON DELETE SET NULL;

CREATE INDEX posts_access_policy_idx ON posts (access_policy_id);
