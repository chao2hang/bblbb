-- BBLBB 信任等级（LinuxDo 式 TL0–TL4，SQLite）
--
-- 与 mysql/mariadb 同版本同构。移植 linux.do（Discourse）五级信任体系：
--   TL0 新用户 / TL1 基本用户 / TL2 成员 / TL3 活跃用户 / TL4 领导者。
--
-- users.trust_level 为可重建缓存（0–4），真实来源是下述行为统计表与
-- 既有 post_reactions / comment_reactions / reports / sanctions 的聚合；
-- trust_level_rules 存每级阈值（requirements_json，种子 = linux.do 默认
-- 数值，评估时读库、缺省回退代码常量）；trust_level_events 只追加升降级
-- 日志（TL3 的 2 周降级宽限期按最近一次 reason='promotion' 到 3 的事件
-- created_at 计算，不另设列）。
--
-- 行为统计来源（均带 user 维度唯一键，天然幂等）：
--   trust_visits        每日访问一行（会话续期时写入，visit_day='YYYY-MM-DD' UTC）
--   trust_topic_views   进入话题（GET 帖子详情时写入，user×post 唯一）
--   trust_comment_reads 阅读楼层（GET 楼层列表时写入，user×comment 唯一，
--                       冗余 post_id 便于按话题聚合与窗口期阅读统计）
--   trust_read_time     阅读时长（客户端心跳上报，服务端按请求≤60s、
--                       按人/日≤7200s 钳制后累计）
-- 点赞（送出/收到的赞及其用户/天数多样性）直接聚合反应表；
-- 举报确认标记聚合 reports（status='resolved' 视为版主确认）；
-- 禁言/封禁聚合 sanctions（kind IN ('mute','ban') 且未撤销，6 个月内）。

ALTER TABLE users ADD COLUMN trust_level INTEGER NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN trust_level_updated_at INTEGER;

-- 每日访问（一天一行）
CREATE TABLE trust_visits (
    user_id TEXT NOT NULL,
    visit_day TEXT NOT NULL,
    first_seen_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, visit_day),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE INDEX trust_visits_day_idx ON trust_visits (visit_day);

-- 进入话题（user×post 唯一）
CREATE TABLE trust_topic_views (
    user_id TEXT NOT NULL,
    post_id TEXT NOT NULL,
    first_viewed_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, post_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
);

CREATE INDEX trust_topic_views_post_idx ON trust_topic_views (post_id);
CREATE INDEX trust_topic_views_time_idx ON trust_topic_views (user_id, first_viewed_at);

-- 阅读楼层（user×comment 唯一；冗余 post_id）
CREATE TABLE trust_comment_reads (
    user_id TEXT NOT NULL,
    comment_id TEXT NOT NULL,
    post_id TEXT NOT NULL,
    first_read_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, comment_id),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (comment_id) REFERENCES comments (id) ON DELETE CASCADE,
    FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
);

CREATE INDEX trust_comment_reads_post_idx ON trust_comment_reads (post_id);
CREATE INDEX trust_comment_reads_time_idx ON trust_comment_reads (user_id, first_read_at);

-- 阅读时长（按人按日累计秒数，写入前服务端钳制）
CREATE TABLE trust_read_time (
    user_id TEXT NOT NULL,
    read_day TEXT NOT NULL,
    seconds INTEGER NOT NULL DEFAULT 0,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, read_day),
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

-- 升降级日志（只追加；from_level NULL = 初始/未知）
CREATE TABLE trust_level_events (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    from_level INTEGER,
    to_level INTEGER NOT NULL,
    reason TEXT NOT NULL
        CHECK (reason IN ('seed', 'promotion', 'demotion', 'manual')),
    note TEXT,
    created_by TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL
);

CREATE INDEX trust_level_events_user_idx ON trust_level_events (user_id, created_at);

-- 每级阈值（requirements_json 结构见 docs/TRUST-LEVELS.md §4）
CREATE TABLE trust_level_rules (
    level INTEGER NOT NULL,
    name TEXT NOT NULL,
    summary TEXT,
    requirements_json TEXT,
    is_enabled INTEGER NOT NULL DEFAULT 1,
    version INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (level),
    CONSTRAINT trust_level_rules_level_ck CHECK (level >= 0 AND level <= 4)
);

INSERT INTO trust_level_rules (level, name, summary, requirements_json, is_enabled, version, updated_at)
VALUES
    (0, '新用户', '注册默认等级；能力受反垃圾限制。', '{}', 1, 1, 1722816000),
    (1, '基本用户', '愿意阅读即可达到的第一级。', '{"topics_entered":5,"posts_read":30,"time_read_seconds":600}', 1, 1, 1722816000),
    (2, '成员', '持续活跃并参与讨论的正式成员。', '{"days_visited":15,"likes_given":1,"likes_received":1,"topics_replied_to":3,"topics_entered":20,"posts_read":100,"time_read_seconds":3600}', 1, 1, 1722816000),
    (3, '活跃用户', '滚动窗口考核，不达标降级（2 周宽限）。', '{"window_days":100,"visit_ratio":0.5,"replied_topics_window":10,"viewed_ratio":0.25,"viewed_cap":500,"read_ratio":0.25,"read_cap":20000,"likes_received_window":20,"likes_given_window":30,"like_distinct_user_divisor":5,"like_distinct_day_divisor":4,"max_flags":5,"no_sanction_months":6}', 1, 1, 1722816000),
    (4, '领导者', '仅可由工作人员手动授予。', '{"manual_only":true}', 1, 1, 1722816000);
