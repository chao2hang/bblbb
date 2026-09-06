
ALTER TABLE posts ADD COLUMN review_status VARCHAR(16) NOT NULL DEFAULT 'none'
    CHECK (review_status IN ('none', 'pending_review'));

CREATE INDEX posts_review_status_idx ON posts (review_status);

CREATE TABLE risk_policies (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    version INTEGER NOT NULL,
    thresholds_json MEDIUMTEXT NOT NULL,
    reason VARCHAR(512) NOT NULL,
    updated_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY risk_policies_version_uq (id, version),
    CONSTRAINT risk_policies_version_ck CHECK (version > 0)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE risk_evaluations (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    author_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    verdict VARCHAR(16) NOT NULL,
    reason_category VARCHAR(32) NULL,
    policy_version INTEGER NOT NULL,
    latency_ms INTEGER NOT NULL,
    reviewed_at BIGINT NULL,
    false_positive INTEGER NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT risk_evaluations_verdict_ck CHECK (verdict IN ('allow', 'pending_review')),
    CONSTRAINT risk_evaluations_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    CONSTRAINT risk_evaluations_author_fk FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX risk_evaluations_post_idx ON risk_evaluations (post_id);
CREATE INDEX risk_evaluations_created_idx ON risk_evaluations (created_at);
