-- BBLBB notifications and reactions migration

-- 通知表
CREATE TABLE notifications (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    type TEXT NOT NULL DEFAULT 'system'
        CHECK (type IN ('system', 'reply', 'mention', 'reaction', 'moderation', 'badge', 'digest')),
    title TEXT NOT NULL,
    body TEXT,
    link TEXT,
    is_read INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    read_at INTEGER,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX notifications_user_id_idx ON notifications (user_id);
CREATE INDEX notifications_user_unread_idx ON notifications (user_id, is_read);

-- 帖子反应表（点赞等）
CREATE TABLE post_reactions (
    post_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    reaction TEXT NOT NULL DEFAULT 'like',
    created_at INTEGER NOT NULL,
    PRIMARY KEY (post_id, user_id, reaction),
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX post_reactions_post_idx ON post_reactions (post_id);
CREATE INDEX post_reactions_user_idx ON post_reactions (user_id);

-- 评论反应表
CREATE TABLE comment_reactions (
    comment_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    reaction TEXT NOT NULL DEFAULT 'like',
    created_at INTEGER NOT NULL,
    PRIMARY KEY (comment_id, user_id, reaction),
    FOREIGN KEY (comment_id) REFERENCES comments (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX comment_reactions_comment_idx ON comment_reactions (comment_id);
CREATE INDEX comment_reactions_user_idx ON comment_reactions (user_id);

-- 审计日志表
CREATE TABLE audit_logs (
    id TEXT PRIMARY KEY NOT NULL,
    actor_id TEXT,
    action TEXT NOT NULL,
    target_type TEXT,
    target_id TEXT,
    metadata TEXT,
    request_id TEXT,
    ip_address TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX audit_logs_actor_idx ON audit_logs (actor_id);
CREATE INDEX audit_logs_target_idx ON audit_logs (target_type, target_id);
CREATE INDEX audit_logs_created_at_idx ON audit_logs (created_at);

-- 事务性发件箱表（Transactional Outbox）
CREATE TABLE outbox_events (
    id TEXT PRIMARY KEY NOT NULL,
    event_type TEXT NOT NULL,
    payload TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'processing', 'sent', 'failed')),
    attempts INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 5,
    next_attempt_at INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    processed_at INTEGER,
    error TEXT
);

CREATE INDEX outbox_status_idx ON outbox_events (status, next_attempt_at);
CREATE INDEX outbox_created_at_idx ON outbox_events (created_at);
