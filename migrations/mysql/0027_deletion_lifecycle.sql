-- BBLBB account deletion lifecycle: legal hold column (M03-PROFILE-08)
-- legal_hold_at NOT NULL = legal hold / investigation freeze (priority 1,
-- docs/RETENTION-PRIVACY.md): deletion requests are rejected and the due
-- execution job is skipped with an audit record. Companion to
-- delete_requested_at (request time) and deleted_at (terminal time).

ALTER TABLE users ADD COLUMN legal_hold_at BIGINT NULL;
