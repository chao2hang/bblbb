-- BBLBB user_preferences reaction notification preference (M07-SHOP-08, SQLite)
--
-- Reaction 通知偏好列：默认开启（1），用户可关闭（0）。
-- 与 mysql/mariadb 同版本同结构。

ALTER TABLE user_preferences ADD COLUMN reaction_notifications INTEGER NOT NULL DEFAULT 1;
