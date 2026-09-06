-- BBLBB social domain (GAP-FIX 社交域, MariaDB)
--
-- Table-for-table equivalent to migrations/sqlite/0060_social.sql:
-- - favorites: post bookmarks; composite PK gives natural idempotency.
-- - user_follows: user follows (CHECK follower != followee).
-- - board_follows: board follows.
-- - conversations + conversation_participants: 1:1 direct-message threads.
-- - messages: conversation messages (soft delete via deleted_at).
-- - achievements: badge catalog (code UNIQUE, version optimistic lock).
-- - user_achievements: unlock rows with equip slot.
-- - api_keys: personal API keys (SHA-256 hash only; prefix for display).
-- Seeds 8 achievements (2 hidden).

CREATE TABLE favorites (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, post_id),
    CONSTRAINT favorites_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT favorites_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX favorites_post_idx ON favorites (post_id);
CREATE INDEX favorites_user_created_idx ON favorites (user_id, created_at);

CREATE TABLE user_follows (
    follower_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    followee_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (follower_id, followee_id),
    CONSTRAINT user_follows_no_self_ck CHECK (follower_id <> followee_id),
    CONSTRAINT user_follows_follower_fk FOREIGN KEY (follower_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT user_follows_followee_fk FOREIGN KEY (followee_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX user_follows_followee_idx ON user_follows (followee_id);

CREATE TABLE board_follows (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    board_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, board_id),
    CONSTRAINT board_follows_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT board_follows_board_fk FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX board_follows_board_idx ON board_follows (board_id);

CREATE TABLE conversations (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    last_message_at BIGINT NOT NULL,
    PRIMARY KEY (id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE conversation_participants (
    conversation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    last_read_at BIGINT NULL,
    PRIMARY KEY (conversation_id, user_id),
    CONSTRAINT conversation_participants_conversation_fk FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE,
    CONSTRAINT conversation_participants_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX conversation_participants_user_idx ON conversation_participants (user_id);

CREATE TABLE messages (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    conversation_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    sender_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    body VARCHAR(2000) NOT NULL,
    created_at BIGINT NOT NULL,
    deleted_at BIGINT NULL,
    PRIMARY KEY (id),
    CONSTRAINT messages_body_len_ck CHECK (CHAR_LENGTH(body) BETWEEN 1 AND 2000),
    CONSTRAINT messages_conversation_fk FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE,
    CONSTRAINT messages_sender_fk FOREIGN KEY (sender_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX messages_conversation_created_idx ON messages (conversation_id, created_at);
CREATE INDEX messages_sender_idx ON messages (sender_id);

CREATE TABLE achievements (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    code VARCHAR(64) NOT NULL,
    name VARCHAR(120) NOT NULL,
    description VARCHAR(500) NOT NULL,
    category VARCHAR(32) NOT NULL,
    condition_type VARCHAR(32) NOT NULL,
    condition_threshold BIGINT NOT NULL,
    reward_exp BIGINT NOT NULL DEFAULT 0,
    reward_coin BIGINT NOT NULL DEFAULT 0,
    is_hidden TINYINT NOT NULL DEFAULT 0,
    is_enabled TINYINT NOT NULL DEFAULT 1,
    sort_order INT NOT NULL DEFAULT 0,
    version BIGINT NOT NULL DEFAULT 1,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY achievements_code_uq (code),
    CONSTRAINT achievements_condition_type_ck CHECK (condition_type IN ('post_count', 'comment_count', 'reaction_received', 'checkin_streak', 'follower_count', 'manual'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX achievements_sort_idx ON achievements (sort_order);

CREATE TABLE user_achievements (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    achievement_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    unlocked_at BIGINT NOT NULL,
    progress BIGINT NOT NULL DEFAULT 0,
    equipped TINYINT NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, achievement_id),
    CONSTRAINT user_achievements_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT user_achievements_achievement_fk FOREIGN KEY (achievement_id) REFERENCES achievements (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX user_achievements_achievement_idx ON user_achievements (achievement_id);

CREATE TABLE api_keys (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    name VARCHAR(64) NOT NULL,
    secret_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    prefix VARCHAR(12) NOT NULL,
    scopes TEXT NOT NULL,
    created_at BIGINT NOT NULL,
    last_used_at BIGINT NULL,
    revoked_at BIGINT NULL,
    PRIMARY KEY (id),
    UNIQUE KEY api_keys_hash_uq (secret_hash),
    CONSTRAINT api_keys_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX api_keys_user_idx ON api_keys (user_id);

INSERT INTO achievements (id, code, name, description, category, condition_type, condition_threshold, reward_exp, reward_coin, is_hidden, is_enabled, sort_order, version, created_at, updated_at)
VALUES
    ('01911fd5-0060-0000-0000-000000000001', 'first_post', '首发帖', '发布第一篇帖子，开启社区之旅', 'content', 'post_count', 1, 10, 5, 0, 1, 1, 1, 1722816000, 1722816000),
    ('01911fd5-0060-0000-0000-000000000002', 'active_commenter', '活跃回复', '累计发表 50 条回复，讨论区的中坚力量', 'content', 'comment_count', 50, 50, 20, 0, 1, 2, 1, 1722816000, 1722816000),
    ('01911fd5-0060-0000-0000-000000000003', 'centurion', '百帖', '累计发布 100 篇帖子，多产创作者', 'content', 'post_count', 100, 200, 100, 0, 1, 3, 1, 1722816000, 1722816000),
    ('01911fd5-0060-0000-0000-000000000004', 'like_magnet', '被赞达人', '内容累计获得 100 次反应，深受社区喜爱', 'social', 'reaction_received', 100, 100, 50, 0, 1, 4, 1, 1722816000, 1722816000),
    ('01911fd5-0060-0000-0000-000000000005', 'streak_7', '连续签到7天', '连续 7 天签到打卡，坚持的力量', 'activity', 'checkin_streak', 7, 30, 15, 0, 1, 5, 1, 1722816000, 1722816000),
    ('01911fd5-0060-0000-0000-000000000006', 'hundred_followers', '百粉', '收获 100 位关注者，社区影响力初显', 'social', 'follower_count', 100, 150, 80, 0, 1, 6, 1, 1722816000, 1722816000),
    ('01911fd5-0060-0000-0000-000000000007', 'streak_30', '30天', '连续 30 天签到打卡，意志如钢', 'activity', 'checkin_streak', 30, 300, 150, 1, 1, 7, 1, 1722816000, 1722816000),
    ('01911fd5-0060-0000-0000-000000000008', 'community_elder', '社区元老', '由社区管理团队授予的至高荣誉', 'special', 'manual', 0, 500, 500, 1, 1, 8, 1, 1722816000, 1722816000);
