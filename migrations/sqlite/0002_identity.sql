-- BBLBB identity migration: email verification and password reset tokens
-- Builds on 0001_skeleton.sql which created users and user_sessions tables

-- Add verification and reset fields to users table
ALTER TABLE users ADD COLUMN email_verified INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN display_name TEXT;
ALTER TABLE users ADD COLUMN bio TEXT;
ALTER TABLE users ADD COLUMN timezone TEXT NOT NULL DEFAULT 'UTC';

-- Email verification tokens (hashed, one-time use)
CREATE TABLE email_verification_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL COLLATE BINARY,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX email_verification_tokens_hash_uq
    ON email_verification_tokens (token_hash);
CREATE INDEX email_verification_tokens_user_idx
    ON email_verification_tokens (user_id);

-- Password reset tokens (hashed, one-time use, 30 min expiry)
CREATE TABLE password_reset_tokens (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL COLLATE BINARY,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX password_reset_tokens_hash_uq
    ON password_reset_tokens (token_hash);
CREATE INDEX password_reset_tokens_user_idx
    ON password_reset_tokens (user_id);
