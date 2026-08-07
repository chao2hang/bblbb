-- BBLBB moderation cases and reports (M05-SCHEMA-01/06, MySQL)
--
-- reports: polymorphic target (target_type + target_id, no single FK),
-- closed reason_code enum; status open/triaged/investigating/resolved/
-- rejected/reopened/withdrawn (withdrawn is reports-only, not cases).
--
-- Dedup (M05-SCHEMA-06): report_dedup_key normalizes (reporter_id,
-- target_type, target_id, reason_code) into one column (same technique as
-- 0040 grant_target_key, avoiding cross-DB NULL uniqueness differences);
-- the dedup window is anchored -- dedup_until is the end of the current
-- anchored window (computed with REPORT_DEDUP_WINDOW_MS in the model layer),
-- at most one row per key per window, enforced by UNIQUE(report_dedup_key,
-- dedup_until); in-window duplicates are rejected at model level when
-- dedup_until > now.
--
-- moderation_cases: cases aggregate reports into a single moderation thread;
-- state machine in STATE-MACHINES.md section 3; priority low/normal/high/urgent.
-- case_reports: many-to-many reports <-> cases (reports may be merged).
-- case_assignments: append-only assignment history (release records released_at).
-- moderation_notes: internal notes (body never exposed via public API).

CREATE TABLE reports (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    reporter_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    target_type VARCHAR(16) NOT NULL,
    target_id VARCHAR(64) NOT NULL,
    reason_code VARCHAR(16) NOT NULL,
    details TEXT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'open',
    report_dedup_key VARCHAR(255) NOT NULL,
    dedup_until BIGINT NOT NULL,
    assigned_to CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY reports_dedup_uq (report_dedup_key, dedup_until),
    CONSTRAINT reports_reporter_fk FOREIGN KEY (reporter_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT reports_assignee_fk FOREIGN KEY (assigned_to) REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT reports_target_type_ck CHECK (target_type IN ('post', 'comment', 'user', 'board')),
    CONSTRAINT reports_reason_ck CHECK (reason_code IN ('spam', 'harassment', 'illegal', 'nsfw', 'misinformation', 'impersonation', 'other')),
    CONSTRAINT reports_status_ck CHECK (status IN ('open', 'triaged', 'investigating', 'resolved', 'rejected', 'reopened', 'withdrawn'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX reports_reporter_idx ON reports (reporter_id);
CREATE INDEX reports_target_idx ON reports (target_type, target_id);
CREATE INDEX reports_status_idx ON reports (status, dedup_until);
CREATE INDEX reports_dedup_key_idx ON reports (report_dedup_key);

CREATE TABLE moderation_cases (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    title VARCHAR(200) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'open',
    priority VARCHAR(8) NOT NULL DEFAULT 'normal',
    assigned_to CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    resolved_at BIGINT NULL,
    resolution TEXT NULL,
    PRIMARY KEY (id),
    CONSTRAINT moderation_cases_assignee_fk FOREIGN KEY (assigned_to) REFERENCES users (id) ON DELETE SET NULL,
    CONSTRAINT moderation_cases_creator_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT moderation_cases_status_ck CHECK (status IN ('open', 'triaged', 'investigating', 'resolved', 'rejected', 'reopened')),
    CONSTRAINT moderation_cases_priority_ck CHECK (priority IN ('low', 'normal', 'high', 'urgent'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX moderation_cases_status_idx ON moderation_cases (status, priority);
CREATE INDEX moderation_cases_assignee_idx ON moderation_cases (assigned_to);
CREATE INDEX moderation_cases_created_idx ON moderation_cases (created_at);

CREATE TABLE case_reports (
    case_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    report_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    added_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    added_at BIGINT NOT NULL,
    PRIMARY KEY (case_id, report_id),
    CONSTRAINT case_reports_case_fk FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    CONSTRAINT case_reports_report_fk FOREIGN KEY (report_id) REFERENCES reports (id) ON DELETE CASCADE,
    CONSTRAINT case_reports_added_by_fk FOREIGN KEY (added_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX case_reports_report_idx ON case_reports (report_id);

CREATE TABLE case_assignments (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    case_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    assignee_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    assigned_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    assigned_at BIGINT NOT NULL,
    released_at BIGINT NULL,
    note TEXT NULL,
    PRIMARY KEY (id),
    CONSTRAINT case_assignments_case_fk FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    CONSTRAINT case_assignments_assignee_fk FOREIGN KEY (assignee_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT case_assignments_assigned_by_fk FOREIGN KEY (assigned_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX case_assignments_case_idx ON case_assignments (case_id, assigned_at);
CREATE INDEX case_assignments_assignee_idx ON case_assignments (assignee_id);

CREATE TABLE moderation_notes (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    case_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    author_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    body TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NULL,
    PRIMARY KEY (id),
    CONSTRAINT moderation_notes_case_fk FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    CONSTRAINT moderation_notes_author_fk FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX moderation_notes_case_idx ON moderation_notes (case_id, created_at);
