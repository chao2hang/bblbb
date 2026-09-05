-- BBLBB 管理域扩展（GAP-FIX 管理域 Part A，MySQL）
--
-- Table-for-table equivalent to migrations/sqlite/0061_admin_ext.sql:
-- 1) site_settings: single-row site settings (id='singleton', CHECK enforced)
--    with optimistic-lock version (If-Match for PATCH /api/v1/admin/settings);
--    seeded with sane defaults.
-- 2) notification_broadcasts: notification broadcast outbox;
--    notifications.broadcast_id links fan-out rows to their broadcast so
--    recall deletes unread rows (is_read=0) by exact association.
-- 3) posts.price_coin (paid unlock pricing; NULL=free). is_featured /
--    is_pinned / is_locked reuse the existing synonymous columns
--    (featured_at / pinned+pinned_at / closed_at) -- no duplicate booleans.
-- 4) tags.status: tag lifecycle marker (NULL=normal; 'merged'=absorbed by
--    merge; orthogonal to is_active).
-- 5) ai_site_config: single-row site-level AI config (switch/data mode/
--    purpose flags/budgets JSON) backing the top-level version of
--    GET/PATCH /api/v1/admin/ai/config.

CREATE TABLE site_settings (
    id VARCHAR(16) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    open_registration TINYINT(1) NOT NULL DEFAULT 1,
    email_verification TINYINT(1) NOT NULL DEFAULT 1,
    anonymous_replies TINYINT(1) NOT NULL DEFAULT 0,
    public_rss TINYINT(1) NOT NULL DEFAULT 1,
    maintenance_mode TINYINT(1) NOT NULL DEFAULT 0,
    site_name VARCHAR(100) NOT NULL DEFAULT 'BBLBB',
    default_lang VARCHAR(16) NOT NULL DEFAULT 'zh-CN',
    api_rate_limit BIGINT NULL,
    version BIGINT NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT site_settings_singleton_ck CHECK (id = 'singleton')
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_0900_as_cs;

INSERT INTO site_settings (id, open_registration, email_verification, anonymous_replies, public_rss, maintenance_mode, site_name, default_lang, api_rate_limit, version, updated_at)
VALUES ('singleton', 1, 1, 0, 1, 0, 'BBLBB', 'zh-CN', 60, 1, 1722816000);

CREATE TABLE notification_broadcasts (
    id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    title VARCHAR(200) NOT NULL,
    body TEXT NOT NULL,
    target_count BIGINT NOT NULL DEFAULT 0,
    created_at BIGINT NOT NULL,
    PRIMARY KEY (id)
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_0900_as_cs;

CREATE INDEX notification_broadcasts_created_idx ON notification_broadcasts (created_at);

ALTER TABLE notifications ADD COLUMN broadcast_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL;

CREATE INDEX notifications_broadcast_idx ON notifications (broadcast_id);

ALTER TABLE posts ADD COLUMN price_coin BIGINT NULL;

ALTER TABLE tags ADD COLUMN status VARCHAR(32) NULL;

CREATE TABLE ai_site_config (
    id VARCHAR(16) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    enabled TINYINT(1) NOT NULL DEFAULT 0,
    data_mode VARCHAR(32) NOT NULL DEFAULT 'redacted',
    flags_json VARCHAR(500) NOT NULL DEFAULT '{}',
    budgets_json VARCHAR(500) NOT NULL DEFAULT '{}',
    version BIGINT NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (id),
    CONSTRAINT ai_site_config_singleton_ck CHECK (id = 'singleton')
) ENGINE = InnoDB DEFAULT CHARACTER SET = utf8mb4 COLLATE = utf8mb4_0900_as_cs;

INSERT INTO ai_site_config (id, enabled, data_mode, flags_json, budgets_json, version, updated_at)
VALUES ('singleton', 0, 'redacted', '{}', '{}', 1, 1722816000);
