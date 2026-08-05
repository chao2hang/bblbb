-- BBLBB boards visibility/posting_mode CHECK parity (M03-SCHEMA-07)
-- 0022 in mysql/mariadb added the columns with NOT NULL DEFAULT only, without
-- the CHECKs that sqlite inlined at column creation. This migration adds the
-- equivalent CHECK constraints so that SCHEMA.md §6 "CHECK 约束三库强制"
-- actually holds on MySQL 8 and MariaDB 10.11.

ALTER TABLE boards ADD CONSTRAINT boards_visibility_ck CHECK (visibility IN ('public', 'members', 'restricted', 'hidden'));
ALTER TABLE boards ADD CONSTRAINT boards_posting_mode_ck CHECK (posting_mode IN ('normal', 'approval', 'readonly', 'closed'));
