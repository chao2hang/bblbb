-- BBLBB moderation actions and revision history (M05-SCHEMA-02, MariaDB)
--
-- moderation_actions: one-shot moderation actions on cases/content --
-- closed action enum, polymorphic target (target_type + target_id);
-- append-only, rows are immutable (corrections go to
-- moderation_action_revisions, never UPDATE the action row).
--
-- moderation_action_revisions: immutable append-only snapshots -- each
-- revision is unique per (action_id, revision) and revision strictly
-- increases (model-layer validation); snapshot_json is the full action row
-- snapshot at revision time (carries correction semantics); change_reason
-- records why.

CREATE TABLE moderation_actions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    case_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NULL,
    actor_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    action VARCHAR(24) NOT NULL,
    target_type VARCHAR(16) NULL,
    target_id VARCHAR(64) NULL,
    reason TEXT NULL,
    metadata_json TEXT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT moderation_actions_case_fk FOREIGN KEY (case_id) REFERENCES moderation_cases (id) ON DELETE CASCADE,
    CONSTRAINT moderation_actions_actor_fk FOREIGN KEY (actor_id) REFERENCES users (id) ON DELETE RESTRICT,
    CONSTRAINT moderation_actions_action_ck CHECK (action IN ('escalate', 'assign', 'resolve', 'reject', 'reopen',
        'hide_content', 'restore_content', 'delete_content', 'issue_sanction', 'revoke_sanction',
        'merge_cases', 'remove_report')),
    CONSTRAINT moderation_actions_target_type_ck CHECK (target_type IN ('post', 'comment', 'user', 'report', 'case', 'sanction'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX moderation_actions_case_idx ON moderation_actions (case_id, created_at);
CREATE INDEX moderation_actions_actor_idx ON moderation_actions (actor_id, created_at);
CREATE INDEX moderation_actions_target_idx ON moderation_actions (target_type, target_id);

CREATE TABLE moderation_action_revisions (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    action_id CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    revision INT NOT NULL,
    snapshot_json TEXT NOT NULL,
    change_reason TEXT NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_bin NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY moderation_action_revisions_uq (action_id, revision),
    CONSTRAINT moderation_action_revisions_action_fk FOREIGN KEY (action_id) REFERENCES moderation_actions (id) ON DELETE CASCADE,
    CONSTRAINT moderation_action_revisions_creator_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX moderation_action_revisions_action_idx ON moderation_action_revisions (action_id, revision);
