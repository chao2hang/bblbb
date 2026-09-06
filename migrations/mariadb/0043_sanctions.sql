
CREATE TABLE sanctions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    kind VARCHAR(16) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'scheduled',
    reason TEXT NULL,
    starts_at BIGINT NOT NULL,
    ends_at BIGINT NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    revoked_at BIGINT NULL,
    revoked_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    revoke_reason TEXT NULL,
    PRIMARY KEY (id),
    CONSTRAINT sanctions_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT sanctions_board_fk FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    CONSTRAINT sanctions_creator_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT sanctions_revoked_by_fk FOREIGN KEY (revoked_by) REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT sanctions_kind_ck CHECK (kind IN ('warning', 'rate_limit', 'mute', 'board_mute', 'ban')),
    CONSTRAINT sanctions_status_ck CHECK (status IN ('scheduled', 'active', 'expired', 'revoked')),
    CONSTRAINT sanctions_timeline_ck CHECK (ends_at IS NULL OR ends_at > starts_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX sanctions_user_status_idx ON sanctions (user_id, status);
CREATE INDEX sanctions_board_idx ON sanctions (board_id);
CREATE INDEX sanctions_ends_at_idx ON sanctions (ends_at);

CREATE TABLE sanction_reversals (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    sanction_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    reversed_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    reason TEXT NOT NULL,
    reversed_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY sanction_reversals_uq (sanction_id),
    CONSTRAINT sanction_reversals_sanction_fk FOREIGN KEY (sanction_id) REFERENCES sanctions (id) ON DELETE CASCADE,
    CONSTRAINT sanction_reversals_reversed_by_fk FOREIGN KEY (reversed_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
