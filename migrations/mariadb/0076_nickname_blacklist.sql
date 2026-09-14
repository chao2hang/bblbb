-- 昵称黑名单（MariaDB）
CREATE TABLE nickname_blacklist (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    nickname VARCHAR(100) NOT NULL,
    nickname_normalized VARCHAR(100) NOT NULL,
    reason VARCHAR(500) NULL,
    created_by VARCHAR(36) NULL,
    created_at BIGINT NOT NULL,
    CONSTRAINT nickname_blacklist_normalized_uq UNIQUE (nickname_normalized)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX nickname_blacklist_created_idx ON nickname_blacklist (created_at);
