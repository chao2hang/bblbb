
CREATE TABLE appeals (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    sanction_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    message TEXT NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'submitted',
    reviewed_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    decided_at BIGINT NULL,
    submitted_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY appeals_sanction_uq (sanction_id),
    CONSTRAINT appeals_sanction_fk FOREIGN KEY (sanction_id) REFERENCES sanctions (id) ON DELETE CASCADE,
    CONSTRAINT appeals_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT appeals_reviewed_by_fk FOREIGN KEY (reviewed_by) REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT appeals_status_ck CHECK (status IN ('submitted', 'reviewing', 'upheld', 'partially_upheld', 'rejected', 'withdrawn'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX appeals_user_idx ON appeals (user_id, status);
CREATE INDEX appeals_status_idx ON appeals (status);

CREATE TABLE appeal_decisions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    appeal_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    reviewer_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    decision VARCHAR(16) NOT NULL,
    decision_note TEXT NULL,
    conflict_of_interest TEXT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT appeal_decisions_appeal_fk FOREIGN KEY (appeal_id) REFERENCES appeals (id) ON DELETE CASCADE,
    CONSTRAINT appeal_decisions_reviewer_fk FOREIGN KEY (reviewer_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT appeal_decisions_decision_ck CHECK (decision IN ('upheld', 'partially_upheld', 'rejected'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX appeal_decisions_appeal_idx ON appeal_decisions (appeal_id, created_at);
