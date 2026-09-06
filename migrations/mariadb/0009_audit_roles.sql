
ALTER TABLE audit_logs ADD COLUMN effective_role VARCHAR(50) NULL;
ALTER TABLE audit_logs ADD COLUMN reason TEXT NULL;
ALTER TABLE audit_logs ADD COLUMN policy_version VARCHAR(32) NULL;
