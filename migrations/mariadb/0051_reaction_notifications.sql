-- BBLBB user_preferences reaction notification preference (M07-SHOP-08, MariaDB)
--
-- Reaction notification preference column: default on (1), user may opt out (0).

ALTER TABLE user_preferences
    ADD COLUMN reaction_notifications TINYINT(1) NOT NULL DEFAULT 1 COMMENT 'enable reaction notifications';
