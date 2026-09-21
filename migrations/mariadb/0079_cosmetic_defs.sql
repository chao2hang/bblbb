-- BBLBB shop cosmetic style registry (M07-SHOP-UI-10, MariaDB)
--
-- cosmetic_defs: admin-defined named cosmetics referenced by product tokens
--   (`nickname.color.<id>` / `avatar.frame.<id>`); style_json holds structured,
--   server-validated style parameters (no arbitrary CSS).
-- v1 kinds: nickname_color / avatar_frame (table CHECK reserves the rest).

CREATE TABLE cosmetic_defs (
    id VARCHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci PRIMARY KEY NOT NULL,
    kind VARCHAR(32) NOT NULL,
    name VARCHAR(64) NOT NULL,
    style_json TEXT NOT NULL,
    status VARCHAR(16) NOT NULL DEFAULT 'active',
    created_by VARCHAR(64) CHARACTER SET ascii COLLATE ascii_general_ci NOT NULL,
    created_at BIGINT NOT NULL,
    updated_at BIGINT NOT NULL,
    CONSTRAINT cosmetic_defs_kind_ck CHECK (kind IN ('nickname_color', 'avatar_frame', 'profile_effect', 'post_effect', 'badge')),
    CONSTRAINT cosmetic_defs_status_ck CHECK (status IN ('active', 'archived'))
);

CREATE INDEX cosmetic_defs_kind_idx ON cosmetic_defs (kind, status);
