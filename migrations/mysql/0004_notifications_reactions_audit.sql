-- BBLBB notifications and reactions migration (MySQL)

-- 通知表
CREATE TABLE notifications (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    user_id VARCHAR(36) NOT NULL,
    type VARCHAR(20) NOT NULL DEFAULT 'system',
    title VARCHAR(200) NOT NULL,
    body TEXT,
    link VARCHAR(500),
    is_read TINYINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    read_at BIGINT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT chk_notification_type CHECK (type IN ('system', 'reply', 'mention', 'reaction', 'moderation', 'badge', 'digest'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX notifications_user_id_idx ON notifications (user_id);
CREATE INDEX notifications_user_unread_idx ON notifications (user_id, is_read);

-- 帖子反应表
CREATE TABLE post_reactions (
    post_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    reaction VARCHAR(20) NOT NULL DEFAULT 'like',
    created_at BIGINT NOT NULL,
    PRIMARY KEY (post_id, user_id, reaction),
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX post_reactions_post_idx ON post_reactions (post_id);
CREATE INDEX post_reactions_user_idx ON post_reactions (user_id);

-- 评论反应表
CREATE TABLE comment_reactions (
    comment_id VARCHAR(36) NOT NULL,
    user_id VARCHAR(36) NOT NULL,
    reaction VARCHAR(20) NOT NULL DEFAULT 'like',
    created_at BIGINT NOT NULL,
    PRIMARY KEY (comment_id, user_id, reaction),
    FOREIGN KEY (comment_id) REFERENCES comments (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX comment_reactions_comment_idx ON comment_reactions (comment_id);
CREATE INDEX comment_reactions_user_idx ON comment_reactions (user_id);

-- 审计日志表
CREATE TABLE audit_logs (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    actor_id VARCHAR(36) NULL,
    action VARCHAR(100) NOT NULL,
    target_type VARCHAR(50) NULL,
    target_id VARCHAR(36) NULL,
    metadata TEXT,
    request_id VARCHAR(100) NULL,
    ip_address VARCHAR(45) NULL,
    created_at BIGINT NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX audit_logs_actor_idx ON audit_logs (actor_id);
CREATE INDEX audit_logs_target_idx ON audit_logs (target_type, target_id);
CREATE INDEX audit_logs_created_at_idx ON audit_logs (created_at);

-- 事务性发件箱表
CREATE TABLE outbox_events (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    event_type VARCHAR(50) NOT NULL,
    payload TEXT NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    attempts INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 5,
    next_attempt_at BIGINT NOT NULL,
    created_at BIGINT NOT NULL,
    processed_at BIGINT NULL,
    error TEXT,
    CONSTRAINT chk_outbox_status CHECK (status IN ('pending', 'processing', 'sent', 'failed'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX outbox_status_idx ON outbox_events (status, next_attempt_at);
CREATE INDEX outbox_created_at_idx ON outbox_events (created_at);
