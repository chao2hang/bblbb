
ALTER TABLE user_sessions ADD COLUMN user_agent TEXT NULL;
ALTER TABLE user_sessions ADD COLUMN ip_prefix_hash TEXT NULL;
ALTER TABLE user_sessions ADD COLUMN revoke_reason TEXT NULL;
ALTER TABLE user_sessions ADD COLUMN version INT NOT NULL DEFAULT 0;
