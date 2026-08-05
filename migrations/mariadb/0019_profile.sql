-- BBLBB user profile, privacy, preferences, level cache and profile revision migration (MySQL)
-- 1) users gains profile fields and the level cache (level is a rebuildable cache;
--    the true source is the M7 experience ledger);
-- 2) user_preferences: display preferences (timezone/locale/theme/notification JSON);
-- 3) user_privacy: privacy settings (email/profile visibility, most restrictive by default);
-- 4) profile_revisions: one revision row per profile change (actor and changes JSON).

ALTER TABLE users ADD COLUMN level BIGINT NOT NULL DEFAULT 1;
ALTER TABLE users ADD COLUMN level_updated_at BIGINT NULL;
ALTER TABLE users ADD COLUMN avatar_attachment_id VARCHAR(36) NULL;
ALTER TABLE users ADD COLUMN signature TEXT NULL;
ALTER TABLE users ADD COLUMN last_login_at BIGINT NULL;
ALTER TABLE users ADD COLUMN delete_requested_at BIGINT NULL;
ALTER TABLE users ADD COLUMN deleted_at BIGINT NULL;

CREATE TABLE user_preferences (
    user_id VARCHAR(36) NOT NULL PRIMARY KEY,
    timezone VARCHAR(64) NOT NULL DEFAULT 'UTC',
    locale VARCHAR(16) NOT NULL DEFAULT 'zh-CN',
    theme_name VARCHAR(64) NULL,
    notification_json TEXT NULL,
    updated_at BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE user_privacy (
    user_id VARCHAR(36) NOT NULL PRIMARY KEY,
    email_visible_to VARCHAR(16) NOT NULL DEFAULT 'nobody',
    profile_visible_to VARCHAR(16) NOT NULL DEFAULT 'everyone',
    updated_at BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT user_privacy_email_visible_check CHECK (email_visible_to IN ('everyone', 'registered', 'nobody')),
    CONSTRAINT user_privacy_profile_visible_check CHECK (profile_visible_to IN ('everyone', 'registered', 'nobody'))
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE profile_revisions (
    id VARCHAR(36) NOT NULL PRIMARY KEY,
    user_id VARCHAR(36) NOT NULL,
    revision BIGINT NOT NULL,
    changes_json TEXT NOT NULL,
    actor_user_id VARCHAR(36) NULL,
    created_at BIGINT NOT NULL,
    FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    UNIQUE (user_id, revision)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE INDEX profile_revisions_user_idx ON profile_revisions (user_id);
