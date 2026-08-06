-- BBLBB 内容访问策略（M04-SCHEMA-06，SQLite）
--
-- content_access_policies：受限内容策略行（OpenAPI access_policy）——
-- kind 封闭枚举 public/logged_in/after_reply/level/paid（与
-- posts.visibility 遗留列值域一致，M04-VISIBILITY-01 形式化）；
-- level 策略需 min_level；paid 策略需 currency_id+amount；
-- reply_grant_persists=1 表示回复删除后授权保留（M04-VISIBILITY-05 冻结规则）；
-- policy_version 为策略版本（升级时评估行为变更）。
--
-- posts.access_policy_id：可空外键（未设=public）；策略删除时置空回退
-- public（显式更新 posts 优先）。

CREATE TABLE content_access_policies (
    id TEXT PRIMARY KEY NOT NULL,
    kind TEXT NOT NULL
        CHECK (kind IN ('public', 'logged_in', 'after_reply', 'level', 'paid')),
    min_level INTEGER,
    currency_id TEXT,
    amount INTEGER,
    reply_grant_persists INTEGER NOT NULL DEFAULT 0,
    policy_version INTEGER NOT NULL DEFAULT 1,
    created_by TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (created_by) REFERENCES users (id) ON DELETE RESTRICT
);

ALTER TABLE posts ADD COLUMN access_policy_id TEXT
    REFERENCES content_access_policies (id) ON DELETE SET NULL;

CREATE INDEX posts_access_policy_idx ON posts (access_policy_id);
