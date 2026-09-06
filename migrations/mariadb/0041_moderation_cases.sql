
CREATE TABLE reports (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    reporter_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    target_type VARCHAR(16) NOT NULL,
    target_id VARCHAR(64) NOT NULL,
    reason_code VARCHAR(16) NOT NULL,
    details TEXT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'open',
    report_dedup_key VARCHAR(255) NOT NULL,
    dedup_until BIGINT NOT NULL,
    assigned_to CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
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
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    title VARCHAR(200) NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'open',
    priority VARCHAR(8) NOT NULL DEFAULT 'normal',
    assigned_to CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
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
    case_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    report_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    added_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    added_at BIGINT NOT NULL,
    PRIMARY KEY (case_id, report_id),
    CONSTRAINT case_reports_case_fk FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    CONSTRAINT case_reports_report_fk FOREIGN KEY (report_id) REFERENCES reports (id) ON DELETE CASCADE,
    CONSTRAINT case_reports_added_by_fk FOREIGN KEY (added_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX case_reports_report_idx ON case_reports (report_id);

CREATE TABLE case_assignments (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    case_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    assignee_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    assigned_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
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
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    case_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    author_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    body TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NULL,
    PRIMARY KEY (id),
    CONSTRAINT moderation_notes_case_fk FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    CONSTRAINT moderation_notes_author_fk FOREIGN KEY (author_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX moderation_notes_case_idx ON moderation_notes (case_id, created_at);
