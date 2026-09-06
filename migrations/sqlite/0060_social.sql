-- BBLBB social domain (GAP-FIX 社交域, SQLite)
--
-- 与 mysql/mariadb 同版本同结构：favorites（帖子收藏，复合主键天然幂等）、
-- user_follows（用户关注，CHECK 禁止关注自己）、board_follows（板块关注）、
-- conversations + conversation_participants（双人私信会话）、messages
-- （会话消息，软删）、achievements（成就定义，code 唯一，version 乐观并发）、
-- user_achievements（解锁记录，装备位）与 api_keys（本人 API 密钥，只存
-- SHA-256 hash，prefix 供人工识别）。种子 8 条成就（含 2 条 hidden）。

CREATE TABLE favorites (
    user_id TEXT NOT NULL,
    post_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, post_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
);

CREATE INDEX favorites_post_idx ON favorites (post_id);
CREATE INDEX favorites_user_created_idx ON favorites (user_id, created_at);

CREATE TABLE user_follows (
    follower_id TEXT NOT NULL,
    followee_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (follower_id, followee_id),
    CHECK (follower_id <> followee_id),
    FOREIGN KEY (follower_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (followee_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX user_follows_followee_idx ON user_follows (followee_id);

CREATE TABLE board_follows (
    user_id TEXT NOT NULL,
    board_id TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, board_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (board_id) REFERENCES boards (id) ON DELETE CASCADE
);

CREATE INDEX board_follows_board_idx ON board_follows (board_id);

CREATE TABLE conversations (
    id TEXT PRIMARY KEY NOT NULL,
    created_at INTEGER NOT NULL,
    last_message_at INTEGER NOT NULL
);

CREATE TABLE conversation_participants (
    conversation_id TEXT NOT NULL,
    user_id TEXT NOT NULL,
    last_read_at INTEGER,
    PRIMARY KEY (conversation_id, user_id),
    FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX conversation_participants_user_idx ON conversation_participants (user_id);

CREATE TABLE messages (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    deleted_at INTEGER,
    CHECK (length(body) BETWEEN 1 AND 2000),
    FOREIGN KEY (conversation_id) REFERENCES conversations (id) ON DELETE CASCADE,
    FOREIGN KEY (sender_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX messages_conversation_created_idx ON messages (conversation_id, created_at);
CREATE INDEX messages_sender_idx ON messages (sender_id);

CREATE TABLE achievements (
    id TEXT PRIMARY KEY NOT NULL,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    category TEXT NOT NULL,
    condition_type TEXT NOT NULL
        CHECK (condition_type IN ('post_count', 'comment_count', 'reaction_received', 'checkin_streak', 'follower_count', 'manual')),
    condition_threshold INTEGER NOT NULL,
    reward_exp INTEGER NOT NULL DEFAULT 0,
    reward_coin INTEGER NOT NULL DEFAULT 0,
    is_hidden INTEGER NOT NULL DEFAULT 0,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    sort_order INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL DEFAULT 1,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);

CREATE INDEX achievements_sort_idx ON achievements (sort_order);

CREATE TABLE user_achievements (
    user_id TEXT NOT NULL,
    achievement_id TEXT NOT NULL,
    unlocked_at INTEGER NOT NULL,
    progress INTEGER NOT NULL DEFAULT 0,
    equipped INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_id, achievement_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (achievement_id) REFERENCES achievements (id) ON DELETE CASCADE
);

CREATE INDEX user_achievements_achievement_idx ON user_achievements (achievement_id);

CREATE TABLE api_keys (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    name TEXT NOT NULL,
    secret_hash TEXT NOT NULL UNIQUE,
    prefix TEXT NOT NULL,
    scopes TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER,
    revoked_at INTEGER,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

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
