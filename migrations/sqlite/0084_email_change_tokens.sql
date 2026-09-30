-- 换绑邮箱：同 mariadb/mysql（SQLite 方言）。
CREATE TABLE email_change_tokens (
    id TEXT NOT NULL PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users (id),
    new_email TEXT NOT NULL,
    token_hash TEXT NOT NULL,
    token_encrypted TEXT NULL,
    expires_at INTEGER NOT NULL,
    consumed_at INTEGER NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX email_change_tokens_user_idx ON email_change_tokens (user_id);
CREATE INDEX email_change_tokens_hash_idx ON email_change_tokens (token_hash);
