-- BBLBB sanctions (M05-SCHEMA-03, MariaDB)
--
-- sanctions: warning/rate_limit/mute/board_mute/ban. board_mute requires
-- board_id; other kinds refuse a board scope (CHECK-enforced, identical
-- across engines). ends_at NULL = permanent (warning/ban are usually
-- permanent); when set it must be later than starts_at. status advances
-- scheduled/active/expired over time at model level; revoked requires
-- revoked_at and revoked_by (CHECK-enforced).
--
-- sanction_reversals: append-only immutable reversal records -- at most one
-- per sanction (UNIQUE(sanction_id)); the reversal evidence chain (who, when,
-- why) can never be overwritten. sanctions.revoked_at/revoked_by/
-- revoke_reason mirror the current state for query convenience.

CREATE TABLE sanctions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    kind VARCHAR(16) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'scheduled',
    reason TEXT NULL,
    starts_at BIGINT NOT NULL,
    ends_at BIGINT NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    revoked_at BIGINT NULL,
    revoked_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    revoke_reason TEXT NULL,
    PRIMARY KEY (id),
    CONSTRAINT sanctions_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT sanctions_board_fk FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE,
    CONSTRAINT sanctions_creator_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT sanctions_revoked_by_fk FOREIGN KEY (revoked_by) REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT sanctions_kind_ck CHECK (kind IN ('warning', 'rate_limit', 'mute', 'board_mute', 'ban')),
    CONSTRAINT sanctions_status_ck CHECK (status IN ('scheduled', 'active', 'expired', 'revoked')),
    CONSTRAINT sanctions_board_scope_ck CHECK ((kind = 'board_mute' AND board_id IS NOT NULL)
        OR (kind != 'board_mute' AND board_id IS NULL)),
    CONSTRAINT sanctions_timeline_ck CHECK (ends_at IS NULL OR ends_at > starts_at),
    CONSTRAINT sanctions_revoked_ck CHECK (status != 'revoked' OR (revoked_at IS NOT NULL AND revoked_by IS NOT NULL))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX sanctions_user_status_idx ON sanctions (user_id, status);
CREATE INDEX sanctions_board_idx ON sanctions (board_id);
CREATE INDEX sanctions_ends_at_idx ON sanctions (ends_at);

CREATE TABLE sanction_reversals (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    sanction_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reversed_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reason TEXT NOT NULL,
    reversed_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY sanction_reversals_uq (sanction_id),
    CONSTRAINT sanction_reversals_sanction_fk FOREIGN KEY (sanction_id) REFERENCES sanctions (id) ON DELETE CASCADE,
    CONSTRAINT sanction_reversals_reversed_by_fk FOREIGN KEY (reversed_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
