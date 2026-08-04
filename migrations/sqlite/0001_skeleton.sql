-- BBLBB schema skeleton only. Expand through new, immutable migrations.
PRAGMA foreign_keys = ON;

CREATE TABLE users (
    id TEXT PRIMARY KEY NOT NULL,
    username_normalized TEXT NOT NULL COLLATE BINARY,
    email_normalized TEXT NOT NULL COLLATE BINARY,
    password_hash TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'active', 'restricted', 'banned', 'pending_delete', 'deleted')),
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE UNIQUE INDEX users_username_normalized_uq
    ON users (username_normalized);
CREATE UNIQUE INDEX users_email_normalized_uq
    ON users (email_normalized);

CREATE TABLE user_sessions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    token_hash TEXT NOT NULL COLLATE BINARY,
    csrf_secret_hash TEXT NOT NULL COLLATE BINARY,
    created_at INTEGER NOT NULL,
    last_seen_at INTEGER NOT NULL,
    idle_expires_at INTEGER NOT NULL,
    absolute_expires_at INTEGER NOT NULL,
    revoked_at INTEGER,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE UNIQUE INDEX user_sessions_token_hash_uq
    ON user_sessions (token_hash);
CREATE INDEX user_sessions_user_id_idx
    ON user_sessions (user_id);
