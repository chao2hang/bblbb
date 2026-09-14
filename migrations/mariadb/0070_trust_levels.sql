-- BBLBB 信任等级（LinuxDo 式 TL0–TL4，MariaDB）
--
-- Table-for-table equivalent to migrations/sqlite/0070_trust_levels.sql:
-- users.trust_level 为可重建缓存（0–4）；trust_visits / trust_topic_views /
-- trust_comment_reads / trust_read_time 为行为统计（唯一键幂等）；
-- trust_level_events 只追加升降级日志；trust_level_rules 存每级阈值
-- （requirements_json，种子 = linux.do 默认数值）。语义与 SQLite 版一致。

ALTER TABLE users ADD COLUMN trust_level INT NOT NULL DEFAULT 0;
ALTER TABLE users ADD COLUMN trust_level_updated_at BIGINT NULL;

-- 每日访问（一天一行）
CREATE TABLE trust_visits (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    visit_day VARCHAR(10) NOT NULL,
    first_seen_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, visit_day),
    CONSTRAINT trust_visits_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX trust_visits_day_idx ON trust_visits (visit_day);

-- 进入话题（user×post 唯一）
CREATE TABLE trust_topic_views (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    first_viewed_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, post_id),
    CONSTRAINT trust_topic_views_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT trust_topic_views_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX trust_topic_views_post_idx ON trust_topic_views (post_id);
CREATE INDEX trust_topic_views_time_idx ON trust_topic_views (user_id, first_viewed_at);

-- 阅读楼层（user×comment 唯一；冗余 post_id）
CREATE TABLE trust_comment_reads (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    comment_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    post_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    first_read_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, comment_id),
    CONSTRAINT trust_comment_reads_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT trust_comment_reads_comment_fk FOREIGN KEY (comment_id) REFERENCES comments (id) ON DELETE CASCADE,
    CONSTRAINT trust_comment_reads_post_fk FOREIGN KEY (post_id) REFERENCES posts (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX trust_comment_reads_post_idx ON trust_comment_reads (post_id);
CREATE INDEX trust_comment_reads_time_idx ON trust_comment_reads (user_id, first_read_at);

-- 阅读时长（按人按日累计秒数，写入前服务端钳制）
CREATE TABLE trust_read_time (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    read_day VARCHAR(10) NOT NULL,
    seconds BIGINT NOT NULL DEFAULT 0,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, read_day),
    CONSTRAINT trust_read_time_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- 升降级日志（只追加；from_level NULL = 初始/未知）
CREATE TABLE trust_level_events (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    from_level INT NULL,
    to_level INT NOT NULL,
    reason VARCHAR(16) NOT NULL,
    note TEXT NULL,
    created_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT trust_level_events_reason_ck CHECK (reason IN ('seed', 'promotion', 'demotion', 'manual')),
    CONSTRAINT trust_level_events_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT trust_level_events_creator_fk FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX trust_level_events_user_idx ON trust_level_events (user_id, created_at);

-- 每级阈值（requirements_json 结构见 docs/TRUST-LEVELS.md §4）
CREATE TABLE trust_level_rules (
    level INT NOT NULL,
    name VARCHAR(64) NOT NULL,
    summary TEXT NULL,
    requirements_json TEXT NULL,
    is_enabled TINYINT(1) NOT NULL DEFAULT 1,
    version BIGINT NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (level),
    CONSTRAINT trust_level_rules_level_ck CHECK (level >= 0 AND level <= 4)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

INSERT INTO trust_level_rules (level, name, summary, requirements_json, is_enabled, version, updated_at)
VALUES
    (0, '新用户', '注册默认等级；能力受反垃圾限制。', '{}', 1, 1, 1722816000),
    (1, '基本用户', '愿意阅读即可达到的第一级。', '{"topics_entered":5,"posts_read":30,"time_read_seconds":600}', 1, 1, 1722816000),
    (2, '成员', '持续活跃并参与讨论的正式成员。', '{"days_visited":15,"likes_given":1,"likes_received":1,"topics_replied_to":3,"topics_entered":20,"posts_read":100,"time_read_seconds":3600}', 1, 1, 1722816000),
    (3, '活跃用户', '滚动窗口考核，不达标降级（2 周宽限）。', '{"window_days":100,"visit_ratio":0.5,"replied_topics_window":10,"viewed_ratio":0.25,"viewed_cap":500,"read_ratio":0.25,"read_cap":20000,"likes_received_window":20,"likes_given_window":30,"like_distinct_user_divisor":5,"like_distinct_day_divisor":4,"max_flags":5,"no_sanction_months":6}', 1, 1, 1722816000),
    (4, '领导者', '仅可由工作人员手动授予。', '{"manual_only":true}', 1, 1, 1722816000);
