-- 昵称黑名单（SQLite）
CREATE TABLE nickname_blacklist (
    id TEXT PRIMARY KEY NOT NULL,
    nickname TEXT NOT NULL,
    nickname_normalized TEXT NOT NULL,
    reason TEXT,
    created_by TEXT,
    created_at INTEGER NOT NULL,
    CONSTRAINT nickname_blacklist_normalized_uq UNIQUE (nickname_normalized)
);

CREATE INDEX nickname_blacklist_created_idx ON nickname_blacklist (created_at);
