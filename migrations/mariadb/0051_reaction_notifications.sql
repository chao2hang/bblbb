
ALTER TABLE user_preferences
    ADD COLUMN reaction_notifications TINYINT(1) NOT NULL DEFAULT 1 COMMENT 'enable reaction notifications';
