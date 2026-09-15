-- Governance tables collation fix (issue #20)
--
-- 0073_platform_governance created bootstrap_tokens / feature_flags /
-- shop_site_config with *_bin column collations (ascii_bin) plus a
-- utf8mb4_bin table default. MariaDB sets the protocol-level BINARY flag
-- on *_bin collation string columns, so sqlx 0.8 decodes them as
-- VARBINARY and every read fails (String decode error): feature_flags
-- falls back to all-default, bootstrap_tokens and shop_site_config read
-- paths break. Repo rule (db/migrate.rs ensure_migration_table note +
-- migration 0004 note): never use *_bin collations on MySQL/MariaDB.
--
-- Applied migrations are immutable (schema_migrations checksum is
-- enforced by run_migrations), so 0073 is fixed by this follow-up
-- instead of an in-place edit: fresh deployments apply 0073 then 0078;
-- existing databases apply 0078 idempotently (production already
-- hand-ALTERed the same columns). Only the table default changes for
-- future columns; existing values are UUID/hex/enum ASCII strings, so
-- the _bin to _general_ci switch is lossless here. SQLite has no
-- protocol-level collation typing (see sqlite/0078 no-op).

ALTER TABLE bootstrap_tokens
    MODIFY id CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    MODIFY token_hash CHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    MODIFY consumed_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    COLLATE = utf8mb4_general_ci;

ALTER TABLE feature_flags
    MODIFY name VARCHAR(32) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    MODIFY updated_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    COLLATE = utf8mb4_general_ci;

ALTER TABLE shop_site_config
    MODIFY id VARCHAR(16) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    MODIFY updated_by CHAR(36) CHARACTER SET ascii COLLATE ascii_general_ci NULL,
    MODIFY default_refund_policy VARCHAR(32) CHARACTER SET utf8mb4 COLLATE utf8mb4_general_ci NOT NULL DEFAULT 'non_refundable',
    COLLATE = utf8mb4_general_ci;
