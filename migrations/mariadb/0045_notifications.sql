
ALTER TABLE notifications
    ADD COLUMN template_key VARCHAR(64) NULL,
    ADD COLUMN resource_type VARCHAR(32) NULL,
    ADD COLUMN resource_id VARCHAR(64) NULL,
    ADD COLUMN delivery_dedup_key VARCHAR(255) NULL,
    ADD COLUMN category VARCHAR(16) NOT NULL DEFAULT 'activity',
    ADD CONSTRAINT notifications_category_ck CHECK (category IN ('activity', 'moderation', 'system', 'security', 'digest'));

CREATE UNIQUE INDEX notifications_delivery_dedup_uq
    ON notifications (user_id, delivery_dedup_key);

CREATE TABLE notification_preferences (
    user_id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    category VARCHAR(16) NOT NULL,
    email_enabled TINYINT NOT NULL DEFAULT 1,
    in_app_enabled TINYINT NOT NULL DEFAULT 1,
    push_enabled TINYINT NOT NULL DEFAULT 1,
    updated_at BIGINT NOT NULL,
    PRIMARY KEY (user_id, category),
    CONSTRAINT notification_preferences_user_fk FOREIGN KEY (user_id) REFERENCES users (id) ON DELETE CASCADE,
    CONSTRAINT notification_preferences_category_ck CHECK (category IN ('activity', 'moderation', 'system', 'security', 'digest')),
    CONSTRAINT notification_preferences_security_ck CHECK (category != 'security' OR email_enabled = 1 OR in_app_enabled = 1 OR push_enabled = 1)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
