-- BBLBB 用户资料、隐私、展示偏好、等级缓存与 profile revision 迁移（M03-SCHEMA-01）
-- 1) users 增加资料字段与等级缓存（level 为可重建缓存，真实来源 M7 经验账户）；
-- 2) user_preferences：展示偏好（时区/语言/主题/通知 JSON）；
-- 3) user_privacy：隐私设置（邮箱/资料可见范围，默认最保守）；
-- 4) profile_revisions：资料每次变更追加一条修订（含 actor 与变更 JSON）。

ALTER TABLE users ADD COLUMN level INTEGER NOT NULL DEFAULT 1;
ALTER TABLE users ADD COLUMN level_updated_at INTEGER;
ALTER TABLE users ADD COLUMN avatar_attachment_id TEXT;
ALTER TABLE users ADD COLUMN signature TEXT;
ALTER TABLE users ADD COLUMN last_login_at INTEGER;
ALTER TABLE users ADD COLUMN delete_requested_at INTEGER;
ALTER TABLE users ADD COLUMN deleted_at INTEGER;

CREATE TABLE user_preferences (
    user_id TEXT PRIMARY KEY NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    locale TEXT NOT NULL DEFAULT 'zh-CN',
    theme_name TEXT,
    notification_json TEXT,
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE TABLE user_privacy (
    user_id TEXT PRIMARY KEY NOT NULL,
    email_visible_to TEXT NOT NULL DEFAULT 'nobody'
        CHECK (email_visible_to IN ('everyone', 'registered', 'nobody')),
    profile_visible_to TEXT NOT NULL DEFAULT 'everyone'
        CHECK (profile_visible_to IN ('everyone', 'registered', 'nobody')),
    updated_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
);

CREATE TABLE profile_revisions (
    id TEXT PRIMARY KEY NOT NULL,
    user_id TEXT NOT NULL,
    revision INTEGER NOT NULL,
    changes_json TEXT NOT NULL,
    actor_user_id TEXT,
    created_at INTEGER NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    UNIQUE (user_id, revision)
);

CREATE INDEX profile_revisions_user_idx ON profile_revisions (user_id);
